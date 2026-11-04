// SPDX-License-Identifier: MPL-2.0
//! The Core state machine: profile namespacing, staged transactions, intents, queries.
//!
//! Sans-IO. Every mutating call stages a transaction and returns a [`Commit`] for the host to make durable;
//! the staged state replaces the visible state only on `finish_commit(id, true)` ("commit before release").
//! One commit may be outstanding at a time; the host serialises calls (the TypeScript driver uses a queue).

use crate::api::*;
use crate::backup::{self, BackupPlan};
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::flows::Tx;
use crate::host::*;
use crate::route::{MAX_OFFERS, RouteOffer};
use crate::state::*;
use std::collections::BTreeMap;
use zeroize::Zeroize;

struct Slot {
    state: ProfileState,
    /// Exactly what the host has durably stored for this profile.
    committed: BTreeMap<String, Vec<u8>>,
}

struct Staged {
    id: u64,
    profile: ProfileId,
    next: ProfileState,
    records: BTreeMap<String, Vec<u8>>,
    changed: Vec<OperationId>,
}

pub struct Core<R: RngCore + CryptoRng> {
    rng: R,
    clock: Box<dyn Clock>,
    home: HomeSummary,
    profiles: BTreeMap<ProfileId, Slot>,
    boot: String,
    next_generation: u64,
    next_commit: u64,
    staged: Option<Staged>,
    /// Routes the host's provider adapter allocated for mailboxes Core has not created yet. Memory only: an offer is
    /// worthless without the install secret the adapter holds, so nothing is lost by dropping them at restart.
    offers: Vec<RouteOffer>,
}

impl<R: RngCore + CryptoRng> Core<R> {
    pub fn new(mut rng: R, clock: impl Clock + 'static, home: HomeSummary) -> Self {
        // A per-boot nonce makes every generation unique across process restarts, so a context captured by a
        // previous process can never match a profile opened by this one.
        let mut b = [0u8; 6];
        rng.fill_bytes(&mut b);
        Core {
            rng,
            clock: Box::new(clock),
            home,
            profiles: BTreeMap::new(),
            boot: crate::crypto::hex(&b),
            next_generation: 1,
            next_commit: 1,
            staged: None,
            offers: vec![],
        }
    }

    /// The host hands over routes for future mailboxes (see [`crate::route`]). Invalid, duplicate and surplus offers
    /// are dropped. Returns how many offers Core holds afterwards; the host tops up whenever this runs low.
    pub fn supply_routes(&mut self, offers: Vec<RouteOffer>) -> usize {
        for o in offers {
            if self.offers.len() >= MAX_OFFERS {
                break;
            }
            if o.is_valid() && !self.offers.iter().any(|x| x.mailbox == o.mailbox) {
                self.offers.push(o);
            }
        }
        self.offers.len()
    }

    /// The person stopped using the Home `relay_uri`: every relationship with another route publishes a new RelaySet
    /// version without it and keeps polling the dropped route for the overlap (immediately closed when `emergency`).
    /// Returns how many relationships had no other route and so stay on that Home.
    pub fn retire_home(
        &mut self,
        ctx: &ProfileExecutionContext,
        relay_uri: &str,
        emergency: bool,
    ) -> Result<(usize, Step), CoreError> {
        self.slot(ctx)?;
        let (stuck, commit) = self.txn(&ctx.profile_id, |tx| {
            Ok(tx.retire_home(relay_uri, emergency))
        })?;
        Ok((
            stuck,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// How many unused route offers Core holds.
    pub fn route_offers(&self) -> usize {
        self.offers.len()
    }

    /// The Home each unused route offer is at, so the adapter can top the pool up where it is thinnest.
    pub fn offer_homes(&self) -> Vec<String> {
        self.offers.iter().map(|o| o.relay_uri.clone()).collect()
    }

    // ------------------------------------------------------------------------------------------ profiles

    /// Open (or create, with no records) a profile from the records its encrypted store returned. Each open gets a
    /// fresh generation, so a context captured before a close/reopen is rejected as stale.
    pub fn open_profile(
        &mut self,
        profile_id: &str,
        records: &[Record],
    ) -> Result<ProfileExecutionContext, CoreError> {
        if profile_id.is_empty()
            || profile_id.len() > 128
            || profile_id.chars().any(char::is_control)
        {
            return Err(CoreError::Invalid("profile id"));
        }
        if self.staged.is_some() {
            return Err(CoreError::Busy);
        }
        let generation = format!("gen-{}-{}", self.boot, self.next_generation);
        let state = ProfileState::decode(generation.clone(), records)?;
        self.next_generation += 1;
        let committed = records
            .iter()
            .map(|r| (r.key.clone(), r.value.clone()))
            .collect();
        self.profiles
            .insert(profile_id.to_string(), Slot { state, committed });
        Ok(ProfileExecutionContext {
            profile_id: profile_id.to_string(),
            generation,
        })
    }

    /// Drop a profile's in-memory state (the host locks its vault). Pending outbox entries survive in the store.
    pub fn close_profile(&mut self, profile_id: &str) {
        if self
            .staged
            .as_ref()
            .is_some_and(|s| s.profile == profile_id)
        {
            self.staged = None;
        }
        self.profiles.remove(profile_id);
    }

    pub fn context_for(&self, profile_id: &str) -> Result<ProfileExecutionContext, CoreError> {
        let slot = self
            .profiles
            .get(profile_id)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        Ok(ProfileExecutionContext {
            profile_id: profile_id.to_string(),
            generation: slot.state.generation.clone(),
        })
    }

    fn slot(&self, ctx: &ProfileExecutionContext) -> Result<&Slot, CoreError> {
        let slot = self
            .profiles
            .get(&ctx.profile_id)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        if slot.state.generation != ctx.generation {
            return Err(CoreError::ProfileAccess("stale profile context"));
        }
        Ok(slot)
    }

    // ------------------------------------------------------------------------------------------ transactions

    fn txn<T>(
        &mut self,
        profile: &str,
        f: impl FnOnce(&mut Tx<'_, R>) -> Result<T, CoreError>,
    ) -> Result<(T, Option<Commit>), CoreError> {
        if self.staged.is_some() {
            return Err(CoreError::Busy);
        }
        let open: Vec<String> = self.profiles.keys().cloned().collect();
        let slot = self
            .profiles
            .get(profile)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        let mut next = slot.state.clone();
        let (value, changed) = {
            let mut tx = Tx {
                st: &mut next,
                rng: &mut self.rng,
                now_ms: self.clock.now_ms(),
                changed: vec![],
                open_profiles: &open,
                offers: &mut self.offers,
            };
            let v = f(&mut tx)?;
            tx.maintain_relays();
            (v, tx.changed)
        };
        let records = next.encode()?;
        let writes = diff(&slot.committed, &records);
        if writes.is_empty() {
            // Nothing durable changed (for example an idempotent retry): visible immediately, nothing to release.
            let slot = self
                .profiles
                .get_mut(profile)
                .ok_or(CoreError::ProfileAccess("unknown profile"))?;
            slot.state = next;
            return Ok((value, None));
        }
        let id = self.next_commit;
        self.next_commit += 1;
        self.staged = Some(Staged {
            id,
            profile: profile.to_string(),
            next,
            records,
            changed,
        });
        Ok((
            value,
            Some(Commit {
                id,
                profile_id: profile.to_string(),
                writes,
            }),
        ))
    }

    /// The host reports whether it made `commit_id` durable. On failure the staged change is discarded, so nothing
    /// derived from it (plaintext, outcomes, network effects) was ever released.
    pub fn finish_commit(&mut self, commit_id: u64, ok: bool) -> Result<Released, CoreError> {
        match self.staged.take() {
            Some(s) if s.id == commit_id => {
                if !ok {
                    return Ok(Released {
                        profile_id: s.profile,
                        changed_operations: vec![],
                    });
                }
                let slot = self
                    .profiles
                    .get_mut(&s.profile)
                    .ok_or(CoreError::ProfileAccess("profile closed during commit"))?;
                slot.state = s.next;
                slot.committed = s.records;
                Ok(Released {
                    profile_id: s.profile,
                    changed_operations: s.changed,
                })
            }
            other => {
                self.staged = other;
                Err(CoreError::Invalid("unknown commit id"))
            }
        }
    }

    // ------------------------------------------------------------------------------------------ intents

    /// Dispatch resolves to a handle, never to success: success is whatever outcome Core later reports.
    pub fn dispatch(&mut self, intent: &Intent) -> Result<Step, CoreError> {
        let slot = self.slot(&intent.ctx)?;
        if intent.operation_id.is_empty() || intent.operation_id.len() > 128 {
            return Err(CoreError::Invalid("operation id"));
        }
        let handle = OperationHandle {
            operation_id: intent.operation_id.clone(),
            profile_id: intent.ctx.profile_id.clone(),
        };
        if let Some(existing) = slot.state.app.ops.get(&intent.operation_id) {
            if existing.kind != intent.body.kind() {
                return Err(CoreError::OperationIdConflict);
            }
            return Ok(Step {
                commit: None,
                handle: Some(handle),
            }); // exact retry: same logical operation
        }
        let (_, commit) = self.txn(&intent.ctx.profile_id, |tx| tx.apply_intent(intent))?;
        Ok(Step {
            commit,
            handle: Some(handle),
        })
    }

    /// Network operations still owed, from the persisted outbox (re-listed after a restart; retry with the same identity).
    pub fn net_ops(&self, ctx: &ProfileExecutionContext) -> Result<Vec<NetOp>, CoreError> {
        Ok(self
            .slot(ctx)?
            .state
            .app
            .outbox
            .iter()
            .map(|e| NetOp {
                id: e.id,
                call: e.call.clone(),
            })
            .collect())
    }

    pub fn net_result(
        &mut self,
        ctx: &ProfileExecutionContext,
        net_id: u64,
        result: NetResult,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| tx.net_result(net_id, result))?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Mailboxes to poll, with the highest relay sequence already processed and committed.
    pub fn poll_targets(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Vec<PollTarget>, CoreError> {
        let st = &self.slot(ctx)?.state;
        let now = self.clock.now_ms();
        let mut boxes: Vec<(&String, MailboxKind, Option<&String>)> = st
            .app
            .invites
            .values()
            .filter(|i| !i.revoked && !i.used && i.expires_at > now)
            .map(|i| {
                // An invite's requests wait at its introduction service; one made before they existed, in a mailbox.
                let kind = if i.intro_service.is_some() {
                    MailboxKind::Intro
                } else {
                    MailboxKind::Relay
                };
                (&i.mailbox, kind, None)
            })
            .collect();
        // Every route of every relationship that has not been closed: the published ones and those still draining.
        boxes.extend(
            st.app
                .relationships
                .values()
                .flat_map(|r| r.my_boxes.iter())
                .filter(|b| !b.closed)
                .map(|b| {
                    let uri = (!b.route.is_legacy()).then_some(&b.route.relay_uri);
                    (&b.mailbox, MailboxKind::Relay, uri)
                }),
        );
        Ok(boxes
            .into_iter()
            .filter_map(|(m, kind, uri)| {
                Some(PollTarget {
                    mailbox: crate::crypto::unhex(m).ok()?,
                    after: st.app.cursors.get(m).copied().unwrap_or(0),
                    kind,
                    relay_uri: uri.cloned(),
                })
            })
            .collect())
    }

    /// Relay items for one polled mailbox. Replays (sequence at or below the committed cursor) are ignored.
    pub fn inbound(
        &mut self,
        ctx: &ProfileExecutionContext,
        mailbox: &[u8],
        items: &[Inbound],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let mailbox = crate::crypto::hex(mailbox);
        let items: Vec<(u64, Vec<u8>)> = items.iter().map(|i| (i.seq, i.bytes.clone())).collect();
        let (_, commit) = self.txn(&ctx.profile_id, |tx| tx.inbound(&mailbox, items))?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    pub fn pending_operations(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Vec<PendingOperationView>, CoreError> {
        Ok(self
            .slot(ctx)?
            .state
            .app
            .ops
            .values()
            .map(|r| PendingOperationView {
                operation_id: r.operation_id.clone(),
                intent_kind: r.kind.clone(),
                outcome: r.outcome.clone(),
            })
            .collect())
    }

    pub fn outcome(
        &self,
        ctx: &ProfileExecutionContext,
        op: &str,
    ) -> Result<Option<OutcomeClass>, CoreError> {
        Ok(self
            .slot(ctx)?
            .state
            .app
            .ops
            .get(op)
            .map(|r| r.outcome.clone()))
    }

    /// TEST ONLY: the profile's app state as JSON, so scenario tests can assert on replay records and public keys.
    /// Compiled only with `test-loopback`, which cannot be combined with `production`.
    #[cfg(feature = "test-loopback")]
    pub fn debug_app_state(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<serde_json::Value, CoreError> {
        serde_json::to_value(&self.slot(ctx)?.state.app).map_err(|_| CoreError::Invalid("state"))
    }

    /// TEST ONLY: SHA-256 (hex) of the profile's device/session state (ratchets, prekeys, ledger). Lets a test prove
    /// that a rejected payload left the cryptographic state untouched. Compiled only with `test-loopback`.
    #[cfg(feature = "test-loopback")]
    pub fn debug_device_digest(&self, ctx: &ProfileExecutionContext) -> Result<String, CoreError> {
        let dev = self
            .slot(ctx)?
            .state
            .device
            .as_ref()
            .ok_or(CoreError::Invalid("no device"))?;
        let bytes = crate::crypto::cbor::encode(&dev.to_cbor());
        Ok(crate::crypto::hex(&crate::crypto::sha256(&[&bytes])))
    }

    /// TEST ONLY: see `Tx::debug_link_messaging_device`.
    #[cfg(feature = "test-loopback")]
    pub fn debug_link_messaging_device(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(String, Step), CoreError> {
        self.slot(ctx)?;
        let (id, commit) = self.txn(&ctx.profile_id, |tx| tx.debug_link_messaging_device())?;
        Ok((
            id,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// TEST ONLY: link a credential the test built (see `Tx::debug_link_credential`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_link_credential(
        &mut self,
        ctx: &ProfileExecutionContext,
        credential_body: &[u8],
    ) -> Result<(String, Step), CoreError> {
        self.slot(ctx)?;
        let (id, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_link_credential(credential_body)
        })?;
        Ok((
            id,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// TEST ONLY: adopt the introduction of one of our own other devices (see `Tx::debug_adopt_sibling_intro`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_adopt_sibling_intro(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &str,
        intro: &[u8],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_adopt_sibling_intro(conversation_id, intro)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// TEST ONLY: deposit an arbitrary authenticated inner payload (see `Tx::debug_inject`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_inject_payload(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &str,
        payload: &[u8],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_inject(conversation_id, payload)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    // ------------------------------------------------------------------------------------------ backup file (ADR-0184)

    /// The password-hashing settings and a fresh random salt for the next backup file. The host derives the key from
    /// the person's password with exactly these and hands it to [`Core::export_history`].
    pub fn backup_plan(&mut self) -> BackupPlan {
        let mut salt = [0u8; backup::SALT_LEN];
        self.rng.fill_bytes(&mut salt);
        BackupPlan {
            kdf: backup::default_kdf(),
            salt: salt.to_vec(),
        }
    }

    /// Seal the profile's history (never any key or authority; see `backup::collect`) into one backup file. Reads
    /// committed state only, so it never conflicts with an outstanding commit and changes nothing.
    pub fn export_history(
        &mut self,
        ctx: &ProfileExecutionContext,
        plan: &BackupPlan,
        key: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        let mut master: [u8; 32] = key
            .try_into()
            .map_err(|_| CoreError::Invalid("backup key"))?;
        let salt: [u8; backup::SALT_LEN] = plan
            .salt
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Invalid("backup salt"))?;
        let mut nonce = [0u8; backup::NONCE_LEN];
        self.rng.fill_bytes(&mut nonce);
        let out = (|| {
            let archive = backup::collect(&self.slot(ctx)?.state.app, self.clock.now_ms())?;
            let mut body = backup::encode_body(&archive);
            let file = backup::seal(&body, plan.kdf, &salt, &nonce, &master);
            body.zeroize();
            Ok(file?)
        })();
        master.zeroize();
        out.map_err(|e: BackupOrCore| e.0)
    }

    /// What the host needs to derive the key for a backup file. Needs no profile: any file can be looked at first.
    pub fn inspect_backup(file: &[u8]) -> Result<backup::BackupHeader, CoreError> {
        backup::inspect(file).map_err(CoreError::Backup)
    }

    /// Restore a backup file into the profile as read-only history. All or nothing: the whole file is authenticated and
    /// validated before anything is staged, and the staged result is one commit.
    pub fn import_history(
        &mut self,
        ctx: &ProfileExecutionContext,
        file: &[u8],
        key: &[u8],
    ) -> Result<(backup::ImportSummary, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        let mut master: [u8; 32] = key
            .try_into()
            .map_err(|_| CoreError::Invalid("backup key"))?;
        let opened = backup::open(file, &master);
        master.zeroize();
        let mut body = opened.map_err(CoreError::Backup)?;
        let archive = backup::decode_body(&body);
        body.zeroize();
        let archive = archive.map_err(CoreError::Backup)?;
        let now = self.clock.now_ms();
        self.txn(&ctx.profile_id, |tx| {
            backup::apply_import(&mut tx.st.app, archive, now).map_err(CoreError::Backup)
        })
    }

    // ------------------------------------------------------------------------------------------ queries

    /// Typed queries over committed state only. The result is the JSON of the matching platform-API view.
    pub fn query(&self, q: &Query) -> Result<serde_json::Value, CoreError> {
        let to = |v: &dyn erased::Ser| v.to_value();
        match q {
            Query::GetProfileList => {
                let list: Vec<ProfileSummary> = self
                    .profiles
                    .iter()
                    .map(|(id, s)| ProfileSummary {
                        id: id.clone(),
                        label: s.state.app.label.clone(),
                    })
                    .collect();
                Ok(to(&list))
            }
            _ => {
                let ctx = q.ctx().ok_or(CoreError::Invalid("query context"))?;
                let st = &self.slot(ctx)?.state;
                match q {
                    Query::GetConversationList { .. } => Ok(to(&conversation_list(st))),
                    Query::GetConversationView {
                        conversation_id, ..
                    } => {
                        if !st.app.relationships.contains_key(conversation_id)
                            && let Some(r) = st.app.restored.get(conversation_id)
                        {
                            return Ok(to(&ConversationView {
                                summary: restored_summary(r),
                                timeline: r.messages.iter().map(restored_item).collect(),
                            }));
                        }
                        let rel = st
                            .app
                            .relationships
                            .get(conversation_id)
                            .ok_or(CoreError::NotFound("conversation"))?;
                        Ok(to(&ConversationView {
                            summary: summary(rel),
                            timeline: rel
                                .messages
                                .iter()
                                .filter(|m| !m.deleted)
                                .map(timeline_item)
                                .collect(),
                        }))
                    }
                    Query::GetIntroductions { .. } => {
                        let now_s = self.clock.now_ms() / 1000;
                        let list: Vec<IntroductionRequest> = st
                            .app
                            .intros
                            .values()
                            // The bootstrap key is the request's deadline: past it the request can no longer be answered.
                            .filter(|i| i.reply_key.expires_at > now_s)
                            .map(|i| IntroductionRequest {
                                id: i.request_id.clone(),
                                from: i.person_id.clone(),
                                display_name: i.display_name.clone(),
                                // A pending request has no binding by construction (ADR-0181 section 7).
                                verification: VerificationCode::Unverified,
                            })
                            .collect();
                        Ok(to(&list))
                    }
                    Query::GetConfirmation { operation_id, .. } => {
                        let now = self.clock.now_ms();
                        let v = st
                            .app
                            .confirmations
                            .iter()
                            .find(|c| &c.operation_id == operation_id && c.expires_at > now)
                            .map(|c| ConfirmationView {
                                nonce: c.nonce.clone(),
                                expires_at: c.expires_at,
                            });
                        Ok(to(&v))
                    }
                    Query::GetIntroductionLoad { .. } => {
                        let l = &st.app.first_contact;
                        let now = self.clock.now_ms().max(l.high_water_ms);
                        let throttled =
                            l.dropped > 0 && l.last_drop_ms + crate::flows::HOUR_MS > now;
                        Ok(to(&IntroductionLoadView {
                            throttled,
                            dropped: if throttled { l.dropped } else { 0 },
                        }))
                    }
                    Query::GetRecoveryKey {
                        key_operation_id, ..
                    } => {
                        let v = st
                            .app
                            .recovery_key
                            .as_ref()
                            .filter(|k| &k.key_operation_id == key_operation_id)
                            .map(|k| RecoveryKeyView {
                                key_operation_id: k.key_operation_id.clone(),
                                groups: k.groups.clone(),
                            });
                        Ok(to(&v))
                    }
                    Query::GetInvite { operation_id, .. } => {
                        let v = st
                            .app
                            .invites
                            .get(operation_id)
                            .filter(|i| !i.revoked)
                            .map(|i| InviteView {
                                operation_id: i.operation_id.clone(),
                                link: i.link.clone(),
                                expires_at: i.expires_at,
                            });
                        Ok(to(&v))
                    }
                    Query::GetHomeSummary { .. } => Ok(to(&self.home)),
                    Query::GetDevices { .. } => Ok(to(&devices_view(st))),
                    Query::GetLinkRequest { .. } | Query::GetLinkOffer { .. } => {
                        Ok(serde_json::Value::Null)
                    }
                    Query::GetRecoveryStatus { .. } => Ok(to(&recovery_status(st))),
                    Query::GetProfileList => Err(CoreError::Invalid("unreachable")),
                }
            }
        }
    }
}

/// Lets the export closure use `?` on both `CoreError` and `BackupError`.
struct BackupOrCore(CoreError);
impl From<CoreError> for BackupOrCore {
    fn from(e: CoreError) -> Self {
        BackupOrCore(e)
    }
}
impl From<backup::BackupError> for BackupOrCore {
    fn from(e: backup::BackupError) -> Self {
        BackupOrCore(CoreError::Backup(e))
    }
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

fn summary(r: &Relationship) -> ConversationSummary {
    let mut notices = vec![];
    if r.integrity_failures > 0 {
        notices.push(SecurityNotice {
            code: SecurityNoticeCode::IntegrityFailure,
            subject: Some(r.person_id.clone()),
        });
    }
    if r.state == RelState::AcceptedUnbound {
        notices.push(SecurityNotice {
            code: SecurityNoticeCode::UnverifiedContact,
            subject: Some(r.person_id.clone()),
        });
    }
    ConversationSummary {
        id: r.id.clone(),
        person_id: r.person_id.clone(),
        display_name: r.display_name.clone(),
        last_message_at: r.messages.last().map(|m| m.sent_at),
        notices,
        relationship: match r.state {
            RelState::AwaitingAcceptance => RelationshipStateCode::AwaitingAcceptance,
            RelState::AcceptedUnbound => RelationshipStateCode::AcceptedUnbound,
            RelState::Establishing => RelationshipStateCode::Establishing,
            RelState::Active => RelationshipStateCode::Active,
        },
    }
}

fn timeline_item(m: &Msg) -> TimelineItem {
    match (&m.operation_id, m.outgoing) {
        (Some(op), true) => TimelineItem::Outgoing {
            message_id: m.id.clone(),
            operation_id: op.clone(),
            text: m.text.clone(),
            sent_at: m.sent_at,
            delivery: m.delivery.clone(),
        },
        // A message our own other device sent (self-sync): ours, with no operation on this device behind it.
        (None, true) => TimelineItem::Outgoing {
            message_id: m.id.clone(),
            operation_id: format!("synced:{}", m.id),
            text: m.text.clone(),
            sent_at: m.sent_at,
            delivery: m.delivery.clone(),
        },
        _ => TimelineItem::Incoming {
            message_id: m.id.clone(),
            text: m.text.clone(),
            sent_at: m.sent_at,
        },
    }
}

fn restored_summary(r: &RestoredChat) -> ConversationSummary {
    ConversationSummary {
        id: r.id.clone(),
        person_id: format!("restored:{}", r.id),
        display_name: r.display_name.clone(),
        last_message_at: r.messages.last().map(|m| m.sent_at),
        notices: vec![],
        relationship: RelationshipStateCode::Restored,
    }
}

/// A restored message has no operation behind it. Outgoing ones keep their old state as far as it is still true: an
/// attempt that never left the old device is shown as not sent, since nothing will ever retry it.
fn restored_item(m: &RestoredMsg) -> TimelineItem {
    if m.outgoing {
        TimelineItem::Outgoing {
            message_id: m.id.clone(),
            operation_id: format!("restored:{}", m.id),
            text: m.text.clone(),
            sent_at: m.sent_at,
            delivery: match m.delivery {
                1 => DeliveryState::Sent,
                2 => DeliveryState::Delivered,
                _ => DeliveryState::Rejected {
                    reason: RejectReason::Unavailable,
                },
            },
        }
    } else {
        TimelineItem::Incoming {
            message_id: m.id.clone(),
            text: m.text.clone(),
            sent_at: m.sent_at,
        }
    }
}

fn conversation_list(st: &ProfileState) -> Vec<ConversationSummary> {
    // (last activity, summary): live conversations and restored history are listed together, newest first.
    let mut v: Vec<(u64, ConversationSummary)> = st
        .app
        .relationships
        .values()
        .map(|r| {
            (
                r.messages.last().map(|m| m.sent_at).unwrap_or(r.created_at),
                summary(r),
            )
        })
        .collect();
    v.extend(
        st.app
            .restored
            .values()
            .filter(|r| !st.app.relationships.contains_key(&r.id))
            .map(|r| {
                (
                    r.messages.last().map(|m| m.sent_at).unwrap_or(r.created_at),
                    restored_summary(r),
                )
            }),
    );
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    v.into_iter().map(|(_, s)| s).collect()
}

fn devices_view(st: &ProfileState) -> DevicesView {
    let head = (|| {
        let chain: Vec<Vec<u8>> = st
            .app
            .acs_chain
            .iter()
            .map(|c| crate::crypto::unhex(c).ok())
            .collect::<Option<_>>()?;
        let creds: Vec<Vec<u8>> = st
            .app
            .acs_credentials
            .iter()
            .map(|c| crate::crypto::unhex(c).ok())
            .collect::<Option<_>>()?;
        crate::control::verify_chain(&chain, &creds).ok()
    })();
    let (Some(head), Some(me)) = (head, st.device.as_ref().map(|d| d.device_id)) else {
        return DevicesView {
            control_state: AccountControlStateCode::Normal,
            devices: vec![],
            link_requests: vec![],
            notices: vec![],
        };
    };
    use crate::control::{Action, PlanError, plan};
    let me_active = head.control.contains(&me) && !head.quarantined.contains(&me);
    let voting = head.control.difference(&head.quarantined).count();
    let frozen = crate::control::safety_frozen(&head);
    let summary = |id: &[u8; 16], role: DeviceRole, state: DeviceStateCode| {
        let hexid = crate::crypto::hex(id);
        let meta = st.app.device_meta.get(&hexid);
        let is_current = *id == me;
        let mut actions = vec![];
        let mut cautions = vec![DeviceCautionCode::HistorySyncUnavailable];
        if is_current && role == DeviceRole::Control && voting == 1 {
            cautions.push(DeviceCautionCode::LastVotingController);
        }
        if me_active && !is_current && !frozen && state != DeviceStateCode::Revoked {
            if role == DeviceRole::Control && state == DeviceStateCode::Active {
                match plan(&head, &me, Action::Suspend, id) {
                    Ok(_) => actions.push(DeviceActionCode::Suspend),
                    Err(PlanError::WouldFreeze) => {
                        cautions.push(DeviceCautionCode::SuspendWouldFreeze)
                    }
                    Err(_) => {}
                }
            }
            if plan(&head, &me, Action::Revoke, id).is_ok() {
                actions.push(DeviceActionCode::Revoke);
            }
        }
        DeviceSummary {
            id: hexid,
            label: meta
                .map(|m| m.label.clone())
                .unwrap_or_else(|| "Linked device".into()),
            role,
            state,
            is_current,
            enrolled_at: meta.map(|m| m.enrolled_at).unwrap_or(st.app.created_at),
            last_seen: if is_current {
                LastSeenClass::Now
            } else {
                LastSeenClass::Never
            },
            actions,
            cautions,
        }
    };
    let mut devices = vec![];
    for id in &head.control {
        let state = if head.quarantined.contains(id) {
            DeviceStateCode::Quarantined
        } else {
            DeviceStateCode::Active
        };
        devices.push(summary(id, DeviceRole::Control, state));
    }
    for id in &head.messaging {
        devices.push(summary(id, DeviceRole::Messaging, DeviceStateCode::Active));
    }
    for id in &head.revoked {
        devices.push(summary(id, DeviceRole::Messaging, DeviceStateCode::Revoked));
    }
    DevicesView {
        control_state: if frozen {
            AccountControlStateCode::SafetyFreeze
        } else {
            AccountControlStateCode::Normal
        },
        devices,
        link_requests: vec![],
        notices: vec![],
    }
}

/// Pass 1 reports `pending`: a recovery key is generated and shown once, but the RecoveryPolicy commitment in the
/// AccountControlState and the recovery flow that would use it are not implemented yet, so Core must not claim `on`.
fn recovery_status(st: &ProfileState) -> RecoveryStatusView {
    RecoveryStatusView {
        policy: RecoveryPolicyState::Pending,
        custodians: if st.app.recovery_key_saved {
            vec![RecoveryCustodianView {
                code: RecoveryCustodianCode::RecoveryKey,
                distribution_bound: false,
            }]
        } else {
            vec![]
        },
        scopes: vec![RecoveryScope::Identity],
        activation_delay_ms: 72 * 60 * 60 * 1000,
        recovery_key_saved: st.app.recovery_key_saved,
        requests: vec![],
    }
}
