// SPDX-License-Identifier: MPL-2.0
//! Canonical 1:1 `ConversationEvent`s (object type 10, spec/cddl/events.cddl, EVENT_MODEL.md). An event is not
//! separately signed: the 1:1 session AEAD authenticates it. Its canonical bytes are what the ratchet carries, what
//! dedupe hashes and what multi-device sync and archive will replay, so they are built and checked in one place.

use crate::crypto::cbor::{self, V};
use crate::crypto::sha256;
use crate::state::SecretHex;
use serde::{Deserialize, Serialize};

pub const PROTOCOL_VERSION: u64 = 1;
pub const KIND_VERSION: u64 = 1;
/// The inner-payload discriminator of an event is its object type (map key 0 = 10).
pub const OBJECT_TYPE: u64 = 10;

pub const K_CREATE: u64 = 1;
pub const K_EDIT: u64 = 2;
pub const K_DELETE: u64 = 3;
pub const K_REACTION: u64 = 4;
pub const K_RECEIPT: u64 = 5;
pub const K_POLL_CREATE: u64 = 8;
pub const K_POLL_VOTE: u64 = 9;
pub const K_POLL_CLOSE: u64 = 10;

/// Bounds of a poll (ADR-0223).
pub const POLL_MIN_OPTIONS: usize = 2;
pub const POLL_MAX_OPTIONS: usize = 12;
pub const POLL_MAX_LABEL: usize = 80;
pub const POLL_MAX_QUESTION: usize = 256;

/// Principal kind 1 = account.
const PRINCIPAL_ACCOUNT: u64 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Hlc {
    pub physical_ms: u64,
    pub logical: u32,
}

impl Hlc {
    /// Next clock value for a local event: strictly after `prev` and not behind the wall clock.
    pub fn next(prev: Hlc, now_ms: u64) -> Hlc {
        if now_ms > prev.physical_ms {
            Hlc {
                physical_ms: now_ms,
                logical: 0,
            }
        } else {
            Hlc {
                physical_ms: prev.physical_ms,
                logical: prev.logical.saturating_add(1),
            }
        }
    }
}

/// RichTextV1 for plain text: one paragraph block holding one text inline.
pub fn rich_text(text: &str) -> V {
    V::M(vec![
        (0, V::U(1)),
        (
            1,
            V::A(vec![V::M(vec![
                (0, V::U(1)),
                (
                    1,
                    V::A(vec![V::M(vec![(0, V::U(1)), (1, V::T(text.to_string()))])]),
                ),
            ])]),
        ),
    ])
}

/// Plain text of a RichTextV1 value. Only semantic data is read: links and mentions contribute their label text and
/// nothing is ever interpreted as markup. Blocks are joined with newlines.
pub fn plain_text(rt: &V) -> Option<String> {
    fn inlines(v: &V, out: &mut String, depth: u32) -> Option<()> {
        if depth > 6 {
            return None;
        }
        for i in v.a().ok()? {
            match i.get(0).ok()?.u().ok()? {
                1 => out.push_str(i.get(1).ok()?.t().ok()?),
                2 | 3 => inlines(i.get(4).ok()?, out, depth + 1)?,
                _ => return None,
            }
        }
        Some(())
    }
    if rt.get(0).ok()?.u().ok()? != 1 {
        return None;
    }
    let mut parts = vec![];
    for b in rt.get(1).ok()?.a().ok()? {
        let mut s = String::new();
        match b.get(0).ok()?.u().ok()? {
            1 | 2 => inlines(b.get(1).ok()?, &mut s, 0)?,
            3 => s.push_str(b.get(2).ok()?.t().ok()?),
            _ => return None,
        }
        parts.push(s);
    }
    Some(parts.join("\n"))
}

/// A parsed `PollCreate` body (group.cddl): options are `(option_id, label)`; visibility 1 immediate, 2 after_vote, 3 after_close.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PollSpec {
    pub question: String,
    pub options: Vec<(u8, String)>,
    pub multiple: bool,
    pub max_selections: Option<u8>,
    pub visibility: u8,
}

/// The `PollCreate` body for a validated spec (keys 0 question RichText, 1 options, 2 mode, 3 max_selections, 4 visibility).
pub fn poll_create_body(p: &PollSpec) -> V {
    let mut m = vec![
        (0, rich_text(&p.question)),
        (
            1,
            V::A(
                p.options
                    .iter()
                    .map(|(id, l)| V::M(vec![(0, V::U(u64::from(*id))), (1, V::T(l.clone()))]))
                    .collect(),
            ),
        ),
        (2, V::U(if p.multiple { 2 } else { 1 })),
    ];
    if let Some(n) = p.max_selections {
        m.push((3, V::U(u64::from(n))));
    }
    m.push((4, V::U(u64::from(p.visibility))));
    V::M(m)
}

/// Read a `PollCreate` body under the ADR-0223 bounds; `None` for anything that breaks one (the event is then dropped).
pub fn parse_poll_create(body: &V) -> Option<PollSpec> {
    let question = plain_text(body.get(0).ok()?)?;
    if question.trim().is_empty() || question.chars().count() > POLL_MAX_QUESTION {
        return None;
    }
    let mut options = vec![];
    for o in body.get(1).ok()?.a().ok()? {
        let id = u8::try_from(o.get(0).ok()?.u().ok()?).ok()?;
        let label = o.get(1).ok()?.t().ok()?.to_string();
        if label.trim().is_empty()
            || label.chars().count() > POLL_MAX_LABEL
            || options
                .iter()
                .any(|(i, l): &(u8, String)| *i == id || *l == label)
        {
            return None;
        }
        options.push((id, label));
    }
    if !(POLL_MIN_OPTIONS..=POLL_MAX_OPTIONS).contains(&options.len()) {
        return None;
    }
    let multiple = match body.get(2).ok()?.u().ok()? {
        1 => false,
        2 => true,
        _ => return None,
    };
    let max_selections = match body.opt(3) {
        None => None,
        Some(n) => {
            let n = u8::try_from(n.u().ok()?).ok()?;
            if !multiple || n < 2 || usize::from(n) > options.len() {
                return None;
            }
            Some(n)
        }
    };
    let visibility = u8::try_from(body.get(4).ok()?.u().ok()?)
        .ok()
        .filter(|v| (1..=3).contains(v))?;
    Some(PollSpec {
        question,
        options,
        multiple,
        max_selections,
        visibility,
    })
}

/// The `PollVote` body: the poll's EventID and the chosen option ids (empty takes the vote back).
pub fn poll_vote_body(poll: [u8; 16], selected: &[u8]) -> V {
    V::M(vec![
        (0, V::B(poll.to_vec())),
        (
            1,
            V::A(selected.iter().map(|s| V::U(u64::from(*s))).collect()),
        ),
    ])
}

/// The `PollClose` body.
pub fn poll_close_body(poll: [u8; 16]) -> V {
    V::M(vec![(0, V::B(poll.to_vec()))])
}

/// Most mentioned accounts in one message (`MessageCreate` key 3 is `[1*64 principal-ref]`).
pub const MAX_MENTIONS: usize = 64;

/// One mention inside the plain text of a message: the half-open character range `[start, end)` that is the label, and the
/// mentioned account (ADR-0199).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MentionSpan {
    pub start: usize,
    pub end: usize,
    pub account: [u8; 32],
}

fn principal_ref(account: &[u8; 32]) -> V {
    V::M(vec![
        (0, V::U(PRINCIPAL_ACCOUNT)),
        (1, V::B(account.to_vec())),
    ])
}

fn text_inline(t: &str) -> V {
    V::M(vec![(0, V::U(1)), (1, V::T(t.to_string()))])
}

/// Spans that can be written: non-empty, in bounds (of `text`, by characters), sorted and not overlapping, at most
/// [`MAX_MENTIONS`] of them.
pub fn mention_spans_valid(text: &str, spans: &[MentionSpan]) -> bool {
    let n = text.chars().count();
    let mut at = 0usize;
    spans.len() <= MAX_MENTIONS
        && spans.iter().all(|s| {
            let ok = s.start >= at && s.end > s.start && s.end <= n;
            at = s.end;
            ok
        })
}

/// RichTextV1 for `text` with a mention inline over each span: one paragraph; each mention's label is the text it covers,
/// so the plain-text fallback is exactly `text` (an older client shows the same words, ADR-0150). `None` when the spans
/// are not [`mention_spans_valid`].
pub fn rich_text_with_mentions(text: &str, spans: &[MentionSpan]) -> Option<V> {
    if !mention_spans_valid(text, spans) {
        return None;
    }
    let chars: Vec<char> = text.chars().collect();
    let slice = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mut inlines = vec![];
    let mut at = 0usize;
    for s in spans {
        if s.start > at {
            inlines.push(text_inline(&slice(at, s.start)));
        }
        inlines.push(V::M(vec![
            (0, V::U(3)),
            (4, V::A(vec![text_inline(&slice(s.start, s.end))])),
            (5, principal_ref(&s.account)),
        ]));
        at = s.end;
    }
    if at < chars.len() || inlines.is_empty() {
        inlines.push(text_inline(&slice(at, chars.len())));
    }
    Some(V::M(vec![
        (0, V::U(1)),
        (1, V::A(vec![V::M(vec![(0, V::U(1)), (1, V::A(inlines))])])),
    ]))
}

/// The mention set (key 3 of `MessageCreate`): each mentioned account once, in byte order.
pub fn mention_set(spans: &[MentionSpan]) -> V {
    let mut a: Vec<[u8; 32]> = spans.iter().map(|s| s.account).collect();
    a.sort();
    a.dedup();
    V::A(a.iter().map(principal_ref).collect())
}

/// The mentions of a received `message.create` body, as spans over [`plain_text`] of its content. The set (key 3) must
/// agree with the inline mentions exactly (MESSENGER_BASELINE_V1: a denormalized index may not disagree with content); if
/// it does not, or a mention is empty, nested, or over the bound, the mentions are ignored and the text is shown as
/// written. Never a reason to refuse the message.
pub fn mention_spans(body: &V) -> Vec<MentionSpan> {
    fn walk(v: &V, pos: &mut usize, out: &mut Vec<MentionSpan>, in_link: bool) -> Option<()> {
        for i in v.a().ok()? {
            match i.get(0).ok()?.u().ok()? {
                1 => *pos += i.get(1).ok()?.t().ok()?.chars().count(),
                2 => walk(i.get(4).ok()?, pos, out, true)?,
                3 => {
                    if in_link {
                        return None;
                    }
                    let start = *pos;
                    walk(i.get(4).ok()?, pos, out, true)?;
                    let p = i.get(5).ok()?;
                    if p.get(0).ok()?.u().ok()? != PRINCIPAL_ACCOUNT {
                        return None;
                    }
                    out.push(MentionSpan {
                        start,
                        end: *pos,
                        account: p.get(1).ok()?.b32().ok()?,
                    });
                }
                _ => return None,
            }
        }
        Some(())
    }
    let mut out = vec![];
    let mut pos = 0usize;
    let blocks = body
        .opt(0)
        .and_then(|rt| rt.get(1).ok())
        .and_then(|b| b.a().ok());
    let mut ok = true;
    for (n, b) in blocks.into_iter().flatten().enumerate() {
        if n > 0 {
            pos += 1;
        }
        let kind = b.get(0).ok().and_then(|k| k.u().ok());
        let step = match kind {
            Some(1 | 2) => b
                .get(1)
                .ok()
                .and_then(|i| walk(i, &mut pos, &mut out, false)),
            Some(3) => b
                .get(2)
                .ok()
                .and_then(|t| t.t().ok())
                .map(|t| pos += t.chars().count()),
            _ => None,
        };
        if step.is_none() {
            ok = false;
            break;
        }
    }
    let declared: Vec<[u8; 32]> = match body.opt(3) {
        None => vec![],
        Some(a) => a
            .a()
            .ok()
            .map(|a| {
                a.iter()
                    .filter_map(|p| {
                        (p.get(0).ok()?.u().ok()? == PRINCIPAL_ACCOUNT)
                            .then(|| p.get(1).ok()?.b32().ok())
                            .flatten()
                    })
                    .collect()
            })
            .unwrap_or_default(),
    };
    let mut inline: Vec<[u8; 32]> = out.iter().map(|s| s.account).collect();
    inline.sort();
    inline.dedup();
    let mut sorted = declared.clone();
    sorted.sort();
    sorted.dedup();
    if !ok
        || out.len() > MAX_MENTIONS
        || out.iter().any(|s| s.end <= s.start)
        || sorted != inline
        || declared.len() != sorted.len()
    {
        return vec![];
    }
    out
}

pub struct Header {
    pub event_id: [u8; 16],
    pub conversation_id: [u8; 32],
    pub author_account: [u8; 32],
    pub author_device: [u8; 16],
    pub clock: Hlc,
    pub kind: u64,
    pub body: V,
}

impl Header {
    /// Canonical W0 bytes. `endpoint_tiebreak` is the authenticated endpoint (device id), per ADR-0121.
    pub fn encode(&self) -> Vec<u8> {
        self.encode_with(None)
    }

    /// As [`Self::encode`], with the key-23 `extensions` map (never empty; `None` omits the key).
    pub fn encode_with(&self, extensions: Option<V>) -> Vec<u8> {
        self.encode_full(extensions, None)
    }

    /// As [`Self::encode_with`], with the key-10 `required_capabilities` set (kinds 15 to 17 carry `["om.group.v1"]`,
    /// ADR-0189 section 1.1). Written in key order, so the bytes stay canonical.
    pub fn encode_full(&self, extensions: Option<V>, required_capabilities: Option<V>) -> Vec<u8> {
        let mut m = vec![
            (0, V::U(OBJECT_TYPE)),
            (1, V::U(1)),
            (2, V::U(PROTOCOL_VERSION)),
            (3, V::B(self.event_id.to_vec())),
            (4, V::B(self.conversation_id.to_vec())),
            (
                5,
                V::M(vec![
                    (0, V::U(PRINCIPAL_ACCOUNT)),
                    (1, V::B(self.author_account.to_vec())),
                ]),
            ),
            (6, V::B(self.author_device.to_vec())),
            (
                7,
                V::M(vec![
                    (0, V::U(self.clock.physical_ms)),
                    (1, V::U(self.clock.logical as u64)),
                    (2, V::B(self.author_device.to_vec())),
                ]),
            ),
            (8, V::U(self.kind)),
            (9, V::U(KIND_VERSION)),
        ];
        if let Some(c) = required_capabilities {
            m.push((10, c));
        }
        m.push((11, self.body.clone()));
        if let Some(x) = extensions {
            m.push((23, x));
        }
        cbor::encode(&V::M(m))
    }
}

pub fn message_create_body(text: &str) -> V {
    V::M(vec![(0, rich_text(text)), (1, V::A(vec![]))])
}

/// A `message.create` body with optional content and attachment references (inline bytes are never possible: the only
/// carrier of attachment content is an `attachment-ref`).
pub fn message_create_body_with(text: Option<&str>, attachments: Vec<V>) -> V {
    message_create_body_reply(text, attachments, None)
}

/// As [`message_create_body_with`], with `reply_to` (key 2): the EventID of the message this one answers. Keys are
/// written in order (0, 1, 2), so the bytes stay canonical.
pub fn message_create_body_reply(
    text: Option<&str>,
    attachments: Vec<V>,
    reply_to: Option<[u8; 16]>,
) -> V {
    message_create_body_mentions(text, attachments, reply_to, &[])
}

/// As [`message_create_body_reply`], with mentions: the text carries mention inlines and key 3 the set. Spans that are not
/// [`mention_spans_valid`] are left out (the text then goes out plain); callers validate first.
pub fn message_create_body_mentions(
    text: Option<&str>,
    attachments: Vec<V>,
    reply_to: Option<[u8; 16]>,
    mentions: &[MentionSpan],
) -> V {
    let mut m = vec![];
    let mut set = None;
    if let Some(t) = text {
        match rich_text_with_mentions(t, mentions).filter(|_| !mentions.is_empty()) {
            Some(rt) => {
                m.push((0, rt));
                set = Some(mention_set(mentions));
            }
            None => m.push((0, rich_text(t))),
        }
    }
    m.push((1, V::A(attachments)));
    if let Some(r) = reply_to {
        m.push((2, V::B(r.to_vec())));
    }
    if let Some(s) = set {
        m.push((3, s));
    }
    V::M(m)
}

/// A parsed, structurally valid inbound event. Authorization (author, conversation) is checked by the caller.
#[derive(Clone, Debug)]
pub struct Event {
    pub event_id: [u8; 16],
    pub conversation_id: [u8; 32],
    pub author_account: [u8; 32],
    pub author_device: [u8; 16],
    pub clock: Hlc,
    pub kind: u64,
    pub body: V,
    /// SHA-256 of the exact canonical bytes (equivocation detection).
    pub digest: [u8; 32],
    /// Key 23 as received (registered presentation extensions are read from it by [`crate::present`]).
    pub extensions: Option<V>,
}

pub(crate) fn mutation_target(ev: &Event) -> Option<[u8; 16]> {
    if !matches!(ev.kind, K_EDIT | K_DELETE | K_REACTION) {
        return None;
    }
    ev.body.get(0).ok()?.b().ok()?.try_into().ok()
}

const DANGLING_TTL_MS: u64 = crate::flows::WRAPPER_TTL_S * 1000;
const MAX_DANGLING: usize = 64;
const MAX_DANGLING_PER_CONVERSATION: usize = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum MutationAuth {
    Direct {
        mine: bool,
    },
    Room {
        epoch: u64,
        mine: bool,
        post: bool,
        reply: bool,
        react: bool,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct WaitingMutation {
    conversation: String,
    event: [u8; 16],
    target: [u8; 16],
    auth: MutationAuth,
    bytes: Option<SecretHex>,
    seen_at_ms: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DanglingMutations(Vec<WaitingMutation>);

impl DanglingMutations {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn expire(&mut self, now_ms: u64) {
        self.0
            .retain(|m| now_ms.saturating_sub(m.seen_at_ms) < DANGLING_TTL_MS);
    }

    pub(crate) fn hold(
        &mut self,
        conversation: &str,
        target: [u8; 16],
        ev: &Event,
        auth: MutationAuth,
        bytes: &[u8],
        now_ms: u64,
    ) {
        self.expire(now_ms);
        let room = matches!(auth, MutationAuth::Room { .. });
        let same = |entry: &WaitingMutation| {
            entry.conversation == conversation
                && matches!(entry.auth, MutationAuth::Room { .. }) == room
        };
        let encoded = crate::crypto::hex(bytes);
        if let Some(entry) = self
            .0
            .iter_mut()
            .find(|entry| same(entry) && entry.event == ev.event_id)
        {
            if entry.bytes.as_deref().is_some_and(|held| held != encoded) {
                entry.bytes = None;
            }
            return;
        }
        if bytes.len() > 65_536
            || self.0.len() >= MAX_DANGLING
            || self.0.iter().filter(|entry| same(entry)).count() >= MAX_DANGLING_PER_CONVERSATION
        {
            return;
        }
        self.0.push(WaitingMutation {
            conversation: conversation.into(),
            event: ev.event_id,
            target,
            auth,
            bytes: Some(encoded.into()),
            seen_at_ms: now_ms,
        });
    }

    pub(crate) fn take(
        &mut self,
        conversation: &str,
        target: [u8; 16],
        room: bool,
        now_ms: u64,
    ) -> Vec<(Event, MutationAuth)> {
        self.expire(now_ms);
        let mut ready = vec![];
        self.0.retain(|entry| {
            if entry.conversation != conversation
                || entry.target != target
                || matches!(entry.auth, MutationAuth::Room { .. }) != room
            {
                return true;
            }
            if let Some(ev) = entry
                .bytes
                .as_deref()
                .and_then(|b| crate::crypto::unhex(b).ok())
                .and_then(|bytes| parse(&bytes))
            {
                ready.push((ev, entry.auth));
            }
            false
        });
        ready
    }
}

/// Parse after the W0 registry check has passed. Returns `None` for anything the 1:1 profile does not accept.
pub fn parse(bytes: &[u8]) -> Option<Event> {
    parse_with(bytes, None)
}

/// Parse a relationship-lane event with the exact required capability for its kind.
pub(crate) fn parse_group_lane(bytes: &[u8], capability: &str) -> Option<Event> {
    parse_with(bytes, Some(capability))
}

fn parse_with(bytes: &[u8], capability: Option<&str>) -> Option<Event> {
    let v = cbor::decode(bytes).ok()?;
    if v.get(0).ok()?.u().ok()? != OBJECT_TYPE || v.get(2).ok()?.u().ok()? != PROTOCOL_VERSION {
        return None;
    }
    // A required capability or a kind version this client does not implement is refused, never half-applied. Key 23 is
    // accepted: the W0 registry check has already enforced every registered extension's owner, criticality and size, and
    // `present::parse_extensions` refuses an unknown critical entry (ADR-0155).
    let caps_ok = match v.opt(10) {
        None => capability.is_none(),
        Some(c) => matches!(c.a().ok()?, [x] if Some(x.t().ok()?) == capability),
    };
    if !caps_ok || v.get(9).ok()?.u().ok()? != KIND_VERSION {
        return None;
    }
    let extensions = match v.opt(23) {
        None => None,
        Some(x) => {
            crate::present::parse_extensions(Some(x))?;
            Some(x.clone())
        }
    };
    let principal = v.get(5).ok()?;
    if principal.get(0).ok()?.u().ok()? != PRINCIPAL_ACCOUNT {
        return None;
    }
    let clock = v.get(7).ok()?;
    let device: [u8; 16] = v.get(6).ok()?.b().ok()?.try_into().ok()?;
    // The clock's tiebreak must equal the authenticated endpoint (ADR-0121); it is never authority.
    if clock.get(2).ok()?.b().ok()? != device {
        return None;
    }
    Some(Event {
        event_id: v.get(3).ok()?.b().ok()?.try_into().ok()?,
        conversation_id: v.get(4).ok()?.b32().ok()?,
        author_account: principal.get(1).ok()?.b32().ok()?,
        author_device: device,
        clock: Hlc {
            physical_ms: clock.get(0).ok()?.u().ok()?,
            logical: clock.get(1).ok()?.u32().ok()?,
        },
        kind: v.get(8).ok()?.u().ok()?,
        body: v.get(11).ok()?.clone(),
        digest: sha256(&[bytes]),
        extensions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mentions_round_trip_and_disagreeing_sets_are_ignored() {
        let (a, b) = ([1u8; 32], [2u8; 32]);
        let text = "hi @Ann and @Bo!";
        let spans = vec![
            MentionSpan {
                start: 3,
                end: 7,
                account: a,
            },
            MentionSpan {
                start: 12,
                end: 15,
                account: b,
            },
        ];
        let body = message_create_body_mentions(Some(text), vec![], None, &spans);
        assert_eq!(plain_text(body.get(0).unwrap()).as_deref(), Some(text));
        assert_eq!(mention_spans(&body), spans);
        // key 3 lists an account the text does not mention: ignored, text unchanged
        let mut m = match body.clone() {
            V::M(m) => m,
            _ => unreachable!(),
        };
        for (k, v) in m.iter_mut() {
            if *k == 3 {
                *v = V::A(vec![principal_ref(&a)]);
            }
        }
        assert!(mention_spans(&V::M(m)).is_empty());
        // missing set, overlapping and out-of-range spans
        assert!(mention_spans(&message_create_body(text)).is_empty());
        assert!(!mention_spans_valid(
            text,
            &[MentionSpan {
                start: 3,
                end: 20,
                account: a
            }]
        ));
        assert!(!mention_spans_valid(
            text,
            &[spans[1].clone(), spans[0].clone()]
        ));
        assert!(!mention_spans_valid(
            text,
            &[MentionSpan {
                start: 3,
                end: 3,
                account: a
            }]
        ));
        assert_eq!(
            message_create_body_mentions(Some(text), vec![], None, &[]),
            message_create_body(text)
        );
    }

    #[test]
    fn plain_text_round_trips_and_event_bytes_are_w0_valid() {
        let h = Header {
            event_id: [1; 16],
            conversation_id: [2; 32],
            author_account: [3; 32],
            author_device: [4; 16],
            clock: Hlc {
                physical_ms: 1_790_000_000_000,
                logical: 7,
            },
            kind: K_CREATE,
            body: message_create_body("héllo\nworld"),
        };
        let bytes = h.encode();
        om_codec::Registry::load()
            .check("ConversationEvent", &bytes, false)
            .expect("W0-valid ConversationEvent");
        let e = parse(&bytes).unwrap();
        assert_eq!(e.clock.logical, 7);
        assert_eq!(
            plain_text(e.body.get(0).unwrap()).as_deref(),
            Some("héllo\nworld")
        );
    }

    #[test]
    fn attachments_and_presentation_extensions_are_w0_valid_and_parse() {
        use crate::attach::AttachmentRef;
        use crate::present::{
            Extensions, Location, Voice, extensions_value, geo_token, parse_extensions,
        };
        let r = AttachmentRef {
            blob_id: [5; 32],
            media_type: "audio/ogg".into(),
            plaintext_size: 100,
            ciphertext_digest: [6; 32],
            key: [7; 32],
            locations: vec!["https://home.example/v1/blobs/ab".into()],
            filename: None,
            width: None,
            height: None,
            duration_ms: Some(900),
            read_cap: Some(vec![8; 48]),
        };
        let loc = Location {
            lat_e7: 107_202_000,
            lon_e7: 1_225_621_000,
            accuracy_m: Some(10),
            label: Some("Home".into()),
        };
        let ext = Extensions {
            voice: Some(Voice {
                attachment_index: 0,
                waveform: Some(vec![3; 20]),
            }),
            location: Some(loc.clone()),
        };
        let text = format!("here {}", geo_token(loc.lat_e7, loc.lon_e7));
        let h = Header {
            event_id: [1; 16],
            conversation_id: [2; 32],
            author_account: [3; 32],
            author_device: [4; 16],
            clock: Hlc {
                physical_ms: 1_790_000_000_000,
                logical: 0,
            },
            kind: K_CREATE,
            body: message_create_body_with(Some(&text), vec![r.to_v()]),
        };
        let bytes = h.encode_with(extensions_value(&ext));
        om_codec::Registry::load()
            .check("ConversationEvent", &bytes, false)
            .expect("W0-valid ConversationEvent with attachment + key 23");
        let e = parse(&bytes).unwrap();
        assert_eq!(parse_extensions(e.extensions.as_ref()).unwrap(), ext);
        let back = AttachmentRef::from_v(&e.body.get(1).unwrap().a().unwrap()[0]).unwrap();
        assert_eq!(back, r);
        // an attachment-only message (no content key) is valid too
        let only = Header {
            body: message_create_body_with(None, vec![r.to_v()]),
            ..h
        }
        .encode();
        om_codec::Registry::load()
            .check("ConversationEvent", &only, false)
            .expect("attachment-only");
    }

    #[test]
    fn hlc_is_monotonic_even_when_the_wall_clock_steps_back() {
        let a = Hlc::next(
            Hlc {
                physical_ms: 0,
                logical: 0,
            },
            100,
        );
        let b = Hlc::next(a, 50);
        let c = Hlc::next(b, 100);
        assert!(a < b && b < c);
    }
}
