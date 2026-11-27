// SPDX-License-Identifier: MPL-2.0
//! Stateless JSON commands for engineering builds. The host supplies protocol verifier verdicts,
//! live facts and time. Archive metadata never supplies any of those facts.

use om_archive_reader::import::{self, Evidence, ImportView, ObjectStatus, Quarantine, Verified};
use om_archive_reader::writer::{self, ExportError, ExportJob, WriteError};
use om_archive_reader::{
    Archive, ArchiveError, EntryKind, Limits, ZipError, read_archive, readable, rehydrate,
};
use om_core::crypto::{hex, sha256, unhex};
use serde::{Deserialize, de::IgnoredAny};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

macro_rules! error_codes {
    ($($variant:ident => $name:literal, $code:literal);+ $(;)?) => {
        #[derive(Clone, Copy)]
        enum Refusal { $($variant),+, Archive(ArchiveError) }

        impl Refusal {
            fn name(self) -> &'static str {
                match self { $(Self::$variant => $name,)+ Self::Archive(error) => Self::archive_class(error).name() }
            }

            fn code(self) -> u8 {
                match self { $(Self::$variant => $code,)+ Self::Archive(error) => Self::archive_class(error).code() }
            }

            fn response(self) -> Value {
                let mut value = json!({ "refused": self.name(), "code": self.code() });
                if let Self::Archive(error) = self {
                    value["reason"] = json!(error.class_name());
                }
                value
            }
        }

        fn error_registry() -> BTreeMap<&'static str, u8> {
            [$((Refusal::$variant.name(), Refusal::$variant.code())),+]
                .into_iter()
                .collect()
        }
    };
}

error_codes! {
    MalformedRequest => "malformed_request", 1;
    NotAnArchive => "not_an_archive", 2;
    EncryptedUnsupported => "encrypted_unsupported", 3;
    InvalidArchive => "invalid_archive", 4;
    IntegrityFailure => "integrity_failure", 5;
    TooLarge => "too_large", 6;
    InvalidScope => "invalid_scope", 7;
    UnauthorizedDevice => "unauthorized_device", 8;
    WrongAccount => "wrong_account", 9;
}

use Refusal::*;

impl From<ArchiveError> for Refusal {
    fn from(error: ArchiveError) -> Self {
        Self::Archive(error)
    }
}

impl Refusal {
    fn archive_class(error: ArchiveError) -> Self {
        match error {
            ArchiveError::NotOmArchive => NotAnArchive,
            ArchiveError::EncryptedUnsupported => EncryptedUnsupported,
            ArchiveError::IntegrityRoot
            | ArchiveError::EntrySize
            | ArchiveError::EntryHash
            | ArchiveError::Zip(ZipError::Crc) => IntegrityFailure,
            ArchiveError::ManifestSizeLimit
            | ArchiveError::Zip(
                ZipError::SizeLimit | ZipError::CountLimit | ZipError::TotalSizeLimit,
            ) => TooLarge,
            ArchiveError::Zip(_)
            | ArchiveError::UnsupportedVersion
            | ArchiveError::UnexpectedLayout
            | ArchiveError::Cbor
            | ArchiveError::Schema
            | ArchiveError::UnknownCritical
            | ArchiveError::SecretMaterial
            | ArchiveError::MissingEntry
            | ArchiveError::UnexpectedEntry => InvalidArchive,
        }
    }
}

type Reply = Result<Value, Refusal>;

fn bound(count: usize, max: u64) -> Result<(), Refusal> {
    if count as u64 > max {
        return Err(TooLarge);
    }
    Ok(())
}

fn bytes(value: &str, max: u64) -> Result<Vec<u8>, Refusal> {
    bound(value.len().div_ceil(2), max)?;
    if !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(MalformedRequest);
    }
    unhex(value).map_err(|_| MalformedRequest)
}

fn fixed<const N: usize>(value: &str) -> Result<[u8; N], Refusal> {
    if value.len() != 2 * N {
        return Err(MalformedRequest);
    }
    bytes(value, N as u64)?
        .try_into()
        .map_err(|_| MalformedRequest)
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Uint {
    Number(u64),
    Decimal(String),
}

impl Uint {
    fn get(self) -> Result<u64, Refusal> {
        match self {
            Self::Number(n) => Ok(n),
            Self::Decimal(s) if s.bytes().all(|b| b.is_ascii_digit()) => {
                s.parse().map_err(|_| MalformedRequest)
            }
            Self::Decimal(_) => Err(MalformedRequest),
        }
    }
}

#[derive(Deserialize)]
#[serde(
    tag = "op",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Op {
    Registry,
    Inspect {
        archive_hex: String,
    },
    ImportReadonly {
        archive_hex: String,
        live_control: Vec<ControlVersion>,
        verdicts: Vec<Verdict>,
    },
    ExportPlan {
        candidates: Vec<Candidate>,
        selection: Selection,
        meta: ManifestMeta,
    },
    Export {
        candidates: Vec<Candidate>,
        selection: Selection,
        meta: ManifestMeta,
    },
    ReadableExport {
        conversations: Vec<Conversation>,
        include_disappearing: Option<bool>,
    },
    RehydratePlan {
        view: PlanView,
        items: Vec<RehydrateItem>,
        context: RehydrateContext,
    },
}

pub(crate) fn portability(request: Value) -> Value {
    let Ok(op) = serde_json::from_value(request) else {
        return MalformedRequest.response();
    };
    execute(op, &Limits::default()).unwrap_or_else(Refusal::response)
}

fn execute(op: Op, limits: &Limits) -> Reply {
    match op {
        Op::Registry => Ok(json!({ "errorCodes": error_registry() })),
        Op::Inspect { archive_hex } => {
            let bytes = bytes(&archive_hex, limits.max_total_size)?;
            Ok(inspect(&read_archive(&bytes, limits)?))
        }
        Op::ImportReadonly {
            archive_hex,
            live_control,
            verdicts,
        } => {
            let bytes = bytes(&archive_hex, limits.max_total_size)?;
            let archive = read_archive(&bytes, limits)?;
            let live = Live::new(live_control, limits)?;
            let verifier = Verifier::new(verdicts, limits)?;
            Ok(import_view(import::import_readonly(
                &archive, &live, &verifier,
            )))
        }
        Op::ExportPlan {
            candidates,
            selection,
            meta,
        } => {
            let job = export_job(candidates, selection, meta, limits)?;
            Ok(json!({ "report": report(&job.report) }))
        }
        Op::Export {
            candidates,
            selection,
            meta,
        } => {
            let job = export_job(candidates, selection, meta, limits)?;
            export(job, limits)
        }
        Op::ReadableExport {
            conversations,
            include_disappearing,
        } => readable_export(conversations, include_disappearing, limits),
        Op::RehydratePlan {
            view,
            items,
            context,
        } => rehydrate_plan(view, items, context, limits),
    }
}

fn inspect(archive: &Archive<'_>) -> Value {
    let mut objects = 0;
    let mut blobs = 0;
    let mut records = 0;
    for entry in &archive.entries {
        match entry.kind {
            EntryKind::CanonicalObject => objects += 1,
            EntryKind::Blob => blobs += 1,
            EntryKind::Record => records += 1,
        }
    }
    json!({
        "manifest": {
            "formatVersion": "om-archive-v1", "archiveId": hex(&archive.archive_id),
            "encryptionProfile": null, "objects": objects, "blobs": blobs + archive.missing_blobs.len(),
            "records": records, "missingBlobs": archive.missing_blobs,
        },
        "modes": { "read_only": "available", "same_account": "requires_live_validation" },
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ControlVersion {
    stream: String,
    version: Uint,
}

struct Live(BTreeMap<String, u64>);

impl Live {
    fn new(facts: Vec<ControlVersion>, limits: &Limits) -> Result<Self, Refusal> {
        bound(facts.len(), limits.max_entries)?;
        let mut versions = BTreeMap::new();
        for ControlVersion { stream, version } in facts {
            if versions.insert(stream, version.get()?).is_some() {
                return Err(MalformedRequest);
            }
        }
        Ok(Self(versions))
    }
}

impl import::LiveControl for Live {
    fn live_version(&self, stream: &str) -> Option<u64> {
        self.0.get(stream).copied()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum EvidenceIn {
    ForeignIdentityLink,
    BridgeAttestation,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VerifiedIn {
    control: Option<ControlVersion>,
    evidence: Option<EvidenceIn>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Verdict {
    object_type: String,
    sha256: String,
    verified: Option<VerifiedIn>,
}

/// A verdict binds the exact object bytes and their declared type. Missing verdicts fail closed.
struct Verifier(BTreeMap<String, BTreeMap<[u8; 32], Option<Verified>>>);

impl Verifier {
    fn new(verdicts: Vec<Verdict>, limits: &Limits) -> Result<Self, Refusal> {
        bound(verdicts.len(), limits.max_entries)?;
        let mut by_type = BTreeMap::<_, BTreeMap<_, _>>::new();
        for verdict in verdicts {
            let verified = verdict
                .verified
                .map(|v| {
                    let control = v
                        .control
                        .map(|c| Ok::<_, Refusal>((c.stream, c.version.get()?)))
                        .transpose()?;
                    let evidence = v.evidence.map(|e| match e {
                        EvidenceIn::ForeignIdentityLink => Evidence::ForeignIdentityLink,
                        EvidenceIn::BridgeAttestation => Evidence::BridgeAttestation,
                    });
                    Ok::<_, Refusal>(Verified { control, evidence })
                })
                .transpose()?;
            if by_type
                .entry(verdict.object_type)
                .or_default()
                .insert(fixed(&verdict.sha256)?, verified)
                .is_some()
            {
                return Err(MalformedRequest);
            }
        }
        Ok(Self(by_type))
    }
}

impl import::ObjectVerifier for Verifier {
    fn verify(&self, declared_type: &str, bytes: &[u8]) -> Option<Verified> {
        self.0.get(declared_type)?.get(&sha256(&[bytes]))?.clone()
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Status {
    VerifiedHistory,
    StaleControl { archived: Uint, live: Uint },
    ControlNeedsLiveValidation { archived: Uint },
    Unverified,
}

impl Status {
    fn into_reader(self) -> Result<ObjectStatus, Refusal> {
        Ok(match self {
            Self::VerifiedHistory => ObjectStatus::VerifiedHistory,
            Self::StaleControl { archived, live } => ObjectStatus::StaleControl {
                archived: archived.get()?,
                live: live.get()?,
            },
            Self::ControlNeedsLiveValidation { archived } => {
                ObjectStatus::ControlNeedsLiveValidation {
                    archived: archived.get()?,
                }
            }
            Self::Unverified => ObjectStatus::Unverified,
        })
    }
}

fn status(value: ObjectStatus) -> Value {
    match value {
        ObjectStatus::VerifiedHistory => json!({ "kind": "verified_history" }),
        ObjectStatus::StaleControl { archived, live } => json!({
            "kind": "stale_control", "archived": archived.to_string(), "live": live.to_string(),
        }),
        ObjectStatus::ControlNeedsLiveValidation { archived } => json!({
            "kind": "control_needs_live_validation", "archived": archived.to_string(),
        }),
        ObjectStatus::Unverified => json!({ "kind": "unverified" }),
    }
}

fn quarantine(reason: Quarantine) -> &'static str {
    match reason {
        Quarantine::NotAMap => "not_a_map",
        Quarantine::NativeAuthorshipClaim(_) => "native_authorship_claim",
        Quarantine::UnknownField(_) => "unknown_field",
        Quarantine::MissingField(_) => "missing_field",
        Quarantine::UnknownProvenance => "unknown_provenance",
        Quarantine::Malformed => "malformed",
    }
}

fn import_view(view: ImportView) -> Value {
    json!({
        "objects": view.objects.into_iter().map(|(path, value)| {
            json!({ "path": path, "status": status(value) })
        }).collect::<Vec<_>>(),
        "records": view.records.into_iter().map(|r| json!({
            "importedRecordId": r.imported_record_id,
            "sourceNetworkNamespace": r.source_network_namespace,
            "claimedLevel": r.claimed_level,
            "effectiveLevel": r.effective_level,
            "downgraded": r.downgraded,
        })).collect::<Vec<_>>(),
        "quarantined": view.quarantined.into_iter().map(|(path, index, reason)| json!({
            "path": path, "index": index, "reason": quarantine(reason),
        })).collect::<Vec<_>>(),
        "missingBlobs": view.missing_blobs,
    })
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ManifestMeta {
    archive_id: String,
    export_scope_hex: String,
    // Preserve the writer's distinction between omitted and explicit null.
    #[serde(default, deserialize_with = "present_hint")]
    source_account_hint: Option<Option<String>>,
    included_conversations: Option<Vec<String>>,
    generated_at_hint: Uint,
    exporter_implementation: Option<String>,
    provenance_profiles: Vec<String>,
}

fn present_hint<'de, D: serde::Deserializer<'de>>(
    de: D,
) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(de).map(Some)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Selection {
    conversations: Option<BTreeSet<String>>,
    from: Option<Uint>,
    to: Option<Uint>,
    include_blobs: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Candidate {
    item: Item,
    conversation: Option<String>,
    time: Option<Uint>,
    disappearing: bool,
    view_once: bool,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
enum Item {
    Object {
        object_type: String,
        bytes_hex: String,
    },
    Blob {
        blob_id: String,
        media_type: String,
        bytes_hex: Option<String>,
        declared: Option<DeclaredBlob>,
    },
    Record {
        record_type: String,
        stem: String,
        bytes_hex: String,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeclaredBlob {
    size: Uint,
    sha256: String,
}

impl Item {
    fn into_reader(self, limits: &Limits, total: &mut u64) -> Result<writer::Item, Refusal> {
        let mut decode = |value: String| {
            *total = total
                .checked_add(value.len().div_ceil(2) as u64)
                .ok_or(TooLarge)?;
            if *total > limits.max_total_size {
                return Err(TooLarge);
            }
            bytes(&value, limits.max_entry_size)
        };
        Ok(match self {
            Self::Object {
                object_type,
                bytes_hex,
            } => writer::Item::Object {
                object_type,
                bytes: decode(bytes_hex)?,
            },
            Self::Blob {
                blob_id,
                media_type,
                bytes_hex,
                declared,
            } => writer::Item::Blob {
                blob_id: fixed(&blob_id)?,
                media_type,
                bytes: bytes_hex.map(decode).transpose()?,
                declared: declared
                    .map(|d| Ok::<_, Refusal>((d.size.get()?, fixed(&d.sha256)?)))
                    .transpose()?,
            },
            Self::Record {
                record_type,
                stem,
                bytes_hex,
            } => writer::Item::Record {
                record_type,
                stem,
                bytes: decode(bytes_hex)?,
            },
        })
    }
}

fn export_job(
    candidates: Vec<Candidate>,
    selection: Selection,
    meta: ManifestMeta,
    limits: &Limits,
) -> Result<ExportJob, Refusal> {
    bound(candidates.len(), limits.max_entries)?;
    let from = selection.from.map(Uint::get).transpose()?;
    let to = selection.to.map(Uint::get).transpose()?;
    if matches!((from, to), (Some(from), Some(to)) if from > to) {
        return Err(InvalidScope);
    }
    let selection = writer::Selection {
        conversations: selection.conversations,
        from,
        to,
        include_blobs: selection.include_blobs,
    };
    let meta = writer::ManifestMeta {
        archive_id: fixed(&meta.archive_id)?,
        export_scope: bytes(&meta.export_scope_hex, limits.max_manifest_size)?,
        source_account_hint: meta.source_account_hint,
        included_conversations: meta.included_conversations,
        generated_at_hint: meta.generated_at_hint.get()?,
        exporter_implementation: meta.exporter_implementation,
        provenance_profiles: meta.provenance_profiles,
    };
    let mut total = 0;
    let candidates = candidates
        .into_iter()
        .map(|c| {
            Ok(writer::Candidate {
                item: c.item.into_reader(limits, &mut total)?,
                conversation: c.conversation,
                time: c.time.map(Uint::get).transpose()?,
                disappearing: c.disappearing,
                view_once: c.view_once,
            })
        })
        .collect::<Result<_, Refusal>>()?;
    Ok(ExportJob::plan(meta, candidates, &selection))
}

fn report(report: &writer::PortabilityReport) -> Value {
    let excluded: BTreeMap<_, _> = report
        .excluded
        .iter()
        .map(|(reason, count)| {
            (
                match reason {
                    writer::Excluded::SecretType => "secret_type",
                    writer::Excluded::ViewOnce => "view_once",
                    writer::Excluded::Disappearing => "disappearing",
                    writer::Excluded::NotSelected => "not_selected",
                },
                count,
            )
        })
        .collect();
    json!({
        "objects": report.objects, "blobs": report.blobs, "records": report.records,
        "omittedBlobs": report.omitted_blobs, "excluded": excluded,
    })
}

/// ExportJob retains the payload bytes for this whole synchronous operation.
struct MemoryStage<'a> {
    paths: BTreeSet<String>,
    limits: &'a Limits,
    archive_hex: Option<String>,
    expected_items: usize,
}

impl writer::Staging for MemoryStage<'_> {
    type Error = Refusal;

    fn stage(&mut self, path: &str, _bytes: &[u8]) -> Result<(), Self::Error> {
        self.paths.insert(path.to_owned());
        Ok(())
    }

    fn is_durable(&self, path: &str) -> bool {
        self.paths.contains(path)
    }

    fn publish(&mut self, archive: &[u8]) -> Result<(), Self::Error> {
        // The writer accepts opaque metadata and record bytes. Verify the complete result before returning it.
        bound(archive.len(), self.limits.max_total_size)?;
        let verified = read_archive(archive, self.limits)?;
        // The writer coalesces duplicate paths. Refuse ambiguous input instead of returning inaccurate counts.
        if verified.entries.len() + verified.missing_blobs.len() != self.expected_items {
            return Err(InvalidScope);
        }
        self.archive_hex = Some(hex(archive));
        Ok(())
    }

    fn discard(&mut self) {
        self.paths.clear();
        self.archive_hex = None;
    }
}

impl From<ExportError<Refusal>> for Refusal {
    fn from(error: ExportError<Refusal>) -> Self {
        match error {
            ExportError::Staging(e) => e,
            ExportError::Write(WriteError::TooManyItems) => TooLarge,
            ExportError::Write(_) | ExportError::NotDurable(_) => InvalidArchive,
        }
    }
}

fn export(job: ExportJob, limits: &Limits) -> Reply {
    let mut stage = MemoryStage {
        paths: BTreeSet::new(),
        limits,
        archive_hex: None,
        expected_items: job.report.objects + job.report.blobs + job.report.records,
    };
    job.stage_all(&mut stage)?;
    let report = job.finish(&mut stage)?;
    Ok(
        json!({ "archiveHex": stage.archive_hex.ok_or(InvalidArchive)?, "report": self::report(&report) }),
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Conversation {
    title: String,
    messages: Vec<Message>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Message {
    time: String,
    author: String,
    text: String,
    disappearing: bool,
    view_once: bool,
}

fn readable_export(
    conversations: Vec<Conversation>,
    include_disappearing: Option<bool>,
    limits: &Limits,
) -> Reply {
    bound(conversations.len(), limits.max_entries)?;
    let mut messages = 0u64;
    let conversations = conversations
        .into_iter()
        .map(|c| {
            messages += c.messages.len() as u64;
            if messages > limits.max_entries {
                return Err(TooLarge);
            }
            let messages = c
                .messages
                .into_iter()
                .map(|m| readable::Message {
                    time: m.time,
                    author: m.author,
                    text: m.text,
                    disappearing: m.disappearing,
                    view_once: m.view_once,
                })
                .collect();
            Ok((c.title, messages))
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    let result = readable::export(&conversations, include_disappearing);
    let mut total = 0;
    let mut files = Vec::new();
    for (path, bytes) in result.files {
        bound(bytes.len(), limits.max_entry_size)?;
        total += bytes.len() as u64;
        if total > limits.max_total_size {
            return Err(TooLarge);
        }
        files.push(json!({ "path": path, "bytesHex": hex(&bytes) }));
    }
    Ok(json!({
        "files": files,
        "disappearingChoice": match result.disappearing_choice {
            readable::DisappearingChoice::Excluded => "excluded",
            readable::DisappearingChoice::IncludedExplicitly => "included_explicitly",
        },
        "excludedDisappearing": result.excluded_disappearing,
        "excludedViewOnce": result.excluded_view_once,
        "isMigrationFormat": result.is_migration_format,
        "warning": result.warning,
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PlanView {
    objects: Vec<ViewObject>,
    // The reader's planner uses only object statuses.
    #[serde(rename = "records")]
    _records: IgnoredAny,
    #[serde(rename = "quarantined")]
    _quarantined: IgnoredAny,
    #[serde(rename = "missingBlobs")]
    _missing_blobs: IgnoredAny,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ViewObject {
    path: String,
    status: Status,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RehydrateItem {
    id: String,
    view_once: bool,
    expiry: Option<Uint>,
    device: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct RehydrateContext {
    authorized_device: bool,
    account_matches: bool,
    now: Uint,
    can_enforce_expiry: bool,
    live_control: Vec<ControlVersion>,
    revoked_devices: Vec<String>,
}

fn rehydrate_plan(
    view: PlanView,
    items: Vec<RehydrateItem>,
    context: RehydrateContext,
    limits: &Limits,
) -> Reply {
    bound(view.objects.len(), limits.max_entries)?;
    bound(items.len(), limits.max_entries)?;
    bound(context.revoked_devices.len(), limits.max_entries)?;
    let view = ImportView {
        objects: view
            .objects
            .into_iter()
            .map(|o| Ok((o.path, o.status.into_reader()?)))
            .collect::<Result<_, Refusal>>()?,
        ..Default::default()
    };
    let items = items
        .into_iter()
        .map(|i| {
            Ok(rehydrate::Item {
                id: i.id,
                view_once: i.view_once,
                expiry: i.expiry.map(Uint::get).transpose()?,
                device: i.device,
            })
        })
        .collect::<Result<Vec<_>, Refusal>>()?;
    let live = Live::new(context.live_control, limits)?;
    let context = rehydrate::Context {
        authorized_device: context.authorized_device,
        account_matches: context.account_matches,
        now: context.now.get()?,
        can_enforce_expiry: context.can_enforce_expiry,
        live: &live,
        revoked_devices: &context.revoked_devices,
    };
    let plan = rehydrate::plan(&view, &items, &context).map_err(|reason| match reason {
        rehydrate::Refusal::NotAuthorizedDevice => UnauthorizedDevice,
        rehydrate::Refusal::WrongAccount => WrongAccount,
    })?;
    Ok(json!({
        "import": plan.import,
        "scheduleDeletion": plan.schedule_deletion.into_iter().map(|(id, expiry)| (id, expiry.to_string())).collect::<Vec<_>>(),
        "droppedExpired": plan.dropped_expired,
        "refusedViewOnce": plan.refused_view_once,
        "skippedUnenforceable": plan.skipped_unenforceable,
        "historyOnlyRevokedDevice": plan.history_only_revoked_device,
        "needsLiveValidation": plan.needs_live_validation,
        "staleControl": plan.stale_control,
        "freshSessionsRequired": plan.fresh_sessions_required,
    }))
}
