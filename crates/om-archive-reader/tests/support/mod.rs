#![allow(dead_code)]
use sha2::{Digest, Sha256};

pub struct Vector {
    pub name: String,
    pub bytes: Vec<u8>,
    pub sha256: String,
}

// A deliberately tiny fixture-specific reader: these public JSON vectors have
// fixed string-valued fields. No JSON parser or dev-dependencies are needed.
fn string_field<'a>(chunk: &'a str, key: &str) -> &'a str {
    let needle = format!("\"{key}\": \"");
    let start = chunk.find(&needle).expect("fixture field exists") + needle.len();
    let tail = &chunk[start..];
    &tail[..tail.find('"').expect("fixture string terminates")]
}
fn hex_val(b: u8) -> u8 {
    match b {
        b'0'..=b'9' => b - b'0',
        b'a'..=b'f' => b - b'a' + 10,
        _ => panic!("invalid fixture hex"),
    }
}
pub fn unhex(s: &str) -> Vec<u8> {
    assert_eq!(s.len() % 2, 0);
    s.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| (hex_val(pair[0]) << 4) | hex_val(pair[1]))
        .collect()
}
pub fn vectors() -> Vec<Vector> {
    const JSON: &str = include_str!("../fixtures/om-archive-v1-vectors.json");
    JSON.split("{\"name\": \"")
        .skip(1)
        .map(|chunk| {
            let name = chunk.split('"').next().expect("fixture name").to_owned();
            let sha256 = string_field(chunk, "sha256").to_owned();
            let bytes = unhex(string_field(chunk, "archive_hex"));
            let calculated = Sha256::digest(&bytes);
            let actual: String = calculated.iter().map(|v| format!("{v:02x}")).collect();
            assert_eq!(actual, sha256, "byte-exact fixture hash for {name}");
            Vector {
                name,
                bytes,
                sha256,
            }
        })
        .collect()
}
pub fn plain() -> Vec<u8> {
    vectors()
        .into_iter()
        .find(|v| v.name == "plain")
        .expect("plain vector")
        .bytes
}

fn get16(b: &[u8], p: usize) -> usize {
    u16::from_le_bytes([b[p], b[p + 1]]) as usize
}
fn get32(b: &[u8], p: usize) -> usize {
    u32::from_le_bytes([b[p], b[p + 1], b[p + 2], b[p + 3]]) as usize
}
fn put16(b: &mut [u8], p: usize, n: u16) {
    b[p..p + 2].copy_from_slice(&n.to_le_bytes());
}
fn put32(b: &mut [u8], p: usize, n: u32) {
    b[p..p + 4].copy_from_slice(&n.to_le_bytes());
}

pub fn entries_from_plain() -> Vec<(String, Vec<u8>)> {
    let base = plain();
    let mut pos = 0usize;
    let mut entries = Vec::new();
    while pos + 30 < base.len() && &base[pos..pos + 4] == b"PK\x03\x04" {
        let length = get32(&base, pos + 18);
        let name = get16(&base, pos + 26);
        let extra = get16(&base, pos + 28);
        let start = pos + 30 + name + extra;
        entries.push((
            String::from_utf8(base[pos + 30..pos + 30 + name].to_vec()).unwrap(),
            base[start..start + length].to_vec(),
        ));
        pos = start + length;
    }
    assert_eq!(entries.len(), 7);
    entries
}

#[derive(Debug)]
pub struct Built {
    pub bytes: Vec<u8>,
    pub locals: Vec<usize>,
    pub central: Vec<usize>,
    pub data: Vec<usize>,
    pub eocd: usize,
}
fn append16(b: &mut Vec<u8>, v: u16) {
    b.extend_from_slice(&v.to_le_bytes());
}
fn append32(b: &mut Vec<u8>, v: u32) {
    b.extend_from_slice(&v.to_le_bytes());
}

pub fn build(entries: &[(String, Vec<u8>)]) -> Built {
    build_extra(entries, None)
}
pub fn build_extra(entries: &[(String, Vec<u8>)], extra_index: Option<usize>) -> Built {
    assert!(entries.len() < usize::from(u16::MAX));
    let mut bytes = Vec::new();
    let mut locals = Vec::new();
    let mut data = Vec::new();
    let mut central = Vec::new();
    for (i, (name, content)) in entries.iter().enumerate() {
        assert!(content.len() < u32::MAX as usize);
        let extra: &[u8] = if extra_index == Some(i) {
            &[0x99, 0x99, 0, 0]
        } else {
            &[]
        };
        locals.push(bytes.len());
        append32(&mut bytes, 0x04034b50);
        append16(&mut bytes, 20);
        append16(&mut bytes, 0x0800);
        append16(&mut bytes, 0);
        append16(&mut bytes, 0);
        append16(&mut bytes, 0x21);
        append32(&mut bytes, crc32fast::hash(content));
        append32(&mut bytes, content.len() as u32);
        append32(&mut bytes, content.len() as u32);
        append16(&mut bytes, name.len() as u16);
        append16(&mut bytes, extra.len() as u16);
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(extra);
        data.push(bytes.len());
        bytes.extend_from_slice(content);
    }
    let cd_start = bytes.len();
    for (i, (name, content)) in entries.iter().enumerate() {
        let extra: &[u8] = if extra_index == Some(i) {
            &[0x99, 0x99, 0, 0]
        } else {
            &[]
        };
        central.push(bytes.len());
        append32(&mut bytes, 0x02014b50);
        append16(&mut bytes, 0x0314);
        append16(&mut bytes, 20);
        append16(&mut bytes, 0x0800);
        append16(&mut bytes, 0);
        append16(&mut bytes, 0);
        append16(&mut bytes, 0x21);
        append32(&mut bytes, crc32fast::hash(content));
        append32(&mut bytes, content.len() as u32);
        append32(&mut bytes, content.len() as u32);
        append16(&mut bytes, name.len() as u16);
        append16(&mut bytes, extra.len() as u16);
        append16(&mut bytes, 0);
        append16(&mut bytes, 0);
        append16(&mut bytes, 0);
        append32(&mut bytes, 0o100644 << 16);
        append32(&mut bytes, locals[i] as u32);
        bytes.extend_from_slice(name.as_bytes());
        bytes.extend_from_slice(extra);
    }
    let cd_len = bytes.len() - cd_start;
    let eocd = bytes.len();
    append32(&mut bytes, 0x06054b50);
    append16(&mut bytes, 0);
    append16(&mut bytes, 0);
    append16(&mut bytes, entries.len() as u16);
    append16(&mut bytes, entries.len() as u16);
    append32(&mut bytes, cd_len as u32);
    append32(&mut bytes, cd_start as u32);
    append16(&mut bytes, 0);
    Built {
        bytes,
        locals,
        central,
        data,
        eocd,
    }
}

pub fn set16(b: &mut [u8], p: usize, value: u16) {
    put16(b, p, value);
}
pub fn set32(b: &mut [u8], p: usize, value: u32) {
    put32(b, p, value);
}
pub fn find(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|v| v == needle)
        .expect("fixture contains sequence")
}
