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
    /// Is the sender of this session envelope absent from every credential we hold for the relationship's account?
    /// Malformed envelopes answer `false`: the decrypt path rejects them.
    pub(super) fn sender_unknown(&self, account: [u8; 32], env: &[u8]) -> bool {
        let Some(dev) = self.st.device.as_ref() else {
            return false;
        };
        let ik = cbor::decode(env)
            .ok()
            .and_then(|v| {
                v.a()
                    .ok()
                    .and_then(|a| a.first())
                    .and_then(|s| s.b().ok().map(<[u8]>::to_vec))
            })
            .filter(|s| s.len() == 33)
            .and_then(|s| <[u8; 32]>::try_from(&s[1..]).ok());
        match ik {
            Some(ik) => !dev.peer_devices.iter().any(|d| d.0 == ik && d.1 == account),
            None => false,
        }
    }

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
    fn acs_update_payload(&self, from: Option<[u8; 32]>) -> Option<Vec<u8>> {
        let (_, chain, creds) = self.acs_head()?;
        let states = chain_states(&chain)?;
        let start = from
            .and_then(|h| states.iter().position(|s| s.hash() == h))
            .unwrap_or(0);
        Some(cbor::encode(&V::M(vec![
            (0, V::U(KIND_ACS_UPDATE)),
            (1, V::A(chain[start..].iter().cloned().map(V::B).collect())),
            (2, V::A(creds.into_iter().map(V::B).collect())),
        ])))
    }

    /// Tell every active contact our current head (after any change of our own AccountControlState). A relationship
    /// we cannot reach right now is marked dirty and retried by `maintain_relays`.
    pub(super) fn broadcast_account_update(&mut self) {
        let convs: Vec<String> = self
            .st
            .app
            .relationships
            .values()
            .filter(|r| r.state == RelState::Active && !r.blocked)
            .map(|r| r.id.clone())
            .collect();
        for conv in convs {
            self.send_account_update(&conv, None);
        }
    }

    fn send_account_update(&mut self, conv: &str, from: Option<[u8; 32]>) {
        let sent = self.acs_update_payload(from).is_some_and(|payload| {
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
            self.send_account_update(&conv, None);
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
        self.send_account_update(conv, Some(have));
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
