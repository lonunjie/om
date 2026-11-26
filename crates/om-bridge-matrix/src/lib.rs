// SPDX-License-Identifier: MPL-2.0
//! Sans-IO adapter for a replaceable bridge endpoint on a Matrix test network.

mod api;
mod events;
mod json;

pub use api::{
    ApiError, Endpoint, Method, Request, SyncBatch, parse_event_response, parse_sync_response,
};
pub use events::{
    Attachment, AttachmentKind, Content, EventKind, ForeignEvent, INTEROP_FIELD, InteropMetadata,
    InteropTrace, NAMESPACE, observe,
};
pub use json::{MAX_EVENT_BYTES, MAX_JSON_DEPTH, MAX_STRING_BYTES, MAX_SYNC_BYTES, ParseError};

use std::collections::BTreeMap;

use events::{body_fits, map, valid_ref};
use om_codec::{Registry, Value as Cbor, cbor};
use om_core::bridge_control::{BridgeAuthorError, BridgeChain, validate_bridge_event};
use om_core::bridge_fidelity::{self as fidelity, Carry, Op, Support, sem};
use om_core::bridge_identity::{Display, LinkTable};
use om_core::bridge_provenance::{
    DEFAULT_HOP_LIMIT, ImportMapping, ImportRefusal, RoomProvenance, Verdict, origin_fingerprint,
};
use om_core::bridge_room::{BOTH, Blocked, BridgeRoom, TO_FOREIGN, TO_OM};
use om_core::crypto::sig::ds_hash;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

pub type EventId = [u8; 16];
pub type BridgeId = [u8; 32];

pub struct InteropCapabilities {
    /// Rich text is omitted from Support because the planner must render it as plain text.
    pub support: Support,
    pub rich_text_lossy: bool,
    pub deletes_are_requests: bool,
    pub polls_are_read_only_summaries: bool,
    pub carries_interop_metadata: bool,
}

pub fn capabilities() -> InteropCapabilities {
    InteropCapabilities {
        support: Support {
            semantics: [
                sem::TEXT,
                sem::ATTACHMENTS,
                sem::REPLIES,
                sem::EDITS,
                sem::DELETES,
                sem::REACTIONS,
                sem::THREADS_TOPICS,
            ]
            .into(),
            max_attachment_bytes: None,
        },
        rich_text_lossy: true,
        deletes_are_requests: true,
        polls_are_read_only_summaries: true,
        carries_interop_metadata: true,
    }
}

/// Checks presentation evidence supplied by Core for this exact handle. Matrix event JSON
/// contains no identity-link proof, and the caller must keep the authenticated link table current.
pub fn verify_identity_link(
    event: &ForeignEvent,
    links: &LinkTable,
    account: &[u8; 32],
    now: u64,
) -> Display {
    links.display(account, NAMESPACE, &event.actor_ref, now)
}

#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Binding {
    pub incarnation: [u8; 16],
    pub room: [u8; 32],
    pub matrix_room: String,
    pub bridge: BridgeId,
    pub endpoint_id: Vec<u8>,
    pub admitted_epoch: u64,
}

/// Authenticated Room facts for a single decision. The host refreshes these before dispatch or
/// materialization, and supplies an active mapping only while its authorization and head are fresh.
pub struct RoomAccess<'a> {
    pub room: &'a BridgeRoom,
    pub mapping: &'a ImportMapping,
    pub epoch: u64,
    /// Time in the same clock domain as BridgeRoom's head observations.
    pub now: u64,
    /// POSIX seconds for foreign provenance and identity-link validity. Never used for ordering.
    pub observed_at: u64,
    /// The host verified that the Matrix room is unencrypted and approved for plaintext.
    pub matrix_plaintext_allowed: bool,
    pub edits_allowed: bool,
    /// Explicit Room approval. A signed bridge mapping alone cannot grant relay.
    pub relay_bridged: bool,
    /// The endpoint's support is insufficient by itself. This fact covers the entire foreign path.
    pub path_preserves_metadata: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Relation<T> {
    Reply(T),
    Thread(T),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Message {
    Text { body: String, rich: bool },
    Attachment(Attachment),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rendering {
    Message {
        message: Message,
        relation: Option<Relation<EventId>>,
    },
    Edit {
        target: EventId,
        body: String,
        rich: bool,
    },
    Reaction {
        target: EventId,
        key: String,
    },
    Redact {
        target: EventId,
        reason: Option<String>,
    },
    /// Host-projected poll creation summary with no individual votes.
    PollSummary {
        body: String,
    },
}

impl Rendering {
    fn class(&self) -> u8 {
        match self {
            Self::Message { .. } => 1,
            Self::Edit { .. } => 2,
            Self::Redact { .. } => 3,
            Self::Reaction { .. } => 4,
            Self::PollSummary { .. } => 8,
        }
    }
}

/// The host constructs this from a decrypted, authenticated OM event.
pub struct ForeignRendering {
    pub event_id: EventId,
    pub epoch: u64,
    pub author: RenderingAuthor,
    pub rendering: Rendering,
}

pub enum RenderingAuthor {
    Native,
    Bridged {
        bridge: BridgeId,
        trace: InteropTrace,
        origin_verified: bool,
    },
}

/// A diagnostic reason with no raw JSON, actor identifiers, or message content.
/// Surface Fidelity (including its flags) as a user-visible omission.
#[derive(Debug, PartialEq, Eq)]
pub enum ExportDrop {
    Inactive,
    OutsideMembership,
    Blocked(Blocked),
    OwnImport,
    RelayDenied,
    Reflection,
    HopLimit,
    AlreadyExported,
    InvalidRendering,
    EditPolicyChanged,
    MappingConflict,
    Fidelity(Vec<u8>),
}

#[derive(Debug, PartialEq, Eq)]
pub enum ImportDrop {
    Inactive,
    WrongScope,
    Reflection,
    Echo,
    Replay,
    Duplicate,
    HopLimit,
    InvalidEvent,
    InvalidTrace,
    MappingConflict,
    Authorship(BridgeAuthorError),
    Core(ImportRefusal),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportedAction {
    Message {
        message: Message,
        relation: Option<Relation<EventId>>,
    },
    Edit {
        target: EventId,
        body: String,
    },
    Reaction {
        target: EventId,
        key: String,
    },
    /// The host may present this request or apply it under its Room policy.
    RedactionRequest {
        target: EventId,
        reason: Option<String>,
    },
    /// Materialize as a bridge-authored message with the supplied fidelity flags.
    UnavailableTarget {
        kind: EventKind,
    },
}

pub struct PreparedImport {
    author: BridgeId,
    incarnation: [u8; 16],
    epoch: u64,
    operation: ImportedAction,
    provenance: Cbor,
    flags: Vec<u8>,
    trace: InteropTrace,
    record: TargetRecord,
}

impl PreparedImport {
    pub fn author(&self) -> BridgeId {
        self.author
    }

    pub fn operation(&self) -> &ImportedAction {
        &self.operation
    }

    /// The frozen W4 ConversationEvent key 14 value.
    pub fn provenance(&self) -> &Cbor {
        &self.provenance
    }

    pub fn flags(&self) -> &[u8] {
        &self.flags
    }

    pub fn trace(&self) -> &InteropTrace {
        &self.trace
    }
}

/// Exact retry data. Persist this before dispatch and keep the same Matrix device and URL on retry.
#[derive(Clone, Deserialize, Serialize)]
pub struct PreparedEmission {
    bridge: BridgeId,
    incarnation: [u8; 16],
    matrix_room: String,
    event_id: EventId,
    epoch: u64,
    class: u8,
    relay: bool,
    native_edit: bool,
    editable: bool,
    txn_id: String,
    request: Request,
    flags: Vec<u8>,
}

impl PreparedEmission {
    pub fn transaction_id(&self) -> &str {
        &self.txn_id
    }

    pub fn flags(&self) -> &[u8] {
        &self.flags
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ConfirmationError {
    WrongBinding,
    Api(ApiError),
    MappingConflict,
}

/// One entry in the mapping-scoped target/echo table. Persist only in the endpoint's private state.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct TargetRecord {
    event_id: EventId,
    foreign_ref: String,
    /// Absent for our exports, which are the entries in the echo table.
    imported_actor: Option<String>,
    editable: bool,
}

/// A single Room mapping. The host scopes persisted records by mapping incarnation.
pub struct MatrixAdapter {
    endpoint: Endpoint,
    binding: Binding,
    by_om: BTreeMap<EventId, TargetRecord>,
    by_foreign: BTreeMap<String, EventId>,
}

/// A Core-checked import. The host commits this with the materialized Room event.
pub struct CheckedImport {
    binding: Binding,
    record: TargetRecord,
    verdict: Verdict,
}

impl MatrixAdapter {
    pub fn new(endpoint: Endpoint, binding: Binding) -> Result<Self, ApiError> {
        if !valid_ref(&binding.matrix_room, '!') || !(16..=32).contains(&binding.endpoint_id.len())
        {
            return Err(ApiError::InvalidArgument);
        }
        Ok(Self {
            endpoint,
            binding,
            by_om: BTreeMap::new(),
            by_foreign: BTreeMap::new(),
        })
    }

    pub fn records(&self) -> impl Iterator<Item = &TargetRecord> {
        self.by_om.values()
    }

    /// Restore before reading either timeline. Duplicate or cross-bound entries fail closed.
    pub fn restore_records(
        &mut self,
        binding: &Binding,
        records: Vec<TargetRecord>,
    ) -> Result<(), ConfirmationError> {
        if binding != &self.binding || !self.by_om.is_empty() {
            return Err(ConfirmationError::WrongBinding);
        }
        let mut restored =
            Self::new(self.endpoint.clone(), binding.clone()).map_err(ConfirmationError::Api)?;
        for record in records {
            if !valid_ref(&record.foreign_ref, '$')
                || !record
                    .imported_actor
                    .as_deref()
                    .is_none_or(|id| valid_ref(id, '@'))
                || restored.by_om.contains_key(&record.event_id)
                || !restored.record(record)
            {
                return Err(ConfirmationError::MappingConflict);
            }
        }
        self.by_om = restored.by_om;
        self.by_foreign = restored.by_foreign;
        Ok(())
    }

    fn record(&mut self, record: TargetRecord) -> bool {
        if let Some(old) = self.by_om.get(&record.event_id) {
            return old == &record;
        }
        if self.by_foreign.contains_key(&record.foreign_ref) {
            return false;
        }
        self.by_foreign
            .insert(record.foreign_ref.clone(), record.event_id);
        self.by_om.insert(record.event_id, record);
        true
    }

    fn foreign(&self, reference: &str) -> Option<&TargetRecord> {
        self.by_foreign
            .get(reference)
            .and_then(|id| self.by_om.get(id))
    }

    fn active(&self, access: &RoomAccess<'_>) -> bool {
        let b = &self.binding;
        let m = access.mapping;
        access.matrix_plaintext_allowed
            && m.active
            && m.bridge == b.bridge
            && m.room == b.room
            && m.namespace == NAMESPACE
            && m.foreign_scope == b.matrix_room
            && access.epoch >= b.admitted_epoch
            && access.room.roster(&b.bridge).is_some_and(|r| {
                r.incarnation == b.incarnation && r.endpoints.contains(&b.endpoint_id)
            })
            && !access
                .room
                .members()
                .must_stop_sending(&b.bridge, &b.endpoint_id)
    }

    fn may_dispatch(
        &self,
        access: &RoomAccess<'_>,
        epoch: u64,
        class: u8,
        relay: bool,
    ) -> Result<(), ExportDrop> {
        if !self.active(access) || !matches!(access.mapping.direction, BOTH | TO_FOREIGN) {
            return Err(ExportDrop::Inactive);
        }
        if epoch < self.binding.admitted_epoch || epoch > access.epoch {
            return Err(ExportDrop::OutsideMembership);
        }
        if relay && !(access.relay_bridged && access.path_preserves_metadata) {
            return Err(ExportDrop::RelayDenied);
        }
        access
            .room
            .may_export(
                &self.binding.bridge,
                &self.binding.endpoint_id,
                class,
                access.now,
            )
            .map_err(ExportDrop::Blocked)
    }

    /// BAC-OUT-1 through BAC-OUT-4. Preparing output performs no I/O and does not update the map.
    pub fn emit(
        &self,
        source: &ForeignRendering,
        access: &RoomAccess<'_>,
        room_secret: &[u8; 32],
    ) -> Result<PreparedEmission, ExportDrop> {
        let b = &self.binding;
        let class = source.rendering.class();
        let relay = matches!(source.author, RenderingAuthor::Bridged { .. });
        // Own imports are refused independently of the opt-in policy.
        if matches!(&source.author, RenderingAuthor::Bridged { bridge, .. } if *bridge == b.bridge)
            || self
                .by_om
                .get(&source.event_id)
                .is_some_and(|r| r.imported_actor.is_some())
        {
            return Err(ExportDrop::OwnImport);
        }
        self.may_dispatch(access, source.epoch, class, relay)?;
        if self.by_om.contains_key(&source.event_id) {
            return Err(ExportDrop::AlreadyExported);
        }
        let mut trace = match &source.author {
            RenderingAuthor::Native => fresh_trace(
                "om",
                origin_fingerprint(room_secret, "om", &cbor::hex(&source.event_id)),
            ),
            RenderingAuthor::Bridged {
                bridge,
                trace,
                origin_verified,
            } => {
                if !origin_verified
                    || trace.traversed_bridge_ids.last() != Some(bridge)
                    || trace.validate().is_err()
                {
                    return Err(ExportDrop::RelayDenied);
                }
                trace.clone()
            }
        };
        if trace.traversed_bridge_ids.contains(&b.bridge) {
            return Err(ExportDrop::Reflection);
        }
        if trace.hop_count >= trace.hop_limit {
            return Err(ExportDrop::HopLimit);
        }
        trace.traversed_bridge_ids.push(b.bridge);
        trace.hop_count += 1;
        let MatrixRendering {
            route,
            mut content,
            flags,
            editable,
        } = self.render(&source.rendering, access.edits_allowed)?;
        let native_edit = content
            .get("m.relates_to")
            .and_then(|relation| relation.get("rel_type"))
            .and_then(Value::as_str)
            == Some("m.replace");
        let meta = InteropMetadata {
            trace,
            fidelity_flags: flags.clone(),
        };
        content.insert(
            INTEROP_FIELD.into(),
            serde_json::to_value(meta).map_err(|_| ExportDrop::InvalidRendering)?,
        );
        let body = serde_json::to_vec(&content).map_err(|_| ExportDrop::InvalidRendering)?;
        if body.len() > MAX_EVENT_BYTES {
            return Err(ExportDrop::InvalidRendering);
        }
        let identity = cbor::encode(&Cbor::Array(vec![
            Cbor::Bytes(b.bridge.to_vec()),
            Cbor::Bytes(b.incarnation.to_vec()),
            Cbor::Text(b.matrix_room.clone()),
            Cbor::Bytes(source.event_id.to_vec()),
        ]));
        let txn_id = cbor::hex(&ds_hash("om-bridge-matrix-txn-v1", &identity));
        let request = match route {
            Route::Send(event_type) => {
                self.endpoint
                    .send_request(&b.matrix_room, event_type, &txn_id, body)
            }
            Route::Redact(event_id) => {
                self.endpoint
                    .redact_request(&b.matrix_room, event_id, &txn_id, body)
            }
        };
        Ok(PreparedEmission {
            bridge: b.bridge,
            incarnation: b.incarnation,
            matrix_room: b.matrix_room.clone(),
            event_id: source.event_id,
            epoch: source.epoch,
            class,
            relay,
            native_edit,
            txn_id,
            request,
            flags,
            editable,
        })
    }

    fn same_binding(&self, prepared: &PreparedEmission) -> bool {
        prepared.bridge == self.binding.bridge
            && prepared.incarnation == self.binding.incarnation
            && prepared.matrix_room == self.binding.matrix_room
    }

    /// Recheck immediately before every attempt, including retries of persisted requests.
    pub fn request<'a>(
        &self,
        prepared: &'a PreparedEmission,
        access: &RoomAccess<'_>,
    ) -> Result<&'a Request, ExportDrop> {
        if !self.same_binding(prepared) || !self.endpoint.owns_request(&prepared.request) {
            return Err(ExportDrop::MappingConflict);
        }
        self.may_dispatch(access, prepared.epoch, prepared.class, prepared.relay)?;
        if prepared.native_edit && !access.edits_allowed {
            return Err(ExportDrop::EditPolicyChanged);
        }
        if self.by_om.contains_key(&prepared.event_id) {
            return Err(ExportDrop::AlreadyExported);
        }
        Ok(&prepared.request)
    }

    /// BAC-OUT-5. Acknowledgements of in-flight requests still populate the echo table.
    pub fn confirm_emit(
        &mut self,
        prepared: &PreparedEmission,
        status: u16,
        bytes: &[u8],
    ) -> Result<String, ConfirmationError> {
        if !self.same_binding(prepared) {
            return Err(ConfirmationError::WrongBinding);
        }
        let foreign_ref = parse_event_response(status, bytes).map_err(ConfirmationError::Api)?;
        if !self.record(TargetRecord {
            event_id: prepared.event_id,
            foreign_ref: foreign_ref.clone(),
            imported_actor: None,
            editable: prepared.editable,
        }) {
            return Err(ConfirmationError::MappingConflict);
        }
        Ok(foreign_ref)
    }

    /// BAC-IN-0 through BAC-IN-6. An error here is a diagnostic drop. For fidelity losses
    /// that affect the timeline, the returned action carries a materializable notice.
    pub fn prepare_import(
        &self,
        event: &ForeignEvent,
        event_id: EventId,
        access: &RoomAccess<'_>,
        provenance: &RoomProvenance,
        room_secret: &[u8; 32],
        identity: Option<(&LinkTable, &[u8; 32])>,
    ) -> Result<PreparedImport, ImportDrop> {
        if !self.active(access) || !matches!(access.mapping.direction, BOTH | TO_OM) {
            return Err(ImportDrop::Inactive);
        }
        let b = &self.binding;
        if event.namespace != NAMESPACE || event.conversation_ref != b.matrix_room {
            return Err(ImportDrop::WrongScope);
        }
        // BAC-IN-2: test the ID regardless of whether the server supplies metadata.
        if let Some(old) = self.foreign(&event.event_ref) {
            return Err(if old.imported_actor.is_none() {
                ImportDrop::Echo
            } else {
                ImportDrop::Replay
            });
        }
        if self.by_om.contains_key(&event_id) {
            return Err(ImportDrop::MappingConflict);
        }
        let fingerprint = origin_fingerprint(room_secret, NAMESPACE, &event.event_ref);
        let (mut trace, origin_verified) = if let Some(meta) = &event.interop_metadata {
            meta.trace
                .validate()
                .map_err(|_| ImportDrop::InvalidTrace)?;
            if meta.trace.traversed_bridge_ids.contains(&b.bridge) {
                return Err(ImportDrop::Reflection);
            }
            let verified = meta.trace.origin_namespace == NAMESPACE
                && meta.trace.origin_fingerprint == fingerprint;
            (meta.trace.clone(), verified)
        } else {
            (
                fresh_trace(NAMESPACE, fingerprint),
                access.path_preserves_metadata,
            )
        };
        // R1: never materialize a terminal trace, including a received 2/3 trace.
        if trace.hop_count + 1 >= trace.hop_limit {
            return Err(ImportDrop::HopLimit);
        }
        if origin_verified && provenance.is_materialized(&fingerprint) {
            return Err(ImportDrop::Duplicate);
        }
        // R2: keep traversal bounds, but replace any uncheckable origin with our observed leg.
        trace.origin_fingerprint = fingerprint;
        trace.origin_namespace = NAMESPACE.into();
        trace.hop_count += 1;
        trace.traversed_bridge_ids.push(b.bridge);
        let (operation, flags) = self.import_action(event, access.edits_allowed)?;
        let actor_verified = identity.is_some_and(|(links, account)| {
            matches!(
                verify_identity_link(event, links, account, access.observed_at),
                Display::Linked { .. }
            )
        });
        let flags = fidelity::with_provenance(flags, actor_verified, origin_verified);
        let mut fields = vec![
            (0, Cbor::Text(NAMESPACE.into())),
            (1, Cbor::Text(event.conversation_ref.clone())),
            (2, Cbor::Text(event.event_ref.clone())),
            (3, Cbor::Text(event.actor_ref.clone())),
            (4, Cbor::Text(event.actor_display.clone())),
        ];
        if let Some(ms) = event.source_time_ms {
            fields.push((5, Cbor::Uint(ms / 1000)));
        }
        fields.extend([
            (6, Cbor::Uint(access.observed_at)),
            (
                7,
                Cbor::Array(flags.iter().map(|&f| Cbor::Uint(u64::from(f))).collect()),
            ),
            (8, Cbor::Bytes(fingerprint.to_vec())),
            (9, trace.to_cbor()),
        ]);
        Ok(PreparedImport {
            author: b.bridge,
            incarnation: b.incarnation,
            epoch: access.epoch,
            operation,
            flags,
            trace,
            provenance: map(fields),
            record: TargetRecord {
                event_id,
                foreign_ref: event.event_ref.clone(),
                imported_actor: Some(event.actor_ref.clone()),
                editable: matches!(
                    event.kind,
                    EventKind::Message | EventKind::Reply | EventKind::Thread
                ) && matches!(event.content, Content::Text { .. }),
            },
        })
    }

    /// Validate the canonical W4 event assembled by the host. The host authenticates the endpoint
    /// and authorizes the OM action at this epoch before calling this function.
    pub fn check_import(
        &self,
        prepared: &PreparedImport,
        body: &[u8],
        access: &RoomAccess<'_>,
        chain: &BridgeChain,
        provenance: &RoomProvenance,
    ) -> Result<CheckedImport, ImportDrop> {
        if !self.active(access) || !matches!(access.mapping.direction, BOTH | TO_OM) {
            return Err(ImportDrop::Inactive);
        }
        if prepared.incarnation != self.binding.incarnation
            || prepared.epoch != access.epoch
            || (matches!(prepared.operation, ImportedAction::Edit { .. }) && !access.edits_allowed)
        {
            return Err(ImportDrop::Inactive);
        }
        if self.foreign(&prepared.record.foreign_ref).is_some()
            || self.by_om.contains_key(&prepared.record.event_id)
        {
            return Err(ImportDrop::MappingConflict);
        }
        validate_bridge_event(body, &self.binding.endpoint_id, chain)
            .map_err(ImportDrop::Authorship)?;
        let value = Registry::load_w4()
            .check("ConversationEvent", body, false)
            .map_err(|_| ImportDrop::InvalidEvent)?;
        if value.get(3) != Some(&Cbor::Bytes(prepared.record.event_id.to_vec()))
            || value.get(6) != Some(&Cbor::Bytes(self.binding.endpoint_id.clone()))
            || value.get(14) != Some(&prepared.provenance)
            || prepared.author != self.binding.bridge
        {
            return Err(ImportDrop::InvalidEvent);
        }
        let mut verdict = provenance
            .check_import(body, access.mapping)
            .map_err(ImportDrop::Core)?;
        // Core verifies a matching MAC on same-namespace provenance. R2 additionally forbids
        // using a re-originated relay leg as a dedup key even though we computed the correct MAC.
        if prepared.flags.contains(&fidelity::ORIGIN_UNVERIFIED) {
            verdict.verified = false;
        }
        Ok(CheckedImport {
            binding: self.binding.clone(),
            record: prepared.record.clone(),
            verdict,
        })
    }

    /// Check, materialization, and commit are serialized per Room by the host. Call this only
    /// after durable materialization. The application persists the target table with that state.
    pub fn commit_import(
        &mut self,
        checked: CheckedImport,
        provenance: &mut RoomProvenance,
    ) -> Result<(), ImportDrop> {
        if checked.binding != self.binding {
            return Err(ImportDrop::MappingConflict);
        }
        if checked.verdict.verified && provenance.is_materialized(&checked.verdict.fingerprint) {
            return Err(ImportDrop::Duplicate);
        }
        if !self.record(checked.record) {
            return Err(ImportDrop::MappingConflict);
        }
        provenance.commit(&checked.verdict);
        Ok(())
    }
}

fn fresh_trace(namespace: &str, fingerprint: [u8; 32]) -> InteropTrace {
    InteropTrace {
        origin_namespace: namespace.into(),
        origin_fingerprint: fingerprint,
        traversed_bridge_ids: Vec::new(),
        hop_count: 0,
        hop_limit: DEFAULT_HOP_LIMIT,
        transformation_ids: Vec::new(),
    }
}

enum Route<'a> {
    Send(&'static str),
    Redact(&'a str),
}

fn text_content(body: &str) -> Result<Map<String, Value>, ExportDrop> {
    if !body_fits(body) {
        return Err(ExportDrop::InvalidRendering);
    }
    Ok(Map::from_iter([
        ("msgtype".into(), json!("m.text")),
        ("body".into(), json!(body)),
    ]))
}

fn attachment_content(attachment: &Attachment) -> Result<Map<String, Value>, ExportDrop> {
    if !attachment.valid() {
        return Err(ExportDrop::InvalidRendering);
    }
    let mut content = Map::from_iter([
        ("msgtype".into(), json!(attachment.kind.msgtype())),
        (
            "body".into(),
            json!(attachment.caption.as_ref().unwrap_or(&attachment.name)),
        ),
        ("url".into(), json!(attachment.mxc)),
    ]);
    if attachment.caption.is_some() {
        content.insert("filename".into(), json!(attachment.name));
    }
    if let Some(bytes) = attachment.bytes {
        content.insert("info".into(), json!({ "size": bytes }));
    }
    Ok(content)
}

fn add_flags(flags: &mut Vec<u8>, more: &[u8]) {
    flags.extend_from_slice(more);
    flags.sort_unstable();
    flags.dedup();
    if flags.len() > 1 {
        flags.retain(|f| *f != fidelity::EXACT);
    }
}

fn missing_target() -> ExportDrop {
    ExportDrop::Fidelity(vec![fidelity::REPLY_TARGET_UNAVAILABLE])
}

struct MatrixRendering<'a> {
    route: Route<'a>,
    content: Map<String, Value>,
    flags: Vec<u8>,
    editable: bool,
}

impl MatrixAdapter {
    fn render(
        &self,
        operation: &Rendering,
        edits_allowed: bool,
    ) -> Result<MatrixRendering<'_>, ExportDrop> {
        let support = capabilities().support;
        let mut route = Route::Send("m.room.message");
        let mut editable = false;
        let (content, flags) = match operation {
            Rendering::Message { message, relation } => {
                let (mut content, op) = match message {
                    Message::Text { body, rich } => {
                        editable = true;
                        (text_content(body)?, Op::Text { rich: *rich })
                    }
                    Message::Attachment(a) => (
                        attachment_content(a)?,
                        Op::Attachment {
                            bytes: a.bytes.unwrap_or_default(),
                        },
                    ),
                };
                let mut flags = support.translate(op).flags;
                if let Some(relation) = relation {
                    let (id, op) = match relation {
                        Relation::Reply(id) => (
                            id,
                            Op::Reply {
                                target_mapped: true,
                            },
                        ),
                        Relation::Thread(id) => (
                            id,
                            Op::Topic {
                                compatible_target: true,
                            },
                        ),
                    };
                    let target = self.by_om.get(id).ok_or_else(missing_target)?;
                    add_flags(&mut flags, &support.translate(op).flags);
                    let reference = &target.foreign_ref;
                    let reply = json!({ "event_id": reference });
                    let matrix_relation = match relation {
                        Relation::Reply(_) => json!({ "m.in_reply_to": reply }),
                        Relation::Thread(_) => json!({
                            "rel_type": "m.thread", "event_id": reference,
                            "is_falling_back": true, "m.in_reply_to": reply,
                        }),
                    };
                    content.insert("m.relates_to".into(), matrix_relation);
                }
                (content, flags)
            }
            Rendering::Edit { target, body, rich } => {
                let target = self.by_om.get(target).ok_or_else(missing_target)?;
                let plan = support.translate(Op::Edit {
                    source_tracked: target.editable && target.imported_actor.is_none(),
                    policy_allows: edits_allowed,
                });
                let mut flags = plan.flags;
                add_flags(
                    &mut flags,
                    &support.translate(Op::Text { rich: *rich }).flags,
                );
                let content = if plan.carry == Carry::Native {
                    let mut content = text_content(&format!("* {body}"))?;
                    content.insert("m.new_content".into(), Value::Object(text_content(body)?));
                    content.insert(
                        "m.relates_to".into(),
                        json!({
                            "rel_type": "m.replace", "event_id": target.foreign_ref,
                        }),
                    );
                    content
                } else {
                    text_content(&format!("Edited version: {body}"))?
                };
                (content, flags)
            }
            Rendering::Reaction { target, key } => {
                if !(1..=64).contains(&key.len()) || key.chars().any(char::is_control) {
                    return Err(ExportDrop::InvalidRendering);
                }
                let plan = support.translate(Op::Reaction {
                    target_mapped: self.by_om.contains_key(target),
                });
                if plan.carry == Carry::NotCarried {
                    return Err(ExportDrop::Fidelity(plan.flags));
                }
                let target = self.by_om.get(target).ok_or_else(missing_target)?;
                route = Route::Send("m.reaction");
                (
                    Map::from_iter([(
                        "m.relates_to".into(),
                        json!({
                            "rel_type": "m.annotation", "event_id": target.foreign_ref, "key": key,
                        }),
                    )]),
                    plan.flags,
                )
            }
            Rendering::Redact { target, reason } => {
                let target = self.by_om.get(target).ok_or_else(missing_target)?;
                let mut content = Map::new();
                if let Some(reason) = reason {
                    if reason.is_empty() || reason.len() > 256 || !body_fits(reason) {
                        return Err(ExportDrop::InvalidRendering);
                    }
                    content.insert("reason".into(), json!(reason));
                }
                route = Route::Redact(&target.foreign_ref);
                (content, support.translate(Op::Delete).flags)
            }
            Rendering::PollSummary { body } => {
                let plan = support.translate(Op::Poll {
                    compatible_privacy: false,
                });
                (
                    text_content(&format!("Poll summary (read-only): {body}"))?,
                    plan.flags,
                )
            }
        };
        Ok(MatrixRendering {
            route,
            content,
            flags,
            editable,
        })
    }

    fn import_action(
        &self,
        event: &ForeignEvent,
        edits_allowed: bool,
    ) -> Result<(ImportedAction, Vec<u8>), ImportDrop> {
        let support = capabilities().support;
        let target = event
            .target_ref
            .as_deref()
            .and_then(|reference| self.foreign(reference));
        let target_id = target.map(|target| target.event_id);
        let unavailable = ImportedAction::UnavailableTarget { kind: event.kind };
        let (action, flags) = match (&event.kind, &event.content) {
            (EventKind::Message | EventKind::Reply | EventKind::Thread, content) => {
                let (message, op) = match content {
                    Content::Text { body, rich } if body_fits(body) => (
                        Message::Text {
                            body: body.clone(),
                            rich: false,
                        },
                        Op::Text { rich: *rich },
                    ),
                    Content::Attachment(a) if a.valid() => (
                        Message::Attachment(a.clone()),
                        Op::Attachment {
                            bytes: a.bytes.unwrap_or_default(),
                        },
                    ),
                    _ => return Err(ImportDrop::InvalidEvent),
                };
                let mut flags = support.translate(op).flags;
                let relation = match event.kind {
                    EventKind::Reply => {
                        add_flags(
                            &mut flags,
                            &support
                                .translate(Op::Reply {
                                    target_mapped: target_id.is_some(),
                                })
                                .flags,
                        );
                        target_id.map(Relation::Reply)
                    }
                    EventKind::Thread => {
                        add_flags(
                            &mut flags,
                            &support
                                .translate(Op::Topic {
                                    compatible_target: target_id.is_some(),
                                })
                                .flags,
                        );
                        if target_id.is_none() {
                            add_flags(&mut flags, &[fidelity::REPLY_TARGET_UNAVAILABLE]);
                        }
                        target_id.map(Relation::Thread)
                    }
                    _ => None,
                };
                (ImportedAction::Message { message, relation }, flags)
            }
            (EventKind::Edit, Content::Text { body, rich }) if body_fits(body) => {
                // Matching a Matrix event ID is not enough to let a different foreign actor edit it.
                let source_tracked = target.is_some_and(|r| {
                    r.editable && r.imported_actor.as_deref() == Some(event.actor_ref.as_str())
                });
                let plan = support.translate(Op::Edit {
                    source_tracked,
                    policy_allows: edits_allowed,
                });
                let mut flags = plan.flags;
                add_flags(
                    &mut flags,
                    &support.translate(Op::Text { rich: *rich }).flags,
                );
                let action = match (plan.carry, target_id) {
                    (Carry::Native, Some(target)) => ImportedAction::Edit {
                        target,
                        body: body.clone(),
                    },
                    _ => {
                        if target_id.is_none() {
                            add_flags(&mut flags, &[fidelity::REPLY_TARGET_UNAVAILABLE]);
                        }
                        let body = format!("Edited version: {body}");
                        ImportedAction::Message {
                            message: Message::Text { body, rich: false },
                            relation: None,
                        }
                    }
                };
                (action, flags)
            }
            (EventKind::Reaction, Content::Reaction { key }) => {
                let plan = support.translate(Op::Reaction {
                    target_mapped: target_id.is_some(),
                });
                (
                    target_id.map_or(unavailable, |target| ImportedAction::Reaction {
                        target,
                        key: key.clone(),
                    }),
                    plan.flags,
                )
            }
            (EventKind::Redaction, Content::Redaction { reason }) => match target_id {
                Some(target) => (
                    ImportedAction::RedactionRequest {
                        target,
                        reason: reason.clone(),
                    },
                    support.translate(Op::Delete).flags,
                ),
                None => (
                    unavailable,
                    vec![
                        fidelity::REPLY_TARGET_UNAVAILABLE,
                        fidelity::DELETE_NOT_PROPAGATED,
                    ],
                ),
            },
            _ => return Err(ImportDrop::InvalidEvent),
        };
        Ok((action, flags))
    }
}
