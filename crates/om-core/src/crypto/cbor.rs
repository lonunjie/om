// SPDX-License-Identifier: MPL-2.0
//! Minimal strict deterministic CBOR (RFC 8949 §4.2.1 core deterministic encoding) for
//! prototype state/wire objects. Subset: unsigned ints, byte/text strings, arrays,
//! maps with unsigned-int keys, true/false/null. Decoder rejects: indefinite lengths,
//! non-shortest heads, unsorted or duplicate map keys, tags, floats, negative ints,
//! invalid UTF-8, trailing bytes, excessive depth, and lengths beyond the remaining input
//! (checked before allocation) — per `CANONICAL_ENCODING.md` "Decoder discipline".

use crate::crypto::Error;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum V {
    U(u64),
    B(Vec<u8>),
    T(String),
    A(Vec<V>),
    /// Map with unsigned-integer keys; encoder sorts, decoder requires strictly ascending.
    M(Vec<(u64, V)>),
    Bool(bool),
    Null,
}

pub const MAX_DEPTH: usize = 24;

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

pub fn encode(v: &V) -> Vec<u8> {
    let mut out = Vec::new();
    enc(v, &mut out);
    out
}

fn enc(v: &V, out: &mut Vec<u8>) {
    match v {
        V::U(n) => head(out, 0, *n),
        V::B(b) => {
            head(out, 2, b.len() as u64);
            out.extend_from_slice(b);
        }
        V::T(s) => {
            head(out, 3, s.len() as u64);
            out.extend_from_slice(s.as_bytes());
        }
        V::A(a) => {
            head(out, 4, a.len() as u64);
            for x in a {
                enc(x, out);
            }
        }
        V::M(m) => {
            let mut idx: Vec<&(u64, V)> = m.iter().collect();
            idx.sort_by_key(|(k, _)| *k);
            for w in idx.windows(2) {
                assert!(w[0].0 != w[1].0, "duplicate CBOR map key {}", w[0].0);
            }
            head(out, 5, m.len() as u64);
            for (k, x) in idx {
                head(out, 0, *k);
                enc(x, out);
            }
        }
        V::Bool(false) => out.push(0xf4),
        V::Bool(true) => out.push(0xf5),
        V::Null => out.push(0xf6),
    }
}

pub fn decode(b: &[u8]) -> Result<V, Error> {
    let mut at = 0usize;
    let v = dec(b, &mut at, 0)?;
    if at != b.len() {
        return Err(Error::Decode("cbor: trailing bytes"));
    }
    Ok(v)
}

fn take<'a>(b: &'a [u8], at: &mut usize, n: usize) -> Result<&'a [u8], Error> {
    if n > b.len().saturating_sub(*at) {
        return Err(Error::Decode("cbor: truncated"));
    }
    let s = &b[*at..*at + n];
    *at += n;
    Ok(s)
}

fn read_head(b: &[u8], at: &mut usize) -> Result<(u8, u64), Error> {
    let ib = take(b, at, 1)?[0];
    let (major, ai) = (ib >> 5, ib & 31);
    let n = match ai {
        0..=23 => ai as u64,
        24 => {
            let v = take(b, at, 1)?[0] as u64;
            if v < 24 {
                return Err(Error::Decode("cbor: non-shortest head"));
            }
            v
        }
        25 => {
            let v = u16::from_be_bytes(take(b, at, 2)?.try_into().unwrap()) as u64;
            if v <= 0xff {
                return Err(Error::Decode("cbor: non-shortest head"));
            }
            v
        }
        26 => {
            let v = u32::from_be_bytes(take(b, at, 4)?.try_into().unwrap()) as u64;
            if v <= 0xffff {
                return Err(Error::Decode("cbor: non-shortest head"));
            }
            v
        }
        27 => {
            let v = u64::from_be_bytes(take(b, at, 8)?.try_into().unwrap());
            if v <= 0xffff_ffff {
                return Err(Error::Decode("cbor: non-shortest head"));
            }
            v
        }
        31 => return Err(Error::Decode("cbor: indefinite length")),
        _ => return Err(Error::Decode("cbor: reserved additional info")),
    };
    Ok((major, n))
}

fn dec(b: &[u8], at: &mut usize, depth: usize) -> Result<V, Error> {
    if depth > MAX_DEPTH {
        return Err(Error::Decode("cbor: too deep"));
    }
    let (major, n) = read_head(b, at)?;
    let remaining = (b.len() - *at) as u64;
    match major {
        0 => Ok(V::U(n)),
        2 => Ok(V::B(
            take(
                b,
                at,
                usize::try_from(n).map_err(|_| Error::Decode("cbor: len"))?,
            )?
            .to_vec(),
        )),
        3 => {
            let s = take(
                b,
                at,
                usize::try_from(n).map_err(|_| Error::Decode("cbor: len"))?,
            )?;
            Ok(V::T(
                String::from_utf8(s.to_vec()).map_err(|_| Error::Decode("cbor: invalid utf-8"))?,
            ))
        }
        4 => {
            if n > remaining {
                return Err(Error::Decode("cbor: array count exceeds input"));
            }
            let mut a = Vec::with_capacity(n as usize);
            for _ in 0..n {
                a.push(dec(b, at, depth + 1)?);
            }
            Ok(V::A(a))
        }
        5 => {
            if n.saturating_mul(2) > remaining {
                return Err(Error::Decode("cbor: map count exceeds input"));
            }
            let mut m: Vec<(u64, V)> = Vec::with_capacity(n as usize);
            for _ in 0..n {
                let (km, k) = read_head(b, at)?;
                if km != 0 {
                    return Err(Error::Decode("cbor: non-uint map key"));
                }
                if let Some((last, _)) = m.last()
                    && k <= *last
                {
                    return Err(Error::Decode("cbor: map keys unsorted or duplicate"));
                }
                let v = dec(b, at, depth + 1)?;
                m.push((k, v));
            }
            Ok(V::M(m))
        }
        7 => match n {
            20 => Ok(V::Bool(false)),
            21 => Ok(V::Bool(true)),
            22 => Ok(V::Null),
            _ => Err(Error::Decode("cbor: unsupported simple/float")),
        },
        1 => Err(Error::Decode("cbor: negative ints not used")),
        _ => Err(Error::Decode("cbor: tags not allowed")),
    }
}

// ---- typed accessors -------------------------------------------------------------------

impl V {
    pub fn get(&self, key: u64) -> Result<&V, Error> {
        self.opt(key).ok_or(Error::Decode("cbor: missing map key"))
    }
    pub fn opt(&self, key: u64) -> Option<&V> {
        match self {
            V::M(m) => m.iter().find(|(k, _)| *k == key).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn u(&self) -> Result<u64, Error> {
        match self {
            V::U(n) => Ok(*n),
            _ => Err(Error::Decode("cbor: expected uint")),
        }
    }
    pub fn u32(&self) -> Result<u32, Error> {
        u32::try_from(self.u()?).map_err(|_| Error::Decode("cbor: u32 overflow"))
    }
    pub fn b(&self) -> Result<&[u8], Error> {
        match self {
            V::B(b) => Ok(b),
            _ => Err(Error::Decode("cbor: expected bstr")),
        }
    }
    pub fn b32(&self) -> Result<[u8; 32], Error> {
        self.b()?
            .try_into()
            .map_err(|_| Error::Decode("cbor: expected 32-byte bstr"))
    }
    pub fn b64(&self) -> Result<[u8; 64], Error> {
        self.b()?
            .try_into()
            .map_err(|_| Error::Decode("cbor: expected 64-byte bstr"))
    }
    pub fn t(&self) -> Result<&str, Error> {
        match self {
            V::T(s) => Ok(s),
            _ => Err(Error::Decode("cbor: expected tstr")),
        }
    }
    pub fn a(&self) -> Result<&[V], Error> {
        match self {
            V::A(a) => Ok(a),
            _ => Err(Error::Decode("cbor: expected array")),
        }
    }
    pub fn m(&self) -> Result<&[(u64, V)], Error> {
        match self {
            V::M(m) => Ok(m),
            _ => Err(Error::Decode("cbor: expected map")),
        }
    }
    pub fn is_null(&self) -> bool {
        matches!(self, V::Null)
    }
}

pub fn opt_b32(v: Option<[u8; 32]>) -> V {
    v.map(|x| V::B(x.to_vec())).unwrap_or(V::Null)
}

pub fn get_opt_b32(v: &V) -> Result<Option<[u8; 32]>, Error> {
    if v.is_null() {
        Ok(None)
    } else {
        Ok(Some(v.b32()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_and_strictness() {
        let v = V::M(vec![
            (2, V::U(500)),
            (1, V::B(vec![1, 2])),
            (3, V::A(vec![V::Null, V::Bool(true)])),
        ]);
        let e = encode(&v);
        let d = decode(&e).unwrap();
        assert_eq!(encode(&d), e);
        assert!(decode(&[0x18, 0x05]).is_err()); // non-shortest
        assert!(decode(&[0xa2, 0x02, 0x00, 0x01, 0x00]).is_err()); // unsorted
        assert!(decode(&[0x5f]).is_err()); // indefinite
        assert!(decode(&[0x9a, 0xff, 0xff, 0xff, 0xff]).is_err()); // count > input
    }
}
