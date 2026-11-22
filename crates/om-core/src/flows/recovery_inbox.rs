//! The controller side of the recovery inbox (ADR-0201 section 8.2, ADR-0202 section 3): a live controller that holds the
//! RecoveryKit registers its inbox at the Home, drains it on the host's tick and acknowledges what it has handed to the
//! verifier. An item alerts a controller; nothing the inbox holds is trusted or needed for safety. Every request and veto it
//! carries goes through the same observation checks as one that arrived any other way, so a forged item fills space and
//! authorizes nothing.

use super::*;
use crate::crypto::cbor;
use crate::flows::recovery_watch as watch;
use crate::host::MonotonicSample;
use crate::state::{EarlierRecoveryInbox, InboxItemRec};

/// Pacing between inbox calls, on Core's clock (never authority).
const DRAIN_EVERY_MS: u64 = 60_000;
// ADR-0201 §2's ordinary foreground polling interval.
const EARLIER_DRAIN_EVERY_MS: u64 = 5 * 60_000;
const INBOX_RETENTION_MS: u64 = 400 * 86_400_000;
const RENEW_EVERY_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const REFUSED_RETRY_MS: u64 = 24 * 60 * 60 * 1000;
/// Items held until the next tick.
const MAX_PENDING: usize = 32;
/// Item ids acknowledged in one call.
const ACK_PAGE: usize = 16;

impl RecoveryInboxRec {
    pub(crate) fn status(&self, epoch: u64, now_ms: u64) -> RecoveryAlertsCode {
        if self.epoch != Some(epoch) {
            return RecoveryAlertsCode::Checking;
        }
        if self.registration_refused {
            return RecoveryAlertsCode::NeedsAttention;
        }
        let recent = self
            .drained_at_ms
            .and_then(|last| now_ms.checked_sub(last))
            .is_some_and(|age| age < EARLIER_DRAIN_EVERY_MS);
        if recent && self.registered_at_ms.is_some_and(|last| last <= now_ms) {
            RecoveryAlertsCode::Ready
        } else {
            RecoveryAlertsCode::Checking
        }
    }
}

fn inbox_item(raw: &[u8]) -> Option<(String, u8, Vec<u8>)> {
    let v = cbor::decode(raw).ok()?;
    let id = v.get(0).ok()?.b().ok()?;
    let kind = v.get(1).ok()?.u().ok()?;
    let bytes = v.get(2).ok()?.b().ok()?;
    (id.len() == 16 && matches!(kind, 1 | 2) && (1..=16_384).contains(&bytes.len()))
        .then(|| (hex(id), kind as u8, bytes.to_vec()))
}

/// Fuzz entry point (V-012): the drained inbox item reader. Compiled only with `fuzz-entry`, which a `production` build refuses.
#[cfg(feature = "fuzz-entry")]
pub(super) fn fuzz_inbox_item(raw: &[u8]) -> Result<(), &'static str> {
    let (id, kind, bytes) = inbox_item(raw).ok_or("item_refused")?;
    assert_eq!(id.len(), 32);
    assert!(matches!(kind, 1 | 2) && (1..=16_384).contains(&bytes.len()));
    Ok(())
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Retain just the old drain credential and verifier before the verified replacement swaps the kit and policy.
    pub(super) fn retire_recovery_inbox(&mut self) {
        let (Some(kit), Some((_, _, policy))) = (self.recovery_kit_value(), self.recovery_ctx())
        else {
            return;
        };
        let Some(key) = policy
            .recovery_key()
            .filter(|_| kit.epoch == policy.epoch())
        else {
            return;
        };
        self.st.app.earlier_recovery_inboxes.insert(
            kit.epoch,
            EarlierRecoveryInbox {
                host: kit.host.clone(),
                port: kit.port,
                cap: hex(&kit.inbox_cap).into(),
                verification_key: hex(key),
                ended_ms: self.now_ms,
                last_drain_ms: None,
            },
        );
        // Queued registration and ack work for the previous epoch must stop with the epoch.
        self.st
            .app
            .outbox
            .retain(|e| !matches!(e.purpose, Purpose::RecoveryInbox { .. }));
        self.st.app.recovery_inbox = RecoveryInboxRec::default();
    }

    fn earlier_inboxes_tick(&mut self, head: &Acs, epoch: u64) {
        let now = self.now_ms;
        self.st
            .app
            .earlier_recovery_inboxes
            .retain(|_, inbox| now.saturating_sub(inbox.ended_ms) < INBOX_RETENTION_MS);
        let inboxes = &mut self.st.app.earlier_recovery_inboxes;
        self.st.app.outbox.retain(|entry| match &entry.call {
            NetCall::RecoveryInboxDrain { recovery_epoch, .. } if *recovery_epoch < epoch => {
                inboxes.contains_key(recovery_epoch)
            }
            _ => true,
        });
        let mut calls = Vec::new();
        for (&earlier_epoch, inbox) in inboxes.iter_mut() {
            if earlier_epoch >= epoch
                || inbox.last_drain_ms.is_some_and(|last| now.saturating_sub(last) < EARLIER_DRAIN_EVERY_MS)
                || self.st.app.outbox.iter().any(|entry| matches!(
                    entry.call,
                    NetCall::RecoveryInboxDrain { recovery_epoch, .. } if recovery_epoch == earlier_epoch
                ))
            {
                continue;
            }
            let Ok(cap) = unhex(&inbox.cap) else { continue };
            inbox.last_drain_ms = Some(now);
            calls.push(NetCall::RecoveryInboxDrain {
                host: inbox.host.clone(),
                port: inbox.port,
                account_id: head.account_id.to_vec(),
                recovery_epoch: earlier_epoch,
                cap,
            });
        }
        for call in calls {
            self.enqueue(call, Purpose::RecoveryInbox { kind: 1 });
        }
    }

    fn earlier_inbox_done(
        &mut self,
        account: &[u8],
        epoch: u64,
        result: Result<Vec<Vec<u8>>, RejectReason>,
    ) {
        let Some(inbox) = self.st.app.earlier_recovery_inboxes.get(&epoch) else {
            return;
        };
        match result {
            Err(RejectReason::NotFound | RejectReason::Unauthorized) => {
                self.st.app.earlier_recovery_inboxes.remove(&epoch);
            }
            Ok(items) if !self.st.app.earlier_recovery_code_used => {
                let Ok(key) = unhex(&inbox.verification_key) else {
                    return;
                };
                // The alert has no voting, veto or approval semantics. Leave the items for other controllers.
                self.st.app.earlier_recovery_code_used = items.iter().take(ACK_PAGE).any(|raw| {
                    let Some((_, 1, bytes)) = inbox_item(raw) else {
                        return false;
                    };
                    crate::recovery::open_request(&bytes, &key).is_ok_and(|request| {
                        request.account_id == account && request.recovery_epoch == epoch
                    })
                });
            }
            _ => {}
        }
    }

    /// Hand what a drain brought to the verifier (it needs the tick's monotonic reading), then keep the inbox registered,
    /// acknowledge what was handled and drain again when due.
    pub(crate) fn inbox_tick(&mut self, mono: &MonotonicSample) {
        let (Some((head, _, policy)), Some(me)) = (
            self.recovery_ctx(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return;
        };
        let epoch = policy.epoch();
        if !watch::live(&head, &me) {
            return;
        }
        if self.st.app.recovery_inbox.epoch != Some(epoch) {
            self.st.app.outbox.retain(|e| {
                !matches!(e.purpose, Purpose::RecoveryInbox { .. })
                    || matches!(e.call, NetCall::RecoveryInboxDrain { recovery_epoch, .. }
                    if self.st.app.earlier_recovery_inboxes.contains_key(&recovery_epoch))
            });
            self.st.app.recovery_inbox = RecoveryInboxRec {
                epoch: Some(epoch),
                ..RecoveryInboxRec::default()
            };
        }
        self.earlier_inboxes_tick(&head, epoch);
        let Some(kit) = self.recovery_kit_value().filter(|kit| kit.epoch == epoch) else {
            return;
        };
        let shared_key = policy.recovery_key().filter(|_| head.control.len() > 1);
        let items = std::mem::take(&mut self.st.app.recovery_inbox.pending);
        for it in items {
            if let Ok(bytes) = unhex(&it.bytes) {
                let _ = match it.kind {
                    1 => self.observe_recovery_request(&bytes, mono),
                    2 => self.observe_recovery_veto(&bytes, mono),
                    _ => Ok(()),
                };
                if it.kind == 1
                    && let Some(home) = it.home
                    && let Some((_, digest)) = crate::recovery::request_shape(&bytes)
                    && let Some(rec) = self.st.app.recovery_requests.get_mut(&hex(&digest))
                    && rec.request == hex(&bytes)
                    && !rec.veto_homes.contains(&home)
                    && rec.veto_homes.len() < 5
                {
                    rec.veto_homes.push(home);
                }
                // This is a shared, destructive acknowledgement. Leave authenticated requests
                // for the other devices, even when this device sees a different parent.
                if it.kind == 1
                    && shared_key
                        .is_some_and(|key| crate::recovery::open_request(&bytes, key).is_ok())
                {
                    continue;
                }
            }
            // The separate veto board survives inbox acknowledgement.
            let acks = &mut self.st.app.recovery_inbox.to_ack;
            if !acks.contains(&it.id) {
                acks.push(it.id);
            }
        }
        let (host, port) = (kit.host.clone(), kit.port);
        let (account_id, cap) = (head.account_id.to_vec(), kit.inbox_cap.to_vec());
        let now = self.now_ms;
        let inbox = &mut self.st.app.recovery_inbox;
        if inbox.in_flight {
            return;
        }
        if !inbox.to_ack.is_empty() {
            let item_ids: Vec<String> = inbox.to_ack.iter().take(ACK_PAGE).cloned().collect();
            inbox.in_flight = true;
            self.enqueue(
                NetCall::RecoveryInboxAck {
                    host,
                    port,
                    account_id,
                    recovery_epoch: epoch,
                    cap,
                    item_ids,
                },
                Purpose::RecoveryInbox { kind: 2 },
            );
            return;
        }
        let retry_ms = if inbox.registration_refused {
            REFUSED_RETRY_MS
        } else {
            DRAIN_EVERY_MS
        };
        if inbox.last_ms != 0 && now.saturating_sub(inbox.last_ms) < retry_ms {
            return;
        }
        inbox.last_ms = now;
        inbox.in_flight = true;
        let renew = inbox
            .registered_at_ms
            .is_none_or(|last| now < last || now - last >= RENEW_EVERY_MS);
        let call = if !renew {
            NetCall::RecoveryInboxDrain {
                host,
                port,
                account_id,
                recovery_epoch: epoch,
                cap,
            }
        } else {
            NetCall::RecoveryInboxRegister {
                host,
                port,
                account_id,
                recovery_epoch: epoch,
                request_id: Some(hex(&self.rand16())),
                cap,
            }
        };
        self.enqueue(
            call,
            Purpose::RecoveryInbox {
                kind: u8::from(!renew),
            },
        );
    }

    /// A single-controller acknowledgement that has not been sent cannot settle a shared inbox.
    pub(super) fn cancel_inbox_acks(&mut self) {
        let before = self.st.app.outbox.len();
        self.st
            .app
            .outbox
            .retain(|e| !matches!(e.purpose, Purpose::RecoveryInbox { kind: 2 }));
        let inbox = &mut self.st.app.recovery_inbox;
        inbox.to_ack.clear();
        if self.st.app.outbox.len() != before {
            inbox.in_flight = false;
        }
    }

    pub(crate) fn inbox_done(
        &mut self,
        kind: u8,
        result: Result<Vec<Vec<u8>>, RejectReason>,
        call: &NetCall,
    ) {
        if let NetCall::RecoveryInboxDrain {
            account_id,
            recovery_epoch,
            ..
        } = call
            && self
                .st
                .app
                .earlier_recovery_inboxes
                .contains_key(recovery_epoch)
        {
            self.earlier_inbox_done(account_id, *recovery_epoch, result);
            return;
        }
        let epoch = match call {
            NetCall::RecoveryInboxRegister { recovery_epoch, .. }
            | NetCall::RecoveryInboxDrain { recovery_epoch, .. }
            | NetCall::RecoveryInboxAck { recovery_epoch, .. } => *recovery_epoch,
            _ => return,
        };
        if self
            .recovery_kit_value()
            .is_none_or(|kit| kit.epoch != epoch)
        {
            return;
        }
        let inbox = &mut self.st.app.recovery_inbox;
        if inbox.epoch != Some(epoch) {
            return;
        }
        inbox.in_flight = false;
        let items = match result {
            Ok(items) => items,
            Err(reason) => {
                if matches!(kind, 1 | 2)
                    && matches!(reason, RejectReason::NotFound | RejectReason::Unauthorized)
                {
                    inbox.registered_at_ms = None;
                    inbox.drained_at_ms = None;
                    inbox.last_ms = 0;
                    if kind == 2 {
                        inbox.to_ack.clear();
                    }
                } else if kind == 0 && !reason.retryable() {
                    inbox.registered_at_ms = None;
                    inbox.registration_refused = true;
                }
                return;
            }
        };
        match kind {
            0 => {
                inbox.registered_at_ms = Some(self.now_ms);
                inbox.registration_refused = false;
            }
            1 => {
                let NetCall::RecoveryInboxDrain { host, port, .. } = call else {
                    return;
                };
                inbox.drained_at_ms = Some(self.now_ms);
                for raw in items.into_iter().take(ACK_PAGE) {
                    let Some((id, kind, bytes)) = inbox_item(&raw) else {
                        continue;
                    };
                    let known =
                        inbox.pending.iter().any(|p| p.id == id) || inbox.to_ack.contains(&id);
                    if !known && inbox.pending.len() < MAX_PENDING {
                        inbox.pending.push(InboxItemRec {
                            id,
                            kind,
                            bytes: hex(&bytes),
                            home: Some((host.clone(), *port)),
                        });
                    }
                }
            }
            _ => {
                let n = inbox.to_ack.len().min(ACK_PAGE);
                inbox.to_ack.drain(..n);
            }
        }
    }
}
