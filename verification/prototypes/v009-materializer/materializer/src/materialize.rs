//! Feasibility prototype for V-009. Not product code. Not for production.
//!
//! Independent ConversationEvent materializer, written directly from the public docs:
//!   EVENT_MODEL.md (EM), MESSENGER_BASELINE_V1.md (MB), TIME_AND_EXPIRY_SEMANTICS.md (TE),
//!   SCOPED_PERMISSIONS_AND_CONVERSATION_ORGANIZATION.md (SP), PROTOCOL_EXTENSIBILITY.md (PX).
//! Rulings marked `D-nn` are this prototype's resolution of a doc ambiguity; they are listed with
//! rationale in ../README.md ("Doc ambiguities and proposed rulings").
//!
//! The materializer is deliberately *incremental* (it applies events in arrival order and buffers
//! events whose target has not arrived yet) so that permutation/duplication testing exercises real
//! order-dependence risks instead of a trivially order-free batch function.

use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, BTreeSet};

/// EventClock total order key. EM "EventClock": (physical_ms, logical_counter, endpoint_tiebreak).
/// D-01: a final EventID tiebreak makes the order total even when a (buggy or hostile) endpoint
/// reuses an identical HLC tuple.
pub type Key = (u64, u64, String, String);

/// ADR-0121 rule 3 / ADR-0128 rule 17: shared-state registers order by `(sending epoch, EventClock)`.
pub type OKey = (u64, Key);

/// EM "Size/resource limits": maximum baseline event envelope 64 KiB. We measure the JSON
/// diagnostic form as a proxy for the CBOR size (D-13).
pub const MAX_EVENT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug)]
pub struct Ev {
    pub id: String,
    pub author: String,
    pub key: Key,
    pub epoch: u64,
    pub kind: String,
    pub body: Map<String, Value>,
    /// Envelope `expiry` (key 12): absolute wall-clock ms; the canonical v1 timer (ADR-0128 rule 16).
    pub expiry: Option<u64>,
}

impl Ev {
    fn okey(&self) -> OKey {
        (self.epoch, self.key.clone())
    }
}

/// Group-control authority as an externally ordered chain of epochs (GROUP_COMMIT_AND_STATE_CONVERGENCE.md
/// §1/§4: OM GroupControlState advances with MLS epochs, never by chat HLC). The corpus supplies the raw
/// chain of control operations per epoch (`role.define`, `role.assign`, `restriction.set`, `member.remove`);
/// the materializer derives the effective authority itself (SP "Capability model", "No ordinary-role deny
/// maze"). It never reads a precomputed capability table (G-V9-5).
#[derive(Clone, Debug, Default)]
struct EpochState {
    roles: BTreeMap<String, BTreeSet<String>>,
    assign: BTreeMap<String, Vec<String>>,
    restr: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Debug, Default)]
pub struct Control {
    epochs: Vec<EpochState>, // index = epoch number
}

fn str_list(v: Option<&Value>) -> Vec<String> {
    v.and_then(Value::as_array)
        .map(|a| a.iter().filter_map(|c| c.as_str().map(str::to_owned)).collect())
        .unwrap_or_default()
}

impl EpochState {
    fn caps_of(&self, principal: &str) -> BTreeSet<String> {
        let Some(rs) = self.assign.get(principal) else { return BTreeSet::new() };
        let mut caps = BTreeSet::new();
        for r in rs {
            if let Some(c) = self.roles.get(r) {
                caps.extend(c.iter().cloned());
            }
        }
        if let Some(r) = self.restr.get(principal) {
            caps.retain(|c| !r.contains(c)); // restrictions win
        }
        caps
    }
}

impl Control {
    pub fn from_json(v: &Value) -> Control {
        let mut c = Control::default();
        let mut cur = EpochState::default();
        if let Some(chain) = v.get("chain").and_then(Value::as_array) {
            for ops in chain {
                for op in ops.as_array().map(Vec::as_slice).unwrap_or_default() {
                    let who = op.get("principal").and_then(Value::as_str).unwrap_or_default().to_owned();
                    match op.get("op").and_then(Value::as_str) {
                        Some("role.define") => {
                            let r = op.get("role").and_then(Value::as_str).unwrap_or_default().to_owned();
                            cur.roles.insert(r, str_list(op.get("caps")).into_iter().collect());
                        }
                        Some("role.assign") => {
                            cur.assign.insert(who, str_list(op.get("roles")));
                        }
                        Some("member.remove") => {
                            cur.assign.remove(&who);
                        }
                        Some("restriction.set") => {
                            let caps: BTreeSet<String> = str_list(op.get("caps")).into_iter().collect();
                            if caps.is_empty() {
                                cur.restr.remove(&who);
                            } else {
                                cur.restr.insert(who, caps);
                            }
                        }
                        _ => {} // unknown control operation: no effect on authority
                    }
                }
                c.epochs.push(cur.clone());
            }
        }
        c
    }
    pub fn has_epoch(&self, epoch: u64) -> bool {
        (epoch as usize) < self.epochs.len()
    }
    /// SP "effective capabilities": role grants minus active explicit restrictions; restrictions win.
    /// D-02: authorization is evaluated against the control state of the epoch the event was sent
    /// under (its authenticated MLS epoch), never against "latest known" state and never by HLC.
    pub fn can(&self, principal: &str, epoch: u64, cap: &str) -> bool {
        self.epochs.get(epoch as usize).is_some_and(|e| e.caps_of(principal).contains(cap))
    }
    /// Membership at an epoch: a principal with a role assignment that has not been removed.
    pub fn is_member(&self, principal: &str, epoch: u64) -> bool {
        self.epochs.get(epoch as usize).is_some_and(|e| e.assign.contains_key(principal))
    }
    pub fn latest_epoch(&self) -> Option<u64> {
        self.epochs.len().checked_sub(1).map(|n| n as u64)
    }
    /// Derived authority per epoch, emitted so the comparison can check it against the Python derivation.
    pub fn authority(&self) -> Value {
        Value::Array(
            self.epochs
                .iter()
                .enumerate()
                .map(|(n, e)| {
                    let members: Map<String, Value> =
                        e.assign.keys().map(|p| (p.clone(), json!(e.caps_of(p).into_iter().collect::<Vec<_>>()))).collect();
                    json!({"epoch": n, "members": members})
                })
                .collect(),
        )
    }
}

pub struct CaseInput {
    pub conversation_id: String,
    pub known_capabilities: BTreeSet<String>,
    pub control: Control,
    /// Local receiver clock for expiry evaluation (ms), and local receipt time per EventID. Both are local
    /// state, not event content (TE §4: disappearing execution uses local time); they are fixed per case so
    /// delivery order cannot change them.
    pub now: Option<u64>,
    pub received_at: BTreeMap<String, u64>,
}

const CONTENT_KINDS: [&str; 2] = ["message.create", "poll.create"];
const KNOWN_KINDS: [&str; 14] = [
    "message.create", "message.edit", "message.delete", "reaction.set", "receipt.update", "pin.set",
    "moderation.redact", "poll.create", "poll.vote", "poll.close", "topic.create", "topic.update",
    "topic.status.set", "message.topic.set",
];

type R<T> = Result<T, String>;

fn get_str<'a>(m: &'a Map<String, Value>, k: &str) -> R<&'a str> {
    match m.get(k) {
        Some(Value::String(s)) if !s.is_empty() => Ok(s),
        _ => Err(format!("malformed:{k}")),
    }
}
fn opt_str<'a>(m: &'a Map<String, Value>, k: &str) -> R<Option<&'a str>> {
    match m.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if !s.is_empty() => Ok(Some(s)),
        _ => Err(format!("malformed:{k}")),
    }
}
fn get_bool(m: &Map<String, Value>, k: &str) -> R<bool> {
    m.get(k).and_then(Value::as_bool).ok_or_else(|| format!("malformed:{k}"))
}
fn get_str_list(m: &Map<String, Value>, k: &str) -> R<Vec<String>> {
    let a = m.get(k).and_then(Value::as_array).ok_or_else(|| format!("malformed:{k}"))?;
    a.iter()
        .map(|x| x.as_str().filter(|s| !s.is_empty()).map(str::to_owned).ok_or_else(|| format!("malformed:{k}")))
        .collect()
}

/// A RichText run of a message body: plain text, or a Mention node carrying the mentioned principal and
/// its display label (ADR-0128 rule 20).
#[derive(Clone, Debug, PartialEq)]
pub enum Run {
    Text(String),
    Mention { principal: String, label: String },
}

fn parse_runs(b: &Map<String, Value>) -> R<Option<Vec<Run>>> {
    let a = match b.get("runs") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::Array(a)) if !a.is_empty() => a,
        _ => return Err("malformed:runs".into()),
    };
    let mut out = vec![];
    for r in a {
        let m = r.as_object().ok_or("malformed:runs")?;
        match m.get("t").and_then(Value::as_str) {
            Some("text") => out.push(Run::Text(get_str(m, "s").map_err(|_| "malformed:runs".to_string())?.to_owned())),
            Some("mention") => out.push(Run::Mention {
                principal: get_str(m, "principal").map_err(|_| "malformed:runs".to_string())?.to_owned(),
                label: get_str(m, "label").map_err(|_| "malformed:runs".to_string())?.to_owned(),
            }),
            _ => return Err("malformed:runs".into()),
        }
    }
    Ok(Some(out))
}

/// Forwarding is new authorship (EM "Forwarding is new authorship", ADR-0128 rule 20): Mention nodes become
/// plain Text carrying the display label, the mention set is dropped, and everything else a private forward
/// must not carry (source ids, locators) is simply not part of the new body.
pub fn forward_body(b: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    if let Some(t) = b.get("text") {
        out.insert("text".into(), t.clone());
    }
    if let Ok(Some(runs)) = parse_runs(b) {
        let plain = runs.into_iter().map(|r| match r {
            Run::Text(s) => s,
            Run::Mention { label, .. } => label,
        });
        out.insert("runs".into(), Value::Array(plain.map(|s| json!({"t": "text", "s": s})).collect()));
    }
    if let Some(a) = b.get("attachments") {
        out.insert("attachments".into(), a.clone());
    }
    out
}

fn run_text(runs: &[Run]) -> String {
    runs.iter()
        .map(|r| match r {
            Run::Text(s) => s.as_str(),
            Run::Mention { label, .. } => label.as_str(),
        })
        .collect()
}

#[derive(Default)]
struct State {
    content: BTreeMap<String, Ev>,              // accepted message.create / poll.create
    rejected: BTreeMap<String, String>,         // event_id -> reason (diagnostic, must converge too)
    unsupported: BTreeMap<String, (Key, String, String)>, // placeholder: key, author, kind
    pending: BTreeMap<String, Vec<Ev>>,          // waiting on target event id
    pending_topic: BTreeMap<String, Vec<Ev>>,    // waiting on topic id
    edits: BTreeMap<String, (OKey, String)>,     // target -> epoch-major LWW (key, replacement text)
    deleted: BTreeSet<String>,
    redacted: BTreeSet<String>,
    reactions: BTreeMap<(String, String, String), (OKey, bool)>, // (actor,target,reaction_key)
    pins: BTreeMap<String, (OKey, bool)>,
    receipts: BTreeMap<(String, String), u8>,    // (event, actor) -> 1 delivered | 2 read
    votes: Vec<(String, String, OKey, Vec<String>)>, // actor, poll, key, options
    closes: BTreeMap<String, OKey>,              // poll -> earliest valid close (epoch-major)
    topic_creates: BTreeMap<String, Vec<Ev>>,
    topic_ops: Vec<Ev>,                          // topic.update / topic.status.set
    msg_topic: Vec<Ev>,                          // message.topic.set
}

pub struct Materializer<'c> {
    case: &'c CaseInput,
    store: BTreeMap<String, Value>, // first-seen raw event per EventID
    log: Vec<Value>,                // arrival log of unique events (for rebuild)
    banned: BTreeSet<String>,       // EventIDs seen with conflicting content (D-03)
    no_id: BTreeSet<String>,        // raw serializations of events without a usable EventID
    st: State,
}

impl<'c> Materializer<'c> {
    pub fn new(case: &'c CaseInput) -> Self {
        Materializer { case, store: BTreeMap::new(), log: vec![], banned: BTreeSet::new(), no_id: BTreeSet::new(), st: State::default() }
    }

    /// Deliver one (possibly duplicated) event. EM test 1: same event three times is idempotent.
    pub fn ingest(&mut self, raw: &Value) {
        let id = match raw.get("event_id").and_then(Value::as_str) {
            Some(s) if !s.is_empty() => s.to_owned(),
            _ => {
                self.no_id.insert(raw.to_string());
                return;
            }
        };
        if self.banned.contains(&id) {
            return;
        }
        if let Some(prev) = self.store.get(&id) {
            if prev == raw {
                return; // exact duplicate: idempotent
            }
            // D-03: two different payloads under one EventID are an equivocation. Every version is
            // excluded; the view is rebuilt so the outcome does not depend on which arrived first.
            self.banned.insert(id);
            self.rebuild();
            return;
        }
        self.store.insert(id, raw.clone());
        self.log.push(raw.clone());
        self.process(raw);
    }

    fn rebuild(&mut self) {
        self.st = State::default();
        let log = std::mem::take(&mut self.log);
        for raw in &log {
            let id = raw.get("event_id").and_then(Value::as_str).unwrap_or_default();
            if !self.banned.contains(id) {
                self.process(raw);
            }
        }
        self.log = log;
    }

    /// Standalone validation (no dependence on other events).
    fn parse(&self, raw: &Value) -> R<Ev> {
        let top = raw.as_object().ok_or("malformed:event")?;
        if raw.to_string().len() > MAX_EVENT_BYTES {
            return Err("oversize".into());
        }
        let id = get_str(top, "event_id")?.to_owned();
        let conv = get_str(top, "conversation_id")?;
        let author = get_str(top, "author")?.to_owned();
        let endpoint = get_str(top, "endpoint")?.to_owned();
        let clock = top.get("clock").and_then(Value::as_array).ok_or("malformed:clock")?;
        if clock.len() != 3 {
            return Err("malformed:clock".into());
        }
        let phys = clock[0].as_u64().ok_or("malformed:clock")?;
        let ctr = clock[1].as_u64().ok_or("malformed:clock")?;
        let tiebreak = clock[2].as_str().ok_or("malformed:clock")?.to_owned();
        // D-04: the endpoint tiebreak must be the authenticated sending endpoint; a mismatch is malformed.
        if tiebreak != endpoint {
            return Err("malformed:clock_endpoint".into());
        }
        let epoch = top.get("epoch").and_then(Value::as_u64).ok_or("malformed:epoch")?;
        let kind = get_str(top, "kind")?.to_owned();
        let req = match top.get("required_capabilities") {
            None => vec![],
            Some(_) => get_str_list(top, "required_capabilities").map_err(|_| "malformed:required_capabilities".to_string())?,
        };
        let body = top.get("body").and_then(Value::as_object).cloned().ok_or("malformed:body")?;
        // EM "reply_to ... MUST reference events in the same ConversationID": an event delivered into
        // this conversation but naming another ConversationID is rejected (D-05 covers reply refs).
        if conv != self.case.conversation_id {
            return Err("wrong_conversation".into());
        }
        // EM "Extension model" 2 + PX critical-unknown: unknown required capability -> placeholder,
        // never reinterpretation.
        if req.iter().any(|c| !self.case.known_capabilities.contains(c)) {
            return Err("unsupported".into());
        }
        if !self.case.control.has_epoch(epoch) {
            // D-06: a referenced control epoch that is not in the authenticated chain is not authority.
            return Err("unknown_epoch".into());
        }
        if !KNOWN_KINDS.contains(&kind.as_str()) {
            // EM extension rule 1: unknown optional content is ignored semantically.
            return Err("ignored_unknown_optional".into());
        }
        // Envelope `expiry` is optional; when present it must be an unsigned integer (D-21).
        let expiry = match top.get("expiry") {
            None | Some(Value::Null) => None,
            Some(v) => Some(v.as_u64().ok_or("malformed:expiry")?),
        };
        let ev = Ev { id, author, key: (phys, ctr, endpoint, String::new()), epoch, kind, body, expiry };
        let ev = Ev { key: (ev.key.0, ev.key.1, ev.key.2.clone(), ev.id.clone()), ..ev };
        self.validate_body(&ev)?;
        self.authorize_standalone(&ev)?;
        Ok(ev)
    }

    fn validate_body(&self, ev: &Ev) -> R<()> {
        let b = &ev.body;
        match ev.kind.as_str() {
            "message.create" => {
                let text = opt_str(b, "text")?;
                let runs = parse_runs(b)?;
                let atts = match b.get("attachments") {
                    None => 0,
                    Some(Value::Array(a)) => a.len(),
                    _ => return Err("malformed:attachments".into()),
                };
                if text.is_some() && runs.is_some() {
                    return Err("malformed:text_and_runs".into());
                }
                if text.is_none() && runs.is_none() && atts == 0 {
                    return Err("malformed:empty_message".into());
                }
                // `expires_after`: optional local bound in ms from receipt (EM "Expiry").
                match b.get("expires_after") {
                    None | Some(Value::Null) => {}
                    Some(v) if v.as_u64().is_some_and(|d| d >= 1) => {} // wire type is uint >= 1 (MessageCreate key 4)
                    _ => return Err("malformed:expires_after".into()),
                }
                opt_str(b, "reply_to")?;
                opt_str(b, "thread_root_event_id")?;
                opt_str(b, "topic_id")?;
            }
            "message.edit" => {
                get_str(b, "target_event_id")?;
                get_str(b, "replacement")?;
            }
            "message.delete" | "moderation.redact" => {
                get_str(b, "target_event_id")?;
            }
            "reaction.set" => {
                get_str(b, "target_event_id")?;
                get_str(b, "reaction_key")?;
                get_bool(b, "present")?;
            }
            "pin.set" => {
                get_str(b, "target_event_id")?;
                get_bool(b, "present")?;
            }
            "receipt.update" => {
                match get_str(b, "receipt_type")? {
                    "delivered" | "read" => {}
                    _ => return Err("malformed:receipt_type".into()),
                }
                get_str_list(b, "event_ids")?;
            }
            "poll.create" => {
                get_str(b, "question")?;
                let opts = get_str_list(b, "options")?;
                let uniq: BTreeSet<_> = opts.iter().collect();
                if opts.is_empty() || uniq.len() != opts.len() {
                    return Err("malformed:options".into());
                }
                match get_str(b, "selection_mode")? {
                    "single" | "multiple" => {}
                    _ => return Err("malformed:selection_mode".into()),
                }
            }
            "poll.vote" => {
                get_str(b, "target_poll_event_id")?;
                let sel = get_str_list(b, "selected_option_ids")?;
                let uniq: BTreeSet<_> = sel.iter().collect();
                if uniq.len() != sel.len() {
                    return Err("malformed:selected_option_ids".into());
                }
            }
            "poll.close" => {
                get_str(b, "target_poll_event_id")?;
            }
            "topic.create" | "topic.update" => {
                get_str(b, "topic_id")?;
                get_str(b, "title")?;
            }
            "topic.status.set" => {
                get_str(b, "topic_id")?;
                match get_str(b, "status")? {
                    "open" | "resolved" | "archived" => {}
                    _ => return Err("malformed:status".into()),
                }
            }
            "message.topic.set" => {
                get_str(b, "target_event_id")?;
                opt_str(b, "topic_id")?; // null clears placement (D-07)
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    /// Capability checks that need only the control chain. SP "Example Room capabilities".
    fn authorize_standalone(&self, ev: &Ev) -> R<()> {
        let c = &self.case.control;
        let can = |cap: &str| c.can(&ev.author, ev.epoch, cap);
        let ok = match ev.kind.as_str() {
            // D-08: room.post, or room.reply when the message is a reply/thread reply.
            "message.create" => {
                let is_reply = ev.body.get("reply_to").is_some_and(|v| !v.is_null())
                    || ev.body.get("thread_root_event_id").is_some_and(|v| !v.is_null());
                can("room.post") || (is_reply && can("room.reply"))
            }
            // D-09: editing requires the author to still hold posting authority at the edit's epoch.
            "message.edit" => can("room.post") || can("room.reply"),
            "message.delete" => true, // D-10: an author may always tombstone their own content
            "moderation.redact" => can("room.redact_others"),
            "reaction.set" => can("room.react"),
            "pin.set" => can("room.pin"),
            "receipt.update" => true,
            "poll.create" => can("room.post"),
            "poll.vote" => can("room.react"), // D-11
            "poll.close" => true,               // creator-or-moderator check needs the poll (D-12)
            "topic.create" => can("room.create_topic"),
            "topic.update" | "topic.status.set" => true, // creator-or-manage_topics needs the topic
            "message.topic.set" => can("room.manage_topics"),
            _ => false,
        };
        if ok { Ok(()) } else { Err("unauthorized".into()) }
    }

    fn process(&mut self, raw: &Value) {
        let id = raw.get("event_id").and_then(Value::as_str).unwrap_or_default().to_owned();
        match self.parse(raw) {
            Err(reason) => {
                if reason == "unsupported" {
                    let kind = raw.get("kind").and_then(Value::as_str).unwrap_or("?").to_owned();
                    // Placeholder only for content-like kinds; a known state operation with an unknown
                    // critical capability is refused (PX "MUST refuse the affected operation") - D-14.
                    if !KNOWN_KINDS.contains(&kind.as_str()) || CONTENT_KINDS.contains(&kind.as_str()) {
                        let key = placeholder_key(raw);
                        let author = raw.get("author").and_then(Value::as_str).unwrap_or("?").to_owned();
                        self.st.unsupported.insert(id.clone(), (key, author, kind));
                    }
                }
                self.st.rejected.insert(id.clone(), reason);
                self.flush(&id);
            }
            Ok(ev) => match ev.kind.as_str() {
                "message.create" | "poll.create" => {
                    self.st.content.insert(id.clone(), ev);
                    self.flush(&id);
                }
                "topic.create" => {
                    let tid = ev.body["topic_id"].as_str().unwrap().to_owned();
                    self.st.topic_creates.entry(tid.clone()).or_default().push(ev);
                    if let Some(waiting) = self.st.pending_topic.remove(&tid) {
                        for w in waiting {
                            self.try_apply(w);
                        }
                    }
                }
                _ => self.try_apply(ev),
            },
        }
    }

    fn flush(&mut self, target: &str) {
        if let Some(waiting) = self.st.pending.remove(target) {
            for w in waiting {
                self.try_apply(w);
            }
        }
    }

    /// Target known bad: rejected (for any reason) or equivocated.
    fn target_bad(&self, t: &str) -> bool {
        self.st.rejected.contains_key(t) || self.banned.contains(t)
    }

    fn reject(&mut self, ev: &Ev, why: &str) {
        self.st.rejected.insert(ev.id.clone(), why.to_owned());
    }

    fn try_apply(&mut self, ev: Ev) {
        let tfield = match ev.kind.as_str() {
            "poll.vote" | "poll.close" => Some("target_poll_event_id"),
            "receipt.update" | "topic.update" | "topic.status.set" => None,
            _ => Some("target_event_id"),
        };
        let target = match tfield {
            None => None,
            Some(f) => {
                let t = ev.body[f].as_str().unwrap().to_owned();
                if !self.st.content.contains_key(&t) {
                    if self.target_bad(&t) {
                        self.reject(&ev, "invalid_target");
                    } else {
                        // EM tests 2/3: edit/delete before original are buffered, not dropped.
                        self.st.pending.entry(t).or_default().push(ev);
                    }
                    return;
                }
                Some(self.st.content[&t].clone())
            }
        };
        let is_msg = |t: &Ev| t.kind == "message.create";
        let is_poll = |t: &Ev| t.kind == "poll.create";
        match ev.kind.as_str() {
            "message.edit" => {
                let t = target.unwrap();
                // MB "Message editing boundary": edits replace RichText of messages only.
                if !is_msg(&t) {
                    return self.reject(&ev, "invalid_target");
                }
                // EM message.edit: "Ordinary author edits are bound to the original author principal".
                if t.author != ev.author {
                    return self.reject(&ev, "not_author");
                }
                let text = ev.body["replacement"].as_str().unwrap().to_owned();
                lww(&mut self.st.edits, t.id.clone(), (ev.okey(), text));
            }
            "message.delete" => {
                let t = target.unwrap();
                if t.author != ev.author {
                    // MB "Author deletion vs moderation redaction": never impersonate the author.
                    return self.reject(&ev, "not_author");
                }
                self.st.deleted.insert(t.id);
            }
            "moderation.redact" => {
                let t = target.unwrap();
                self.st.redacted.insert(t.id);
            }
            "reaction.set" => {
                let t = target.unwrap();
                let k = (ev.author.clone(), t.id.clone(), ev.body["reaction_key"].as_str().unwrap().to_owned());
                let present = ev.body["present"].as_bool().unwrap();
                lww(&mut self.st.reactions, k, (ev.okey(), present));
            }
            "pin.set" => {
                let t = target.unwrap();
                let present = ev.body["present"].as_bool().unwrap();
                lww(&mut self.st.pins, t.id.clone(), (ev.okey(), present));
            }
            "receipt.update" => {
                let level = if ev.body["receipt_type"] == "read" { 2 } else { 1 };
                for e in get_str_list(&ev.body, "event_ids").unwrap() {
                    let slot = self.st.receipts.entry((e, ev.author.clone())).or_insert(0);
                    *slot = (*slot).max(level); // receipts are monotonic (read implies delivered)
                }
            }
            "poll.vote" => {
                let t = target.unwrap();
                if !is_poll(&t) {
                    return self.reject(&ev, "invalid_target");
                }
                let opts = get_str_list(&t.body, "options").unwrap();
                let sel = get_str_list(&ev.body, "selected_option_ids").unwrap();
                if sel.iter().any(|s| !opts.contains(s)) {
                    return self.reject(&ev, "invalid_option");
                }
                if t.body["selection_mode"] == "single" && sel.len() > 1 {
                    return self.reject(&ev, "invalid_option");
                }
                self.st.votes.push((ev.author.clone(), t.id.clone(), ev.okey(), sel));
            }
            "poll.close" => {
                let t = target.unwrap();
                if !is_poll(&t) {
                    return self.reject(&ev, "invalid_target");
                }
                // MB polls: "A creator or authorized room moderator may close a poll".
                let c = &self.case.control;
                if t.author != ev.author && !c.can(&ev.author, ev.epoch, "room.redact_others") {
                    return self.reject(&ev, "unauthorized");
                }
                // MB: poll.close is monotonic -> the earliest valid close in EventClock order wins.
                let slot = self.st.closes.entry(t.id.clone()).or_insert_with(|| ev.okey());
                if ev.okey() < *slot {
                    *slot = ev.okey();
                }
            }
            "message.topic.set" => {
                let t = target.unwrap();
                if !is_msg(&t) {
                    return self.reject(&ev, "invalid_target");
                }
                if let Some(tid) = ev.body["topic_id"].as_str() {
                    if !self.st.topic_creates.contains_key(tid) {
                        self.st.pending_topic.entry(tid.to_owned()).or_default().push(ev);
                        return;
                    }
                }
                self.st.msg_topic.push(ev);
            }
            "topic.update" | "topic.status.set" => {
                let tid = ev.body["topic_id"].as_str().unwrap().to_owned();
                if !self.st.topic_creates.contains_key(&tid) {
                    self.st.pending_topic.entry(tid).or_default().push(ev);
                    return;
                }
                self.st.topic_ops.push(ev);
            }
            _ => unreachable!("{}", ev.kind),
        }
    }

    /// Render the canonical semantic view (EM "Materialized views"). Everything is sorted.
    pub fn view(&self) -> Value {
        let st = &self.st;
        let c = &self.case.control;
        // Topics (SP "Persistent Topics"). D-15: duplicate topic.create for one TopicID -> earliest wins.
        let mut topics = Map::new();
        let mut topic_creator: BTreeMap<String, String> = BTreeMap::new();
        for (tid, creates) in &st.topic_creates {
            let win = creates.iter().min_by(|a, b| a.okey().cmp(&b.okey())).unwrap();
            topic_creator.insert(tid.clone(), win.author.clone());
            let mut title = (win.okey(), win.body["title"].as_str().unwrap().to_owned());
            // The create is the initial write of both registers: title and status "open" (V-009 REF-5).
            let mut status: (OKey, String) = (win.okey(), "open".to_owned());
            for op in st.topic_ops.iter().filter(|o| o.body["topic_id"] == tid.as_str()) {
                // D-16: topic.update/status.set by the topic creator or a room.manage_topics holder.
                if op.author != win.author && !c.can(&op.author, op.epoch, "room.manage_topics") {
                    continue;
                }
                if op.kind == "topic.update" {
                    if op.okey() > title.0 {
                        title = (op.okey(), op.body["title"].as_str().unwrap().to_owned());
                    }
                } else if op.okey() > status.0 {
                    status = (op.okey(), op.body["status"].as_str().unwrap().to_owned());
                }
            }
            topics.insert(tid.clone(), json!({
                "creator": win.author, "create_event": win.id, "title": title.1,
                "status": status.1,
            }));
        }
        let rejected_topic_ops: BTreeSet<String> = st
            .topic_ops
            .iter()
            .filter(|op| {
                let cr = topic_creator.get(op.body["topic_id"].as_str().unwrap());
                cr.is_some_and(|cr| *cr != op.author && !c.can(&op.author, op.epoch, "room.manage_topics"))
            })
            .map(|op| op.id.clone())
            .collect();
        let dup_topic_creates: BTreeSet<String> = st
            .topic_creates
            .values()
            .flat_map(|v| {
                let win = v.iter().min_by(|a, b| a.okey().cmp(&b.okey())).unwrap().id.clone();
                v.iter().filter(move |e| e.id != win).map(|e| e.id.clone())
            })
            .collect();

        // Message topic placement: LWW over valid message.topic.set, else create-time topic_id.
        let mut mt: BTreeMap<String, (OKey, Option<String>)> = BTreeMap::new();
        for ev in &st.msg_topic {
            let t = ev.body["target_event_id"].as_str().unwrap().to_owned();
            let tid = ev.body["topic_id"].as_str().map(str::to_owned);
            lww(&mut mt, t, (ev.okey(), tid));
        }

        // R-235 / ADR-0128 rule 16: content is deleted at min(envelope expiry, receipt + expires_after), so
        // never after the absolute `expiry`. `expires_after` counts from the local receipt time; if no
        // receipt time is recorded for the event it counts from `now` (D-22), so it cannot fire early.
        let mut deadlines: BTreeMap<String, u64> = BTreeMap::new();
        for (id, ev) in &st.content {
            let local = ev.body.get("expires_after").and_then(Value::as_u64).and_then(|d| {
                self.case.received_at.get(id).copied().or(self.case.now).map(|r| r.saturating_add(d))
            });
            if let Some(t) = [ev.expiry, local].into_iter().flatten().min() {
                deadlines.insert(id.clone(), t);
            }
        }
        let expired: BTreeSet<String> = deadlines
            .iter()
            .filter(|(_, t)| self.case.now.is_some_and(|n| **t <= n))
            .map(|(i, _)| i.clone())
            .collect();
        let visible = |id: &str| !st.deleted.contains(id) && !st.redacted.contains(id) && !expired.contains(id);
        let latest = self.case.control.latest_epoch();
        let mut timeline: Vec<(Key, Value)> = vec![];
        for (id, ev) in &st.content {
            let state = if expired.contains(id) {
                "expired" // R-235: content gone at its deadline
            } else if st.redacted.contains(id) {
                "redacted" // MB: moderation shown as moderation, not as author delete
            } else if st.deleted.contains(id) {
                "deleted"
            } else {
                "visible"
            };
            let mut e = Map::new();
            e.insert("id".into(), json!(id));
            e.insert("type".into(), json!(if ev.kind == "poll.create" { "poll" } else { "message" }));
            e.insert("author".into(), json!(ev.author));
            e.insert("state".into(), json!(state));
            if ev.kind == "message.create" {
                let edited = st.edits.get(id);
                if state == "visible" {
                    let runs = parse_runs(&ev.body).ok().flatten();
                    let text = edited
                        .map(|x| x.1.clone())
                        .or_else(|| ev.body.get("text").and_then(Value::as_str).map(str::to_owned))
                        .or_else(|| runs.as_ref().map(|r| run_text(r)));
                    e.insert("text".into(), json!(text));
                    e.insert("attachments".into(), json!(ev.body.get("attachments").and_then(Value::as_array).map_or(0, Vec::len)));
                    // R-239 / ADR-0128 rule 20: a Mention of a current member renders live; a mention of a
                    // non-member renders as plain text (inert). Membership is the latest known roster
                    // (D-19): a mention is presentation, not authorization. An edit replaces the content
                    // with plain text, so an edited message carries no mentions.
                    let (mut live, mut inert) = (vec![], vec![]);
                    if edited.is_none() {
                        for r in runs.iter().flatten() {
                            if let Run::Mention { principal, label } = r {
                                if latest.is_some_and(|l| self.case.control.is_member(principal, l)) {
                                    live.push(principal.clone());
                                } else {
                                    inert.push(label.clone());
                                }
                            }
                        }
                    }
                    live.sort();
                    inert.sort();
                    e.insert("mentions".into(), json!({"live": live, "inert": inert}));
                }
                e.insert("edited".into(), json!(edited.is_some()));
                // D-05: a reference that does not resolve inside this conversation is shown as
                // "original unavailable"; the referring message itself stays valid.
                for f in ["reply_to", "thread_root_event_id"] {
                    if let Some(r) = ev.body.get(f).and_then(Value::as_str) {
                        e.insert(f.into(), json!({"id": r, "resolved": st.content.contains_key(r)}));
                    }
                }
                let topic = match mt.get(id) {
                    Some((_, t)) => t.clone(),
                    None => ev.body.get("topic_id").and_then(Value::as_str).filter(|t| topics.contains_key(*t)).map(str::to_owned),
                };
                e.insert("topic".into(), json!(topic));
            } else {
                e.insert("question".into(), ev.body["question"].clone());
            }
            if state == "visible" {
                let mut rx: BTreeMap<String, Vec<String>> = BTreeMap::new();
                for ((actor, target, rk), (_, present)) in &st.reactions {
                    if target == id && *present {
                        rx.entry(rk.clone()).or_default().push(actor.clone());
                    }
                }
                e.insert("reactions".into(), json!(rx));
            }
            timeline.push((ev.key.clone(), Value::Object(e)));
        }
        for (id, (key, author, kind)) in &st.unsupported {
            timeline.push((key.clone(), json!({"id": id, "type": "unsupported", "kind": kind, "author": author})));
        }
        timeline.sort_by(|a, b| a.0.cmp(&b.0));

        // Polls: MB "Latest authorized vote state per (actor, poll) wins"; close monotonic; votes
        // later than the effective close in EventClock order do not alter the tally.
        let mut polls = Map::new();
        for (id, ev) in st.content.iter().filter(|(_, e)| e.kind == "poll.create") {
            let close = st.closes.get(id);
            let mut latest: BTreeMap<String, (OKey, Vec<String>)> = BTreeMap::new();
            for (actor, poll, key, sel) in &st.votes {
                if poll != id || close.is_some_and(|ck| key > ck) {
                    continue;
                }
                lww(&mut latest, actor.clone(), (key.clone(), sel.clone()));
            }
            let mut tally: BTreeMap<String, u64> = get_str_list(&ev.body, "options").unwrap().into_iter().map(|o| (o, 0)).collect();
            let mut votes = Map::new();
            for (actor, (_, sel)) in &latest {
                if sel.is_empty() {
                    continue; // D-17: an empty selection retracts the vote
                }
                let mut s = sel.clone();
                s.sort();
                for o in &s {
                    *tally.get_mut(o).unwrap() += 1;
                }
                votes.insert(actor.clone(), json!(s));
            }
            polls.insert(id.clone(), json!({"closed": close.is_some(), "votes": votes, "tally": tally}));
        }

        let pin_registers: BTreeMap<&String, bool> = st.pins.iter().map(|(t, (_, p))| (t, *p)).collect();
        // D-18: a pin on deleted/redacted content is kept as register state but not displayed.
        let pinned: Vec<&String> = st.pins.iter().filter(|(t, (_, p))| *p && visible(t)).map(|(t, _)| t).collect();

        let mut receipts: BTreeMap<String, BTreeMap<String, &str>> = BTreeMap::new();
        for ((e, actor), lvl) in &st.receipts {
            if st.content.contains_key(e) {
                receipts.entry(e.clone()).or_default().insert(actor.clone(), if *lvl == 2 { "read" } else { "delivered" });
            }
        }

        let mut rejected: BTreeMap<String, String> = st.rejected.clone();
        for id in &self.banned {
            rejected.insert(id.clone(), "equivocation".into());
        }
        for id in &rejected_topic_ops {
            rejected.insert(id.clone(), "unauthorized".into());
        }
        for id in &dup_topic_creates {
            rejected.insert(id.clone(), "duplicate_topic".into());
        }
        let mut dangling: Vec<String> = st.pending.values().chain(st.pending_topic.values()).flatten().map(|e| e.id.clone()).collect();
        dangling.sort();

        json!({
            "timeline": timeline.into_iter().map(|x| x.1).collect::<Vec<_>>(),
            "pins": pinned,
            "pin_registers": pin_registers,
            "edit_registers": st.edits.iter().map(|(t, (_, x))| (t.clone(), x.clone())).collect::<BTreeMap<_, _>>(),
            "reaction_registers": st.reactions.iter().filter(|((_, t, _), _)| st.content.contains_key(t))
                .map(|((a, t, k), (_, p))| json!([a, t, k, p])).collect::<Vec<_>>(),
            "reply_refs": st.content.iter().filter(|(_, e)| e.kind == "message.create").map(|(id, e)| {
                let r = |f: &str| e.body.get(f).and_then(Value::as_str).map(|x| st.content.contains_key(x));
                (id.clone(), json!({"reply_to": r("reply_to"), "thread_root": r("thread_root_event_id")}))
            }).collect::<BTreeMap<_, _>>(),
            "deadlines": deadlines,
            "expired": expired,
            "authority": self.case.control.authority(),
            "deleted": st.deleted,
            "redacted": st.redacted,
            "polls": polls,
            "topics": topics,
            "message_topics": mt.into_iter().map(|(k, v)| (k, v.1)).collect::<BTreeMap<_, _>>(),
            "receipts": receipts,
            "rejected": rejected,
            "dangling": dangling,
            "malformed_without_id": self.no_id.len(),
        })
    }
}

fn placeholder_key(raw: &Value) -> Key {
    let c = raw.get("clock").and_then(Value::as_array);
    let g = |i: usize| c.and_then(|c| c.get(i)).and_then(Value::as_u64).unwrap_or(0);
    let ep = c.and_then(|c| c.get(2)).and_then(Value::as_str).unwrap_or("").to_owned();
    (g(0), g(1), ep, raw.get("event_id").and_then(Value::as_str).unwrap_or("").to_owned())
}

/// Last-writer-wins by EventClock key (EM reaction/pin/edit: "latest authorized state wins").
fn lww<K: Ord, V>(m: &mut BTreeMap<K, (OKey, V)>, k: K, v: (OKey, V)) {
    match m.get(&k) {
        Some(cur) if cur.0 >= v.0 => {}
        _ => {
            m.insert(k, v);
        }
    }
}

pub fn case_input(case: &Value) -> CaseInput {
    CaseInput {
        conversation_id: case["conversation_id"].as_str().unwrap_or("").to_owned(),
        known_capabilities: case["known_capabilities"]
            .as_array()
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_owned)).collect())
            .unwrap_or_default(),
        control: Control::from_json(&case["control"]),
        now: case["local"]["now"].as_u64(),
        received_at: case["local"]["received_at"]
            .as_object()
            .map(|m| m.iter().filter_map(|(k, v)| v.as_u64().map(|t| (k.clone(), t))).collect())
            .unwrap_or_default(),
    }
}

pub fn materialize(ci: &CaseInput, delivery: &[&Value]) -> Value {
    let mut m = Materializer::new(ci);
    for e in delivery {
        m.ingest(e);
    }
    m.view()
}
