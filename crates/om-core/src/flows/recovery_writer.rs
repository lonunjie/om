//! The RecoveryRecord writer (ADR-0199 section 6): what a controller device keeps at the Home so a person who lost every
//! device can start again.
//!
//! The first write follows the confirmation that the recovery key is stored. `generation = (acs_generation << 20) |
//! refresh`: the device that authored an ACS state writes refresh 0 in the same transaction that stages the state; the
//! steward (the lowest live controller DeviceID) writes refresh slots from 1 for a state it did not author, after a
//! delay, and refreshes the snapshot when it changes (at most daily) and the record at least every 90 days. The record
//! is sealed once per staged write; a retry resends the same bytes and request id. A refused compare-and-swap is
//! settled by reading the record: a copy at or above ours means we are behind and ours is dropped.

use super::*;
use crate::crypto::binding::Credential;
use crate::recovery_record::{
    self as rr, AcsEvidence, AcsHeadRef, ProviderHome, RecordKeys, RecoveryKit, RecoveryRecord,
    RelationshipSnapshot, Relationships, SnapshotEntry,
};
use std::collections::BTreeSet;

/// A steward writes for a state it did not author only after this long (the author had its chance).
pub const STEWARD_DELAY_MS: u64 = 3_600_000;
/// A changed relationship snapshot is rewritten at most this often.
pub const SNAPSHOT_REFRESH_MS: u64 = 86_400_000;
/// The record is rewritten at least this often.
pub const MAX_QUIET_MS: u64 = 90 * 86_400_000;
/// A staged write older than this is shown as needing attention.
pub const ATTENTION_MS: u64 = 86_400_000;
const MAX_REFRESH: u32 = (1 << rr::REFRESH_BITS) - 1;

impl AppState {
    /// Settings status for the record: off without a place or kit, finishing setup until the first write is
    /// acknowledged, ready after, and needing attention when a write has not landed for a day.
    pub fn recovery_record_status(&self, now_ms: u64) -> RecoveryRecordStatus {
        let w = &self.recovery_writer;
        if self.recovery_kit.is_none() {
            return RecoveryRecordStatus::Off;
        }
        if w.paused_for_exclusion {
            return RecoveryRecordStatus::NeedsNewKey;
        }
        if let Some(s) = &w.staged
            && now_ms.saturating_sub(s.staged_at_ms) >= ATTENTION_MS
        {
            return RecoveryRecordStatus::NeedsAttention;
        }
        if w.acked_generation == 0 {
            RecoveryRecordStatus::FinishingSetup
        } else {
            RecoveryRecordStatus::Ready
        }
    }
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    pub(super) fn recovery_kit_value(&self) -> Option<RecoveryKit> {
        let k = self.st.app.recovery_kit.as_ref()?;
        RecoveryKit::from_bytes(&unhex(k).ok()?).ok()
    }

    /// The snapshot of the live relationships (what ADR-0190 section 3.1 needs to notify each contact).
    pub(crate) fn recovery_snapshot(&self) -> RelationshipSnapshot {
        let mut entries = vec![];
        for r in self.st.app.relationships.values() {
            if r.state != RelState::Active || r.blocked {
                continue;
            }
            let (Some(canon), Ok(acct), Ok(dev)) = (
                r.canon_id.as_ref().and_then(|c| unhex(c).ok()),
                unhex(&r.peer_account_id),
                unhex(&r.peer_device_id),
            ) else {
                continue;
            };
            let (Ok(conversation_id), Ok(peer_account_id), Ok(dev)) = (
                <[u8; 32]>::try_from(canon),
                <[u8; 32]>::try_from(acct),
                <[u8; 16]>::try_from(dev),
            ) else {
                continue;
            };
            let routes: Vec<V> = r
                .peer_relays
                .entries
                .iter()
                .filter(|e| e.is_valid())
                .take(4)
                .map(RelayEntry::to_v)
                .collect();
            let Some(km) = r.peer_delivery.as_ref().and_then(|pd| {
                Some(KeyMaterial {
                    key_id: unhex(&pd.public.key_id).ok()?.try_into().ok()?,
                    public_key: unhex(&pd.public.public_key).ok()?,
                    route_binding: unhex(&pd.public.route_binding).ok()?.try_into().ok()?,
                    expires_at: pd.public.expires_at,
                })
            }) else {
                continue;
            };
            if routes.is_empty() {
                continue;
            }
            entries.push(SnapshotEntry {
                conversation_id,
                peer_account_id,
                label: Some(r.display_name.chars().take(64).collect::<String>())
                    .filter(|l| !l.is_empty()),
                routes,
                targets: vec![(dev, km.to_v())],
                key_fetch_cap: None,
                peer_head: None,
            });
            if entries.len() == rr::MAX_RELATIONSHIPS {
                break;
            }
        }
        RelationshipSnapshot {
            entries,
            truncated: false,
        }
    }

    fn is_steward(&self, head: &Acs) -> bool {
        let Some(me) = self.st.device.as_ref().map(|d| d.device_id) else {
            return false;
        };
        head.control
            .iter()
            .filter(|d| !head.quarantined.contains(*d) && !head.revoked.contains(*d))
            .min()
            .is_some_and(|lowest| *lowest == me)
    }

    /// Seal the record for `head` at `refresh`. `None` when the evidence does not hold together (nothing is written).
    fn seal_recovery_record(
        &mut self,
        kit: &RecoveryKit,
        head: &Acs,
        chain: &[Vec<u8>],
        creds: &[Vec<u8>],
        refresh: u32,
        snapshot: RelationshipSnapshot,
    ) -> Option<Vec<u8>> {
        let generation = rr::record_generation(head.generation, refresh).ok()?;
        let n = chain.len();
        let tail: Vec<Vec<u8>> = chain[n.saturating_sub(rr::MAX_STATES)..n - 1].to_vec();
        let mut referenced = BTreeSet::new();
        for s in tail.iter().chain(chain.last()) {
            let a = Acs::from_body(&rr::signed_body(s).ok()?).ok()?;
            referenced.extend(a.credentials.iter().map(|(_, h)| *h));
        }
        let mut seen = BTreeSet::new();
        let credentials: Vec<Vec<u8>> = creds
            .iter()
            .filter(|b| {
                Credential::from_body(b)
                    .map(|c| referenced.contains(&c.hash()) && seen.insert(c.hash()))
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        let policy = unhex(self.st.app.recovery_policy.as_ref()?).ok()?;
        let epoch = crate::recovery::RecoveryPolicy::from_body(&policy)
            .ok()?
            .epoch();
        let mut record = RecoveryRecord {
            generation,
            recovery_epoch: epoch,
            head: AcsHeadRef {
                generation: head.generation,
                state_hash: head.hash(),
            },
            evidence: AcsEvidence {
                head: chain.last()?.clone(),
                tail,
                credentials,
                policy,
            },
            providers: vec![ProviderHome::new(&kit.host, kit.port, Some(kit.scope))],
            relationships: Relationships::Plain(snapshot),
            extensions: None,
        };
        record.verify().ok()?;
        // Too big for the largest bucket: drop the oldest-added contacts and say so (`truncated`).
        let padded = loop {
            match record.to_padded_bytes() {
                Ok(p) => break p,
                Err(_) => match &mut record.relationships {
                    Relationships::Plain(s) if !s.entries.is_empty() => {
                        s.entries.pop();
                        s.truncated = true;
                    }
                    _ => return None,
                },
            }
        };
        let mut nonce = [0u8; 12];
        self.rng.fill_bytes(&mut nonce);
        rr::seal_envelope(&RecordKeys::from_kit(kit), &kit.scope, nonce, &padded).ok()
    }

    fn stage_recovery_put(&mut self, kit: &RecoveryKit, generation: u64, envelope: Vec<u8>) {
        // A newer staged write replaces an older one that has not been acknowledged.
        self.st
            .app
            .outbox
            .retain(|e| !matches!(e.purpose, Purpose::RecoveryPut { .. }));
        let request_id = self.rand16().to_vec();
        let if_match = self.st.app.recovery_writer.acked_etag.clone();
        self.st.app.recovery_writer.staged = Some(StagedPut {
            generation,
            request_id: hex(&request_id),
            envelope: hex(&envelope),
            if_match: if_match.clone(),
            staged_at_ms: self.now_ms,
        });
        self.enqueue(
            NetCall::RecoveryPut {
                host: kit.host.clone(),
                port: kit.port,
                locator: kit.locator().to_vec(),
                write_cap: kit.write_cap.to_vec(),
                request_id,
                if_match,
                bytes: envelope,
            },
            Purpose::RecoveryPut { generation },
        );
    }

    pub(super) fn discard_recovery_puts(&mut self) {
        self.st.app.recovery_writer.staged = None;
        self.st.app.outbox.retain(|e| {
            !matches!(
                e.purpose,
                Purpose::RecoveryPut { .. } | Purpose::RecoveryReconcile { .. }
            )
        });
    }

    /// Called at the end of every transaction: stage a write when the schedule calls for one.
    pub(crate) fn maintain_recovery_record(&mut self) {
        if !self.st.app.recovery_writer.enabled {
            return;
        }
        let Some(kit) = self.recovery_kit_value() else {
            return;
        };
        let Some((head, chain, creds)) = self.acs_head() else {
            self.discard_recovery_puts();
            return;
        };
        // An eligible controller could have received the kit even if delivery was not acknowledged.
        // Recovery itself advances the policy epoch but derives the record key from the same root.
        // Walk back across those verified recovery links and stop at a new key's policy transition.
        let Some(states) = multi::chain_states(&chain) else {
            self.discard_recovery_puts();
            return;
        };
        let excluded = states
            .windows(2)
            .rev()
            .take_while(|pair| {
                let (parent, child) = (&pair[0], &pair[1]);
                parent.recovery_commitment == child.recovery_commitment
                    || self
                        .st
                        .app
                        .recovery_links
                        .iter()
                        .any(|link| link.child_hash == hex(&child.hash()))
            })
            .any(|pair| {
                let previous = &pair[0];
                previous
                    .control
                    .difference(&previous.quarantined)
                    .any(|id| !recovery_watch::live(&head, id))
            });
        self.st.app.recovery_writer.paused_for_exclusion = excluded;
        let current_policy = self
            .st
            .app
            .recovery_policy
            .as_deref()
            .and_then(|h| unhex(h).ok())
            .and_then(|body| crate::recovery::RecoveryPolicy::from_body(&body).ok())
            .is_some_and(|policy| {
                head.recovery_commitment == Some(policy.hash()) && policy.epoch() == kit.epoch
            });
        let live = self
            .st
            .device
            .as_ref()
            .is_some_and(|dev| recovery_watch::live(&head, &dev.device_id));
        if excluded || !current_policy || !live || !self.st.app.acs_forks.is_empty() {
            self.discard_recovery_puts();
            return;
        }
        let now = self.now_ms;
        let head_hash = hex(&head.hash());
        let snapshot = self.recovery_snapshot();
        let content = hex(&crypto::sha256(&[&snapshot.to_bytes()]));
        let steward = self.is_steward(&head);
        let w = &mut self.st.app.recovery_writer;
        let authored = std::mem::take(&mut w.authored);
        let mut slot = None;
        if w.head_hash.as_deref() != Some(head_hash.as_str()) {
            w.head_hash = Some(head_hash);
            w.head_seen_ms = now;
            w.refresh = 0;
            if authored {
                slot = Some(0);
                w.steward_pending = false;
            } else {
                w.steward_pending = true;
            }
        } else {
            let acked_acs = rr::generation_acs(w.acked_generation);
            let staged_acs = w.staged.as_ref().map(|s| rr::generation_acs(s.generation));
            let covered = acked_acs >= head.generation || staged_acs == Some(head.generation);
            let quiet = now.saturating_sub(w.last_write_ms);
            if w.steward_pending
                && steward
                && !covered
                && now.saturating_sub(w.head_seen_ms) >= STEWARD_DELAY_MS
            {
                slot = Some(w.refresh + 1);
                w.steward_pending = false;
            } else if steward
                && covered
                && w.staged.is_none()
                && ((w.last_content.as_deref() != Some(content.as_str())
                    && quiet >= SNAPSHOT_REFRESH_MS)
                    || quiet >= MAX_QUIET_MS)
            {
                slot = Some(w.refresh + 1);
            }
        }
        let Some(refresh) = slot else {
            return;
        };
        if refresh > MAX_REFRESH {
            return;
        }
        let Some(envelope) =
            self.seal_recovery_record(&kit, &head, &chain, &creds, refresh, snapshot)
        else {
            return;
        };
        let generation = rr::record_generation(head.generation, refresh).unwrap_or(0);
        let w = &mut self.st.app.recovery_writer;
        w.refresh = refresh;
        w.last_write_ms = now;
        w.last_content = Some(content);
        self.stage_recovery_put(&kit, generation, envelope);
    }

    /// The Home acknowledged the put: its `ETag` is the next compare-and-swap.
    pub(crate) fn recovery_put_done(&mut self, generation: u64, etag: Option<Vec<u8>>) {
        let w = &mut self.st.app.recovery_writer;
        if w.staged
            .as_ref()
            .is_some_and(|s| s.generation == generation)
        {
            w.staged = None;
        }
        if generation >= w.acked_generation {
            w.acked_generation = generation;
            w.acked_etag = etag.and_then(|e| String::from_utf8(e).ok());
        }
    }

    /// A refused put (a moved ETag, usually another writer's newer record): read the record to settle it.
    pub(crate) fn recovery_put_refused(&mut self, generation: u64, reason: RejectReason) {
        let Some(kit) = self.recovery_kit_value() else {
            return;
        };
        if reason == RejectReason::StaleState {
            self.enqueue(
                NetCall::RecoveryGet {
                    host: kit.host.clone(),
                    port: kit.port,
                    locator: kit.locator().to_vec(),
                    read_cap: kit.read_cap.to_vec(),
                },
                Purpose::RecoveryReconcile { generation },
            );
        }
        // any other refusal leaves the staged write for the status to surface; it is not resent blindly
    }

    /// The reconcile read came back (`items` = [etag, envelope]) or the record is gone (`None`).
    pub(crate) fn recovery_reconciled(&mut self, generation: u64, got: Option<(Vec<u8>, Vec<u8>)>) {
        let Some(kit) = self.recovery_kit_value() else {
            return;
        };
        let staged = self.st.app.recovery_writer.staged.clone();
        let Some(staged) = staged.filter(|s| s.generation == generation) else {
            return;
        };
        let (etag, remote_generation) = match &got {
            Some((etag, envelope)) => {
                let Ok(rr::Plain::Record(r)) =
                    rr::open(&RecordKeys::from_kit(&kit), &kit.scope, envelope)
                else {
                    return; // not readable as ours: leave it for the status to surface
                };
                (String::from_utf8(etag.clone()).ok(), r.generation)
            }
            None => (None, 0),
        };
        if remote_generation >= generation {
            // Another writer's record is at or above ours: ours is dropped, and its ETag is what we compare against.
            let w = &mut self.st.app.recovery_writer;
            w.staged = None;
            w.acked_generation = remote_generation;
            w.acked_etag = etag;
            return;
        }
        // Ours is newer: resend the same sealed bytes against the current ETag with a fresh request id.
        let request_id = self.rand16().to_vec();
        let Ok(envelope) = unhex(&staged.envelope) else {
            return;
        };
        self.st.app.recovery_writer.staged = Some(StagedPut {
            request_id: hex(&request_id),
            if_match: etag.clone(),
            ..staged
        });
        self.enqueue(
            NetCall::RecoveryPut {
                host: kit.host.clone(),
                port: kit.port,
                locator: kit.locator().to_vec(),
                write_cap: kit.write_cap.to_vec(),
                request_id,
                if_match: etag,
                bytes: envelope,
            },
            Purpose::RecoveryPut { generation },
        );
    }
}
