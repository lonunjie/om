//! OM-CBOR-v1, checked and zero-copy for strings, byte strings and raw encodings.
//! This module deliberately does NOT use serde, unbounded recursion, or a CBOR crate.
use crate::ArchiveError;

#[derive(Debug, Clone)]
pub(crate) struct Node<'a> {
    pub(crate) value: Value<'a>,
    pub(crate) encoded: &'a [u8],
}

#[derive(Debug, Clone)]
pub(crate) enum Value<'a> {
    Unsigned(u64),
    #[allow(dead_code)] // kept so the decoded value is complete; only its type matters here
    Negative(u64), // value = -1 - unsigned argument
    Bytes(&'a [u8]),
    Text(&'a str),
    Array(Vec<Node<'a>>),
    Map(Vec<(Node<'a>, Node<'a>)>),
    Bool(bool),
    Null,
}

impl<'a> Node<'a> {
    pub(crate) fn text(&self) -> Option<&'a str> {
        if let Value::Text(s) = &self.value {
            Some(*s)
        } else {
            None
        }
    }
    pub(crate) fn bytes(&self) -> Option<&'a [u8]> {
        if let Value::Bytes(v) = &self.value {
            Some(*v)
        } else {
            None
        }
    }
    pub(crate) fn uint(&self) -> Option<u64> {
        if let Value::Unsigned(v) = &self.value {
            Some(*v)
        } else {
            None
        }
    }
    pub(crate) fn array(&self) -> Option<&[Node<'a>]> {
        if let Value::Array(ref v) = self.value {
            Some(v)
        } else {
            None
        }
    }
    pub(crate) fn map(&self) -> Option<&[(Node<'a>, Node<'a>)]> {
        if let Value::Map(ref v) = self.value {
            Some(v)
        } else {
            None
        }
    }
    pub(crate) fn get(&self, key: &str) -> Option<&Node<'a>> {
        self.map()?
            .iter()
            .find_map(|(k, v)| (k.text() == Some(key)).then_some(v))
    }
}

struct Decoder<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Decoder<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ArchiveError> {
        let end = self.pos.checked_add(n).ok_or(ArchiveError::Cbor)?;
        let v = self.bytes.get(self.pos..end).ok_or(ArchiveError::Cbor)?;
        self.pos = end;
        Ok(v)
    }
    fn argument(&mut self, ai: u8) -> Result<u64, ArchiveError> {
        let (n, floor): (usize, u64) = match ai {
            0..=23 => return Ok(u64::from(ai)),
            24 => (1, 24),
            25 => (2, 256),
            26 => (4, 65536),
            27 => (8, 4_294_967_296),
            _ => return Err(ArchiveError::Cbor),
        };
        let mut v = 0u64;
        for &b in self.take(n)? {
            v = (v << 8) | u64::from(b);
        }
        if v < floor {
            return Err(ArchiveError::Cbor);
        }
        Ok(v)
    }
    fn item(&mut self, depth: u8) -> Result<Node<'a>, ArchiveError> {
        if depth > 16 {
            return Err(ArchiveError::Cbor);
        }
        let start = self.pos;
        let head = *self.take(1)?.first().ok_or(ArchiveError::Cbor)?;
        let major = head >> 5;
        let ai = head & 31;
        let value = match major {
            0 => Value::Unsigned(self.argument(ai)?),
            1 => Value::Negative(self.argument(ai)?),
            2 | 3 => {
                let n = self.argument(ai)?;
                let n = usize::try_from(n).map_err(|_| ArchiveError::Cbor)?;
                let b = self.take(n)?;
                if major == 2 {
                    Value::Bytes(b)
                } else {
                    Value::Text(std::str::from_utf8(b).map_err(|_| ArchiveError::Cbor)?)
                }
            }
            4 => {
                let n = self.argument(ai)?;
                if depth == 16 && n > 0 {
                    return Err(ArchiveError::Cbor);
                }
                // Even the smallest item uses one byte; reject impossible counts up front.
                if n > (self.bytes.len() - self.pos) as u64 {
                    return Err(ArchiveError::Cbor);
                }
                let mut result = Vec::new();
                for _ in 0..n {
                    result.push(self.item(depth.checked_add(1).ok_or(ArchiveError::Cbor)?)?);
                }
                Value::Array(result)
            }
            5 => {
                let n = self.argument(ai)?;
                if depth == 16 && n > 0 {
                    return Err(ArchiveError::Cbor);
                }
                if n > ((self.bytes.len() - self.pos) / 2) as u64 {
                    return Err(ArchiveError::Cbor);
                }
                let mut result = Vec::new();
                let mut last: Option<&[u8]> = None;
                for _ in 0..n {
                    let key = self.item(depth.checked_add(1).ok_or(ArchiveError::Cbor)?)?;
                    if let Some(previous) = last
                        && previous >= key.encoded
                    {
                        return Err(ArchiveError::Cbor);
                    }
                    last = Some(key.encoded);
                    let val = self.item(depth.checked_add(1).ok_or(ArchiveError::Cbor)?)?;
                    result.push((key, val));
                }
                Value::Map(result)
            }
            7 => match ai {
                20 => Value::Bool(false),
                21 => Value::Bool(true),
                22 => Value::Null,
                _ => return Err(ArchiveError::Cbor),
            },
            _ => return Err(ArchiveError::Cbor), // tags, floats, other simples
        };
        Ok(Node {
            value,
            encoded: &self.bytes[start..self.pos],
        })
    }
}

pub(crate) fn decode(input: &[u8]) -> Result<Node<'_>, ArchiveError> {
    let mut d = Decoder {
        bytes: input,
        pos: 0,
    };
    let node = d.item(0)?;
    if d.pos != input.len() {
        return Err(ArchiveError::Cbor);
    }
    Ok(node)
}
