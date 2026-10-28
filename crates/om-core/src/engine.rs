// SPDX-License-Identifier: MPL-2.0
//! The Core state machine: profile namespacing, staged transactions, intents, queries.
//!
//! Sans-IO. Every mutating call stages a transaction and returns a [`Commit`] for the host to make durable;
//! the staged state replaces the visible state only on `finish_commit(id, true)` ("commit before release").
//! One commit may be outstanding at a time; the host serialises calls (the TypeScript driver uses a queue).

use crate::api::*;
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::flows::Tx;
use crate::host::*;
use crate::state::*;
use std::collections::BTreeMap;

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
        }
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
            };
            let v = f(&mut tx)?;
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
        let mut boxes: Vec<&String> = st
            .app
            .invites
            .values()
            .filter(|i| !i.revoked && !i.used && i.expires_at > now)
            .map(|i| &i.mailbox)
            .collect();
        boxes.extend(st.app.relationships.values().map(|r| &r.my_mailbox));
        Ok(boxes
            .into_iter()
            .filter_map(|m| {
                Some(PollTarget {
                    mailbox: crate::crypto::unhex(m).ok()?,
                    after: st.app.cursors.get(m).copied().unwrap_or(0),
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
                        let rel = st
                            .app
                            .relationships
                            .get(conversation_id)
                            .ok_or(CoreError::NotFound("conversation"))?;
                        Ok(to(&ConversationView {
                            summary: summary(rel),
                            timeline: rel.messages.iter().map(timeline_item).collect(),
                        }))
                    }
                    Query::GetIntroductions { .. } => {
                        let list: Vec<IntroductionRequest> = st
                            .app
                            .intros
                            .values()
                            .map(|i| IntroductionRequest {
                                id: i.request_id.clone(),
                                from: i.person_id.clone(),
                                display_name: i.display_name.clone(),
                            })
                            .collect();
                        Ok(to(&list))
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
    ConversationSummary {
        id: r.id.clone(),
        person_id: r.person_id.clone(),
        display_name: r.display_name.clone(),
        last_message_at: r.messages.last().map(|m| m.sent_at),
        notices,
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
        _ => TimelineItem::Incoming {
            message_id: m.id.clone(),
            text: m.text.clone(),
            sent_at: m.sent_at,
        },
    }
}

fn conversation_list(st: &ProfileState) -> Vec<ConversationSummary> {
    let mut v: Vec<&Relationship> = st.app.relationships.values().collect();
    v.sort_by(|a, b| {
        b.messages
            .last()
            .map(|m| m.sent_at)
            .unwrap_or(b.created_at)
            .cmp(&a.messages.last().map(|m| m.sent_at).unwrap_or(a.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    v.into_iter().map(summary).collect()
}

fn devices_view(st: &ProfileState) -> DevicesView {
    let devices = st
        .device
        .as_ref()
        .map(|d| {
            vec![DeviceSummary {
                id: crate::crypto::hex(&d.device_id),
                label: "This device".into(),
                role: DeviceRole::Control,
                state: DeviceStateCode::Active,
                is_current: true,
                enrolled_at: st.app.created_at,
                last_seen: LastSeenClass::Now,
                // The only voting controller can be neither suspended nor revoked (ACCOUNT_CONTINUITY_AND_CONTROL.md section 13).
                actions: vec![],
                cautions: vec![
                    DeviceCautionCode::LastVotingController,
                    DeviceCautionCode::HistorySyncUnavailable,
                ],
            }]
        })
        .unwrap_or_default();
    DevicesView {
        control_state: AccountControlStateCode::Normal,
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
