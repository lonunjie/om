//! Independent oracle over the effect log. It re-derives every rule from the installation records alone and does not
//! reuse broker state (taint, counters) - so a broker bug cannot hide itself.

use crate::world::{Effect, Installation, World};
use std::collections::BTreeMap;

pub fn check(installs: &[Installation], original_caps: &BTreeMap<String, Vec<String>>, w: &World) -> Vec<String> {
    let mut v = vec![];
    let inst = |sid: &str| installs.iter().find(|i| i.service_id == sid);
    let bundle_of: BTreeMap<u64, bool> = w.invocations.iter().map(|x| (x.0, x.4)).collect();
    let hop_of: BTreeMap<u64, (u64, u32, String)> = w.invocations.iter().map(|x| (x.0, (x.1, x.2, x.3.clone()))).collect();
    let mut read_before: BTreeMap<u64, bool> = BTreeMap::new();
    let mut sends: BTreeMap<(String, u64), u32> = BTreeMap::new();
    for (invid, e) in &w.effects {
        let (root, hop, svc) = hop_of.get(invid).cloned().unwrap_or((0, 0, String::new()));
        let Some(i) = inst(&svc) else { v.push(format!("effect from uninstalled service {svc}")); continue };
        let has = |c: &str| i.granted_capabilities.contains(c);
        let in_scope = |r: &str| i.scope_rooms.iter().any(|x| x == r);
        match e {
            Effect::Draft { room, .. } => {
                if !has("draft.create") || room.as_deref().is_some_and(|r| !in_scope(r)) {
                    v.push(format!("{svc}: draft without capability/scope"));
                }
            }
            Effect::ReadContext { .. } => {
                if !has("context.read") {
                    v.push(format!("{svc}: context read without capability"));
                }
                read_before.insert(*invid, true);
            }
            Effect::Fetch { url, .. } => {
                let host = url.strip_prefix("https://").map(|r| r.split(['/', '?', '#']).next().unwrap_or(""));
                if !has("network.fetch") || !host.is_some_and(|h| i.network_allowlist.iter().any(|a| a == h)) {
                    v.push(format!("{svc}: egress to non-allowlisted destination {url}"));
                }
                let tainted = bundle_of.get(invid).copied().unwrap_or(false) || read_before.get(invid).copied().unwrap_or(false);
                if tainted && !i.plaintext_egress {
                    v.push(format!("{svc}: plaintext-tainted egress {url}"));
                }
            }
            Effect::Message { author_kind, author, via_account, room, .. } => {
                if author_kind != "service" || author != &svc {
                    v.push(format!("{svc}: message authored as {author_kind}:{author}"));
                }
                if !has("message.send") || !in_scope(room) {
                    v.push(format!("{svc}: message without capability/scope in {room}"));
                }
                if let Some(a) = via_account {
                    let ok = i.representation.as_ref().is_some_and(|g| &g.represented_account == a && g.rooms.contains(room) && g.allowed_actions.iter().any(|x| x == "send_message"));
                    if !ok {
                        v.push(format!("{svc}: representation of {a} without grant"));
                    }
                }
                if hop > i.hop_limit {
                    v.push(format!("{svc}: message at hop {hop} > limit {}", i.hop_limit));
                }
                let n = sends.entry((svc.clone(), root)).or_insert(0);
                *n += 1;
                if *n > i.max_sends_per_root {
                    v.push(format!("{svc}: {} messages for one root invocation > budget {}", n, i.max_sends_per_root));
                }
            }
            Effect::ProposedRoomAction { room, .. } => {
                if !has("room.propose_action") || !in_scope(room) {
                    v.push(format!("{svc}: room action proposal without capability/scope"));
                }
            }
            Effect::CapabilityRequest { .. } => {}
        }
    }
    for i in installs {
        let now: Vec<String> = i.granted_capabilities.iter().cloned().collect();
        if original_caps.get(&i.service_id) != Some(&now) {
            v.push(format!("{}: capabilities changed without approval", i.service_id));
        }
    }
    v
}
