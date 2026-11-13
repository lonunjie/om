// SPDX-License-Identifier: MPL-2.0
//! The JSON command layer over [`om_core::Core`]. Pure safe Rust and host-independent, so it is unit-tested natively.

use om_core::Core;
use om_core::api::{HomeSummary, Intent, ProfileExecutionContext, Query};
use om_core::attach::Meta;
use om_core::backup::BackupPlan;
use om_core::crypto::rng::{self, CryptoRng, RngCore};
use om_core::direct::{DirectFrame, ErrorCode, FetchState, TRANSFER_ID_LEN};
use om_core::groups::flow::{
    DeviceRelease, GroupEvent, GroupIntent, GroupQuery, GroupReply, GroupView, GroupWire, Sender,
    Target,
};
use om_core::host::{BlobLimits, Clock, CoreError, Inbound, NetResult, Record};
use om_core::route::RouteOffer;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(
    tag = "cmd",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Command {
    Init {
        home: HomeSummary,
    },
    OpenProfile {
        profile_id: String,
        records: Vec<Record>,
    },
    CloseProfile {
        profile_id: String,
    },
    ContextFor {
        profile_id: String,
    },
    Dispatch {
        intent: Intent,
    },
    FinishCommit {
        commit_id: u64,
        ok: bool,
    },
    NetOps {
        ctx: ProfileExecutionContext,
    },
    NetResult {
        ctx: ProfileExecutionContext,
        net_id: u64,
        result: NetResult,
    },
    PollTargets {
        ctx: ProfileExecutionContext,
    },
    Inbound {
        ctx: ProfileExecutionContext,
        mailbox: String,
        items: Vec<Inbound>,
    },
    Query {
        query: Query,
    },
    PendingOperations {
        ctx: ProfileExecutionContext,
    },
    Outcome {
        ctx: ProfileExecutionContext,
        operation_id: String,
    },
    /// Settings and a fresh salt for the next backup file (ADR-0184).
    BackupPlan,
    /// Seal the profile's history into a backup file; `key` is the host's Argon2id output for `plan` (hex).
    BackupExport {
        ctx: ProfileExecutionContext,
        plan: BackupPlan,
        key: String,
    },
    /// Read a backup file's public header (what the host needs to derive the key); needs no profile.
    BackupInspect {
        file: String,
    },
    /// Restore a backup file into the profile as read-only history: all of it or none of it.
    BackupImport {
        ctx: ProfileExecutionContext,
        file: String,
        key: String,
    },
    /// Routes the provider adapter allocated for mailboxes Core has not created yet. Answers how many Core holds.
    SupplyRoutes {
        routes: Vec<RouteOffer>,
    },
    /// The Home (relay_uri) of each route offer Core still holds.
    OfferHomes,
    /// What the BlobStore advertises (`om.blob.spool.v1` limits); `null` when it offers none. Without it attachments are
    /// refused (`unsupported`), never sent inline.
    SetBlobLimits {
        limits: Option<BlobLimits>,
    },
    /// Seal an attachment on this device (`plaintext` hex). Answers `{prepared, commit}`, or `{refused}` with the typed
    /// reason (`too_large` with the largest size, `storage_full`, `no_blob_store`, ...) before anything is sent. The host
    /// stages `prepared.ciphertext` under `prepared.blobId` and uses it only after `commit` is durable.
    PrepareAttachment {
        ctx: ProfileExecutionContext,
        plaintext: String,
        meta: Meta,
    },
    /// The ciphertext of a prepared or outgoing attachment again from its plaintext (a host that lost its staged bytes).
    RestageAttachment {
        ctx: ProfileExecutionContext,
        blob_id: String,
        plaintext: String,
    },
    /// The fetch for one attachment of a message: `{call: get_blob ...}` for the host to perform and hash-check.
    AttachmentFetch {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// Verify fetched bytes against the reference's digest and length, then decrypt: `{opened}` or `{refused}`.
    OpenAttachment {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
        ciphertext: String,
    },
    /// Seal a file too big for the delivery service so it can be offered directly (`plaintext` hex). Answers
    /// `{prepared, commit}` or `{refused}`. Nothing is stored; the sealed object is held by Core for serving.
    PrepareDirectAttachment {
        ctx: ProfileExecutionContext,
        plaintext: String,
        meta: Meta,
    },
    /// After a restart: hand the original file back so a persisted offer can be served again (`{armed: true}` or `{refused}`).
    ArmDirectOffer {
        ctx: ProfileExecutionContext,
        blob_id: String,
        plaintext: String,
    },
    /// One frame from the other person's device (the sender side). Answers `{frames, commit}`.
    DirectServe {
        ctx: ProfileExecutionContext,
        frame: Value,
    },
    /// Frames the sender may send now, given how many bytes the channel still has buffered.
    DirectPump {
        ctx: ProfileExecutionContext,
        buffered: usize,
    },
    /// The first frame of a fetch of a direct attachment (the receiver side): `{frame}` or `{refused}`.
    DirectFetchStart {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// One frame from the sender; answers the frames to send back.
    DirectFetchHandle {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
        frame: Value,
    },
    /// `{state: idle|requested|receiving|complete|failed, held}` or `null` when no fetch is open.
    DirectFetchProgress {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// Decrypt a completed direct fetch: `{opened}` or `{refused}`.
    DirectFetchOpen {
        ctx: ProfileExecutionContext,
        message_id: String,
        index: usize,
    },
    /// Whether this build links a group engine (`true` only with `groups-openmls` here; test builds use a fake).
    GroupsAvailable,
    /// Ordinary groups (ADR-0177), one typed step of the person (create, invite, accept, decline, leave, remove,
    /// text, commit published or accepted). Answers `{reply: {events, out}, commit}` or `{refused: code}`. `groups_unavailable`
    /// when this build does not link a group engine. The wire in `out` is Core-internal JSON (groups plan Q2), not a
    /// frozen format; the host carries each item as the `to` says and hands the answer to `group_receive`.
    GroupDo {
        ctx: ProfileExecutionContext,
        intent: GroupIntentJson,
    },
    /// Something that arrived for groups: `from` is what the transport authenticated (`{relationship}` or `{device}`).
    GroupReceive {
        ctx: ProfileExecutionContext,
        from: GroupFromJson,
        wire: GroupWire,
    },
    /// Typed groups queries: `groups`, `group`, `invites`, `messages`.
    GroupQuery {
        ctx: ProfileExecutionContext,
        query: GroupQueryJson,
    },
    /// The person stopped using one of their Homes: relationships with another route publish a RelaySet without it.
    /// Answers how many relationships had no other route and so stay on that Home.
    RetireHome {
        ctx: ProfileExecutionContext,
        relay_uri: String,
        #[serde(default)]
        emergency: bool,
    },
}

/// The typed group intent as JSON: `kind` plus camelCase fields, byte strings as hex.
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum GroupIntentJson {
    Create {
        name: String,
        relationship_ids: Vec<String>,
    },
    Invite {
        conversation_id: String,
        relationship_id: String,
    },
    Accept {
        invite_id: String,
        #[serde(default)]
        siblings: Vec<DeviceRelease>,
    },
    Decline {
        invite_id: String,
    },
    Leave {
        conversation_id: String,
    },
    Remove {
        conversation_id: String,
        account: String,
    },
    SendText {
        conversation_id: String,
        text: String,
    },
    CommitPublished {
        intent_id: String,
    },
    CommitAccepted {
        intent_id: String,
    },
}

impl GroupIntentJson {
    fn into_intent(self) -> Result<GroupIntent, CoreError> {
        Ok(match self {
            Self::Create {
                name,
                relationship_ids,
            } => GroupIntent::Create {
                name,
                relationship_ids,
            },
            Self::Invite {
                conversation_id,
                relationship_id,
            } => GroupIntent::Invite {
                conversation_id: unhex(&conversation_id)?,
                relationship_id,
            },
            Self::Accept {
                invite_id,
                siblings,
            } => GroupIntent::Accept {
                invite_id,
                siblings,
            },
            Self::Decline { invite_id } => GroupIntent::Decline { invite_id },
            Self::Leave { conversation_id } => GroupIntent::Leave {
                conversation_id: unhex(&conversation_id)?,
            },
            Self::Remove {
                conversation_id,
                account,
            } => GroupIntent::Remove {
                conversation_id: unhex(&conversation_id)?,
                account: unhex(&account)?,
            },
            Self::SendText {
                conversation_id,
                text,
            } => GroupIntent::SendText {
                conversation_id: unhex(&conversation_id)?,
                text,
            },
            Self::CommitPublished { intent_id } => GroupIntent::CommitPublished { intent_id },
            Self::CommitAccepted { intent_id } => GroupIntent::CommitAccepted { intent_id },
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum GroupFromJson {
    Relationship(String),
    Device(String),
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum GroupQueryJson {
    Groups,
    Group { conversation_id: String },
    Invites,
    Messages { conversation_id: String },
}

fn group_event_json(e: &GroupEvent) -> Value {
    use om_core::crypto::hex;
    match e {
        GroupEvent::Created { conversation_id } => {
            json!({ "kind": "created", "conversationId": hex(conversation_id) })
        }
        GroupEvent::InviteSent { invite_id } => {
            json!({ "kind": "invite_sent", "inviteId": invite_id })
        }
        GroupEvent::InviteReceived { invite_id } => {
            json!({ "kind": "invite_received", "inviteId": invite_id })
        }
        GroupEvent::InviteAccepted { invite_id } => {
            json!({ "kind": "invite_accepted", "inviteId": invite_id })
        }
        GroupEvent::InviteDeclined { invite_id } => {
            json!({ "kind": "invite_declined", "inviteId": invite_id })
        }
        GroupEvent::CommitPrepared { intent_id } => {
            json!({ "kind": "commit_prepared", "intentId": intent_id })
        }
        GroupEvent::AdmitBlocked { invite_id, reason } => {
            json!({ "kind": "admit_blocked", "inviteId": invite_id, "reason": reason })
        }
        GroupEvent::MemberAdded {
            conversation_id,
            account,
        } => {
            json!({ "kind": "member_added", "conversationId": hex(conversation_id), "account": hex(account) })
        }
        GroupEvent::MemberRemoved {
            conversation_id,
            account,
        } => {
            json!({ "kind": "member_removed", "conversationId": hex(conversation_id), "account": hex(account) })
        }
        GroupEvent::Joined { conversation_id } => {
            json!({ "kind": "joined", "conversationId": hex(conversation_id) })
        }
        GroupEvent::NoLongerMember { conversation_id } => {
            json!({ "kind": "no_longer_member", "conversationId": hex(conversation_id) })
        }
        GroupEvent::MessageSent {
            conversation_id,
            id,
        } => json!({ "kind": "message_sent", "conversationId": hex(conversation_id), "id": id }),
        GroupEvent::MessageReceived {
            conversation_id,
            id,
            sender_account,
        } => {
            json!({ "kind": "message_received", "conversationId": hex(conversation_id), "id": id,
                "senderAccount": hex(sender_account) })
        }
        GroupEvent::Dropped(reason) => json!({ "kind": "dropped", "reason": reason }),
    }
}

fn group_reply_json(r: &GroupReply) -> Value {
    use om_core::crypto::hex;
    json!({
        "events": r.events.iter().map(group_event_json).collect::<Vec<_>>(),
        "out": r.out.iter().map(|o| json!({
            "to": match &o.to {
                Target::Relationship(id) => json!({ "relationship": id }),
                Target::Device { account, device } =>
                    json!({ "device": hex(device), "account": hex(account) }),
            },
            "wire": serde_json::to_value(&o.wire).unwrap_or(Value::Null),
        })).collect::<Vec<_>>(),
    })
}

fn group_view_json(v: &GroupView) -> Value {
    match v {
        GroupView::Groups(g) => json!({ "kind": "groups", "groups": g }),
        GroupView::Group(d) => json!({ "kind": "group", "group": d }),
        GroupView::Invites(i) => json!({ "kind": "invites", "invites": i }),
        GroupView::Messages(m) => json!({ "kind": "messages", "messages": m }),
    }
}

pub struct HostClock<F: Fn() -> u64>(pub F);
impl<F: Fn() -> u64> Clock for HostClock<F> {
    fn now_ms(&self) -> u64 {
        (self.0)()
    }
}

/// One Core instance behind the JSON API. `Bridge::new` takes the RNG and clock so the native tests can inject
/// deterministic ones; the wasm entry point passes the system RNG.
pub struct Bridge<R: RngCore + CryptoRng> {
    make_rng: Option<Box<dyn FnOnce() -> R>>,
    clock: Option<Box<dyn Fn() -> u64>>,
    core: Option<Core<R>>,
}

impl<R: RngCore + CryptoRng> Bridge<R> {
    pub fn new(make_rng: impl FnOnce() -> R + 'static, clock: impl Fn() -> u64 + 'static) -> Self {
        Bridge {
            make_rng: Some(Box::new(make_rng)),
            clock: Some(Box::new(clock)),
            core: None,
        }
    }

    /// Handle one request; always returns a JSON response, never panics on malformed input.
    pub fn handle(&mut self, request: &[u8]) -> Vec<u8> {
        let out = match serde_json::from_slice::<Command>(request) {
            Err(e) => err("invalid", &format!("malformed command: {e}")),
            Ok(cmd) => match self.run(cmd) {
                Ok(v) => json!({ "ok": v }),
                Err(e) => core_err(&e),
            },
        };
        serde_json::to_vec(&out)
            .unwrap_or_else(|_| br#"{"err":{"code":"internal","message":"encode"}}"#.to_vec())
    }

    fn core(&mut self) -> Result<&mut Core<R>, CoreError> {
        self.core
            .as_mut()
            .ok_or(CoreError::Invalid("not initialised"))
    }

    fn group_result(
        &mut self,
        f: impl FnOnce(
            &mut Core<R>,
        ) -> Result<
            (
                Result<GroupReply, om_core::groups::GroupError>,
                om_core::host::Step,
            ),
            CoreError,
        >,
    ) -> Result<Value, CoreError> {
        let (out, step) = f(self.core()?)?;
        Ok(match out {
            Ok(r) => {
                json!({ "reply": group_reply_json(&r), "commit": serde_json::to_value(&step.commit).unwrap_or(Value::Null) })
            }
            Err(e) => json!({ "refused": e.0 }),
        })
    }

    fn run(&mut self, cmd: Command) -> Result<Value, CoreError> {
        let enc = |v: &dyn erased::Ser| v.to_value();
        match cmd {
            Command::Init { home } => {
                if self.core.is_some() {
                    return Err(CoreError::Invalid("already initialised"));
                }
                let (Some(make_rng), Some(clock)) = (self.make_rng.take(), self.clock.take())
                else {
                    return Err(CoreError::Invalid("not initialisable"));
                };
                self.core = Some(Core::new(make_rng(), HostClock(clock), home));
                Ok(Value::Null)
            }
            Command::OpenProfile {
                profile_id,
                records,
            } => Ok(enc(&self.core()?.open_profile(&profile_id, &records)?)),
            Command::CloseProfile { profile_id } => {
                self.core()?.close_profile(&profile_id);
                Ok(Value::Null)
            }
            Command::ContextFor { profile_id } => {
                Ok(enc(&self.core()?.context_for(&profile_id)?))
            }
            Command::Dispatch { intent } => Ok(enc(&self.core()?.dispatch(&intent)?)),
            Command::FinishCommit { commit_id, ok } => {
                Ok(enc(&self.core()?.finish_commit(commit_id, ok)?))
            }
            Command::NetOps { ctx } => Ok(enc(&self.core()?.net_ops(&ctx)?)),
            Command::NetResult {
                ctx,
                net_id,
                result,
            } => Ok(enc(&self.core()?.net_result(&ctx, net_id, result)?)),
            Command::PollTargets { ctx } => Ok(enc(&self.core()?.poll_targets(&ctx)?)),
            Command::Inbound {
                ctx,
                mailbox,
                items,
            } => {
                let mailbox =
                    om_core::crypto::unhex(&mailbox).map_err(|_| CoreError::Invalid("mailbox"))?;
                Ok(enc(&self.core()?.inbound(&ctx, &mailbox, &items)?))
            }
            Command::Query { query } => self.core()?.query(&query),
            Command::PendingOperations { ctx } => {
                Ok(enc(&self.core()?.pending_operations(&ctx)?))
            }
            Command::Outcome { ctx, operation_id } => {
                Ok(enc(&self.core()?.outcome(&ctx, &operation_id)?))
            }
            Command::BackupPlan => Ok(enc(&self.core()?.backup_plan())),
            Command::BackupExport { ctx, plan, key } => {
                let key = unhex(&key)?;
                Ok(json!(om_core::crypto::hex(
                    &self.core()?.export_history(&ctx, &plan, &key)?
                )))
            }
            Command::BackupInspect { file } => Ok(enc(&Core::<R>::inspect_backup(&unhex(&file)?)?)),
            Command::BackupImport { ctx, file, key } => {
                let (file, key) = (unhex(&file)?, unhex(&key)?);
                let (summary, commit) = self.core()?.import_history(&ctx, &file, &key)?;
                Ok(json!({ "summary": enc(&summary), "commit": enc(&commit) }))
            }
            Command::OfferHomes => Ok(json!(self.core()?.offer_homes())),
            Command::SupplyRoutes { routes } => Ok(json!(self.core()?.supply_routes(routes))),
            Command::SetBlobLimits { limits } => {
                self.core()?.set_blob_limits(limits);
                Ok(Value::Null)
            }
            Command::PrepareAttachment {
                ctx,
                plaintext,
                meta,
            } => {
                let (out, step) =
                    self.core()?
                        .prepare_attachment(&ctx, &unhex(&plaintext)?, meta)?;
                Ok(match out {
                    Ok(prepared) => {
                        json!({ "prepared": enc(&prepared), "commit": enc(&step.commit) })
                    }
                    // A refusal can still carry a commit (housekeeping the same transition did); the host finishes it.
                    Err(e) => json!({ "refused": enc(&e), "commit": enc(&step.commit) }),
                })
            }
            Command::RestageAttachment {
                ctx,
                blob_id,
                plaintext,
            } => Ok(
                match self
                    .core()?
                    .restage_attachment(&ctx, &blob_id, &unhex(&plaintext)?)
                {
                    Ok(ct) => json!({ "ciphertext": om_core::crypto::hex(&ct) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::AttachmentFetch {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.attachment_fetch(&ctx, &message_id, index) {
                    Ok(call) => json!({ "call": enc(&call) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::OpenAttachment {
                ctx,
                message_id,
                index,
                ciphertext,
            } => Ok(
                match self
                    .core()?
                    .open_attachment(&ctx, &message_id, index, &unhex(&ciphertext)?)
                {
                    Ok(o) => json!({ "opened": enc(&o) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::PrepareDirectAttachment {
                ctx,
                plaintext,
                meta,
            } => {
                let (out, step) =
                    self.core()?
                        .prepare_direct_attachment(&ctx, &unhex(&plaintext)?, meta)?;
                Ok(match out {
                    Ok(prepared) => {
                        json!({ "prepared": enc(&prepared), "commit": enc(&step.commit) })
                    }
                    Err(e) => json!({ "refused": enc(&e), "commit": enc(&step.commit) }),
                })
            }
            Command::ArmDirectOffer {
                ctx,
                blob_id,
                plaintext,
            } => Ok(
                match self
                    .core()?
                    .arm_direct_offer(&ctx, &blob_id, &unhex(&plaintext)?)
                {
                    Ok(()) => json!({ "armed": true }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::DirectServe { ctx, frame } => {
                let frame = frame_from_json(&frame)?;
                let (frames, step) = self.core()?.direct_serve(&ctx, &frame)?;
                Ok(json!({
                    "frames": frames.iter().map(frame_to_json).collect::<Vec<_>>(),
                    "commit": enc(&step.commit),
                }))
            }
            Command::DirectPump { ctx, buffered } => {
                let frames = self.core()?.direct_pump(&ctx, buffered)?;
                Ok(json!(frames.iter().map(frame_to_json).collect::<Vec<_>>()))
            }
            Command::DirectFetchStart {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.direct_fetch_start(&ctx, &message_id, index) {
                    Ok(f) => json!({ "frame": frame_to_json(&f) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::DirectFetchHandle {
                ctx,
                message_id,
                index,
                frame,
            } => {
                let frame = frame_from_json(&frame)?;
                Ok(
                    match self
                        .core()?
                        .direct_fetch_handle(&ctx, &message_id, index, &frame)
                    {
                        Ok(frames) => {
                            json!({ "frames": frames.iter().map(frame_to_json).collect::<Vec<_>>() })
                        }
                        Err(e) => json!({ "refused": enc(&e) }),
                    },
                )
            }
            Command::DirectFetchProgress {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.direct_fetch_progress(&ctx, &message_id, index) {
                    None => Value::Null,
                    Some((state, held)) => {
                        let (name, error) = match state {
                            FetchState::Idle => ("idle", None),
                            FetchState::Requested => ("requested", None),
                            FetchState::Receiving => ("receiving", None),
                            FetchState::Complete => ("complete", None),
                            FetchState::Failed(e) => ("failed", Some(enc(&e))),
                        };
                        json!({ "state": name, "held": held, "error": error })
                    }
                },
            ),
            Command::DirectFetchOpen {
                ctx,
                message_id,
                index,
            } => Ok(
                match self.core()?.direct_fetch_open(&ctx, &message_id, index) {
                    Ok(o) => json!({ "opened": enc(&o) }),
                    Err(e) => json!({ "refused": enc(&e) }),
                },
            ),
            Command::GroupsAvailable => Ok(json!(Core::<R>::groups_available())),
            Command::GroupDo { ctx, intent } => {
                let intent = intent.into_intent()?;
                self.group_result(|c| c.group_do(&ctx, intent))
            }
            Command::GroupReceive { ctx, from, wire } => {
                let from = match from {
                    GroupFromJson::Relationship(id) => Sender::Relationship(id),
                    GroupFromJson::Device(d) => Sender::Device(unhex(&d)?),
                };
                self.group_result(|c| c.group_receive(&ctx, from, wire))
            }
            Command::GroupQuery { ctx, query } => {
                let q = match query {
                    GroupQueryJson::Groups => GroupQuery::Groups,
                    GroupQueryJson::Invites => GroupQuery::Invites,
                    GroupQueryJson::Group { conversation_id } => GroupQuery::Group {
                        conversation_id: unhex(&conversation_id)?,
                    },
                    GroupQueryJson::Messages { conversation_id } => GroupQuery::Messages {
                        conversation_id: unhex(&conversation_id)?,
                    },
                };
                Ok(match self.core()?.group_query(&ctx, q)? {
                    Ok(v) => group_view_json(&v),
                    Err(e) => json!({ "refused": e.0 }),
                })
            }
            Command::RetireHome {
                ctx,
                relay_uri,
                emergency,
            } => {
                let (stuck, step) = self.core()?.retire_home(&ctx, &relay_uri, emergency)?;
                Ok(json!({ "stuck": stuck, "commit": enc(&step.commit) }))
            }
        }
    }
}

/// The production constructor: system CSPRNG (host entropy on wasm) and a host clock.
pub fn system_bridge(clock: impl Fn() -> u64 + 'static) -> Bridge<rng::SystemRng> {
    Bridge::new(rng::system_rng, clock)
}

fn err(code: &str, message: &str) -> Value {
    json!({ "err": { "code": code, "message": message } })
}

fn unhex(s: &str) -> Result<Vec<u8>, CoreError> {
    om_core::crypto::unhex(s).map_err(|_| CoreError::Invalid("hex"))
}

fn hex_array<const N: usize>(v: &Value, key: &str) -> Result<[u8; N], CoreError> {
    let bytes = unhex(
        v.get(key)
            .and_then(Value::as_str)
            .ok_or(CoreError::Invalid("frame"))?,
    )?;
    bytes.try_into().map_err(|_| CoreError::Invalid("frame"))
}

/// JSON shape of a direct-transfer frame: `{kind, ...}` with bytes as hex and camelCase keys (no wire codec: the host
/// maps its channel's records to this).
fn frame_to_json(f: &DirectFrame) -> Value {
    use om_core::crypto::hex;
    match f {
        DirectFrame::FetchBlob {
            request_id,
            read_cap,
            blob_id,
            digest,
        } => json!({ "kind": "fetch_blob", "requestId": hex(request_id), "readCap": hex(read_cap),
            "blobId": hex(blob_id), "digest": hex(digest) }),
        DirectFrame::Begin {
            transfer_id,
            total_bytes,
            sha256,
        } => json!({ "kind": "begin", "transferId": hex(transfer_id), "totalBytes": total_bytes,
            "sha256": hex(sha256) }),
        DirectFrame::Chunk {
            transfer_id,
            offset,
            bytes,
        } => {
            json!({ "kind": "chunk", "transferId": hex(transfer_id), "offset": offset, "bytes": hex(bytes) })
        }
        DirectFrame::End { transfer_id } => {
            json!({ "kind": "end", "transferId": hex(transfer_id) })
        }
        DirectFrame::Ack {
            transfer_id,
            offset,
        } => json!({ "kind": "ack", "transferId": hex(transfer_id), "offset": offset }),
        DirectFrame::Error { id, code } => {
            json!({ "kind": "error", "id": hex(id), "code": *code as u16 })
        }
    }
}

fn frame_from_json(v: &Value) -> Result<DirectFrame, CoreError> {
    let bad = CoreError::Invalid("frame");
    let u64_of = |k: &str| {
        v.get(k)
            .and_then(Value::as_u64)
            .ok_or(CoreError::Invalid("frame"))
    };
    Ok(match v.get("kind").and_then(Value::as_str).ok_or(bad)? {
        "fetch_blob" => DirectFrame::FetchBlob {
            request_id: hex_array::<TRANSFER_ID_LEN>(v, "requestId")?,
            read_cap: unhex(
                v.get("readCap")
                    .and_then(Value::as_str)
                    .ok_or(CoreError::Invalid("frame"))?,
            )?,
            blob_id: hex_array(v, "blobId")?,
            digest: hex_array(v, "digest")?,
        },
        "begin" => DirectFrame::Begin {
            transfer_id: hex_array(v, "transferId")?,
            total_bytes: u64_of("totalBytes")?,
            sha256: hex_array(v, "sha256")?,
        },
        "chunk" => DirectFrame::Chunk {
            transfer_id: hex_array(v, "transferId")?,
            offset: u64_of("offset")?,
            bytes: unhex(
                v.get("bytes")
                    .and_then(Value::as_str)
                    .ok_or(CoreError::Invalid("frame"))?,
            )?,
        },
        "end" => DirectFrame::End {
            transfer_id: hex_array(v, "transferId")?,
        },
        "ack" => DirectFrame::Ack {
            transfer_id: hex_array(v, "transferId")?,
            offset: u64_of("offset")?,
        },
        "error" => DirectFrame::Error {
            id: hex_array(v, "id")?,
            code: match u64_of("code")? {
                1 => ErrorCode::Unavailable,
                3 => ErrorCode::Integrity,
                _ => ErrorCode::Protocol,
            },
        },
        _ => return Err(CoreError::Invalid("frame")),
    })
}

fn core_err(e: &CoreError) -> Value {
    if let CoreError::Backup(b) = e {
        // `backup_wrong_password`, `backup_tampered`, ...: one code per cause, so the UI can say the right thing.
        return err(&format!("backup_{}", b.code()), &e.to_string());
    }
    let code = match e {
        CoreError::ProfileAccess(_) => "profile_access",
        CoreError::OperationIdConflict => "operation_id_conflict",
        CoreError::Busy => "busy",
        CoreError::CorruptState(_) => "corrupt_state",
        CoreError::NotFound(_) => "not_found",
        CoreError::Invalid(_) => "invalid",
        CoreError::Backup(_) => "invalid",
    };
    err(code, &e.to_string())
}

mod erased {
    pub trait Ser {
        fn to_value(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
        }
    }
}
