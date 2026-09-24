//! V-013 feasibility model: exhaustive explicit-state exploration of two OM bridges (B1: OM <-> a Matrix-like
//! network "M", B2: OM <-> an XMPP-like network "X") plus an optional OM-unaware foreign-side bridge F (M <-> X) that
//! either preserves or strips OM interop metadata. Every interleaving of deliveries and scripted actions is explored;
//! properties are checked in every reachable state and at quiescence.
//!
//! The adapter rules implemented here are the normative rules of ../BRIDGE_ADAPTER_CONTRACT.md (IDs in comments).
//! Derived from INTEROPERABILITY_BRIDGES_AND_FOREIGN_PROVENANCE.md and TRANSPORT_ADAPTERS_AND_INTEROP.md.
//! Written by a Claude agent. Feasibility prototype, not product code.

use sha2::{Digest, Sha256};
use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::hash::{Hash, Hasher};

type BridgeId = u8;
const MAX_EVENTS: usize = 24;
const HOP_LIMIT: u8 = 3;
const ROOM: &str = "room-1";

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Net {
    Om,
    M,
    X,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    Msg,
    Edit,
    React,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Flag {
    Exact,
    LossyFormatting,
    EditAsNewMessage,
    TargetUnavailable,
    ForeignActorUnverified,
    OriginUnverified,
}

struct Caps {
    rich: bool,
    edits: bool,
    reactions: bool,
    carries_trace: bool,
}
fn caps(n: Net) -> Caps {
    match n {
        Net::Om => Caps { rich: true, edits: true, reactions: true, carries_trace: true },
        Net::M => Caps { rich: true, edits: true, reactions: true, carries_trace: true }, // custom event content keys
        Net::X => Caps { rich: false, edits: false, reactions: false, carries_trace: true }, // stanza extension element
    }
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Trace {
    origin: (Net, u32),
    traversed: Vec<BridgeId>,
    hop: u8,
}

/// Event on a foreign network.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct FEv {
    net: Net,
    id: u32,
    actor: String,
    display: String,
    kind: Kind,
    target: Option<u32>,
    rich: bool,
    trace: Option<Trace>,
    by_bridge: Option<BridgeId>,
    by_f: bool,
    exported_from: Option<u32>,
    flags: BTreeSet<Flag>,
    root: (Net, u32), // ground truth (never visible to adapters)
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
enum Author {
    Account(String),
    Bridge(BridgeId),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Prov {
    ns: Net,
    event_ref: u32,
    actor_ref: String,
    display_hint: String,
    flags: BTreeSet<Flag>,
    linked: bool,
    trace: Trace,
}

/// Event in the OM Room.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct OEv {
    id: u32,
    author: Author,
    native: bool, // created by an OM client (ground truth)
    epoch: u8,
    kind: Kind,
    target: Option<u32>,
    rich: bool,
    prov: Option<Prov>,
    fp: String,
    root: (Net, u32),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, Default)]
struct BridgeSt {
    admitted: Option<u8>,
    removed: Option<u8>,
    disclosed: bool,
    echo: BTreeSet<u32>,
    om_to_f: BTreeMap<u32, u32>,
    f_to_om: BTreeMap<u32, u32>,
    dropped: Vec<(u32, Flag)>,
    past: Vec<(u8, u8)>, // closed membership intervals [admitted, removed)
}

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Listener {
    B(BridgeId),
    F,
}
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum Ref {
    Om(u32),
    F(u32),
}

#[derive(Clone, Debug)]
enum Env {
    OmPost { author: &'static str, kind: Kind, target: Option<u32>, rich: bool },
    ForeignPost { net: Net, actor: &'static str, display: &'static str, kind: Kind, target: Option<u32>, rich: bool },
    Admit(BridgeId),
    Remove(BridgeId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FMode {
    Absent,
    Preserve,
    Strip,
}

/// Contract switches: all false = BRIDGE_ADAPTER_CONTRACT.md; each true disables one rule (fault injection).
#[derive(Clone, Copy, Debug, Default)]
struct Mutation {
    fp_scope_bridge: bool,  // BAC-FP-1: fingerprint scoped by BridgeID instead of (Room, origin)
    relay_bridged: bool,    // BAC-OUT-3: re-export events authored by another BridgePrincipal
    ignore_trace: bool,     // BAC-IN-1/3: ignore InteropTrace
    no_echo_table: bool,    // BAC-IN-2
    export_any_epoch: bool, // BAC-OUT-1: export events outside [admission, removal)
    trust_display: bool,    // BAC-ID-1: author as the Account whose name matches the display name
    no_disclosure: bool,    // BAC-SEC-1: admit a bridge without the E2EE-boundary disclosure
    attach_unmapped: bool,  // BAC-MAP-2: attach an unmapped reaction/edit to the latest mapped message
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct State {
    step: usize,
    epoch: u8,
    om: Vec<OEv>,
    foreign: Vec<FEv>,
    bridges: BTreeMap<BridgeId, BridgeSt>,
    pending: BTreeSet<(Listener, Ref)>,
    next_id: u32,
}

struct World {
    script: Vec<Env>,
    fmode: FMode,
    mu: Mutation,
    links: BTreeSet<(Net, &'static str)>, // verified ForeignIdentityLinks
}

/// BAC-OUT-1: was the bridge an admitted recipient at this epoch (current or any past membership interval)?
fn member_at(st: &BridgeSt, epoch: u8) -> bool {
    (st.admitted.is_some_and(|a| epoch >= a) && st.removed.is_none_or(|r| epoch < r)) || st.past.iter().any(|(a, r)| epoch >= *a && epoch < *r)
}

fn bridge_net(b: BridgeId) -> Net {
    if b == 1 { Net::M } else { Net::X }
}

/// BAC-FP-1: origin_fingerprint = H("OM-INTEROP-FP-v1" || om_room || origin_namespace || origin_event_ref)[..8]
fn fingerprint(origin: (Net, u32), scope_bridge: Option<BridgeId>) -> String {
    let mut h = Sha256::new();
    h.update(b"OM-INTEROP-FP-v1\0");
    h.update(ROOM.as_bytes());
    h.update(format!("|{:?}|{}", origin.0, origin.1).as_bytes());
    if let Some(b) = scope_bridge {
        h.update([b]);
    }
    h.finalize()[..8].iter().map(|x| format!("{x:02x}")).collect()
}

impl World {
    fn init(&self) -> State {
        let mut bridges = BTreeMap::new();
        for b in [1u8, 2] {
            bridges.insert(b, BridgeSt { admitted: Some(0), disclosed: !self.mu.no_disclosure, ..Default::default() });
        }
        State { step: 0, epoch: 0, om: vec![], foreign: vec![], bridges, pending: BTreeSet::new(), next_id: 1 }
    }

    fn listeners(&self, net: Net) -> Vec<Listener> {
        let mut l = vec![];
        match net {
            Net::Om => l.extend([Listener::B(1), Listener::B(2)]),
            Net::M => l.push(Listener::B(1)),
            Net::X => l.push(Listener::B(2)),
        }
        if net != Net::Om && self.fmode != FMode::Absent {
            l.push(Listener::F);
        }
        l
    }

    fn post_om(&self, s: &mut State, mut e: OEv) {
        e.id = s.next_id;
        s.next_id += 1;
        let id = e.id;
        s.om.push(e);
        for l in self.listeners(Net::Om) {
            s.pending.insert((l, Ref::Om(id)));
        }
    }

    fn post_foreign(&self, s: &mut State, mut f: FEv) -> u32 {
        f.id = s.next_id;
        s.next_id += 1;
        let (id, net) = (f.id, f.net);
        s.foreign.push(f);
        for l in self.listeners(net) {
            s.pending.insert((l, Ref::F(id)));
        }
        id
    }

    fn env(&self, s: &mut State, a: &Env) {
        match a {
            Env::OmPost { author, kind, target, rich } => {
                let id = s.next_id;
                let e = OEv { id, author: Author::Account(author.to_string()), native: true, epoch: s.epoch, kind: *kind, target: *target, rich: *rich,
                              prov: None, fp: fingerprint((Net::Om, id), None), root: (Net::Om, id) };
                self.post_om(s, e);
            }
            Env::ForeignPost { net, actor, display, kind, target, rich } => {
                let id = s.next_id;
                let f = FEv { net: *net, id, actor: actor.to_string(), display: display.to_string(), kind: *kind, target: *target, rich: *rich, trace: None,
                              by_bridge: None, by_f: false, exported_from: None, flags: BTreeSet::new(), root: (*net, id) };
                self.post_foreign(s, f);
            }
            Env::Admit(b) => {
                s.epoch += 1;
                let st = s.bridges.get_mut(b).unwrap();
                if let (Some(a), Some(r)) = (st.admitted, st.removed) {
                    st.past.push((a, r));
                }
                st.admitted = Some(s.epoch);
                st.removed = None;
                st.disclosed = !self.mu.no_disclosure; // BAC-SEC-1: admission and disclosure are one Room transition
            }
            Env::Remove(b) => {
                s.epoch += 1;
                s.bridges.get_mut(b).unwrap().removed = Some(s.epoch);
            }
        }
    }

    fn active_at(&self, st: &BridgeSt, epoch: u8) -> bool {
        self.mu.export_any_epoch || member_at(st, epoch)
    }

    /// Outbound: OM event -> foreign network (BAC-OUT-*).
    fn outbound(&self, s: &mut State, b: BridgeId, om_id: u32) {
        let e = s.om.iter().find(|e| e.id == om_id).unwrap().clone();
        let net = bridge_net(b);
        let st = s.bridges[&b].clone();
        if e.author == Author::Bridge(b) {
            return; // BAC-OUT-2: never re-export own imports
        }
        if matches!(e.author, Author::Bridge(_)) && !self.mu.relay_bridged {
            return; // BAC-OUT-3: do not re-export another bridge's imports (default)
        }
        if !self.active_at(&st, e.epoch) || !st.disclosed && !self.mu.no_disclosure {
            return; // BAC-OUT-1: only events of epochs where this bridge was an admitted, disclosed recipient
        }
        let c = caps(net);
        let mut flags = BTreeSet::new();
        let mut kind = e.kind;
        let mut target = None;
        if let Some(t) = e.target {
            match st.om_to_f.get(&t) {
                Some(ft) => target = Some(*ft),
                None if self.mu.attach_unmapped => target = st.om_to_f.values().max().copied(),
                None => {
                    s.bridges.get_mut(&b).unwrap().dropped.push((om_id, Flag::TargetUnavailable)); // BAC-MAP-2
                    return;
                }
            }
        }
        match kind {
            Kind::Edit if !c.edits => {
                kind = Kind::Msg;
                target = None;
                flags.insert(Flag::EditAsNewMessage); // BAC-FID-2
            }
            Kind::React if !c.reactions => {
                s.bridges.get_mut(&b).unwrap().dropped.push((om_id, Flag::TargetUnavailable));
                return; // BAC-FID-3: unsupported reaction is dropped and reported, never approximated
            }
            _ => {}
        }
        if e.rich && !c.rich {
            flags.insert(Flag::LossyFormatting);
        }
        if flags.is_empty() {
            flags.insert(Flag::Exact);
        }
        let prior = e.prov.as_ref().map(|p| p.trace.clone());
        let trace = Trace {
            origin: prior.as_ref().map(|t| t.origin).unwrap_or((Net::Om, e.id)),
            traversed: prior.as_ref().map(|t| t.traversed.clone()).unwrap_or_default().into_iter().chain([b]).collect(),
            hop: prior.as_ref().map(|t| t.hop).unwrap_or(0) + 1,
        };
        let f = FEv { net, id: 0, actor: format!("bridge-{b}"), display: "via OM".into(), kind, target, rich: e.rich && c.rich,
                      trace: if c.carries_trace { Some(trace) } else { None }, by_bridge: Some(b), by_f: false, exported_from: Some(om_id), flags, root: e.root };
        let fid = self.post_foreign(s, f);
        let st = s.bridges.get_mut(&b).unwrap();
        st.echo.insert(fid); // BAC-IN-2
        st.om_to_f.insert(om_id, fid);
        st.f_to_om.insert(fid, om_id);
    }

    /// Inbound: foreign event -> OM Room (BAC-IN-*).
    fn inbound(&self, s: &mut State, b: BridgeId, fid: u32) {
        let f = s.foreign.iter().find(|x| x.id == fid).unwrap().clone();
        let st = s.bridges[&b].clone();
        if !(st.admitted.is_some_and(|a| s.epoch >= a) && st.removed.is_none()) && !self.mu.export_any_epoch {
            return; // BAC-IN-0: a removed bridge imports nothing (stale mapping race)
        }
        if !self.mu.no_echo_table && st.echo.contains(&fid) {
            return; // BAC-IN-2: own echo
        }
        let trace = if self.mu.ignore_trace { None } else { f.trace.clone() };
        if let Some(t) = &trace {
            if t.traversed.contains(&b) {
                return; // BAC-IN-1: this bridge already carried the event
            }
            if t.hop >= HOP_LIMIT {
                return; // BAC-IN-4
            }
        }
        let origin = trace.as_ref().map(|t| t.origin).unwrap_or((f.net, f.id));
        let fp = fingerprint(origin, if self.mu.fp_scope_bridge { Some(b) } else { None });
        if s.om.iter().any(|e| e.fp == fp) {
            return; // BAC-IN-3: origin already materialised in this Room
        }
        let mut flags = BTreeSet::new();
        let mut kind = f.kind;
        let mut target = None;
        if let Some(t) = f.target {
            match st.f_to_om.get(&t) {
                Some(ot) => target = Some(*ot),
                None => {
                    kind = Kind::Msg;
                    flags.insert(Flag::TargetUnavailable); // BAC-MAP-2: shown as a new message, never attached elsewhere
                }
            }
        }
        let linked = self.links.contains(&(f.net, f.actor.as_str()));
        if !linked {
            flags.insert(Flag::ForeignActorUnverified);
        }
        if trace.is_none() {
            flags.insert(Flag::OriginUnverified);
        }
        let author = if self.mu.trust_display && ["alice", "bob"].contains(&f.display.to_lowercase().as_str()) {
            Author::Account(f.display.to_lowercase())
        } else {
            Author::Bridge(b) // BAC-ID-1
        };
        let tr = trace.clone().unwrap_or(Trace { origin, traversed: vec![], hop: 0 });
        let tr = Trace { traversed: tr.traversed.into_iter().chain([b]).collect(), hop: tr.hop + 1, origin: tr.origin };
        let prov = Prov { ns: f.net, event_ref: f.id, actor_ref: f.actor.clone(), display_hint: f.display.clone(), flags, linked, trace: tr };
        let e = OEv { id: 0, author, native: false, epoch: s.epoch, kind, target, rich: f.rich, prov: Some(prov), fp, root: f.root };
        let oid = s.next_id;
        self.post_om(s, e);
        let st = s.bridges.get_mut(&b).unwrap();
        st.f_to_om.insert(fid, oid);
        st.om_to_f.insert(oid, fid);
    }

    /// OM-unaware foreign-side bridge F: relays plain messages between M and X, preserving or stripping metadata.
    fn frelay(&self, s: &mut State, fid: u32) {
        let f = s.foreign.iter().find(|x| x.id == fid).unwrap().clone();
        if f.by_f || f.kind != Kind::Msg {
            return;
        }
        let net = if f.net == Net::M { Net::X } else { Net::M };
        let g = FEv { net, id: 0, actor: format!("f:{}", f.actor), display: f.display.clone(), kind: Kind::Msg, target: None, rich: f.rich && caps(net).rich,
                      // Preserve = F forwards OM interop metadata and, for native foreign events, a stable origin reference
                      trace: if self.fmode == FMode::Preserve { Some(f.trace.clone().unwrap_or(Trace { origin: (f.net, f.id), traversed: vec![], hop: 0 })) } else { None },
                      by_bridge: None, by_f: true, exported_from: None,
                      flags: BTreeSet::new(), root: f.root };
        self.post_foreign(s, g);
    }

    fn successors(&self, s: &State) -> Vec<State> {
        let mut out = vec![];
        if s.step < self.script.len() {
            let mut t = s.clone();
            self.env(&mut t, &self.script[s.step]);
            t.step += 1;
            out.push(t);
        }
        for p in &s.pending {
            let mut t = s.clone();
            t.pending.remove(p);
            match *p {
                (Listener::B(b), Ref::Om(id)) => self.outbound(&mut t, b, id),
                (Listener::B(b), Ref::F(id)) => self.inbound(&mut t, b, id),
                (Listener::F, Ref::F(id)) => self.frelay(&mut t, id),
                (Listener::F, Ref::Om(_)) => {}
            }
            out.push(t);
        }
        out
    }

    // ---------------------------------------------------------------- properties (checked in every state)
    fn check(&self, s: &State) -> Result<(), String> {
        if s.om.len() + s.foreign.len() > MAX_EVENTS {
            return Err("Bounded: unbounded echo (event bound exceeded)".into());
        }
        for e in &s.om {
            // TypedAuthor: bridge-imported content is never Account-authored.
            if matches!(e.author, Author::Account(_)) && !e.native {
                return Err(format!("TypedAuthor: imported event {} authored as {:?}", e.id, e.author));
            }
            if !e.native {
                let p = e.prov.as_ref().ok_or("ForeignProvenance: imported event without provenance")?;
                let src = s.foreign.iter().find(|f| f.id == p.event_ref).ok_or("ForeignProvenance: dangling event_ref")?;
                if p.ns == Net::Om || src.net != p.ns || src.actor != p.actor_ref || src.display != p.display_hint {
                    return Err(format!("ForeignProvenance: event {} provenance does not match its source", e.id));
                }
                if p.linked != self.links.contains(&(p.ns, p.actor_ref.as_str())) {
                    return Err("IdentityLink: linked flag without a verified ForeignIdentityLink".into());
                }
                if p.flags.contains(&Flag::ForeignActorUnverified) == p.linked {
                    return Err("Fidelity: foreign_actor_unverified flag wrong".into());
                }
            }
        }
        for f in &s.foreign {
            if let (Some(b), Some(src)) = (f.by_bridge, f.exported_from) {
                let st = &s.bridges[&b];
                let e = s.om.iter().find(|e| e.id == src).unwrap();
                // E2EEBoundaryDisclosed: exported only from epochs where this bridge was an admitted, disclosed recipient.
                let in_window = member_at(st, e.epoch);
                if !st.disclosed || !in_window {
                    return Err(format!("E2EEBoundaryDisclosed: event {} exported by bridge {b} outside its disclosed membership", e.id));
                }
                // Fidelity oracle (independent of the adapter code path).
                let c = caps(f.net);
                let mut want = BTreeSet::new();
                if e.rich && !c.rich {
                    want.insert(Flag::LossyFormatting);
                }
                if e.kind == Kind::Edit && !c.edits {
                    want.insert(Flag::EditAsNewMessage);
                }
                if want.is_empty() {
                    want.insert(Flag::Exact);
                }
                if f.flags != want {
                    return Err(format!("Fidelity: export of {} has flags {:?}, expected {:?}", e.id, f.flags, want));
                }
                if (f.kind == Kind::Edit && !c.edits) || (f.kind == Kind::React && !c.reactions) {
                    return Err("Fidelity: unsupported semantics silently emitted".into());
                }
                // TargetMapping: a reply/edit/reaction target is the mapped copy of the OM target, or nothing.
                if f.target.is_some() && (e.target.is_none() || st.om_to_f.get(&e.target.unwrap()) != f.target.as_ref()) {
                    return Err(format!("TargetMapping: export of {} attached to the wrong foreign target", e.id));
                }
            }
        }
        Ok(())
    }

    /// Reflection suppression at quiescence: each ground-truth origin appears in the Room at most `max` times, and
    /// every extra copy is flagged origin_unverified.
    fn check_quiescent(&self, s: &State, max: usize) -> Result<(), String> {
        let mut by_root: BTreeMap<(Net, u32), Vec<&OEv>> = BTreeMap::new();
        for e in &s.om {
            by_root.entry(e.root).or_default().push(e);
        }
        for (r, es) in by_root {
            if es.len() > max {
                return Err(format!("NoReflection: origin {r:?} materialised {} times", es.len()));
            }
            if es.len() > 1 && es.iter().skip(1).any(|e| !e.prov.as_ref().is_some_and(|p| p.flags.contains(&Flag::OriginUnverified))) {
                return Err(format!("NoReflection: unflagged duplicate of origin {r:?}"));
            }
        }
        Ok(())
    }
}

struct Outcome {
    states: usize,
    quiescent: usize,
    max_events: usize,
    violation: Option<String>,
}

fn explore(w: &World, max_copies: usize) -> Outcome {
    let mut seen: HashSet<u64> = HashSet::new();
    let mut stack = vec![w.init()];
    let mut o = Outcome { states: 0, quiescent: 0, max_events: 0, violation: None };
    while let Some(s) = stack.pop() {
        let mut h = DefaultHasher::new();
        s.hash(&mut h);
        if !seen.insert(h.finish()) {
            continue;
        }
        o.states += 1;
        o.max_events = o.max_events.max(s.om.len() + s.foreign.len());
        if let Err(e) = w.check(&s) {
            o.violation = Some(e);
            return o;
        }
        let succ = w.successors(&s);
        if succ.is_empty() {
            o.quiescent += 1;
            if let Err(e) = w.check_quiescent(&s, max_copies) {
                o.violation = Some(e);
                return o;
            }
        }
        stack.extend(succ);
    }
    o
}

fn msg(author: &'static str, rich: bool) -> Env {
    Env::OmPost { author, kind: Kind::Msg, target: None, rich }
}

fn main() {
    use Env::*;
    let none = Mutation::default();
    let links: BTreeSet<(Net, &'static str)> = [(Net::M, "alice_m")].into_iter().collect();
    // (id, description, script, F mode, mutation, max copies per origin, expected violated property or None)
    type Row = (&'static str, &'static str, Vec<Env>, FMode, Mutation, usize, Option<&'static str>);
    let rows: Vec<Row> = vec![
        ("two_bridges_om_origin", "OM rich message + edit + reaction through B1 (Matrix-like) and B2 (XMPP-like)",
         vec![msg("alice", true), OmPost { author: "alice", kind: Kind::Edit, target: Some(1), rich: false }, OmPost { author: "bob", kind: Kind::React, target: Some(1), rich: false }],
         FMode::Absent, none, 1, None),
        ("foreign_origin_both_networks", "foreign messages from M and X, one impersonating 'Alice', one with a verified identity link",
         vec![ForeignPost { net: Net::M, actor: "mallory_m", display: "Alice", kind: Kind::Msg, target: None, rich: true },
              ForeignPost { net: Net::M, actor: "alice_m", display: "Alice", kind: Kind::Msg, target: None, rich: false },
              ForeignPost { net: Net::X, actor: "carol_x", display: "Carol", kind: Kind::Msg, target: None, rich: false }],
         FMode::Absent, none, 1, None),
        ("foreign_side_bridge_preserves_trace", "OM message loops OM->M->F->X->OM and OM->X->F->M->OM; F keeps interop metadata",
         vec![msg("alice", false)], FMode::Preserve, none, 1, None),
        ("duplicate_import_two_bridges", "one foreign event reaches the Room via B1 directly and via F+B2",
         vec![ForeignPost { net: Net::M, actor: "dave_m", display: "Dave", kind: Kind::Msg, target: None, rich: false }], FMode::Preserve, none, 1, None),
        ("foreign_side_bridge_strips_trace", "RESIDUAL: F strips metadata; reflections are bounded and flagged origin_unverified, not suppressed",
         vec![msg("alice", false)], FMode::Strip, none, 3, None),
        ("foreign_origin_strip", "RESIDUAL: a foreign event relayed by a metadata-stripping F reaches the Room twice, both flagged origin_unverified",
         vec![ForeignPost { net: Net::M, actor: "dave_m", display: "Dave", kind: Kind::Msg, target: None, rich: false }], FMode::Strip, none, 2, None),
        ("revocation_race", "B2 removed while an OM message and a foreign reply are in flight; B2 re-admitted later",
         vec![msg("alice", false), Remove(2), msg("bob", false), ForeignPost { net: Net::X, actor: "carol_x", display: "Carol", kind: Kind::Msg, target: None, rich: false }, Admit(2)],
         FMode::Absent, none, 1, None),
        ("unmapped_targets", "reaction/edit to a message that was never exported (sent before B2 admission) and foreign reply to an unknown target",
         vec![Remove(2), msg("alice", false), Admit(2), OmPost { author: "bob", kind: Kind::Edit, target: Some(1), rich: false },
              ForeignPost { net: Net::M, actor: "erin_m", display: "Erin", kind: Kind::React, target: Some(99), rich: false }],
         FMode::Absent, none, 1, None),
        // ---- fault injection: each must be caught
        ("mut_relay_bridged_strip", "BAC-OUT-3 disabled + trace-stripping F: infinite echo", vec![msg("alice", false)], FMode::Strip,
         Mutation { relay_bridged: true, ..none }, 3, Some("Bounded")),
        ("mut_fp_scope_bridge", "BAC-FP-1 violated (bridge-scoped fingerprint): duplicate import via two bridges",
         vec![ForeignPost { net: Net::M, actor: "dave_m", display: "Dave", kind: Kind::Msg, target: None, rich: false }], FMode::Preserve,
         Mutation { fp_scope_bridge: true, ..none }, 1, Some("NoReflection")),
        ("mut_ignore_trace", "BAC-IN-1/3 disabled: reflections of OM-origin messages", vec![msg("alice", false)], FMode::Preserve,
         Mutation { ignore_trace: true, ..none }, 1, Some("NoReflection")),
        ("mut_no_echo_table", "BAC-IN-2 disabled on a network that drops metadata on echo", vec![msg("alice", false)], FMode::Absent,
         Mutation { no_echo_table: true, ignore_trace: true, ..none }, 1, Some("NoReflection")),
        ("mut_export_any_epoch", "BAC-OUT-1 disabled: retroactive export of pre-admission history",
         vec![Remove(2), msg("alice", false), Admit(2)], FMode::Absent, Mutation { export_any_epoch: true, ..none }, 1, Some("E2EEBoundaryDisclosed")),
        ("mut_no_disclosure", "BAC-SEC-1 disabled: bridge admitted without E2EE-boundary disclosure", vec![msg("alice", false)], FMode::Absent,
         Mutation { no_disclosure: true, ..none }, 1, Some("E2EEBoundaryDisclosed")),
        ("mut_trust_display", "BAC-ID-1 disabled: foreign display name 'Alice' becomes AccountPrincipal alice",
         vec![ForeignPost { net: Net::M, actor: "mallory_m", display: "Alice", kind: Kind::Msg, target: None, rich: false }], FMode::Absent,
         Mutation { trust_display: true, ..none }, 1, Some("TypedAuthor")),
        ("mut_attach_unmapped", "BAC-MAP-2 disabled: an unmapped reaction attaches to an unrelated message",
         vec![Remove(1), msg("alice", false), Admit(1), msg("bob", false), OmPost { author: "carol", kind: Kind::Edit, target: Some(1), rich: false }],
         FMode::Absent, Mutation { attach_unmapped: true, ..none }, 1, Some("TargetMapping")),
    ];
    let mut all_ok = true;
    let mut json = vec![];
    for (id, desc, script, fmode, mu, max, expect) in rows {
        let w = World { script, fmode, mu, links: links.clone() };
        let o = explore(&w, max);
        let got = o.violation.as_deref().map(|v| v.split(':').next().unwrap().to_string());
        let ok = got.as_deref() == expect;
        all_ok &= ok;
        eprintln!("{} {:<38} states={:<7} quiescent={:<6} max_events={:<3} {}", if ok { "ok " } else { "BAD" }, id, o.states, o.quiescent, o.max_events,
                  o.violation.as_deref().unwrap_or("no violation"));
        json.push(format!(
            "  {{\"id\": \"{id}\", \"description\": \"{desc}\", \"expect\": {}, \"violation\": {}, \"ok\": {ok}, \"states\": {}, \"quiescent_states\": {}, \"max_events\": {}}}",
            expect.map(|e| format!("\"{e}\"")).unwrap_or("null".into()),
            o.violation.as_ref().map(|v| format!("\"{}\"", v.replace('"', "'"))).unwrap_or("null".into()),
            if expect.is_some() { "\"n/a (stopped at first violation)\"".to_string() } else { o.states.to_string() },
            if expect.is_some() { "null".into() } else { o.quiescent.to_string() },
            o.max_events));
    }
    println!("{{\n \"all_ok\": {all_ok},\n \"bridges\": [\"B1: OM<->M (Matrix-like: rich, edits, reactions, metadata)\", \"B2: OM<->X (XMPP-like: plain text, no edits, no reactions, metadata extension)\", \"F: OM-unaware M<->X bridge (absent | preserve | strip metadata)\"],\n \"hop_limit\": {HOP_LIMIT},\n \"event_bound\": {MAX_EVENTS},\n \"scenarios\": [\n{}\n ]\n}}", json.join(",\n"));
    std::process::exit(if all_ok { 0 } else { 1 });
}
