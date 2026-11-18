// SPDX-License-Identifier: MPL-2.0
//! ADR-0215 account-private nickname records, carried only on authenticated sibling sessions.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub(super) const KIND_PREFERENCE: u64 = 14;
const CODEC: [u8; 32] = [
    0x02, 0x8e, 0xc8, 0x0e, 0x9d, 0x67, 0x06, 0x3c, 0xa6, 0x70, 0xcb, 0xfc, 0x13, 0xfc, 0x5d, 0xf9,
    0x96, 0x11, 0xae, 0xed, 0x3b, 0x72, 0x95, 0x40, 0x1c, 0xee, 0x15, 0x95, 0x4d, 0xa1, 0xb4, 0x1f,
];
const MAX_ITEMS: usize = 256;
const MAX_ORIGINS: usize = 64;
const MAX_PER_PASS: usize = 8;
const RETRY_MS: u64 = 30_000;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    sequence: u64,
    items: BTreeMap<String, NicknameItem>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct NicknameItem {
    origins: BTreeMap<String, Origin>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Origin {
    latest: Option<Mutation>,
    tombstone: Option<Mutation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Mutation {
    generation: u64,
    sequence: u64,
    logical: u64,
    value: Option<String>,
    seen: BTreeMap<String, u64>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    delivery: BTreeMap<String, Delivery>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Delivery {
    RetryAt(u64),
    Acked,
}

fn eligible(head: &Acs, device: &[u8; 16]) -> bool {
    (head.control.contains(device) || head.messaging.contains(device))
        && !head.quarantined.contains(device)
}

fn item_key(rel: &Relationship) -> Option<String> {
    let canon = rel.canon_id.as_deref()?;
    (rel.state == RelState::Active && !rel.blocked && rel.relink.is_none())
        .then(|| item_id(canon, &rel.peer_account_id))
}

fn item_id(conversation: &str, account: &str) -> String {
    format!("{conversation}:{account}")
}

fn observes(origin: &str, m: &Mutation, other_origin: &str, seq: u64) -> bool {
    (origin == other_origin && m.sequence >= seq)
        || m.seen.get(other_origin).is_some_and(|s| *s >= seq)
}

impl NicknameItem {
    fn next(&self, generation: u64, sequence: u64, value: Option<String>) -> Option<Mutation> {
        let mut seen = BTreeMap::<String, u64>::new();
        let mut logical = 0;
        for (device, origin) in &self.origins {
            let Some(m) = &origin.latest else { continue };
            logical = logical.max(m.logical);
            for (id, seq) in m.seen.iter().chain(std::iter::once((device, &m.sequence))) {
                let at = seen.entry(id.clone()).or_default();
                *at = (*at).max(*seq);
            }
        }
        (seen.len() <= MAX_ORIGINS).then_some(Mutation {
            generation,
            sequence,
            logical: logical.saturating_add(1),
            value,
            seen,
            delivery: BTreeMap::new(),
        })
    }

    fn insert(&mut self, device: &str, m: Mutation) -> bool {
        if self.origins.len() >= MAX_ORIGINS && !self.origins.contains_key(device) {
            return false;
        }
        let origin = self.origins.entry(device.to_string()).or_default();
        if origin
            .latest
            .as_ref()
            .is_none_or(|prev| m.sequence > prev.sequence)
        {
            origin.latest = Some(m.clone());
        }
        // Even an out-of-order clear is evidence. A value must descend from every retained clear.
        if m.value.is_none()
            && origin
                .tombstone
                .as_ref()
                .is_none_or(|prev| m.sequence > prev.sequence)
        {
            origin.tombstone = Some(m);
        }
        true
    }

    fn nickname(&self) -> Option<String> {
        let heads = || {
            self.origins
                .iter()
                .filter_map(|(id, o)| o.latest.as_ref().map(|m| (id, m)))
        };
        heads()
            .filter(|(id, m)| {
                m.value.is_some()
                    && self.origins.iter().all(|(other, o)| {
                        o.tombstone
                            .as_ref()
                            .is_none_or(|t| observes(id, m, other, t.sequence))
                    })
            })
            .filter(|(id, m)| {
                !heads().any(|(other, n)| other != *id && observes(other, n, id, m.sequence))
            })
            .max_by_key(|(id, m)| (m.generation, m.logical, *id))
            .and_then(|(_, m)| m.value.clone())
    }
}

impl Mutation {
    fn encode(&self, profile: &[u8; 32], key: &str) -> Vec<u8> {
        let seen = self
            .seen
            .iter()
            .map(|(d, seq)| V::A(vec![V::T(d.clone()), V::U(*seq)]))
            .collect();
        cbor::encode(&V::M(vec![
            (0, V::U(KIND_PREFERENCE)),
            (1, V::B(CODEC.to_vec())),
            (2, V::U(1)),
            (3, V::B(profile.to_vec())),
            (4, V::T(key.to_string())),
            (5, V::U(self.sequence)),
            (6, V::U(self.generation)),
            (7, V::U(self.logical)),
            (8, V::A(seen)),
            (9, self.value.clone().map_or(V::Null, V::T)),
        ]))
    }

    fn decode(p: &V, sender: &str) -> Option<Self> {
        let sequence = p.get(5).ok()?.u().ok()?;
        let generation = p.get(6).ok()?.u().ok()?;
        let logical = p.get(7).ok()?.u().ok()?;
        let context = p.get(8).ok()?.a().ok()?;
        if sequence == 0 || logical == 0 || context.len() > MAX_ORIGINS {
            return None;
        }
        let mut seen = BTreeMap::new();
        for entry in context {
            let [d, s] = entry.a().ok()? else { return None };
            let d = d.t().ok()?;
            let s = s.u().ok()?;
            if d.len() != 32
                || h16(d).is_err()
                || s == 0
                || (d == sender && s >= sequence)
                || seen.insert(d.to_string(), s).is_some()
            {
                return None;
            }
        }
        let value = match p.get(9).ok()? {
            V::Null => None,
            V::T(n) if valid_nickname(n) && n.trim() == n => Some(n.clone()),
            _ => return None,
        };
        Some(Self {
            generation,
            sequence,
            logical,
            value,
            seen,
            delivery: BTreeMap::new(),
        })
    }
}

impl<R: RngCore + CryptoRng> Tx<'_, R> {
    pub(super) fn record_nickname(&mut self, conv: &str, value: Option<String>) {
        let Some(key) = self.st.app.relationships.get(conv).and_then(item_key) else {
            return;
        };
        let Some((head, ..)) = self.acs_head() else {
            return;
        };
        let Some(me) = self.st.device.as_ref().map(|d| d.device_id) else {
            return;
        };
        let prefs = &mut self.st.app.preferences;
        if !eligible(&head, &me)
            || (prefs.items.len() >= MAX_ITEMS && !prefs.items.contains_key(&key))
        {
            return;
        }
        let Some(seq) = prefs.sequence.checked_add(1) else {
            return;
        };
        let item = prefs.items.entry(key).or_default();
        if let Some(m) = item.next(head.generation, seq, value)
            && item.insert(&hex(&me), m)
        {
            prefs.sequence = seq;
        }
    }

    pub(super) fn prune_preference_outbox(&mut self) {
        let Some(me) = self.st.device.as_ref() else {
            return;
        };
        // sync_own_head installed only the devices eligible under the verified head.
        self.st.app.outbox.retain(|e| match &e.purpose {
            Purpose::DepositPreference { device_id, .. } => h16(device_id).ok().is_some_and(|id| {
                me.peer_devices.iter().any(|(_, account, device, active)| {
                    *active && account == &me.account_id && device == &id
                }) && me.peer_devices.iter().any(|(_, account, device, active)| {
                    *active && account == &me.account_id && device == &me.device_id
                })
            }),
            _ => true,
        });
    }

    pub(crate) fn maintain_preferences(&mut self) {
        if self.st.app.preferences.items.is_empty() {
            return;
        }
        let Some((head, ..)) = self.acs_head() else {
            return;
        };
        let Some(me) = self.st.device.as_ref().map(|d| d.device_id) else {
            return;
        };
        let peers: Vec<String> = head
            .control
            .iter()
            .chain(&head.messaging)
            .filter(|d| **d != me && eligible(&head, d))
            .map(|d| hex(d))
            .collect();
        if peers.is_empty() || !eligible(&head, &me) {
            return;
        }
        let pending: std::collections::BTreeSet<_> = self
            .st
            .app
            .outbox
            .iter()
            .filter_map(|e| match &e.purpose {
                Purpose::DepositPreference { device_id, .. } => Some(device_id.clone()),
                _ => None,
            })
            .collect();
        let local = hex(&me);
        let now = self.now_ms;
        let mut owed = vec![];
        for (key, item) in &mut self.st.app.preferences.items {
            let Some(origin) = item.origins.get_mut(&local) else {
                continue;
            };
            let separate_clear = origin
                .tombstone
                .as_ref()
                .zip(origin.latest.as_ref())
                .is_some_and(|(t, latest)| t.sequence != latest.sequence);
            for record in origin
                .tombstone
                .iter_mut()
                .filter(|_| separate_clear)
                .chain(origin.latest.iter_mut())
            {
                record.delivery.retain(|d, _| peers.contains(d));
                for d in &peers {
                    let due = match record.delivery.get(d) {
                        Some(Delivery::Acked) => false,
                        Some(Delivery::RetryAt(at)) => *at <= now,
                        None => true,
                    };
                    if due && owed.len() < MAX_PER_PASS && !pending.contains(d) {
                        record
                            .delivery
                            .insert(d.clone(), Delivery::RetryAt(now.saturating_add(RETRY_MS)));
                        owed.push((d.clone(), record.encode(&head.account_id, key)));
                    }
                }
            }
        }
        for (device, payload) in owed {
            self.deliver_sibling(&device, &payload, true);
        }
    }

    pub(super) fn on_preference(&mut self, sender: &[u8; 16], p: &V) -> bool {
        if p.get(1).and_then(|v| v.b32()).ok() != Some(CODEC)
            || p.get(2).and_then(|v| v.u()).ok() != Some(1)
        {
            return true; // Unknown codec: consume the authenticated frame without interpreting it.
        }
        self.accept_preference(sender, p).is_some()
    }

    fn accept_preference(&mut self, sender: &[u8; 16], p: &V) -> Option<()> {
        let (head, chain, _) = self.acs_head()?;
        let me = self.st.device.as_ref()?.device_id;
        if p.get(3).ok()?.b32().ok()? != head.account_id
            || sender == &me
            || !eligible(&head, sender)
            || !eligible(&head, &me)
        {
            return None;
        }
        let key = p.get(4).ok()?.t().ok()?;
        let (canon, peer) = key.split_once(':')?;
        if canon.len() != 64 || peer.len() != 64 || h32(canon).is_err() || h32(peer).is_err() {
            return None;
        }
        let sender_id = hex(sender);
        if p.opt(6).is_none() {
            let seq = p.get(5).ok()?.u().ok()?;
            if let Some(origin) = self
                .st
                .app
                .preferences
                .items
                .get_mut(key)
                .and_then(|item| item.origins.get_mut(&hex(&me)))
            {
                for record in origin.latest.iter_mut().chain(origin.tombstone.iter_mut()) {
                    if record.sequence == seq {
                        record.delivery.insert(sender_id.clone(), Delivery::Acked);
                    }
                }
            }
            return Some(());
        }
        let mutation = Mutation::decode(p, &sender_id)?;
        // The chain was fully verified by acs_head. Eligibility is required at authorship and now.
        if mutation.generation > head.generation
            || !chain
                .iter()
                .rev()
                .filter_map(|s| ceremony::chain_head(s))
                .find(|acs| acs.generation == mutation.generation)
                .is_some_and(|acs| eligible(&acs, sender))
        {
            return Some(()); // No receipt: the origin can retry after our ACS catches up.
        }
        let conv = self
            .st
            .app
            .relationships
            .values()
            .find(|r| item_key(r).as_deref() == Some(key))?
            .id
            .clone();
        let seq = mutation.sequence;
        let items = &mut self.st.app.preferences.items;
        if items.len() >= MAX_ITEMS && !items.contains_key(key) {
            return Some(());
        }
        let item = items.entry(key.to_string()).or_default();
        if !item.insert(&sender_id, mutation) {
            return Some(());
        }
        self.st.app.relationships.get_mut(&conv)?.nickname = item.nickname();
        let ack = cbor::encode(&V::M(vec![
            (0, V::U(KIND_PREFERENCE)),
            (1, V::B(CODEC.to_vec())),
            (2, V::U(1)),
            (3, V::B(head.account_id.to_vec())),
            (4, V::T(item_id(canon, peer))),
            (5, V::U(seq)),
        ]));
        if self
            .st
            .app
            .outbox
            .iter()
            .filter(|e| matches!(e.purpose, Purpose::DepositPreference { .. }))
            .take(MAX_ORIGINS)
            .count()
            < MAX_ORIGINS
        {
            self.deliver_sibling(&sender_id, &ack, true);
        }
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concurrent_values_converge_and_only_causal_recreation_survives_a_clear() {
        let mutation = |value: &str| {
            NicknameItem::default()
                .next(1, 1, Some(value.into()))
                .unwrap()
        };
        let a = mutation("Alice");
        let b = mutation("Bob");
        let clear = NicknameItem::default().next(1, 2, None).unwrap();

        for order in [false, true] {
            let mut item = NicknameItem::default();
            if order {
                item.insert("a", a.clone());
                item.insert("b", b.clone());
            } else {
                item.insert("b", b.clone());
                item.insert("a", a.clone());
            }
            assert_eq!(item.nickname().as_deref(), Some("Bob"));
            item.insert("a", clear.clone());
            assert_eq!(item.nickname(), None);
            item.insert("b", b.clone()); // a replay of the concurrent value
            assert_eq!(item.nickname(), None);
            let recreate = item.next(1, 3, Some("New name".into())).unwrap();
            item.insert("b", recreate);
            item.insert("a", clear.clone()); // a late duplicate cannot erase its descendant
            assert_eq!(item.nickname().as_deref(), Some("New name"));
        }
    }
}
