// SPDX-License-Identifier: MPL-2.0

use std::fmt::Write;
use std::net::Ipv6Addr;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::events::{ForeignEvent, observe_value, valid_ref};
use crate::json::{self, MAX_EVENT_BYTES, MAX_STRING_BYTES, MAX_SYNC_BYTES, ParseError};

const API: &str = "/_matrix/client/v3";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApiError {
    InvalidBaseUrl,
    InvalidArgument,
    HttpStatus(u16),
    Response(ParseError),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Endpoint {
    base_url: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub enum Method {
    Get,
    Put,
}

/// Data for the caller's HTTP transport. Attach Matrix authorization out of band.
#[derive(Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct Request {
    pub method: Method,
    pub url: String,
    /// PUT requests use application/json.
    pub body: Vec<u8>,
}

impl Endpoint {
    /// A configured HTTPS origin, with an optional trailing slash.
    pub fn new(base_url: &str) -> Result<Self, ApiError> {
        let invalid = ApiError::InvalidBaseUrl;
        let authority = base_url.strip_prefix("https://").ok_or(invalid)?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        if authority.is_empty() || authority.len() > 512 {
            return Err(invalid);
        }
        let port = if let Some(ip) = authority.strip_prefix('[') {
            let (ip, rest) = ip.split_once(']').ok_or(invalid)?;
            ip.parse::<Ipv6Addr>().map_err(|_| invalid)?;
            rest
        } else {
            let (host, rest) = authority.split_at(authority.find(':').unwrap_or(authority.len()));
            if host.is_empty()
                || !host.as_bytes()[0].is_ascii_alphanumeric()
                || !host
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
            {
                return Err(invalid);
            }
            rest
        };
        if !port.is_empty() {
            let port = port.strip_prefix(':').ok_or(invalid)?;
            if !port.bytes().all(|b| b.is_ascii_digit())
                || !port.parse::<u16>().is_ok_and(|n| n > 0)
            {
                return Err(invalid);
            }
        }
        Ok(Self {
            base_url: format!("https://{authority}"),
        })
    }

    pub(crate) fn owns_request(&self, request: &Request) -> bool {
        request.method == Method::Put
            && request
                .url
                .strip_prefix(&self.base_url)
                .is_some_and(|path| path.starts_with("/_matrix/client/v3/rooms/"))
    }

    pub fn sync_request(&self, since: Option<&str>) -> Result<Request, ApiError> {
        let mut url = format!("{}{API}/sync", self.base_url);
        if let Some(since) = since {
            if since.is_empty() || since.len() > MAX_STRING_BYTES {
                return Err(ApiError::InvalidArgument);
            }
            url.push_str("?since=");
            url.push_str(&encode_component(since));
        }
        Ok(Request {
            method: Method::Get,
            url,
            body: Vec::new(),
        })
    }

    pub(crate) fn send_request(
        &self,
        room_id: &str,
        event_type: &str,
        txn: &str,
        body: Vec<u8>,
    ) -> Request {
        let room = encode_component(room_id);
        let kind = encode_component(event_type);
        Request {
            method: Method::Put,
            url: format!("{}{API}/rooms/{room}/send/{kind}/{txn}", self.base_url),
            body,
        }
    }

    pub(crate) fn redact_request(
        &self,
        room_id: &str,
        event_id: &str,
        txn: &str,
        body: Vec<u8>,
    ) -> Request {
        let room = encode_component(room_id);
        let target = encode_component(event_id);
        Request {
            method: Method::Put,
            url: format!("{}{API}/rooms/{room}/redact/{target}/{txn}", self.base_url),
            body,
        }
    }
}

fn encode_component(value: &str) -> String {
    let mut encoded = String::new();
    for b in value.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            encoded.push(char::from(b));
        } else {
            // Writing to String cannot fail.
            let _ = write!(encoded, "%{b:02X}");
        }
    }
    encoded
}

#[derive(Debug, PartialEq, Eq)]
pub struct SyncBatch {
    pub next_batch: String,
    /// A limited timeline can have a gap. The caller decides whether to advance its cursor.
    pub limited: bool,
    pub events: Vec<Result<ForeignEvent, ParseError>>,
}

fn success(status: u16, bytes: &[u8], cap: usize) -> Result<Value, ApiError> {
    if status != 200 {
        return Err(ApiError::HttpStatus(status));
    }
    let value = json::parse(bytes, cap).map_err(ApiError::Response)?;
    if !value.is_object() || value.get("errcode").is_some() {
        return Err(ApiError::Response(ParseError::InvalidResponse));
    }
    Ok(value)
}

/// Parses just the joined-room timeline selected by the authenticated binding.
/// An invalid event is reported in place. Malformed JSON or a malformed envelope refuses the batch.
pub fn parse_sync_response(
    status: u16,
    bytes: &[u8],
    room_id: &str,
) -> Result<SyncBatch, ApiError> {
    if !valid_ref(room_id, '!') {
        return Err(ApiError::InvalidArgument);
    }
    let invalid = ApiError::Response(ParseError::InvalidResponse);
    let value = success(status, bytes, MAX_SYNC_BYTES)?;
    let next_batch = value
        .get("next_batch")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or(invalid)?
        .to_owned();
    let mut batch = SyncBatch {
        next_batch,
        limited: false,
        events: Vec::new(),
    };
    let mut scope = &value;
    for key in ["rooms", "join", room_id, "timeline"] {
        let Some(found) = scope.get(key) else {
            return Ok(batch);
        };
        found.as_object().ok_or(invalid)?;
        scope = found;
    }
    batch.limited = scope
        .get("limited")
        .map(|v| v.as_bool().ok_or(invalid))
        .transpose()?
        .unwrap_or(false);
    let events = scope
        .get("events")
        .and_then(Value::as_array)
        .ok_or(invalid)?;
    for value in events {
        let within_limit =
            serde_json::to_vec(value).is_ok_and(|bytes| bytes.len() <= MAX_EVENT_BYTES);
        let result = if within_limit {
            observe_value(room_id, value)
        } else {
            Err(ParseError::ByteLimit)
        };
        match result {
            Ok(Some(event)) => batch.events.push(Ok(event)),
            Ok(None) => {}
            Err(error) => batch.events.push(Err(error)),
        }
    }
    Ok(batch)
}

/// The response body shape is shared by Matrix send and redact.
pub fn parse_event_response(status: u16, bytes: &[u8]) -> Result<String, ApiError> {
    let value = success(status, bytes, MAX_EVENT_BYTES)?;
    let reference = value
        .get("event_id")
        .and_then(Value::as_str)
        .filter(|id| valid_ref(id, '$'))
        .ok_or(ApiError::Response(ParseError::InvalidResponse))?;
    Ok(reference.to_owned())
}
