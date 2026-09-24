//! Feasibility prototype for V-005. Not product code. Not for production.
//!
//! OM-CBOR-v1 value model, canonical encoder and strict decoder, written from
//! RFC 8949 §3 (encoding), §4.2.1 (core deterministic encoding requirements) and
//! spec/registries/om-cbor-profile.json. Every error is one of the profile's
//! error classes.

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Uint(u64),
    /// Negative integer -1 - n
    Nint(u64),
    Bytes(Vec<u8>),
    Text(String),
    Array(Vec<Value>),
    Map(Vec<(Value, Value)>),
    Bool(bool),
    Null,
}

pub type Class = &'static str;

#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_depth: usize,
    pub max_items: u64,
    pub max_string: u64,
    /// lenient = accept any map-key order and mixed key types (used only to read
    /// CDDL-tool-generated examples, never for signed/hashed input)
    pub lenient: bool,
}

impl Limits {
    pub const OM: Limits = Limits { max_depth: 24, max_items: 4096, max_string: 65536, lenient: false };
}

// ------------------------------------------------------------------ encoder

fn put_head(out: &mut Vec<u8>, major: u8, n: u64) {
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

pub fn encode_into(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Uint(n) => put_head(out, 0, *n),
        Value::Nint(n) => put_head(out, 1, *n),
        Value::Bytes(b) => {
            put_head(out, 2, b.len() as u64);
            out.extend_from_slice(b);
        }
        Value::Text(s) => {
            put_head(out, 3, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        Value::Array(a) => {
            put_head(out, 4, a.len() as u64);
            for x in a {
                encode_into(x, out);
            }
        }
        Value::Map(m) => {
            // RFC 8949 §4.2.1: sort by the bytewise lexicographic order of encoded keys
            let mut enc: Vec<(Vec<u8>, &Value)> = m.iter().map(|(k, x)| (encode(k), x)).collect();
            enc.sort_by(|a, b| a.0.cmp(&b.0));
            put_head(out, 5, enc.len() as u64);
            for (k, x) in enc {
                out.extend_from_slice(&k);
                encode_into(x, out);
            }
        }
        Value::Bool(false) => out.push(0xf4),
        Value::Bool(true) => out.push(0xf5),
        Value::Null => out.push(0xf6),
    }
}

pub fn encode(v: &Value) -> Vec<u8> {
    let mut out = Vec::new();
    encode_into(v, &mut out);
    out
}

// ------------------------------------------------------------------ strict decoder

pub struct Decoder<'a> {
    buf: &'a [u8],
    pos: usize,
    lim: Limits,
}

impl<'a> Decoder<'a> {
    fn byte(&mut self) -> Result<u8, Class> {
        let b = *self.buf.get(self.pos).ok_or("truncated")?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: u64) -> Result<&'a [u8], Class> {
        let n = usize::try_from(n).map_err(|_| "truncated")?;
        if self.buf.len() - self.pos < n {
            return Err("truncated");
        }
        let s = &self.buf[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }

    /// Reads the argument for additional-info `ai`, enforcing shortest form.
    fn argument(&mut self, major: u8, ai: u8) -> Result<u64, Class> {
        let nc: Class = if major <= 1 { "noncanonical_int" } else { "noncanonical_length" };
        let (n, min) = match ai {
            0..=23 => return Ok(ai as u64),
            24 => (self.take(1)?[0] as u64, 24u64),
            25 => {
                let b = self.take(2)?;
                (u16::from_be_bytes([b[0], b[1]]) as u64, 0x100)
            }
            26 => {
                let b = self.take(4)?;
                (u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as u64, 0x1_0000)
            }
            27 => {
                let b = self.take(8)?;
                (u64::from_be_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]), 0x1_0000_0000)
            }
            28..=30 => return Err("malformed"),
            _ => return Err(if major >= 2 && major != 6 { "indefinite_length" } else { "malformed" }),
        };
        if n < min {
            return Err(nc);
        }
        Ok(n)
    }

    fn item(&mut self, depth: usize) -> Result<Value, Class> {
        let ib = self.byte()?;
        let major = ib >> 5;
        let ai = ib & 0x1f;
        match major {
            6 => return Err("tag_forbidden"),
            7 => {
                return match ai {
                    20 => Ok(Value::Bool(false)),
                    21 => Ok(Value::Bool(true)),
                    22 => Ok(Value::Null),
                    25..=27 => Err("float_forbidden"),
                    28..=30 => Err("malformed"),
                    31 => Err("indefinite_length"),
                    _ => Err("invalid_simple"),
                }
            }
            _ => {}
        }
        let n = self.argument(major, ai)?;
        match major {
            0 => Ok(Value::Uint(n)),
            1 => Ok(Value::Nint(n)),
            2 | 3 => {
                if n > self.lim.max_string {
                    return Err("size_limit");
                }
                let s = self.take(n)?;
                if major == 2 {
                    Ok(Value::Bytes(s.to_vec()))
                } else {
                    std::str::from_utf8(s).map(|t| Value::Text(t.to_owned())).map_err(|_| "invalid_utf8")
                }
            }
            4 | 5 => {
                let d = depth + 1;
                if d > self.lim.max_depth {
                    return Err("depth_limit");
                }
                if n > self.lim.max_items {
                    return Err("count_limit");
                }
                if major == 4 {
                    let mut a = Vec::with_capacity(n as usize);
                    for _ in 0..n {
                        a.push(self.item(d)?);
                    }
                    Ok(Value::Array(a))
                } else {
                    let mut m = Vec::with_capacity(n as usize);
                    let mut prev: Option<(usize, usize)> = None;
                    let mut first_major: Option<u8> = None;
                    for _ in 0..n {
                        let ks = self.pos;
                        let k = self.item(d)?;
                        let ke = self.pos;
                        let km = match k {
                            Value::Uint(_) => 0,
                            Value::Text(_) => 3,
                            _ => return Err("key_type"),
                        };
                        if !self.lim.lenient {
                            if *first_major.get_or_insert(km) != km {
                                return Err("key_type");
                            }
                        }
                        if let Some((ps, pe)) = prev {
                            let (a, b) = (&self.buf[ps..pe], &self.buf[ks..ke]);
                            if a == b {
                                return Err("duplicate_key");
                            }
                            if !self.lim.lenient && b < a {
                                return Err("unsorted_keys");
                            }
                        }
                        prev = Some((ks, ke));
                        let x = self.item(d)?;
                        m.push((k, x));
                    }
                    if self.lim.lenient {
                        // duplicates can be non-adjacent when order is not enforced
                        for i in 0..m.len() {
                            for j in i + 1..m.len() {
                                if m[i].0 == m[j].0 {
                                    return Err("duplicate_key");
                                }
                            }
                        }
                    }
                    Ok(Value::Map(m))
                }
            }
            _ => unreachable!(),
        }
    }
}

/// Decode exactly one data item under OM-CBOR-v1. `max_bytes` bounds the whole input.
pub fn decode(buf: &[u8], lim: Limits, max_bytes: usize) -> Result<Value, Class> {
    if buf.len() > max_bytes {
        return Err("size_limit");
    }
    let mut d = Decoder { buf, pos: 0, lim };
    let v = d.item(0)?;
    if d.pos != buf.len() {
        return Err("trailing_bytes");
    }
    Ok(v)
}

// ------------------------------------------------------------------ helpers

impl Value {
    pub fn get(&self, key: u64) -> Option<&Value> {
        match self {
            Value::Map(m) => m.iter().find(|(k, _)| *k == Value::Uint(key)).map(|(_, v)| v),
            _ => None,
        }
    }
}

pub fn hex(b: &[u8]) -> String {
    const H: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        s.push(H[(x >> 4) as usize] as char);
        s.push(H[(x & 15) as usize] as char);
    }
    s
}

pub fn unhex(s: &str) -> Vec<u8> {
    let v = |c: u8| match c {
        b'0'..=b'9' => c - b'0',
        b'a'..=b'f' => c - b'a' + 10,
        b'A'..=b'F' => c - b'A' + 10,
        _ => panic!("bad hex"),
    };
    s.as_bytes().chunks(2).map(|p| v(p[0]) << 4 | v(p[1])).collect()
}

/// JSON diagnostic form used by verification/cbor fixtures ({"$bytes": hex}, {"$map": [[k, v]]}).
pub fn from_diag(j: &serde_json::Value) -> Value {
    use serde_json::Value as J;
    match j {
        J::Null => Value::Null,
        J::Bool(b) => Value::Bool(*b),
        J::Number(n) => {
            if let Some(u) = n.as_u64() {
                Value::Uint(u)
            } else if let Some(i) = n.as_i64() {
                Value::Nint((-1 - i) as u64)
            } else if n.to_string().starts_with('-') {
                // below i64::MIN is not used by any vector
                panic!("unsupported negative integer {n}")
            } else {
                panic!("float in diag input: {n}")
            }
        }
        J::String(s) => Value::Text(s.clone()),
        J::Array(a) => Value::Array(a.iter().map(from_diag).collect()),
        J::Object(o) => {
            if let Some(J::String(h)) = o.get("$bytes") {
                Value::Bytes(unhex(h))
            } else if let Some(J::Array(ps)) = o.get("$map") {
                Value::Map(ps.iter().map(|p| (from_diag(&p[0]), from_diag(&p[1]))).collect())
            } else {
                Value::Map(o.iter().map(|(k, v)| (Value::Text(k.clone()), from_diag(v))).collect())
            }
        }
    }
}
