// SPDX-License-Identifier: MPL-2.0

use om_codec::{Registry, Value as Cbor, cbor};
use om_core::bridge_fidelity;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::json::{self, MAX_EVENT_BYTES, MAX_STRING_BYTES, ParseError};

pub const NAMESPACE: &str = "matrix";
pub const INTEROP_FIELD: &str = "org.lonunjie.om.interop.v1";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignEvent {
    pub namespace: &'static str,
    pub conversation_ref: String,
    pub event_ref: String,
    pub actor_ref: String,
    /// Plain display hint. This adapter uses the MXID fallback.
    pub actor_display: String,
    pub source_time_ms: Option<u64>,
    pub kind: EventKind,
    pub target_ref: Option<String>,
    pub content: Content,
    pub interop_metadata: Option<InteropMetadata>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventKind {
    Message,
    Reply,
    Thread,
    Edit,
    Reaction,
    Redaction,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttachmentKind {
    File,
    Image,
    Audio,
    Video,
}

impl AttachmentKind {
    pub(crate) fn msgtype(self) -> &'static str {
        match self {
            Self::File => "m.file",
            Self::Image => "m.image",
            Self::Audio => "m.audio",
            Self::Video => "m.video",
        }
    }
}

/// Inert rendering data. Media transfer and validation belong to the host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attachment {
    pub kind: AttachmentKind,
    pub name: String,
    pub caption: Option<String>,
    pub mxc: String,
    pub bytes: Option<u64>,
}

impl Attachment {
    pub(crate) fn valid(&self) -> bool {
        !self.name.is_empty()
            && self.name.len() <= 255
            && !self.name.chars().any(char::is_control)
            && self.caption.as_deref().is_none_or(body_fits)
            && valid_mxc(&self.mxc)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Content {
    Text { body: String, rich: bool },
    Attachment(Attachment),
    Reaction { key: String },
    Redaction { reason: Option<String> },
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InteropMetadata {
    pub trace: InteropTrace,
    pub fidelity_flags: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct InteropTrace {
    pub origin_namespace: String,
    pub origin_fingerprint: [u8; 32],
    pub traversed_bridge_ids: Vec<[u8; 32]>,
    pub hop_count: u64,
    pub hop_limit: u64,
    pub transformation_ids: Vec<Vec<u8>>,
}

impl InteropTrace {
    pub(crate) fn validate(&self) -> Result<(), ParseError> {
        if self.origin_namespace.len() > 128
            || self.traversed_bridge_ids.len() > 16
            || self.transformation_ids.len() > 16
            || self.transformation_ids.iter().any(|id| id.len() > 32)
        {
            return Err(ParseError::InvalidTrace);
        }
        let wrapper = map([(0, Cbor::Uint(56)), (1, Cbor::Uint(1)), (2, self.to_cbor())]);
        Registry::load_w4()
            .check_body("InteropTrace", &cbor::encode(&wrapper))
            .map_err(|_| ParseError::InvalidTrace)?;
        Ok(())
    }

    pub(crate) fn to_cbor(&self) -> Cbor {
        map([
            (0, Cbor::Text(self.origin_namespace.clone())),
            (1, Cbor::Bytes(self.origin_fingerprint.to_vec())),
            (
                2,
                Cbor::Array(
                    self.traversed_bridge_ids
                        .iter()
                        .map(|id| Cbor::Bytes(id.to_vec()))
                        .collect(),
                ),
            ),
            (3, Cbor::Uint(self.hop_count)),
            (4, Cbor::Uint(self.hop_limit)),
            (
                5,
                Cbor::Array(
                    self.transformation_ids
                        .iter()
                        .cloned()
                        .map(Cbor::Bytes)
                        .collect(),
                ),
            ),
        ])
    }
}

pub(crate) fn map(fields: impl IntoIterator<Item = (u64, Cbor)>) -> Cbor {
    Cbor::Map(
        fields
            .into_iter()
            .map(|(key, value)| (Cbor::Uint(key), value))
            .collect(),
    )
}

pub(crate) fn valid_ref(value: &str, prefix: char) -> bool {
    value.starts_with(prefix)
        && (2..=512).contains(&value.len())
        && !value.chars().any(|c| c.is_control() || c.is_whitespace())
}

fn valid_mxc(value: &str) -> bool {
    value.len() <= 2048
        && value
            .strip_prefix("mxc://")
            .and_then(|uri| uri.split_once('/'))
            .is_some_and(|(server, media)| {
                !server.is_empty()
                    && !media.is_empty()
                    && server
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b".-:[]".contains(&b))
                    && media
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            })
}

fn field<'a>(object: &'a Map<String, Value>, key: &str) -> Result<&'a str, ParseError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .ok_or(ParseError::InvalidEvent)
}

fn optional_text<'a>(
    object: &'a Map<String, Value>,
    key: &str,
) -> Result<Option<&'a str>, ParseError> {
    object
        .get(key)
        .map(|v| v.as_str().ok_or(ParseError::InvalidEvent))
        .transpose()
}

fn target(value: &str) -> Result<String, ParseError> {
    if valid_ref(value, '$') {
        Ok(value.into())
    } else {
        Err(ParseError::InvalidEvent)
    }
}

fn object(value: &Value) -> Result<&Map<String, Value>, ParseError> {
    value.as_object().ok_or(ParseError::InvalidEvent)
}

fn text(content: &Map<String, Value>) -> Result<Content, ParseError> {
    let body = field(content, "body")?;
    if !body_fits(body) {
        return Err(ParseError::InvalidEvent);
    }
    let rich = content.contains_key("formatted_body") || content.contains_key("format");
    // Foreign HTML is intentionally discarded. Only the plain Matrix body crosses the boundary.
    Ok(Content::Text {
        body: body.into(),
        rich,
    })
}

fn message(
    content: &Map<String, Value>,
) -> Result<Option<(EventKind, Option<String>, Content)>, ParseError> {
    let relation = content.get("m.relates_to").map(object).transpose()?;
    let relation_type = relation
        .map(|r| optional_text(r, "rel_type"))
        .transpose()?
        .flatten();
    if relation_type == Some("m.replace") {
        let relation = relation.ok_or(ParseError::InvalidEvent)?;
        let _fallback = field(content, "body")?;
        let edited = object(
            content
                .get("m.new_content")
                .ok_or(ParseError::InvalidEvent)?,
        )?;
        if !matches!(field(content, "msgtype")?, "m.text" | "m.notice")
            || field(content, "msgtype")? != field(edited, "msgtype")?
        {
            return Ok(None);
        }
        return Ok(Some((
            EventKind::Edit,
            Some(target(field(relation, "event_id")?)?),
            text(edited)?,
        )));
    }

    let (kind, target_ref) = match (relation_type, relation) {
        (Some("m.thread"), Some(rel)) => {
            (EventKind::Thread, Some(target(field(rel, "event_id")?)?))
        }
        (None, Some(rel)) if rel.contains_key("m.in_reply_to") => {
            let reply = object(&rel["m.in_reply_to"])?;
            (EventKind::Reply, Some(target(field(reply, "event_id")?)?))
        }
        (None, _) => (EventKind::Message, None),
        _ => return Ok(None),
    };
    let attachment_kind = match field(content, "msgtype")? {
        "m.text" | "m.notice" => return Ok(Some((kind, target_ref, text(content)?))),
        "m.file" => AttachmentKind::File,
        "m.image" => AttachmentKind::Image,
        "m.audio" => AttachmentKind::Audio,
        "m.video" => AttachmentKind::Video,
        _ => return Ok(None),
    };
    if content.contains_key("file") {
        return Err(ParseError::EncryptedEvent);
    }
    let info = content.get("info").map(object).transpose()?;
    let bytes = info
        .and_then(|info| info.get("size"))
        .map(|size| size.as_u64().ok_or(ParseError::InvalidEvent))
        .transpose()?;
    let body = field(content, "body")?;
    let filename = optional_text(content, "filename")?;
    let attachment = Attachment {
        kind: attachment_kind,
        name: filename.unwrap_or(body).into(),
        caption: filename.filter(|name| *name != body).map(|_| body.into()),
        mxc: field(content, "url")?.into(),
        bytes,
    };
    if !attachment.valid() {
        return Err(ParseError::InvalidEvent);
    }
    Ok(Some((kind, target_ref, Content::Attachment(attachment))))
}

pub(crate) fn observe_value(
    room_id: &str,
    value: &Value,
) -> Result<Option<ForeignEvent>, ParseError> {
    let event = object(value)?;
    if event.contains_key("state_key") {
        return Ok(None);
    }
    let event_type = field(event, "type")?;
    if event_type == "m.room.encrypted" {
        return Err(ParseError::EncryptedEvent);
    }
    if !matches!(
        event_type,
        "m.room.message" | "m.reaction" | "m.room.redaction"
    ) {
        return Ok(None);
    }
    if !valid_ref(room_id, '!') || optional_text(event, "room_id")?.is_some_and(|id| id != room_id)
    {
        return Err(ParseError::InvalidEvent);
    }
    let event_ref = field(event, "event_id")?;
    let sender = field(event, "sender")?;
    if !valid_ref(event_ref, '$')
        || !valid_ref(sender, '@')
        || !sender[1..]
            .split_once(':')
            .is_some_and(|(local, server)| !local.is_empty() && !server.is_empty())
    {
        return Err(ParseError::InvalidEvent);
    }
    // Already-redacted messages and reactions do not carry an application operation.
    if event_type != "m.room.redaction" && value.pointer("/unsigned/redacted_because").is_some() {
        return Ok(None);
    }
    let content = object(event.get("content").ok_or(ParseError::InvalidEvent)?)?;
    let (kind, target_ref, content_value) = match event_type {
        "m.room.message" => match message(content)? {
            Some(parts) => parts,
            None => return Ok(None),
        },
        "m.reaction" => {
            let rel = object(
                content
                    .get("m.relates_to")
                    .ok_or(ParseError::InvalidEvent)?,
            )?;
            if field(rel, "rel_type")? != "m.annotation" {
                return Ok(None);
            }
            let key = field(rel, "key")?;
            if !(1..=64).contains(&key.len()) || key.chars().any(char::is_control) {
                return Err(ParseError::InvalidEvent);
            }
            (
                EventKind::Reaction,
                Some(target(field(rel, "event_id")?)?),
                Content::Reaction { key: key.into() },
            )
        }
        "m.room.redaction" => {
            let current = optional_text(content, "redacts")?;
            let legacy = optional_text(event, "redacts")?;
            if current.zip(legacy).is_some_and(|(a, b)| a != b) {
                return Err(ParseError::InvalidEvent);
            }
            let reference = current.or(legacy).ok_or(ParseError::InvalidEvent)?;
            let reason = optional_text(content, "reason")?;
            if reason.is_some_and(|r| r.is_empty() || r.len() > 256) {
                return Err(ParseError::InvalidEvent);
            }
            (
                EventKind::Redaction,
                Some(target(reference)?),
                Content::Redaction {
                    reason: reason.map(str::to_owned),
                },
            )
        }
        _ => return Ok(None),
    };
    let interop_metadata = content
        .get(INTEROP_FIELD)
        .map(|raw| {
            let meta: InteropMetadata =
                serde_json::from_value(raw.clone()).map_err(|_| ParseError::InvalidTrace)?;
            if !bridge_fidelity::flags_valid(&meta.fidelity_flags) {
                return Err(ParseError::InvalidTrace);
            }
            meta.trace.validate()?;
            Ok(meta)
        })
        .transpose()?;
    let source_time_ms = event
        .get("origin_server_ts")
        .map(|v| v.as_u64().ok_or(ParseError::InvalidEvent))
        .transpose()?;
    Ok(Some(ForeignEvent {
        namespace: NAMESPACE,
        conversation_ref: room_id.into(),
        event_ref: event_ref.into(),
        actor_ref: sender.into(),
        actor_display: sender.into(),
        source_time_ms,
        kind,
        target_ref,
        content: content_value,
        interop_metadata,
    }))
}

/// Observe one event in the enclosing Matrix room. An irrelevant event returns `Ok(None)`.
pub fn observe(room_id: &str, bytes: &[u8]) -> Result<Option<ForeignEvent>, ParseError> {
    observe_value(room_id, &json::parse(bytes, MAX_EVENT_BYTES)?)
}

pub(crate) fn body_fits(body: &str) -> bool {
    body.len() <= MAX_STRING_BYTES && !body.contains('\0')
}
