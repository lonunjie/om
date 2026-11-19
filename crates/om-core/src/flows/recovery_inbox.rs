//! The controller side of the recovery inbox (ADR-0201 section 8.2, ADR-0202 section 3): a live controller that holds the
//! RecoveryKit registers its inbox at the Home, drains it on the host's tick and acknowledges what it has handed to the
//! verifier. An item alerts a controller; nothing the inbox holds is trusted or needed for safety. Every request and veto it
//! carries goes through the same observation checks as one that arrived any other way, so a forged item fills space and
//! authorizes nothing.

use super::*;
use crate::crypto::cbor;
use crate::flows::recovery_watch as watch;
use crate::host::MonotonicSample;
use crate::state::InboxItemRec;

/// Pacing between inbox calls, on Core's clock (never authority).
const DRAIN_EVERY_MS: u64 = 60_000;
const RENEW_EVERY_MS: u64 = 30 * 24 * 60 * 60 * 1000;
const REFUSED_RETRY_MS: u64 = 24 * 60 * 60 * 1000;
/// Items held until the next tick.
const MAX_PENDING: usize = 32;
/// Item ids acknowledged in one call.
const ACK_PAGE: usize = 16;

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Hand what a drain brought to the verifier (it needs the tick's monotonic reading), then keep the inbox registered,
    /// acknowledge what was handled and drain again when due.
    pub(crate) fn inbox_tick(&mut self, mono: &MonotonicSample) {
        let (Some(kit), Some((head, _, policy)), Some(me)) = (
            self.recovery_kit_value(),
            self.recovery_ctx(),
            self.st.device.as_ref().map(|d| d.device_id),
        ) else {
            return;
        };
        let epoch = policy.epoch();
        if !watch::live(&head, &me) || kit.epoch != epoch {
            return;
        }
        if self.st.app.recovery_inbox.epoch != Some(epoch) {
            self.st.app.outbox.retain(|e| !matches!(e.purpose, Purpose::RecoveryInbox { .. }));
            self.st.app.recovery_inbox = RecoveryInboxRec {
                epoch: Some(epoch),
                ..RecoveryInboxRec::default()
            };
        }
        let shared_policy = self
            .recovery_ctx()
            .filter(|(head, _, _)| head.control.len() > 1)
            .map(|(_, _, policy)| policy);
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
                    && shared_policy
                        .as_ref()
                        .and_then(|policy| policy.recovery_key())
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
        let retry_ms = if inbox.registration_refused { REFUSED_RETRY_MS } else { DRAIN_EVERY_MS };
        if inbox.last_ms != 0 && now.saturating_sub(inbox.last_ms) < retry_ms {
            return;
        }
        inbox.last_ms = now;
        inbox.in_flight = true;
        let renew = inbox.registered_at_ms.is_none_or(|last| now < last || now - last >= RENEW_EVERY_MS);
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

    pub(crate) fn inbox_done(&mut self, kind: u8, result: Result<Vec<Vec<u8>>, RejectReason>, call: &NetCall) {
        let epoch = match call {
            NetCall::RecoveryInboxRegister { recovery_epoch, .. }
            | NetCall::RecoveryInboxDrain { recovery_epoch, .. }
            | NetCall::RecoveryInboxAck { recovery_epoch, .. } => *recovery_epoch,
            _ => return,
        };
        if self.recovery_kit_value().is_none_or(|kit| kit.epoch != epoch) {
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
                if matches!(kind, 1 | 2) && matches!(reason, RejectReason::NotFound | RejectReason::Unauthorized) {
                    inbox.registered_at_ms = None;
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
                for raw in items.into_iter().take(ACK_PAGE) {
                    let Ok(v) = cbor::decode(&raw) else { continue };
                    let (Some(id), Some(kind), Some(bytes)) = (
                        v.get(0)
                            .ok()
                            .and_then(|x| x.b().ok())
                            .filter(|b| b.len() == 16),
                        v.get(1)
                            .ok()
                            .and_then(|x| x.u().ok())
                            .filter(|k| matches!(k, 1 | 2)),
                        v.get(2)
                            .ok()
                            .and_then(|x| x.b().ok())
                            .filter(|b| !b.is_empty() && b.len() <= 16_384),
                    ) else {
                        continue;
                    };
                    let id = hex(id);
                    let known =
                        inbox.pending.iter().any(|p| p.id == id) || inbox.to_ack.contains(&id);
                    if !known && inbox.pending.len() < MAX_PENDING {
                        inbox.pending.push(InboxItemRec {
                            id,
                            kind: kind as u8,
                            bytes: hex(bytes),
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
