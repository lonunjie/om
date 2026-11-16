// SPDX-License-Identifier: MPL-2.0
//! Owed device-enrollment notices (ADR-0193). Approving a link records, in the same transaction as the successor, one
//! durable `OwedNotice` per other device; each is delivered, re-sent on a schedule and cleared only by that device's
//! acknowledgement (`KIND_ENROLLED_ACK`, 13). A sibling that has not verified the notice does not advance its head
//! across the enrollment (`stage_enrollment`), so it fails closed whatever happens here.

use super::ceremony::{KIND_ENROLLED, MAX_CHAIN_TAIL, MAX_MSG, chain_head, notice_state};
use super::*;

/// The acknowledgement of an applied or already held enrollment notice: `{0: 13, 1: notice_digest}`.
pub(super) const KIND_ENROLLED_ACK: u64 = 13;
/// At most this many notices are owed at once (ADR-0193 section 6).
pub(super) const MAX_OWED: usize = 32;
/// Waiting records and inbound-triggered resends are tried at most this often.
const MIN_RETRY_MS: u64 = 60_000;

/// The wait before an unacknowledged send is repeated, after `attempts` sends: 15 minutes, 1 hour, 6 hours, then
/// every 24 hours (ADR-0193 section 4). There is no give-up.
pub(super) fn backoff_ms(attempts: u32) -> u64 {
    const MIN: u64 = 60_000;
    match attempts {
        0 | 1 => 15 * MIN,
        2 => 60 * MIN,
        3 => 6 * 60 * MIN,
        _ => 24 * 60 * MIN,
    }
}

fn digest_hex(notice: &[u8]) -> String {
    hex(&crypto::sha256(&[notice]))
}

impl<R: CryptoRng + RngCore> Tx<'_, R> {
    /// Record the notice owed to every other device the parent head lists (control or messaging, not the approver).
    /// A quarantined one is `Held`. Fails, adding nothing, when the bound would be passed.
    pub(super) fn owe_notice(
        &mut self,
        siblings: &[[u8; 16]],
        quarantined: &std::collections::BTreeSet<[u8; 16]>,
        state_hash: &[u8; 32],
        notice: &[u8],
    ) -> Result<(), RejectReason> {
        let fresh: Vec<&[u8; 16]> = siblings
            .iter()
            .filter(|d| {
                !self.st.app.owed_notices.iter().any(|o| {
                    o.sibling_device_id == hex(*d) && o.enrollment_state_hash == hex(state_hash)
                })
            })
            .collect();
        if self.st.app.owed_notices.len() + fresh.len() > MAX_OWED {
            return Err(RejectReason::Quota);
        }
        for d in fresh {
            self.st.app.owed_notices.push(OwedNotice {
                sibling_device_id: hex(d),
                enrollment_state_hash: hex(state_hash),
                notice_digest: digest_hex(notice),
                notice: hex(notice),
                created_at: self.now_ms,
                attempts: 0,
                last_attempt_at: 0,
                state: if quarantined.contains(d) {
                    OwedState::Held
                } else {
                    OwedState::Waiting
                },
            });
        }
        Ok(())
    }

    /// The signed notice for the enrollment that produced `state` (any owed record, then the display list).
    fn notice_for_state(&self, state: &[u8; 32]) -> Option<Vec<u8>> {
        let key = hex(state);
        self.st
            .app
            .owed_notices
            .iter()
            .find(|o| o.enrollment_state_hash == key)
            .and_then(|o| unhex(&o.notice).ok())
            .or_else(|| {
                self.st
                    .app
                    .notices
                    .iter()
                    .filter_map(|n| unhex(&n.signed).ok())
                    .find(|n| notice_state(n) == Some(*state))
            })
    }

    /// The `KIND_ENROLLED` payloads that tell `sibling` of every enrollment owed to it, in chain order. Each carries
    /// at most `MAX_CHAIN_TAIL` states and the notice of every enrollment those states add, so a sibling far behind
    /// catches up one contiguous window at a time. Built at send time from `chain` and the stored notices. Returns the
    /// digests each payload covers with it. `tip_sig` is a controller-signed head of the newest state, when we have one.
    pub(super) fn enrolled_payloads(
        &self,
        sibling: &str,
        chain: &[Vec<u8>],
        creds: &[Vec<u8>],
        tip_sig: Option<&[u8]>,
    ) -> Vec<(Vec<String>, Vec<u8>)> {
        let states: Vec<Acs> = chain.iter().filter_map(|c| chain_head(c)).collect();
        if states.len() != chain.len() {
            return vec![];
        }
        let mut owed: Vec<(usize, String)> = self
            .st
            .app
            .owed_notices
            .iter()
            .filter(|o| o.sibling_device_id == sibling && o.state != OwedState::Held)
            .filter_map(|o| {
                let at = states
                    .iter()
                    .position(|s| hex(&s.hash()) == o.enrollment_state_hash)?;
                Some((at, o.notice_digest.clone()))
            })
            .collect();
        owed.sort();
        let mut out = vec![];
        let mut i = 0;
        let mut prev_end: Option<usize> = None;
        while i < owed.len() {
            let first = owed[i].0;
            let start = match prev_end {
                Some(e) if e < first && first - e <= MAX_CHAIN_TAIL => e + 1,
                _ => first.saturating_sub(MAX_CHAIN_TAIL - 1),
            };
            let mut end = first;
            let mut j = i + 1;
            while j < owed.len() && owed[j].0 - start < MAX_CHAIN_TAIL {
                end = owed[j].0;
                j += 1;
            }
            let digests: Vec<String> = owed[i..j].iter().map(|(_, d)| d.clone()).collect();
            i = j;
            prev_end = Some(end);
            let mut notices: Vec<Vec<u8>> = (start..=end)
                .filter_map(|k| self.notice_for_state(&states[k].hash()))
                .collect();
            if notices.is_empty() {
                continue;
            }
            while notices.len() > MAX_CHAIN_TAIL {
                notices.remove(0);
            }
            let newest = notices.pop().unwrap_or_default();
            let mut map = vec![
                (0, V::U(KIND_ENROLLED)),
                (1, V::B(newest)),
                (
                    2,
                    V::A(chain[start..=end].iter().cloned().map(V::B).collect()),
                ),
                (3, V::A(creds.iter().cloned().map(V::B).collect())),
            ];
            if !notices.is_empty() {
                map.push((5, V::A(notices.into_iter().map(V::B).collect())));
            }
            if end + 1 == states.len()
                && let Some(h) = tip_sig
            {
                map.push((4, V::B(h.to_vec())));
            }
            let payload = cbor::encode(&V::M(map));
            if payload.len() > MAX_MSG {
                continue;
            }
            out.push((digests, payload));
        }
        out
    }

    /// Try to deliver everything owed to `sibling` now. `force` ignores the retry spacing. Returns whether any record
    /// changed. Records that cannot be sent stay (or become) `Waiting`; nothing is ever dropped for age.
    pub(super) fn try_owed(&mut self, sibling: &str, force: bool) -> bool {
        let now = self.now_ms;
        let due = self.st.app.owed_notices.iter().any(|o| {
            o.sibling_device_id == sibling
                && match o.state {
                    OwedState::Held => false,
                    OwedState::Waiting => {
                        force || o.last_attempt_at == 0 || now >= o.last_attempt_at + MIN_RETRY_MS
                    }
                    OwedState::Queued => force || now >= o.last_attempt_at + backoff_ms(o.attempts),
                }
        });
        if !due {
            return false;
        }
        let Some((head, chain, creds)) = self.acs_head() else {
            return false;
        };
        let tip_sig = self.tip_sig(&head);
        let payloads = self.enrolled_payloads(sibling, &chain, &creds, tip_sig.as_deref());
        self.send_owed_payloads(sibling, payloads)
    }

    /// Put each payload into the session with `sibling`, in order, and record the outcome on the records it covers:
    /// `Queued` when the send was accepted, `Waiting` when there is no path (and for everything after it).
    pub(super) fn send_owed_payloads(
        &mut self,
        sibling: &str,
        payloads: Vec<(Vec<String>, Vec<u8>)>,
    ) -> bool {
        let now = self.now_ms;
        let mut changed = false;
        let mut path = true;
        let mut covered: Vec<String> = vec![];
        for (digests, payload) in payloads {
            covered.extend(digests.iter().cloned());
            let sent = path && self.deliver_enrolled(sibling, &payload);
            path = sent;
            for o in
                self.st.app.owed_notices.iter_mut().filter(|o| {
                    o.sibling_device_id == sibling && digests.contains(&o.notice_digest)
                })
            {
                if o.state == OwedState::Held {
                    continue;
                }
                o.attempts = if sent && o.state != OwedState::Queued {
                    1
                } else {
                    o.attempts.saturating_add(1)
                };
                o.last_attempt_at = now;
                o.state = if sent {
                    OwedState::Queued
                } else {
                    OwedState::Waiting
                };
                changed = true;
            }
        }
        // A record no payload can be built for (no notice in reach) is an attempt that found no way.
        for o in self.st.app.owed_notices.iter_mut().filter(|o| {
            o.sibling_device_id == sibling
                && o.state != OwedState::Held
                && !covered.contains(&o.notice_digest)
        }) {
            o.last_attempt_at = now;
            o.state = OwedState::Waiting;
            changed = true;
        }
        changed
    }

    /// A controller-signed head of our tip for the payload, when this device may sign one.
    fn tip_sig(&mut self, tip: &Acs) -> Option<Vec<u8>> {
        let dev = self.st.device.as_ref()?;
        self.st
            .app
            .carry_sign_head_for_peers(&dev.signer, &dev.device_id, tip, self.rng)
    }

    /// Every unacknowledged notice: reconcile with the head, then send what is due. Run on every intent and inbound
    /// batch and by the host's `notice_tick`, so no timer lives in Core.
    pub(crate) fn owed_tick(&mut self) {
        self.flush_owed_acks();
        self.owed_reconcile();
        let mut sibs: Vec<String> = self
            .st
            .app
            .owed_notices
            .iter()
            .map(|o| o.sibling_device_id.clone())
            .collect();
        sibs.sort();
        sibs.dedup();
        for s in sibs {
            self.try_owed(&s, false);
        }
    }

    /// Drop the records of a device a verified head no longer lists or revoked; hold a quarantined device's and
    /// release them (`Held` to `Waiting`) when it is active again.
    fn owed_reconcile(&mut self) {
        // An acknowledgement owed to a device the head no longer lists is dropped with it.
        if let Some((head, _, _)) = self.acs_head() {
            self.st.app.owed_acks.retain(|a| {
                unhex(&a.sibling_device_id)
                    .ok()
                    .and_then(|b| <[u8; 16]>::try_from(b.as_slice()).ok())
                    .is_some_and(|d| {
                        (head.control.contains(&d) || head.messaging.contains(&d))
                            && !head.revoked.contains(&d)
                    })
            });
        }
        if self.st.app.owed_notices.is_empty() {
            return;
        }
        if let Some((head, _, _)) = self.acs_head() {
            let listed = |d: &[u8; 16]| head.control.contains(d) || head.messaging.contains(d);
            self.st.app.owed_notices.retain(|o| {
                unhex(&o.sibling_device_id)
                    .ok()
                    .and_then(|b| <[u8; 16]>::try_from(b.as_slice()).ok())
                    .is_some_and(|d| listed(&d) && !head.revoked.contains(&d))
            });
            for o in self.st.app.owed_notices.iter_mut() {
                let quarantined = unhex(&o.sibling_device_id)
                    .ok()
                    .and_then(|b| <[u8; 16]>::try_from(b.as_slice()).ok())
                    .is_some_and(|d| head.quarantined.contains(&d));
                match (quarantined, o.state) {
                    (true, s) if s != OwedState::Held => o.state = OwedState::Held,
                    (false, OwedState::Held) => o.state = OwedState::Waiting,
                    _ => {}
                }
            }
        }
    }

    /// The sibling spoke on a verified session: whatever is owed to it goes now (spaced by `MIN_RETRY_MS`).
    pub(super) fn owed_on_inbound(&mut self, sender: &[u8; 16]) {
        let s = hex(sender);
        if self
            .st
            .app
            .owed_notices
            .iter()
            .any(|o| o.sibling_device_id == s)
        {
            self.try_owed_inbound(&s);
        }
    }

    fn try_owed_inbound(&mut self, sibling: &str) {
        let now = self.now_ms;
        // A reply from the device: due again as soon as the retry spacing allows, whatever the long schedule says.
        for o in self.st.app.owed_notices.iter_mut() {
            if o.sibling_device_id == sibling
                && o.state == OwedState::Queued
                && now >= o.last_attempt_at + MIN_RETRY_MS
            {
                o.state = OwedState::Waiting;
            }
        }
        self.try_owed(sibling, false);
    }

    /// `RetryDeviceNotice`: try now, ignoring the spacing.
    pub(super) fn retry_device_notice(&mut self, device_id: &str) -> OutcomeClass {
        if !self
            .st
            .app
            .owed_notices
            .iter()
            .any(|o| o.sibling_device_id == device_id)
        {
            return OutcomeClass::Rejected {
                reason: RejectReason::NotFound,
            };
        }
        self.owed_reconcile();
        self.try_owed(device_id, true);
        OutcomeClass::Committed
    }

    /// `SetStrictDeviceNotice`.
    pub(super) fn set_strict_notice(&mut self, enabled: bool) -> OutcomeClass {
        self.st.app.link_strict_notice = enabled;
        OutcomeClass::Committed
    }

    /// A sibling answered `KIND_ENROLLED`: the digest must be one owed to exactly that sender, anything else is
    /// ignored. The acknowledgement carries no authority and changes nothing but the owed list.
    pub(super) fn on_enrolled_ack(&mut self, sender: &[u8; 16], p: &V) -> bool {
        let Some(d) = p.get(1).and_then(|x| x.b32()).ok() else {
            return false;
        };
        let (who, dig) = (hex(sender), hex(&d));
        self.st
            .app
            .owed_notices
            .retain(|o| !(o.sibling_device_id == who && o.notice_digest == dig));
        true
    }

    /// After a verified `KIND_ENROLLED` was applied (or already held): tell the sender, one ack per notice. A reply
    /// that has no route yet is kept and sent when one exists, so the answer is never lost to timing.
    pub(super) fn ack_enrolled(&mut self, conv: &str, sender: [u8; 16], notices: &[Vec<u8>]) {
        let who = hex(&sender);
        for n in notices {
            let digest = crypto::sha256(&[n]);
            if self.send_ack(Some(conv), &who, &digest) {
                continue;
            }
            let rec = OwedAck {
                sibling_device_id: who.clone(),
                notice_digest: hex(&digest),
            };
            if !self.st.app.owed_acks.contains(&rec) && self.st.app.owed_acks.len() < 4 * MAX_OWED {
                self.st.app.owed_acks.push(rec);
            }
        }
    }

    /// One `{0: 13, 1: digest}` through a session with `device`: first the conversation it came on, then any.
    fn send_ack(&mut self, first: Option<&str>, device: &str, digest: &[u8; 32]) -> bool {
        let ack = cbor::encode(&V::M(vec![
            (0, V::U(KIND_ENROLLED_ACK)),
            (1, V::B(digest.to_vec())),
        ]));
        let mut convs: Vec<String> = first.map(str::to_string).into_iter().collect();
        convs.extend(
            self.st
                .app
                .relationships
                .values()
                .filter(|r| r.state == RelState::Active && !r.blocked)
                .map(|r| r.id.clone())
                .filter(|id| Some(id.as_str()) != first),
        );
        for conv in convs {
            if !self.ensure_dev(&conv, device, true) {
                continue;
            }
            let purpose = Purpose::DepositAcs {
                conversation_id: conv.clone(),
            };
            if self
                .with_dev(&conv, device, |tx| tx.send_control(&conv, &ack, purpose))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    /// Send the acknowledgements that were waiting for a route.
    fn flush_owed_acks(&mut self) {
        let pending = self.st.app.owed_acks.clone();
        for a in pending {
            let Ok(d) = unhex(&a.notice_digest)
                .ok()
                .and_then(|b| <[u8; 32]>::try_from(b.as_slice()).ok())
                .ok_or(())
            else {
                self.st.app.owed_acks.retain(|x| *x != a);
                continue;
            };
            if self.send_ack(None, &a.sibling_device_id, &d) {
                self.st.app.owed_acks.retain(|x| *x != a);
            }
        }
    }
}
