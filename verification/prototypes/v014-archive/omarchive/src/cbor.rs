//! Strict deterministic CBOR (RFC 8949 §4.2.1 core deterministic encoding) with the OM-CBOR-v1 constraints of
//! CANONICAL_ENCODING.md: definite lengths only, no floats, no tags, no duplicate or unsorted map keys, valid UTF-8,
//! bounded depth and counts checked before allocation, and no trailing bytes. Feasibility prototype, not product code.

use std::fmt;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Uint(u64),
    /// Negative integer with value `-1 - n`.
    Nint(u64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    /// Stored in canonical (encoded-key bytewise) order after `canonicalize`; the decoder guarantees it.
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CborError {
    Truncated,
    TrailingBytes,
    NonMinimalLength,
    Indefinite,
    Float,
    Tag,
    UnsupportedSimple,
    InvalidUtf8,
    UnsortedOrDuplicateKey,
    TooDeep,
    TooLarge,
}

impl fmt::Display for CborError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

pub const MAX_DEPTH: usize = 16;

fn head(out: &mut Vec<u8>, major: u8, n: u64) {
    let m = major << 5;
    if n < 24 {
        out.push(m | n as u8);
    } else if n <= 0xff {
        out.push(m | 24);
        out.push(n as u8);
    } else if n <= 0xffff {
        out.push(m | 25);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= 0xffff_ffff {
        out.push(m | 26);
        out.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&n.to_be_bytes());
    }
}

/// Deterministic encoding. Map keys are sorted by their encoded bytes; duplicate keys panic (programmer error).
pub fn encode(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    enc(v, &mut out);
    out
}

fn enc(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Uint(n) => head(out, 0, *n),
        Value::Nint(n) => head(out, 1, *n),
        Value::Bytes(b) => {
            head(out, 2, b.len() as u64);
            out.extend_from_slice(b);
        }
        Value::Text(s) => {
            head(out, 3, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        Value::Array(a) => {
            head(out, 4, a.len() as u64);
            for x in a {
                enc(x, out);
            }
        }
        Value::Map(m) => {
            let mut kv: Vec<(Vec<u8>, &Value)> = m.iter().map(|(k, v)| (encode(k), v)).collect();
            kv.sort_by(|a, b| a.0.cmp(&b.0));
            for w in kv.windows(2) {
                assert!(w[0].0 != w[1].0, "duplicate CBOR map key");
            }
            head(out, 5, kv.len() as u64);
            for (k, v) in kv {
                out.extend_from_slice(&k);
                enc(v, out);
            }
        }
        Value::Bool(false) => out.push(0xf4),
        Value::Bool(true) => out.push(0xf5),
        Value::Null => out.push(0xf6),
    }
}

/// Strict decode: the whole input must be exactly one canonical item.
pub fn decode(input: &[u8]) -> Result<Value, CborError> {
    let mut d = Dec { b: input, i: 0 };
    let v = d.item(0)?;
    if d.i != input.len() {
        return Err(CborError::TrailingBytes);
    }
    Ok(v)
}

struct Dec<'a> {
    b: &'a [u8],
    i: usize,
}

impl Dec<'_> {
    fn take(&mut self, n: usize) -> Result<&[u8], CborError> {
        if self.b.len() - self.i < n {
            return Err(CborError::Truncated);
        }
        let s = &self.b[self.i..self.i + n];
        self.i += n;
        Ok(s)
    }

    fn arg(&mut self, ai: u8) -> Result<u64, CborError> {
        let (n, min) = match ai {
            0..=23 => return Ok(ai as u64),
            24 => (self.take(1)?[0] as u64, 24),
            25 => (u16::from_be_bytes(self.take(2)?.try_into().unwrap()) as u64, 0x100),
            26 => (u32::from_be_bytes(self.take(4)?.try_into().unwrap()) as u64, 0x1_0000),
            27 => (u64::from_be_bytes(self.take(8)?.try_into().unwrap()), 0x1_0000_0000),
            31 => return Err(CborError::Indefinite),
            _ => return Err(CborError::Truncated),
        };
        if n < min {
            return Err(CborError::NonMinimalLength);
        }
        Ok(n)
    }

    fn remaining(&self) -> u64 {
        (self.b.len() - self.i) as u64
    }

    fn item(&mut self, depth: usize) -> Result<Value, CborError> {
        if depth > MAX_DEPTH {
            return Err(CborError::TooDeep);
        }
        let ib = self.take(1)?[0];
        let (major, ai) = (ib >> 5, ib & 0x1f);
        match major {
            0 => Ok(Value::Uint(self.arg(ai)?)),
            1 => Ok(Value::Nint(self.arg(ai)?)),
            2 | 3 => {
                let n = self.arg(ai)?;
                if n > self.remaining() {
                    return Err(CborError::Truncated);
                }
                let s = self.take(n as usize)?.to_vec();
                if major == 2 {
                    Ok(Value::Bytes(s))
                } else {
                    String::from_utf8(s).map(Value::Text).map_err(|_| CborError::InvalidUtf8)
                }
            }
            4 => {
                let n = self.arg(ai)?;
                // every element needs at least one byte: reject counts that cannot fit before allocating
                if n > self.remaining() {
                    return Err(CborError::TooLarge);
                }
                let mut a = Vec::with_capacity(n as usize);
                for _ in 0..n {
                    a.push(self.item(depth + 1)?);
                }
                Ok(Value::Array(a))
            }
            5 => {
                let n = self.arg(ai)?;
                if n.saturating_mul(2) > self.remaining() {
                    return Err(CborError::TooLarge);
                }
                let mut m = Vec::with_capacity(n as usize);
                let mut prev: Option<Vec<u8>> = None;
                for _ in 0..n {
                    let start = self.i;
                    let k = self.item(depth + 1)?;
                    let kb = self.b[start..self.i].to_vec();
                    if let Some(p) = &prev {
                        if kb <= *p {
                            return Err(CborError::UnsortedOrDuplicateKey);
                        }
                    }
                    prev = Some(kb);
                    let v = self.item(depth + 1)?;
                    m.push((k, v));
                }
                Ok(Value::Map(m))
            }
            6 => Err(CborError::Tag),
            7 => match ai {
                20 => Ok(Value::Bool(false)),
                21 => Ok(Value::Bool(true)),
                22 => Ok(Value::Null),
                25..=27 => Err(CborError::Float),
                31 => Err(CborError::Indefinite),
                _ => Err(CborError::UnsupportedSimple),
            },
            _ => unreachable!(),
        }
    }
}

// ---- small typed accessors used by the archive schema layer

impl Value {
    pub fn text(s: &str) -> Value {
        Value::Text(s.to_string())
    }
    pub fn get(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m.iter().find(|(k, _)| matches!(k, Value::Text(t) if t == key)).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Value::Map(m) => m.iter().filter_map(|(k, _)| if let Value::Text(t) = k { Some(t.as_str()) } else { None }).collect(),
            _ => vec![],
        }
    }
    pub fn as_text(&self) -> Option<&str> {
        if let Value::Text(t) = self { Some(t) } else { None }
    }
    pub fn as_bytes(&self) -> Option<&[u8]> {
        if let Value::Bytes(b) = self { Some(b) } else { None }
    }
    pub fn as_u64(&self) -> Option<u64> {
        if let Value::Uint(n) = self { Some(*n) } else { None }
    }
    pub fn as_bool(&self) -> Option<bool> {
        if let Value::Bool(b) = self { Some(*b) } else { None }
    }
    pub fn as_array(&self) -> Option<&[Value]> {
        if let Value::Array(a) = self { Some(a) } else { None }
    }
}

/// Build a text-keyed map (order irrelevant: encoding sorts).
pub fn map(entries: Vec<(&str, Value)>) -> Value {
    Value::Map(entries.into_iter().map(|(k, v)| (Value::text(k), v)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_order() {
        let v = map(vec![("bb", Value::Uint(1)), ("a", Value::Nint(0)), ("c", Value::Array(vec![Value::Null]))]);
        let e = encode(&v);
        // "a" (0x61 0x61) < "c" (0x61 0x63) < "bb" (0x62 ...): bytewise order of encoded keys
        assert_eq!(e, vec![0xa3, 0x61, b'a', 0x20, 0x61, b'c', 0x81, 0xf6, 0x62, b'b', b'b', 0x01]);
        assert_eq!(encode(&decode(&e).unwrap()), e);
    }

    #[test]
    fn rejects_noncanonical() {
        assert_eq!(decode(&[0x18, 0x05]), Err(CborError::NonMinimalLength));
        assert_eq!(decode(&[0x9f, 0xff]), Err(CborError::Indefinite));
        assert_eq!(decode(&[0xf9, 0, 0]), Err(CborError::Float));
        assert_eq!(decode(&[0xc0, 0x00]), Err(CborError::Tag));
        assert_eq!(decode(&[0xa2, 0x61, b'b', 0, 0x61, b'a', 0]), Err(CborError::UnsortedOrDuplicateKey));
        assert_eq!(decode(&[0xa2, 0x61, b'a', 0, 0x61, b'a', 0]), Err(CborError::UnsortedOrDuplicateKey));
        assert_eq!(decode(&[0x62, 0xff, 0xfe]), Err(CborError::InvalidUtf8));
        assert_eq!(decode(&[0x00, 0x00]), Err(CborError::TrailingBytes));
        assert_eq!(decode(&[0x9b, 0, 0, 0, 1, 0, 0, 0, 0]), Err(CborError::TooLarge));
        let mut deep = vec![0x81; 40];
        deep.push(0);
        assert_eq!(decode(&deep), Err(CborError::TooDeep));
    }
}
