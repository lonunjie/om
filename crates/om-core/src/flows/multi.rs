//! Multi-device behavior of a relationship: refreshing the pinned head of a contact's account (K7), holding what
//! arrives from a device that head does not list yet (ADR-0144, K12), and publishing our own head to contacts.
//!
//! Design: ONE_TO_ONE_SESSION_MANAGEMENT.md sections 3 and 4 (K5-K12), ACCOUNT_CONTINUITY_AND_CONTROL.md section 4
//! (successor rules), ADR-0144. The head a contact pins is only ever replaced by a verified descendant of itself: an
//! older head is ignored, a head at the same generation with another hash is a fork and is refused, and nothing is
//! trusted because a message claims it.

use super::*;
use std::collections::BTreeMap;

/// Our AccountControl chain and credentials, sent inside a relationship: `{0: 8, 1: [signed state], 2: [credential]}`.
pub(super) const KIND_ACS_UPDATE: u64 = 8;
/// A request for the peer's chain from the state we hold: `{0: 9, 1: hash of the head we hold}`.
pub(super) const KIND_ACS_REQUEST: u64 = 9;
/// Wrappers held per relationship while a peer head is stale (ADR-0144: bounded; a full list is a counted failure).
pub(super) const MAX_HELD: usize = 8;
/// A held wrapper is dropped after this long without the head arriving (the wrapper itself lives a week).
pub(super) const HELD_TTL_MS: u64 = 7 * DAY_MS;
/// A head request is sent at most once per this interval per relationship; an answer is served at most as often.
pub(super) const HEAD_ASK_INTERVAL_MS: u64 = 60 * 1000;
pub(super) const HEAD_SERVE_INTERVAL_MS: u64 = 10 * 1000;
/// Chain states accepted in one update (the protocol cap is 64 links).
const MAX_UPDATE_STATES: usize = 65;
/// Credentials accepted in one update: an account has at most this many devices.
const MAX_UPDATE_CREDS: usize = 64;

/// The verdict on one inner payload. `Held` is no verdict at all: the sender is not listed in the head we hold, which
/// may be stale, so the wrapper waits for a refresh instead of being rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Inner {
    Accepted,
    Rejected,
    Held,
}

impl From<bool> for Inner {
    fn from(ok: bool) -> Self {
        if ok { Inner::Accepted } else { Inner::Rejected }
    }
}

/// The state ids (hashes) of a signed chain, in order, or `None` when any state is malformed.
fn chain_states(chain: &[Vec<u8>]) -> Option<Vec<Acs>> {
    chain
        .iter()
        .map(|s| {
            let v = cbor::decode(s).ok()?;
            Acs::from_body(v.get(1).ok()?.b().ok()?).ok()
        })
        .collect()
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// Keep a wrapper until the peer head is refreshed. `false` when the bounded list is full.
    pub(super) fn hold(&mut self, conv: &str, wrapper: &[u8]) -> bool {
        let now = self.now_ms;
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return false;
        };
        let bytes = hex(wrapper);
        if rel.held.iter().any(|h| h.bytes == bytes) {
            return true; // the same wrapper again: already waiting
        }
        if rel.held.len() >= MAX_HELD {
            return false;
        }
        rel.held.push(HeldWrapper {
            bytes,
            since_ms: now,
        });
        true
    }

    /// Ask the peer for its head. Rate limited per relationship; a no-op when no route to the peer exists yet.
    pub(super) fn ask_head(&mut self, conv: &str) {
        let now = self.now_ms;
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return;
        };
        if rel.state != RelState::Active
            || rel.blocked
            || (rel.head_asked_ms != 0 && now < rel.head_asked_ms + HEAD_ASK_INTERVAL_MS)
        {
            return;
        }
        let Some(have) = hb(&rel.peer_acs_body)
            .ok()
            .and_then(|b| Acs::from_body(&b).ok())
        else {
            return;
        };
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(KIND_ACS_REQUEST)),
            (1, V::B(have.hash().to_vec())),
        ]));
        let sent = self.send_control(
            conv,
            &payload,
            Purpose::DepositAcs {
                conversation_id: conv.to_string(),
            },
        );
        if sent && let Some(rel) = self.st.app.relationships.get_mut(conv) {
            rel.head_asked_ms = now;
        }
    }

    /// Our chain (the whole chain, or from the state the peer says it holds) with every credential, as an update.
    fn acs_update_payload(&self, from: Option<[u8; 32]>, intros: &[Vec<u8>]) -> Option<Vec<u8>> {
        let (_, chain, creds) = self.acs_head()?;
        let states = chain_states(&chain)?;
        let start = from
            .and_then(|h| states.iter().position(|s| s.hash() == h))
            .unwrap_or(0);
        let mut fields = vec![
            (0, V::U(KIND_ACS_UPDATE)),
            (1, V::A(chain[start..].iter().cloned().map(V::B).collect())),
            (2, V::A(creds.into_iter().map(V::B).collect())),
        ];
        if !intros.is_empty() {
            // Introductions of our other devices (field 4): decoded, so the receiver reads them as maps.
            let list = intros
                .iter()
                .filter_map(|i| cbor::decode(i).ok())
                .collect::<Vec<_>>();
            fields.push((4, V::A(list)));
        }
        Some(cbor::encode(&V::M(fields)))
    }

    /// Tell every active contact our current head (after any change of our own AccountControlState). A relationship
    /// we cannot reach right now is marked dirty and retried by `maintain_relays`.
    pub(super) fn broadcast_account_update(&mut self) {
        self.sync_own_head();
        let convs: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.state == RelState::Active && !r.blocked)
            .map(|r| r.id.clone())
            .collect();
        for conv in convs {
            self.send_account_update(&conv, None, &[]);
        }
    }

    pub(super) fn send_account_update(
        &mut self,
        conv: &str,
        from: Option<[u8; 32]>,
        intros: &[Vec<u8>],
    ) {
        let sent = self
            .acs_update_payload(from, intros)
            .is_some_and(|payload| {
                self.send_control(
                    conv,
                    &payload,
                    Purpose::DepositAcs {
                        conversation_id: conv.to_string(),
                    },
                )
            });
        if let Some(rel) = self.st.app.relationships.get_mut(conv) {
            rel.head_dirty = !sent;
        }
    }

    /// Run with `maintain_relays`: resend heads that could not be sent, and drop held wrappers that waited too long.
    pub(super) fn retry_dirty_heads(&mut self) {
        let now = self.now_ms;
        for rel in self.st.app.relationships.values_mut() {
            rel.held.retain(|h| h.since_ms + HELD_TTL_MS > now);
        }
        let dirty: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.head_dirty && r.state == RelState::Active && !r.blocked)
            .map(|r| r.id.clone())
            .collect();
        for conv in dirty {
            self.send_account_update(&conv, None, &[]);
        }
    }

    /// The peer asked for our head from the state it holds (authenticated by the session; rate limited).
    pub(super) fn on_acs_request(&mut self, conv: &str, p: &V) -> bool {
        let now = self.now_ms;
        let Ok(have) = p.get(1).and_then(|x| x.b32()) else {
            return false;
        };
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return false;
        };
        if rel.head_sent_ms != 0 && now < rel.head_sent_ms + HEAD_SERVE_INTERVAL_MS {
            return true; // answered a moment ago: not an attack, no effect
        }
        rel.head_sent_ms = now;
        self.send_account_update(conv, Some(have), &[]);
        true
    }

    /// The peer sent its chain. Replace the pinned head only by a verified descendant of it (K7).
    pub(super) fn on_acs_update(&mut self, conv: &str, p: &V) -> bool {
        let (Some(chain), Some(creds)) = (bytes_list(p.get(1)), bytes_list(p.get(2))) else {
            return false;
        };
        if chain.is_empty()
            || chain.len() > MAX_UPDATE_STATES
            || creds.len() > MAX_UPDATE_CREDS
            || chain.iter().chain(creds.iter()).any(|x| x.len() > 8192)
        {
            return false;
        }
        match self.peer_refresh(conv, &chain, &creds) {
            Some(changed) => {
                self.adopt_intros(conv, p);
                if changed {
                    self.retry_held(conv);
                }
                true
            }
            None => false,
        }
    }

    /// Verify `chain` against the head pinned for the relationship and adopt it when it is a strict descendant.
    /// `Some(true)` adopted, `Some(false)` nothing to do (same head, or an older one that is ignored), `None` refused.
    fn peer_refresh(&mut self, conv: &str, chain: &[Vec<u8>], creds: &[Vec<u8>]) -> Option<bool> {
        let rel = self.st.app.relationships.get(conv)?.clone();
        let old = Acs::from_body(&hb(&rel.peer_acs_body).ok()?).ok()?;
        let account = h32(&rel.peer_account_id).ok()?;
        if old.account_id != account {
            return None;
        }
        let states = chain_states(chain)?;
        // Every state is of this account; a chain of another account is never a refresh.
        if states.iter().any(|s| s.account_id != account) {
            return None;
        }
        let head_seen = states.last()?;
        let old_hash = old.hash();
        match head_seen.generation.cmp(&old.generation) {
            std::cmp::Ordering::Less => return Some(false), // K7: never accept an older head
            std::cmp::Ordering::Equal => {
                return if head_seen.hash() == old_hash {
                    Some(false)
                } else {
                    None // two heads at one height: a fork, never a refresh
                };
            }
            std::cmp::Ordering::Greater => {}
        }
        // The new head must descend from the pinned one: the pinned state is in the chain, and every later state is a
        // verified successor of the one before it (signers checked against the PARENT's committed credentials).
        let at = states.iter().position(|s| s.hash() == old_hash)?;
        let mut all: Vec<Vec<u8>> = rel
            .peer_credentials
            .iter()
            .filter_map(|c| unhex(c).ok())
            .collect();
        for c in creds {
            if !all.contains(c) {
                all.push(c.clone());
            }
        }
        let mut head = old;
        for signed in &chain[at + 1..] {
            head = control::verify_successor(&head, signed, &all).ok()?;
        }
        // Keep exactly the credentials the new head commits; one it does not commit never authenticates anyone.
        let kept: Vec<Vec<u8>> = all
            .into_iter()
            .filter(|c| {
                binding::Credential::from_body(c).is_ok_and(|cr| {
                    cr.account_id == account
                        && head
                            .credentials
                            .iter()
                            .any(|(d, h)| *d == cr.device_id && *h == cr.hash())
                })
            })
            .collect();
        let dev = self.st.device.as_mut()?;
        dev.trust_peer(head.body(), kept.clone()).ok()?;
        let r = self.st.app.relationships.get_mut(conv)?;
        r.peer_acs_body = hex(&head.body());
        r.peer_credentials = kept.iter().map(|c| hex(c)).collect();
        // A signer the new head no longer lists stops counting at once (K9).
        Self::refresh_peer_relays(r);
        Some(true)
    }

    /// The head was refreshed: process what waited for it, oldest first. A wrapper that is still unknown holds again.
    fn retry_held(&mut self, conv: &str) {
        let Some(rel) = self.st.app.relationships.get_mut(conv) else {
            return;
        };
        let held = std::mem::take(&mut rel.held);
        let since: BTreeMap<Hex, u64> =
            held.iter().map(|h| (h.bytes.clone(), h.since_ms)).collect();
        for h in held {
            if let Ok(w) = unhex(&h.bytes) {
                self.on_session_envelope(conv, &w);
            }
        }
        if let Some(rel) = self.st.app.relationships.get_mut(conv) {
            for h in &mut rel.held {
                if let Some(s) = since.get(&h.bytes) {
                    h.since_ms = *s; // a wrapper that waits again keeps its original age
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------------- devices

/// The peer-side fields of a relationship hold ONE device of the conversation. The first-contact peer device lives in
/// them permanently; any other device (the peer's, or one of our own) is kept in [`Multi`] and swapped in for the
/// duration of a step, so every function written for "the peer device" works unchanged for each device.
fn swap_side(rel: &mut Relationship, side: &mut PeerDev) {
    std::mem::swap(&mut rel.peer_device_id, &mut side.device_id);
    std::mem::swap(&mut rel.peer_ik, &mut side.ik);
    std::mem::swap(&mut rel.peer_relays, &mut side.relays);
    std::mem::swap(&mut rel.peer_lineage, &mut side.lineage);
    std::mem::swap(&mut rel.peer_delivery, &mut side.delivery);
    std::mem::swap(&mut rel.peer_keys, &mut side.keys);
    std::mem::swap(&mut rel.route_health, &mut side.route_health);
}

/// Devices of one account tracked per conversation (a bound: an account has at most 64 devices).
const MAX_DEVICES: usize = 64;
/// A fetch capability or service name from an introduction is bounded before it is stored.
const MAX_CAP: usize = 64;
const MAX_SERVICE: usize = 256;

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    /// The identity key on a session envelope (`None` when malformed; the decrypt path rejects those).
    pub(super) fn envelope_sender(env: &[u8]) -> Option<[u8; 32]> {
        let v = cbor::decode(env).ok()?;
        let s = v.a().ok()?.first()?.b().ok()?;
        (s.len() == 33).then(|| <[u8; 32]>::try_from(&s[1..]).ok())?
    }

    /// Run `f` with the peer side of the relationship set to device `dev` (hex). `None` when the conversation has no
    /// such device. The first-contact peer device needs no swap.
    pub(super) fn with_dev<T>(
        &mut self,
        conv: &str,
        dev: &str,
        f: impl FnOnce(&mut Self) -> T,
    ) -> Option<T> {
        let mine = self.st.device.as_ref().map(|d| d.account_id)?;
        let own_head = self
            .st
            .device
            .as_ref()
            .and_then(|d| d.peers.get(&mine))
            .map(|(_, p)| {
                (
                    hex(&p.acs_body),
                    p.credential_bodies
                        .iter()
                        .map(|c| hex(c))
                        .collect::<Vec<_>>(),
                )
            });
        let rel = self.st.app.relationships.get_mut(conv)?;
        if rel.peer_device_id == dev {
            return Some(f(self));
        }
        let (mut side, sibling) = if let Some(s) = rel.multi.extra.remove(dev) {
            (s, false)
        } else {
            (rel.multi.siblings.remove(dev)?, true)
        };
        swap_side(rel, &mut side);
        // A device of our own account is verified against OUR head, not the peer's.
        let saved = sibling.then(|| {
            let saved = (
                rel.peer_account_id.clone(),
                rel.peer_acs_body.clone(),
                rel.peer_credentials.clone(),
            );
            rel.peer_account_id = hex(&mine);
            if let Some((body, creds)) = own_head.clone() {
                rel.peer_acs_body = body;
                rel.peer_credentials = creds;
            }
            saved
        });
        rel.multi.in_view = true;
        let out = f(self);
        if let Some(rel) = self.st.app.relationships.get_mut(conv) {
            rel.multi.in_view = false;
            if let Some((account, body, creds)) = saved {
                rel.peer_account_id = account;
                rel.peer_acs_body = body;
                rel.peer_credentials = creds;
            }
            swap_side(rel, &mut side);
            let map = if sibling {
                &mut rel.multi.siblings
            } else {
                &mut rel.multi.extra
            };
            map.insert(dev.to_string(), side);
        }
        Some(out)
    }

    /// Run `f` for the device that sent an authenticated session message (creating its record on first sight; it is
    /// listed as active in the head the envelope was verified against).
    pub(super) fn for_sender<T>(
        &mut self,
        conv: &str,
        sender: [u8; 16],
        sibling: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> Option<T> {
        let dev = hex(&sender);
        if !self.ensure_dev(conv, &dev, sibling) {
            return None;
        }
        self.with_dev(conv, &dev, f)
    }

    /// Make sure the conversation has a record for device `dev` of the peer's account (or of ours when `sibling`).
    pub(super) fn ensure_dev(&mut self, conv: &str, dev: &str, sibling: bool) -> bool {
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return false;
        };
        if !sibling && rel.peer_device_id == dev {
            return true;
        }
        if rel.multi.extra.contains_key(dev) || rel.multi.siblings.contains_key(dev) {
            return true;
        }
        if rel.multi.extra.len() + rel.multi.siblings.len() >= 2 * MAX_DEVICES {
            return false;
        }
        let Some(account) = self.account_of(rel, sibling) else {
            return false;
        };
        let Some(ik) = self.st.device.as_ref().and_then(|d| {
            d.peer_devices
                .iter()
                .find(|p| p.1 == account && hex(&p.2) == dev && p.3)
                .map(|p| p.0)
        }) else {
            return false;
        };
        let side = PeerDev {
            device_id: dev.to_string(),
            ik: hex(&ik),
            ..PeerDev::default()
        };
        if let Some(rel) = self.st.app.relationships.get_mut(conv) {
            let map = if sibling {
                &mut rel.multi.siblings
            } else {
                &mut rel.multi.extra
            };
            map.insert(dev.to_string(), side);
        }
        true
    }

    fn account_of(&self, rel: &Relationship, sibling: bool) -> Option<[u8; 32]> {
        if sibling {
            self.st.device.as_ref().map(|d| d.account_id)
        } else {
            h32(&rel.peer_account_id).ok()
        }
    }

    /// Does the head we hold list the device now on the peer side of `rel` as active and not quarantined (K5)?
    pub(super) fn cur_device_eligible(&self, rel: &Relationship) -> bool {
        let (Some(d), Ok(ik), Ok(id), Ok(account)) = (
            self.st.device.as_ref(),
            h32(&rel.peer_ik),
            h16(&rel.peer_device_id),
            h32(&rel.peer_account_id),
        ) else {
            return false;
        };
        d.peer_devices
            .iter()
            .any(|p| p.0 == ik && p.1 == account && p.2 == id && p.3)
    }

    pub(super) fn primary_eligible(&self, rel: &Relationship) -> bool {
        !rel.multi.in_view && self.cur_device_eligible(rel)
    }

    /// Every other device that should get a copy of what we send in this conversation: the peer's other active devices
    /// and our own other active devices, as `(device id, is ours)`.
    pub(super) fn conv_devices(&self, rel: &Relationship) -> Vec<(String, bool)> {
        let (Some(d), Ok(peer)) = (self.st.device.as_ref(), h32(&rel.peer_account_id)) else {
            return vec![];
        };
        d.peer_devices
            .iter()
            .filter(|p| p.3)
            .filter_map(|p| {
                let id = hex(&p.2);
                if p.1 == peer && id != rel.peer_device_id {
                    Some((id, false))
                } else if p.1 == d.account_id && p.2 != d.device_id {
                    Some((id, true))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Our own account's head, registered like a contact's so envelopes from our other devices verify (self-sync).
    pub(super) fn sync_own_head(&mut self) {
        let Some(last) = self.st.app.acs_chain.last().and_then(|c| unhex(c).ok()) else {
            return;
        };
        let Some(head) = cbor::decode(&last)
            .ok()
            .and_then(|v| Acs::from_body(v.get(1).ok()?.b().ok()?).ok())
        else {
            return;
        };
        let Some(dev) = self.st.device.as_ref() else {
            return;
        };
        if dev.account_id != head.account_id
            || dev
                .peers
                .get(&head.account_id)
                .is_some_and(|(h, _)| *h == head.hash())
        {
            return;
        }
        // Verified once, when it changes (the chain is ours, but a corrupted record must not be trusted blindly).
        let Some((head, _, creds)) = self.acs_head() else {
            return;
        };
        if let Some(dev) = self.st.device.as_mut() {
            let _ = dev.trust_peer(head.body(), creds);
        }
    }

    // ------------------------------------------------------------------------------------------------ fan-out

    /// One copy of `payload` for every other eligible device. A copy with no session or route yet waits.
    pub(super) fn fan_out(
        &mut self,
        conv: &str,
        m: &Msg,
        payload: &[u8],
    ) -> Result<Vec<Fan>, CoreError> {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return Ok(vec![]);
        };
        let mut out = vec![];
        for (dev, sibling) in self.conv_devices(&rel) {
            if !self.ensure_dev(conv, &dev, sibling) {
                continue;
            }
            let op = m.operation_id.clone().unwrap_or_default();
            let (state, unconfirmed) = self.fan_copy(conv, &dev, &m.id, &op, payload)?;
            if state == FanState::Waiting
                && let Some(r) = self.st.app.relationships.get_mut(conv)
            {
                r.multi.waiting = true;
            }
            out.push(Fan {
                device_id: dev,
                sibling,
                event: (state == FanState::Waiting || unconfirmed).then(|| hex(payload)),
                state,
            });
        }
        Ok(out)
    }

    /// Seal `payload` for device `dev` now if there is a session, a route and a key; otherwise ask for a session
    /// (when we hold a fetch capability) and report that the copy waits.
    fn fan_copy(
        &mut self,
        conv: &str,
        dev: &str,
        msg_id: &str,
        op: &str,
        payload: &[u8],
    ) -> Result<(FanState, bool), CoreError> {
        let sealed = self
            .with_dev(conv, dev, |tx| tx.seal_here(conv, dev, msg_id, op, payload))
            .unwrap_or(Ok(None))?;
        match sealed {
            Some(unconfirmed) => Ok((FanState::Queued, unconfirmed)),
            None => {
                self.want_session(conv, dev);
                Ok((FanState::Waiting, false))
            }
        }
    }

    /// In the view of the device: encrypt, seal and enqueue a copy. `Ok(None)` when it cannot be sealed yet.
    fn seal_here(
        &mut self,
        conv: &str,
        dev: &str,
        msg_id: &str,
        op: &str,
        payload: &[u8],
    ) -> Result<Option<bool>, CoreError> {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return Ok(None);
        };
        let Ok(ik) = h32(&rel.peer_ik) else {
            return Ok(None);
        };
        if !self.cur_device_eligible(&rel)
            || !self.st.device.as_ref().is_some_and(|d| d.has_session(&ik))
        {
            return Ok(None);
        }
        let Ok((pd, route)) = self.peer_target(&rel) else {
            return Ok(None);
        };
        let unconfirmed = self
            .st
            .device
            .as_ref()
            .is_some_and(|d| d.session_unconfirmed(&ik));
        let (env, _) = self
            .device()?
            .encrypt(&ik, payload)
            .map_err(|_| CoreError::Invalid("encrypt"))?;
        let wrapper = self
            .seal_to(&pd, &route, &env)
            .map_err(|_| CoreError::Invalid("seal"))?;
        self.enqueue_sealed_for(
            conv,
            Some(dev),
            &route,
            wrapper,
            &env,
            Purpose::DepositFan {
                operation_id: op.to_string(),
                conversation_id: conv.to_string(),
                message_id: msg_id.to_string(),
                device_id: dev.to_string(),
            },
        );
        Ok(Some(unconfirmed))
    }

    /// Send a control payload to every other device of the conversation that can take it now.
    pub(super) fn send_to_other_devices(
        &mut self,
        conv: &str,
        payload: &[u8],
        purpose: impl Fn() -> Purpose,
    ) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        if rel.multi.in_view {
            return;
        }
        for (dev, sibling) in self.conv_devices(&rel) {
            if !self.ensure_dev(conv, &dev, sibling) {
                continue;
            }
            let has_session = self
                .with_dev(conv, &dev, |tx| {
                    tx.st
                        .app
                        .relationships
                        .get(conv)
                        .and_then(|r| h32(&r.peer_ik).ok())
                        .zip(tx.st.device.as_ref())
                        .is_some_and(|(ik, d)| d.has_session(&ik))
                })
                .unwrap_or(false);
            if has_session {
                self.with_dev(conv, &dev, |tx| tx.send_control(conv, payload, purpose()));
            }
        }
    }

    // ------------------------------------------------------------------------------------------------ sessions

    fn fetch_info(&self, conv: &str, dev: &str) -> Option<(Option<String>, Vec<u8>)> {
        let rel = self.st.app.relationships.get(conv)?;
        let (service, cap) = if rel.peer_device_id == dev {
            (
                rel.multi.primary_fetch_service.clone(),
                rel.multi.primary_fetch_cap.clone(),
            )
        } else {
            let d = rel
                .multi
                .extra
                .get(dev)
                .or_else(|| rel.multi.siblings.get(dev))?;
            (d.fetch_service.clone(), d.fetch_cap.clone())
        };
        let cap = unhex(&cap).ok().filter(|c| !c.is_empty())?;
        Some((service, cap))
    }

    pub(super) fn set_fetching(&mut self, conv: &str, dev: &str, v: bool) {
        if let Some(d) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.multi.extra.get_mut(dev).or(r.multi.siblings.get_mut(dev)))
        {
            d.fetching = v;
        }
    }

    /// We hold the capability for this device's key bundle and have no session with it: fetch the bundle (we
    /// initiate). A device whose capability we do not hold initiates toward us.
    fn want_session(&mut self, conv: &str, dev: &str) {
        let Some((service, capability)) = self.fetch_info(conv, dev) else {
            return;
        };
        let Some(rel) = self.st.app.relationships.get(conv) else {
            return;
        };
        let (Ok(ik), Ok(id)) = (
            rel.multi
                .extra
                .get(dev)
                .or_else(|| rel.multi.siblings.get(dev))
                .map(|d| d.ik.as_str())
                .map_or_else(|| h32(&rel.peer_ik), h32),
            unhex(dev),
        ) else {
            return;
        };
        let fetching = rel
            .multi
            .extra
            .get(dev)
            .or_else(|| rel.multi.siblings.get(dev))
            .is_some_and(|d| d.fetching);
        if fetching || self.st.device.as_ref().is_none_or(|d| d.has_session(&ik)) {
            return;
        }
        self.set_fetching(conv, dev, true);
        self.enqueue(
            NetCall::FetchKeyMaterial {
                device_id: id,
                service,
                capability,
            },
            Purpose::FetchDeviceKeys {
                conversation_id: conv.to_string(),
                device_id: dev.to_string(),
            },
        );
    }

    /// Make sure every device we hold a capability for and have no session with is being fetched.
    pub(super) fn start_missing_sessions(&mut self, conv: &str) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        if rel.multi.in_view {
            return;
        }
        for (dev, _) in self.conv_devices(&rel) {
            self.want_session(conv, &dev);
        }
    }

    /// The bundle arrived: start the session and open it with our current RelaySet (the device needs our routes to
    /// answer). Copies that waited for this session go out.
    pub(super) fn on_device_keys(&mut self, conv: &str, dev: &str, items: Vec<Vec<u8>>) {
        self.set_fetching(conv, dev, false);
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        let sibling = rel.multi.siblings.contains_key(dev);
        let Some(account) = self.account_of(&rel, sibling) else {
            return;
        };
        let Some((acs_body, cred)) = self.st.device.as_ref().and_then(|d| {
            let (_, p) = d.peers.get(&account)?;
            let cred = p
                .credential_bodies
                .iter()
                .find(|c| {
                    binding::Credential::from_body(c).is_ok_and(|c| hex(&c.device_id) == dev)
                })?
                .clone();
            Some((p.acs_body.clone(), cred))
        }) else {
            return;
        };
        if registry()
            .check(
                "AsyncKeyBundle",
                items.first().map(Vec::as_slice).unwrap_or(&[]),
                false,
            )
            .is_err()
        {
            return;
        }
        let view = PeerView {
            trusted_acs_hash: ds_hash(binding::LBL_ACS_HASH, &acs_body),
            acs_body: &acs_body,
            credential_body: &cred,
            now: self.now_s(),
        };
        let Some(d) = self.st.device.as_mut() else {
            return;
        };
        if d.start_session(&items, &view, self.rng).is_err() {
            return;
        }
        self.introduce_to(conv, dev);
        self.flush_waiting(conv);
    }

    /// Open a new session with our RelaySet for this relationship (a new signed version naming the same routes), to
    /// every device, so all of them keep the same lineage.
    fn introduce_to(&mut self, conv: &str, dev: &str) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        let Some((signed, node, records)) =
            self.compose_set_update(&rel, rel.my_relays.entries.clone())
        else {
            return;
        };
        let payload = cbor::encode(&V::M(vec![
            (0, V::U(KIND_RELAYSET)),
            (1, V::B(signed)),
            (2, V::A(records.into_iter().map(V::B).collect())),
        ]));
        let purpose = || Purpose::DepositRelaySet {
            conversation_id: conv.to_string(),
        };
        let sent = self
            .with_dev(conv, dev, |tx| tx.send_control(conv, &payload, purpose()))
            .unwrap_or(false);
        if sent && let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.my_lineage.offer(node);
        }
    }

    // ------------------------------------------------------------------------------------------------ waiting copies

    /// Send the copies that waited for a session, a route or a key, and drop the ones whose device is no longer
    /// eligible (K5: a revoked device never gets a message it did not already have).
    pub(super) fn flush_waiting(&mut self, conv: &str) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        if rel.multi.in_view || !rel.multi.waiting {
            return;
        }
        let mut work = vec![];
        for m in &rel.messages {
            for f in m.fan.iter().filter(|f| f.state == FanState::Waiting) {
                work.push((
                    m.id.clone(),
                    m.operation_id.clone().unwrap_or_default(),
                    f.clone(),
                ));
            }
        }
        let live: Vec<(String, bool)> = self.conv_devices(&rel);
        let mut still = false;
        for (mid, op, f) in work {
            let (Some(event), true) = (
                f.event.as_deref().and_then(|e| unhex(e).ok()),
                live.iter()
                    .any(|(d, s)| *d == f.device_id && *s == f.sibling),
            ) else {
                self.drop_fan(conv, &mid, &f.device_id);
                continue;
            };
            if !self.ensure_dev(conv, &f.device_id, f.sibling) {
                still = true;
                continue;
            }
            match self.fan_copy(conv, &f.device_id, &mid, &op, &event) {
                Ok((FanState::Queued, unconfirmed)) => {
                    self.set_fan(conv, &mid, &f.device_id, FanState::Queued, unconfirmed);
                }
                _ => still = true,
            }
        }
        if let Some(r) = self.st.app.relationships.get_mut(conv) {
            r.multi.waiting = still;
        }
    }

    fn drop_fan(&mut self, conv: &str, msg: &str, dev: &str) {
        if let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == msg))
        {
            m.fan.retain(|f| f.device_id != dev);
        }
    }

    fn set_fan(&mut self, conv: &str, msg: &str, dev: &str, state: FanState, keep_event: bool) {
        if let Some(f) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == msg))
            .and_then(|m| m.fan.iter_mut().find(|f| f.device_id == dev))
        {
            f.state = state;
            if !keep_event {
                f.event = None;
            }
        }
    }

    pub(super) fn mark_fan(&mut self, conv: &str, msg: &str, dev: &str, state: FanState) {
        if let Some(f) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == msg))
            .and_then(|m| m.fan.iter_mut().find(|f| f.device_id == dev))
        {
            f.state = state;
        }
    }

    pub(super) fn fan_is_sibling(&self, conv: &str, msg: &str, dev: &str) -> bool {
        self.st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.messages.iter().find(|m| m.id == msg))
            .and_then(|m| m.fan.iter().find(|f| f.device_id == dev))
            .is_some_and(|f| f.sibling)
    }

    /// A deposit for the recipient's side finished. The message is Sent when nothing for the recipient is still
    /// queued or in the outbox; self-sync copies and copies that wait never hold that up.
    pub(super) fn finish_copy(&mut self, conv: &str, msg: &str, op: &str) {
        let pending = self
            .st
            .app
            .relationships
            .get(conv)
            .and_then(|r| r.messages.iter().find(|m| m.id == msg))
            .is_some_and(|m| {
                m.envelope.is_some()
                    || m.fan
                        .iter()
                        .any(|f| !f.sibling && f.state == FanState::Queued)
            });
        if pending {
            return;
        }
        self.set_delivery(conv, msg, DeliveryState::Sent);
        self.set_outcome(&op.to_string(), OutcomeClass::Committed);
    }

    // ------------------------------------------------------------------------------------------------ after inbound

    /// A device of the conversation spoke on a verified session.
    pub(super) fn after_inbound(&mut self, conv: &str, sender: [u8; 16], replaced: bool) {
        let dev = hex(&sender);
        if replaced {
            self.resend_unconfirmed(conv, &dev);
        } else if let Some(r) = self.st.app.relationships.get_mut(conv) {
            // It answered on the session our copies went out on: they were received, no need to keep the event.
            for m in r.messages.iter_mut() {
                for f in m.fan.iter_mut().filter(|f| f.device_id == dev) {
                    if f.state != FanState::Waiting {
                        f.event = None;
                    }
                }
            }
        }
        self.flush_waiting(conv);
        self.start_missing_sessions(conv);
    }

    /// Our initiation to `dev` lost a crossed race (K4): everything we sent on it, which the device will never be able
    /// to read, goes again on the session that won. Same EventIDs, so a copy that did arrive is deduplicated.
    fn resend_unconfirmed(&mut self, conv: &str, dev: &str) {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return;
        };
        let primary = rel.peer_device_id == dev;
        let mut work = vec![];
        for m in &rel.messages {
            if let Some(f) = m.fan.iter().find(|f| f.device_id == dev)
                && f.state != FanState::Waiting
                && let Some(ev) = f.event.as_deref().and_then(|e| unhex(e).ok())
            {
                work.push((m.id.clone(), m.operation_id.clone().unwrap_or_default(), ev));
            }
        }
        for (mid, op, ev) in work {
            // Drop what is still in the outbox for the dead session.
            self.st.app.outbox.retain(|e| match &e.purpose {
                Purpose::DepositFan {
                    message_id,
                    device_id,
                    ..
                } => !(message_id == &mid && device_id == dev),
                Purpose::DepositMessage { message_id, .. } => !(primary && message_id == &mid),
                _ => true,
            });
            if primary {
                let _ = self.resend_primary(conv, &mid, &op, &ev);
            } else if let Ok((state, _)) = self.fan_copy(conv, dev, &mid, &op, &ev) {
                self.set_fan(conv, &mid, dev, state, false);
                if state == FanState::Queued {
                    self.set_delivery(conv, &mid, DeliveryState::PendingNetwork);
                }
            }
        }
    }

    fn resend_primary(
        &mut self,
        conv: &str,
        mid: &str,
        op: &str,
        event: &[u8],
    ) -> Result<(), CoreError> {
        let Some(rel) = self.st.app.relationships.get(conv).cloned() else {
            return Ok(());
        };
        let Ok((pd, route)) = self.peer_target(&rel) else {
            return Ok(());
        };
        let ik = h32(&rel.peer_ik)?;
        let (env, _) = self
            .device()?
            .encrypt(&ik, event)
            .map_err(|_| CoreError::Invalid("encrypt"))?;
        let wrapper = self
            .seal_to(&pd, &route, &env)
            .map_err(|_| CoreError::Invalid("seal"))?;
        if let Some(m) = self
            .st
            .app
            .relationships
            .get_mut(conv)
            .and_then(|r| r.messages.iter_mut().find(|m| m.id == mid))
        {
            m.envelope = Some(hex(&env));
            m.delivery = DeliveryState::PendingNetwork;
            if let Some(f) = m.fan.iter_mut().find(|f| f.device_id == rel.peer_device_id) {
                f.state = FanState::Queued;
                f.event = None;
            }
        }
        self.enqueue_sealed(
            conv,
            &route,
            wrapper,
            &env,
            Purpose::DepositMessage {
                operation_id: op.to_string(),
                conversation_id: conv.to_string(),
                message_id: mid.to_string(),
            },
        );
        Ok(())
    }

    // ------------------------------------------------------------------------------------------------ self-sync in

    /// A message our own other device sent, delivered to us on the conversation (self-sync). It is shown as ours.
    /// Edits and deletes of our own messages from another device are not applied (not built; ignored, never half-applied).
    pub(super) fn on_synced_event(
        &mut self,
        conv: &str,
        ev: &events::Event,
        sender: [u8; 16],
    ) -> bool {
        let Some(mine) = self.st.device.as_ref().map(|d| d.account_id) else {
            return false;
        };
        let now = self.now_ms;
        let Some(r) = self.st.app.relationships.get_mut(conv) else {
            return false;
        };
        let Some(canon) = r.canon_id.as_deref() else {
            return false;
        };
        if hex(&ev.conversation_id) != canon
            || ev.author_account != mine
            || ev.author_device != sender
        {
            return false;
        }
        let eid = hex(&ev.event_id);
        if r.equivocated.contains(&eid) {
            return false;
        }
        if ev.kind != events::K_CREATE {
            return true;
        }
        let Some(parts) = super::blobs::read_create(ev) else {
            return false;
        };
        let id = format!("msg-{eid}");
        if let Some(old) = r.messages.iter().position(|m| m.id == id) {
            if r.messages[old].digest == hex(&ev.digest) {
                return true; // the same logical message again (a second copy, a re-send on another session)
            }
            r.messages.remove(old);
            r.equivocated.insert(eid);
            return false;
        }
        let _ = now;
        r.messages.push(Msg {
            id,
            outgoing: true,
            operation_id: None,
            text: parts.text,
            sent_at: ev.clock.physical_ms,
            delivery: DeliveryState::Sent,
            envelope: None,
            digest: hex(&ev.digest),
            clock_ms: ev.clock.physical_ms,
            clock_logical: ev.clock.logical,
            deleted: false,
            queued_event: None,
            fan: vec![],
            attachments: parts.attachments,
            ext: parts.ext,
        });
        true
    }

    // ------------------------------------------------------------------------------------------------ introductions

    /// A device's introduction into a conversation: `{0: device id, 1: its signed RelaySet, 2: [its signed key
    /// records], 3: key service, 4: fetch capability}`. The RelaySet and the key records are signed by that device and
    /// verified against the head, like anything it would send itself; the service and capability are not signed (a
    /// wrong one only makes our fetch fail). Returns whether the device's routes were adopted.
    pub(super) fn adopt_intro(&mut self, conv: &str, intro: &V, sibling: bool) -> bool {
        let (Ok(id), Ok(signed)) = (
            intro.get(0).and_then(|x| x.b()).map(<[u8]>::to_vec),
            intro.get(1).and_then(|x| x.b()).map(<[u8]>::to_vec),
        ) else {
            return false;
        };
        let dev = hex(&id);
        if self
            .st
            .app
            .relationships
            .get(conv)
            .is_none_or(|r| !sibling && r.peer_device_id == dev)
            || !self.ensure_dev(conv, &dev, sibling)
        {
            return false;
        }
        let records = bytes_list(intro.get(2)).unwrap_or_default();
        let p = V::M(vec![
            (0, V::U(KIND_RELAYSET)),
            (1, V::B(signed)),
            (2, V::A(records.into_iter().map(V::B).collect())),
        ]);
        let adopted = self
            .with_dev(conv, &dev, |tx| tx.on_relayset(conv, &p))
            .unwrap_or(false);
        if !adopted {
            return false;
        }
        let service = intro
            .opt(3)
            .and_then(|x| x.t().ok())
            .filter(|t| t.len() <= MAX_SERVICE)
            .map(String::from);
        let cap = intro
            .opt(4)
            .and_then(|x| x.b().ok())
            .filter(|c| !c.is_empty() && c.len() <= MAX_CAP)
            .map(<[u8]>::to_vec);
        if let Some(d) = self.st.app.relationships.get_mut(conv).and_then(|r| {
            r.multi
                .extra
                .get_mut(&dev)
                .or(r.multi.siblings.get_mut(&dev))
        }) && d.fetch_cap.is_empty()
            && let Some(cap) = cap
        {
            d.fetch_cap = hex(&cap);
            d.fetch_service = service;
        }
        true
    }

    /// The introductions in an account update (field 4), after the head they depend on has been refreshed.
    pub(super) fn adopt_intros(&mut self, conv: &str, p: &V) {
        let Some(list) = p.opt(4).and_then(|x| x.a().ok()) else {
            return;
        };
        for intro in list.iter().take(MAX_DEVICES) {
            let _ = self.adopt_intro(conv, intro, false);
        }
        self.flush_waiting(conv);
        self.start_missing_sessions(conv);
    }

    /// TEST ONLY (`test-loopback`): adopt the introduction of one of our own other devices, which the device-link
    /// ceremony will supply to the approving device.
    #[cfg(feature = "test-loopback")]
    pub fn debug_adopt_sibling_intro(&mut self, conv: &str, intro: &[u8]) -> Result<(), CoreError> {
        let v = cbor::decode(intro).map_err(|_| CoreError::Invalid("intro"))?;
        self.sync_own_head();
        if !self.adopt_intro(conv, &v, true) {
            return Err(CoreError::Invalid("intro not adopted"));
        }
        self.flush_waiting(conv);
        self.start_missing_sessions(conv);
        Ok(())
    }
}
