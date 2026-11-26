// SPDX-License-Identifier: MPL-2.0

use std::cell::Cell;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Number, Value};

/// Byte limits apply to decompressed input.
pub const MAX_EVENT_BYTES: usize = 64 * 1024;
pub const MAX_SYNC_BYTES: usize = 1024 * 1024;
pub const MAX_JSON_DEPTH: usize = 24;
pub const MAX_STRING_BYTES: usize = 16 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParseError {
    ByteLimit,
    DepthLimit,
    StringLimit,
    DuplicateKey,
    InvalidJson,
    InvalidEvent,
    InvalidTrace,
    InvalidResponse,
    /// The reference adapter does not handle Matrix E2EE.
    EncryptedEvent,
}

#[derive(Clone, Copy)]
struct Bounded<'a> {
    depth: usize,
    refusal: &'a Cell<Option<ParseError>>,
}

impl Bounded<'_> {
    fn refuse<E: de::Error>(self, reason: ParseError) -> E {
        self.refusal.set(Some(reason));
        E::custom("Matrix JSON refused")
    }

    fn child(self) -> Self {
        Self {
            depth: self.depth + 1,
            ..self
        }
    }
}

impl<'de> DeserializeSeed<'de> for Bounded<'_> {
    type Value = Value;

    fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if self.depth > MAX_JSON_DEPTH {
            return Err(self.refuse(ParseError::DepthLimit));
        }
        d.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Bounded<'_> {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a bounded JSON value")
    }

    fn visit_unit<E: de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Value, E> {
        Number::from_f64(value)
            .map(Value::Number)
            .ok_or_else(|| self.refuse(ParseError::InvalidJson))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Value, E> {
        if value.len() > MAX_STRING_BYTES {
            return Err(self.refuse(ParseError::StringLimit));
        }
        Ok(Value::String(value.into()))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Value, A::Error> {
        let mut items = Vec::new();
        while let Some(item) = seq.next_element_seed(self.child())? {
            items.push(item);
        }
        Ok(Value::Array(items))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Value, A::Error> {
        let mut entries = Map::new();
        while let Some(key) = map.next_key::<String>()? {
            if key.len() > MAX_STRING_BYTES {
                return Err(self.refuse(ParseError::StringLimit));
            }
            if entries.contains_key(&key) {
                return Err(self.refuse(ParseError::DuplicateKey));
            }
            entries.insert(key, map.next_value_seed(self.child())?);
        }
        Ok(Value::Object(entries))
    }
}

pub(crate) fn parse(input: &[u8], max_bytes: usize) -> Result<Value, ParseError> {
    if input.len() > max_bytes {
        return Err(ParseError::ByteLimit);
    }
    let refusal = Cell::new(None);
    let mut decoder = serde_json::Deserializer::from_slice(input);
    let value = Bounded {
        depth: 0,
        refusal: &refusal,
    }
    .deserialize(&mut decoder)
    .map_err(|_| refusal.get().unwrap_or(ParseError::InvalidJson))?;
    decoder.end().map_err(|_| ParseError::InvalidJson)?;
    Ok(value)
}
