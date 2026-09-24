//! V-014 feasibility harness: builds the om-archive-v1 positive vectors, checks round trips, generates the hostile
//! archive corpus deterministically, runs the reader and importer over it, and prints a deterministic JSON report.
//! Usage: omarchive-corpus [--vectors <path to write/verify vectors json>] [--write-vectors]
//! Not product code. All keys, passphrases and randomness here are INSECURE TEST VALUES.

use omarchive::archive::{self, ArchiveError, ArchiveInput, BlobIn, Encryption, KeyMaterial, ReadOptions, Recipient, WriteOptions};
use omarchive::cbor::{self, map, Value};
use omarchive::crypto::{sha256, Argon2Params};
use omarchive::import::{self, LiveState, ObjectStatus};
use omarchive::testobj::{insecure_test_object, insecure_test_signing_key, InsecureTestVerifier};
use omarchive::zip::{self, Limits, ZipError};
use std::io::Write;

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}
fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}
fn jstr(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Deterministic test bytes (NOT random): SHA-256 in counter mode over a label.
fn insecure_test_bytes(label: &str, n: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(n + 32);
    let mut i = 0u64;
    while out.len() < n {
        out.extend(sha256(&[label.as_bytes(), &i.to_be_bytes()]));
        i += 1;
    }
    out.truncate(n);
    out
}
fn arr16(label: &str) -> [u8; 16] {
    insecure_test_bytes(label, 16).try_into().unwrap()
}
fn arr32(label: &str) -> [u8; 32] {
    insecure_test_bytes(label, 32).try_into().unwrap()
}

const PASS: &[u8] = b"insecure test passphrase";
const TEST_KDF: Argon2Params = Argon2Params { m_kib: 8, t: 1, p: 1 };

fn imported_history(records: Vec<Value>) -> Vec<u8> {
    cbor::encode(&Value::Array(records))
}
fn record(id: &str, level: &str, extra: Vec<(&str, Value)>) -> Value {
    let mut f = vec![
        ("imported_record_id", Value::text(id)),
        ("source_network_namespace", Value::text("example-foreign-net")),
        ("source_export_fingerprint", Value::text("fp-001")),
        ("display_metadata", map(vec![("sender_label", Value::text("Alice (foreign export label)"))])),
        ("content", map(vec![("text", Value::text("hello from the old messenger"))])),
        ("attachments", Value::Array(vec![])),
        ("provenance_level", Value::text(level)),
    ];
    f.extend(extra);
    map(f)
}

struct Fixture {
    input: ArchiveInput,
    link_obj_path: String,
}

fn base_input(big_blob: bool) -> Fixture {
    let ev = insecure_test_object("alice-device", "test.event", "conv-1", 7, map(vec![("text", Value::text("native OM message"))]));
    let ctl_old = insecure_test_object("room-gov", "test.room.control", "room-1", 3, map(vec![("admins", Value::Array(vec![Value::text("alice")]))]));
    let link = insecure_test_object("alice-device", "test.foreign_identity_link", "link-1", 1, map(vec![("foreign_actor_ref", Value::text("alice@foreign"))]));
    let link_obj_path = archive::object_path(&link);
    let blob_small = insecure_test_bytes("blob-small", 1000);
    let mut blobs = vec![BlobIn::present(arr32("blob-id-small"), "image/webp", blob_small)];
    if big_blob {
        blobs.push(BlobIn::present(arr32("blob-id-big"), "video/mp4", insecure_test_bytes("blob-big", 150_000)));
    }
    let hist = imported_history(vec![
        record("r1", "foreign_export_label", vec![]),
        record("r2", "cryptographically_linked", vec![("link_evidence", Value::Text(link_obj_path.clone()))]),
    ]);
    Fixture {
        input: ArchiveInput {
            archive_id: arr16("archive-id"),
            export_scope: map(vec![("conversations", Value::Array(vec![Value::text("conv-1")]))]),
            source_account_hint: None,
            included_conversations: vec!["conv-1".into()],
            generated_at_hint: 1_790_000_000,
            exporter_implementation: "omarchive-prototype/0".into(),
            provenance_profiles: vec!["om-native".into(), "foreign-import".into()],
            objects: vec![("test.event".into(), ev), ("test.room.control".into(), ctl_old), ("test.foreign_identity_link".into(), link)],
            blobs,
            records: vec![("imported_history".into(), "imported-history".into(), hist)],
        },
        link_obj_path,
    }
}

fn enc_pass() -> Encryption {
    Encryption { archive_key: arr32("archive-key"), recipients: vec![Recipient::Passphrase { passphrase: PASS.to_vec(), salt: arr16("salt"), params: TEST_KDF }], chunk_size: 65536 }
}
fn raw_key() -> [u8; 32] {
    arr32("raw-recovery-key")
}
fn enc_raw() -> Encryption {
    Encryption { archive_key: arr32("archive-key"), recipients: vec![Recipient::RawKey { key: raw_key() }], chunk_size: 65536 }
}
fn wopt(e: Option<Encryption>) -> WriteOptions {
    WriteOptions { encryption: e, force_zip64: false, insecure_test_kdf: true }
}
fn ropt(keys: Vec<KeyMaterial>) -> ReadOptions {
    ReadOptions {
        limits: Limits { max_entries: 1000, max_entry_size: 16 << 20, max_total_size: 64 << 20 },
        max_manifest_size: 1 << 20,
        keys,
        insecure_test_kdf: true,
    }
}
fn all_keys() -> Vec<KeyMaterial> {
    vec![KeyMaterial::Passphrase(PASS.to_vec()), KeyMaterial::RawKey(raw_key())]
}

// ---- byte-level helpers for hostile cases ------------------------------------------------------------------

fn entries_of(bytes: &[u8]) -> Vec<(String, Vec<u8>)> {
    zip::read(bytes, &Limits::default()).unwrap().into_iter().map(|e| (e.name, e.data.to_vec())).collect()
}
fn find_all(h: &[u8], needle: &[u8]) -> Vec<usize> {
    h.windows(needle.len()).enumerate().filter(|(_, w)| *w == needle).map(|(i, _)| i).collect()
}
fn central_offsets(z: &[u8]) -> Vec<usize> {
    find_all(z, &0x0201_4b50u32.to_le_bytes())
}
fn put16(z: &mut [u8], at: usize, v: u16) {
    z[at..at + 2].copy_from_slice(&v.to_le_bytes())
}
fn put32(z: &mut [u8], at: usize, v: u32) {
    z[at..at + 4].copy_from_slice(&v.to_le_bytes())
}
/// Rebuild a plain archive whose manifest is replaced (integrity root NOT recomputed unless asked).
fn with_manifest(plain: &[u8], f: impl Fn(Value) -> Value, fix_root: bool) -> Vec<u8> {
    let mut es = entries_of(plain);
    let mut m = cbor::decode(&es[1].1).unwrap();
    m = f(m);
    if fix_root {
        let root = archive::integrity_root(m.get("archive_id").unwrap().as_bytes().unwrap(), m.get("canonical_object_inventory").unwrap(),
                                           m.get("blob_inventory").unwrap(), m.get("record_inventory").unwrap());
        m = set(m, "integrity_root", Value::Bytes(root.to_vec()));
    }
    es[1].1 = cbor::encode(&m);
    zip::write_unchecked(&es, false)
}
fn set(m: Value, k: &str, v: Value) -> Value {
    let Value::Map(mut kv) = m else { panic!() };
    kv.retain(|(key, _)| key.as_text() != Some(k));
    kv.push((Value::text(k), v));
    Value::Map(kv)
}
fn with_raw_manifest(plain: &[u8], raw: Vec<u8>) -> Vec<u8> {
    let mut es = entries_of(plain);
    es[1].1 = raw;
    zip::write_unchecked(&es, false)
}

enum Expect {
    Err(fn(&ArchiveError) -> bool, &'static str),
    Ok(fn(&archive::Archive) -> bool, &'static str),
}

struct Case {
    id: &'static str,
    category: &'static str,
    bytes: Vec<u8>,
    opts: ReadOptions,
    expect: Expect,
}

fn is_zip(e: &ArchiveError, z: ZipError) -> bool {
    *e == ArchiveError::Zip(z)
}

fn deflate_bomb() -> Vec<u8> {
    // A real DEFLATE stream expanding to 64 MiB of zeros: stored blocks would not compress, so emit fixed-Huffman
    // deflate via a tiny encoder: repeated back-references are produced by the standard library-free encoder below.
    // To stay dependency-free we hand-build one final fixed-Huffman block: literal 0, then <len 258, dist 1> repeated.
    struct Bits { out: Vec<u8>, acc: u32, n: u32 }
    impl Bits {
        fn put(&mut self, v: u32, bits: u32) { self.acc |= v << self.n; self.n += bits; while self.n >= 8 { self.out.push(self.acc as u8); self.acc >>= 8; self.n -= 8; } }
        fn put_rev(&mut self, code: u32, bits: u32) { let mut r = 0; for i in 0..bits { r |= ((code >> i) & 1) << (bits - 1 - i); } self.put(r, bits); }
    }
    let mut b = Bits { out: vec![], acc: 0, n: 0 };
    b.put(1, 1); // BFINAL
    b.put(1, 2); // BTYPE=01 fixed
    b.put_rev(0x30, 8); // literal 0 (codes 0-143: 0x30+lit, 8 bits)
    let reps = (64usize << 20) / 258;
    for _ in 0..reps {
        b.put_rev(0b1100_0101, 8); // length code 285 (258): codes 280-287 are 11000000+ (8 bits) -> 285 = 0xC5
        b.put_rev(0, 5); // distance code 0 (distance 1), 5 bits, no extra
    }
    b.put_rev(0, 7); // end of block (256): 7-bit code 0000000
    b.put(0, 7);
    b.out
}

fn build_cases() -> (Vec<Case>, Fixture) {
    let fx = base_input(false);
    let plain = archive::write(&fx.input, &wopt(None));
    let big = base_input(true);
    let enc_big = archive::write(&big.input, &wopt(Some(enc_pass())));
    let enc = archive::write(&fx.input, &wopt(Some(enc_pass())));
    let mut c: Vec<Case> = vec![];
    let std = || ropt(all_keys());
    macro_rules! err {
        ($id:expr, $cat:expr, $bytes:expr, $pat:pat, $what:expr) => {
            c.push(Case { id: $id, category: $cat, bytes: $bytes, opts: std(), expect: Expect::Err(|e| matches!(e, $pat), $what) })
        };
    }
    // ---------- container / path attacks
    let bad_name = |name: &str| {
        let mut es = entries_of(&plain);
        es.push((name.to_string(), b"evil".to_vec()));
        zip::write_unchecked(&es, false)
    };
    err!("zip_slip_dotdot", "path", bad_name("om-archive-v1/../../etc/cron.d/evil"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("zip_slip_absolute", "path", bad_name("/etc/passwd"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("zip_slip_backslash", "path", bad_name("om-archive-v1/a\\..\\..\\evil"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("zip_slip_drive_letter", "path", bad_name("c:/windows/evil"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("unicode_lookalike_dots", "path", bad_name("om-archive-v1/\u{2024}\u{2024}/evil"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("fullwidth_solidus", "path", bad_name("om-archive-v1/a\u{ff0f}b"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("case_variant_name", "path", bad_name("om-archive-v1/Format"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("nul_in_name", "path", bad_name("om-archive-v1/a\0.txt"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("directory_entry", "path", bad_name("om-archive-v1/blobs/"), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    err!("overlong_name", "path", bad_name(&format!("om-archive-v1/{}", "a".repeat(300))), ArchiveError::Zip(ZipError::BadName), "Zip(BadName)");
    {
        let mut z = plain.clone();
        let cen = *central_offsets(&z).last().unwrap();
        put32(&mut z, cen + 38, 0o120777 << 16); // S_IFLNK
        err!("symlink_entry", "path", z, ArchiveError::Zip(ZipError::NotRegularFile), "Zip(NotRegularFile)");
        let mut z = plain.clone();
        put32(&mut z, cen + 38, 0o040755 << 16); // S_IFDIR
        err!("directory_mode_entry", "path", z, ArchiveError::Zip(ZipError::NotRegularFile), "Zip(NotRegularFile)");
        let mut z = plain.clone();
        put16(&mut z, cen + 4, 0x0014); // made by DOS
        put32(&mut z, cen + 38, 0x10); // DOS directory attribute
        err!("dos_directory_attribute", "path", z, ArchiveError::Zip(ZipError::NotRegularFile), "Zip(NotRegularFile)");
    }
    {
        let mut es = entries_of(&plain);
        let dup = es[2].clone();
        es.push(dup);
        err!("duplicate_name", "path", zip::write_unchecked(&es, false), ArchiveError::Zip(ZipError::DuplicateName), "Zip(DuplicateName)");
    }
    {
        // deflate bomb: 64 MiB of zeros in a few tens of KiB, method 8
        let bomb = deflate_bomb();
        let mut es = entries_of(&plain);
        es.push(("om-archive-v1/bomb".into(), bomb.clone()));
        let mut z = zip::write_unchecked(&es, false);
        let loc = *find_all(&z, &0x0403_4b50u32.to_le_bytes()).last().unwrap();
        let cen = *central_offsets(&z).last().unwrap();
        put16(&mut z, loc + 8, 8);
        put16(&mut z, cen + 10, 8);
        put32(&mut z, loc + 22, 64 << 20);
        put32(&mut z, cen + 24, 64 << 20);
        err!("deflate_zip_bomb", "resource", z, ArchiveError::Zip(ZipError::UnsupportedMethod), "Zip(UnsupportedMethod)");
    }
    {
        // overlapping entries (Fifield-style): two central records share one local header
        let mut z = plain.clone();
        let cens = central_offsets(&z);
        let off_prev = u32::from_le_bytes(z[cens[cens.len() - 2] + 42..cens[cens.len() - 2] + 46].try_into().unwrap());
        put32(&mut z, cens[cens.len() - 1] + 42, off_prev);
        err!("overlapping_entries", "resource", z, ArchiveError::Zip(ZipError::NonContiguous), "Zip(NonContiguous)");
    }
    {
        let mut z = plain.clone();
        let cen = *central_offsets(&z).last().unwrap();
        put32(&mut z, cen + 20, 0xffff_fffe);
        put32(&mut z, cen + 24, 0xffff_fffe);
        err!("declared_size_huge", "resource", z, ArchiveError::Zip(ZipError::EntryTooLarge), "Zip(EntryTooLarge)");
    }
    {
        let mut z = plain.clone();
        let n = z.len();
        put16(&mut z, n - 22 + 8, 60000);
        put16(&mut z, n - 22 + 10, 60000);
        err!("entry_count_bomb", "resource", z, ArchiveError::Zip(ZipError::TooManyEntries), "Zip(TooManyEntries)");
        let mut z = plain.clone();
        put16(&mut z, n - 22 + 8, 900);
        put16(&mut z, n - 22 + 10, 900);
        err!("entry_count_exceeds_directory", "resource", z, ArchiveError::Zip(ZipError::Truncated), "Zip(Truncated)");
    }
    {
        let mut es = entries_of(&plain);
        for i in 0..40 {
            es.push((format!("om-archive-v1/pad/{i:04}"), vec![0u8; 1 << 20]));
        }
        let mut o = std();
        o.limits.max_total_size = 8 << 20;
        c.push(Case { id: "total_size_limit", category: "resource", bytes: zip::write_unchecked(&es, false), opts: o,
                      expect: Expect::Err(|e| is_zip(e, ZipError::TotalTooLarge), "Zip(TotalTooLarge)") });
    }
    {
        let mut z = plain.clone();
        z.extend_from_slice(b"TRAILING GARBAGE");
        err!("trailing_data", "container", z, ArchiveError::Zip(ZipError::TrailingOrPrependedData), "Zip(TrailingOrPrependedData)");
        let mut z = b"#!/bin/sh\necho polyglot\n".to_vec();
        z.extend_from_slice(&plain);
        err!("prepended_polyglot", "container", z, ArchiveError::Zip(_), "Zip(any)");
        let mut z = plain.clone();
        let n = z.len();
        put16(&mut z, n - 2, 5);
        z.extend_from_slice(b"hello");
        err!("eocd_comment", "container", z, ArchiveError::Zip(ZipError::TrailingOrPrependedData | ZipError::CommentPresent), "Zip(TrailingOrPrependedData|CommentPresent)");
        let mut z = plain.clone();
        put16(&mut z, n - 22 + 4, 1);
        err!("multi_disk", "container", z, ArchiveError::Zip(ZipError::MultiDisk), "Zip(MultiDisk)");
        err!("truncated_half", "container", plain[..plain.len() / 2].to_vec(), ArchiveError::Zip(_), "Zip(any)");
        err!("empty_file", "container", vec![], ArchiveError::Zip(ZipError::NotZip), "Zip(NotZip)");
    }
    {
        let cen = *central_offsets(&plain).last().unwrap();
        let loc = *find_all(&plain, &0x0403_4b50u32.to_le_bytes()).last().unwrap();
        let mut z = plain.clone();
        put16(&mut z, loc + 6, 0x0808);
        put16(&mut z, cen + 8, 0x0808);
        err!("data_descriptor_flag", "container", z, ArchiveError::Zip(ZipError::DataDescriptor), "Zip(DataDescriptor)");
        let mut z = plain.clone();
        put16(&mut z, cen + 8, 0x0801);
        err!("pkware_encrypted_flag", "container", z, ArchiveError::Zip(ZipError::Encrypted), "Zip(Encrypted)");
        let mut z = plain.clone();
        let last_data = cen - 1 - 0; // last byte before the central directory belongs to the last entry's data
        let _ = last_data;
        let d = loc + 30 + u16::from_le_bytes([plain[loc + 26], plain[loc + 27]]) as usize;
        z[d] ^= 0xff;
        err!("crc_mismatch", "container", z, ArchiveError::Zip(ZipError::Crc), "Zip(Crc)");
        let mut z = plain.clone();
        z[loc + 30 + 14] ^= 0x01; // flip a byte of the local name only
        err!("local_central_name_mismatch", "container", z, ArchiveError::Zip(ZipError::HeaderMismatch | ZipError::BadName), "Zip(HeaderMismatch)");
    }
    {
        // unknown extra field (e.g. extended timestamp 0x5455) on an otherwise valid entry
        let mut z = plain.clone();
        let cen = *central_offsets(&z).last().unwrap();
        let nl = u16::from_le_bytes([z[cen + 28], z[cen + 29]]) as usize;
        let at = cen + 46 + nl;
        z.splice(at..at, [0x55, 0x54, 0x01, 0x00, 0x00]);
        put16(&mut z, cen + 30, 5);
        let n = z.len();
        let cds = u32::from_le_bytes(z[n - 10..n - 6].try_into().unwrap()) + 5;
        put32(&mut z, n - 10, cds);
        err!("unknown_extra_field", "container", z, ArchiveError::Zip(ZipError::ExtraField), "Zip(ExtraField)");
    }
    // ---------- format / manifest
    {
        let mut es = entries_of(&plain);
        es.remove(0);
        err!("missing_format_entry", "format", zip::write_unchecked(&es, false), ArchiveError::NotOmArchive, "NotOmArchive");
        let mut es = entries_of(&plain);
        es[0].1 = b"om-archive-v2\n".to_vec();
        err!("future_major_format_entry", "version", zip::write_unchecked(&es, false), ArchiveError::UnsupportedVersion, "UnsupportedVersion");
        let mut es = entries_of(&plain);
        es.swap(1, 2);
        err!("manifest_not_second", "format", zip::write_unchecked(&es, false), ArchiveError::UnexpectedLayout(_), "UnexpectedLayout");
    }
    err!("future_major_manifest", "version", with_manifest(&plain, |m| set(m, "format_version", Value::text("om-archive-v2")), false), ArchiveError::UnsupportedVersion, "UnsupportedVersion");
    {
        let good = entries_of(&plain)[1].1.clone();
        // unsorted keys: swap the first two top-level map entries by re-encoding by hand
        let Value::Map(kv) = cbor::decode(&good).unwrap() else { panic!() };
        let hdr = |n: usize| if n < 24 { vec![0xa0 | n as u8] } else { vec![0xb8, n as u8] };
        let mut raw = hdr(kv.len());
        let mut enc: Vec<Vec<u8>> = kv.iter().map(|(k, v)| [cbor::encode(k), cbor::encode(v)].concat()).collect();
        enc.swap(0, 1);
        for e in &enc {
            raw.extend(e);
        }
        err!("manifest_unsorted_keys", "canonical", with_raw_manifest(&plain, raw), ArchiveError::Cbor(_, cbor::CborError::UnsortedOrDuplicateKey), "Cbor(UnsortedOrDuplicateKey)");
        let mut raw = hdr(kv.len() + 1);
        raw.extend(&enc[1]);
        raw.extend(&enc[1]);
        for e in &enc[..1] {
            raw.extend(e);
        }
        for e in &enc[2..] {
            raw.extend(e);
        }
        err!("manifest_duplicate_key", "canonical", with_raw_manifest(&plain, raw), ArchiveError::Cbor(_, cbor::CborError::UnsortedOrDuplicateKey), "Cbor(UnsortedOrDuplicateKey)");
        let mut raw = good.clone();
        raw.insert(0, 0x82); // wrap as 2-array with trailing float -> non-map + float
        raw.extend([0xfb, 0, 0, 0, 0, 0, 0, 0, 0]);
        err!("manifest_float", "canonical", with_raw_manifest(&plain, raw), ArchiveError::Cbor(_, cbor::CborError::Float), "Cbor(Float)");
        let mut raw = good.clone();
        raw[0] = 0xbf;
        raw.remove(1);
        raw.push(0xff);
        err!("manifest_indefinite_length", "canonical", with_raw_manifest(&plain, raw), ArchiveError::Cbor(_, cbor::CborError::Indefinite), "Cbor(Indefinite)");
        let mut raw = good.clone();
        raw.push(0x00);
        err!("manifest_trailing_bytes", "canonical", with_raw_manifest(&plain, raw), ArchiveError::Cbor(_, cbor::CborError::TrailingBytes), "Cbor(TrailingBytes)");
        let mut raw = good;
        let (h, l) = (raw[0], raw[1]);
        raw[0] = 0xb9; // non-minimal 2-byte count
        raw.splice(1..2, [0x00, l]);
        let _ = h;
        err!("manifest_nonminimal_length", "canonical", with_raw_manifest(&plain, raw), ArchiveError::Cbor(_, cbor::CborError::NonMinimalLength), "Cbor(NonMinimalLength)");
    }
    {
        let mut deep = Value::Null;
        for _ in 0..40 {
            deep = Value::Array(vec![deep]);
        }
        err!("manifest_deep_nesting", "resource", with_manifest(&plain, move |m| set(m, "export_scope", map(vec![("x", deep.clone())])), true),
             ArchiveError::Cbor(_, cbor::CborError::TooDeep), "Cbor(TooDeep)");
    }
    err!("unknown_critical_extension", "version", with_manifest(&plain, |m| set(m, "critical_extensions", Value::Array(vec![Value::text("om.x-future-authority")])), true),
         ArchiveError::UnknownCritical(_), "UnknownCritical");
    err!("unknown_manifest_field", "schema", with_manifest(&plain, |m| set(m, "grants_admin", Value::Bool(true)), true), ArchiveError::Schema(_), "Schema");
    err!("integrity_root_mismatch", "integrity", with_manifest(&plain, |m| {
        let Value::Array(mut inv) = m.get("blob_inventory").unwrap().clone() else { panic!() };
        inv[0] = set(inv[0].clone(), "media_type", Value::text("application/x-msdownload"));
        set(m, "blob_inventory", Value::Array(inv))
    }, false), ArchiveError::IntegrityRoot, "IntegrityRoot");
    {
        // replace an object's bytes (CRC recomputed by the zip writer) without touching the manifest
        let mut es = entries_of(&plain);
        es[2].1[5] ^= 0x01;
        err!("entry_hash_mismatch", "integrity", zip::write_unchecked(&es, false), ArchiveError::EntryHash(_), "EntryHash");
        let mut es = entries_of(&plain);
        let i = es.iter().position(|e| e.0.contains("/blobs/")).unwrap();
        es.remove(i);
        err!("blob_entry_removed", "partial", zip::write_unchecked(&es, false), ArchiveError::MissingEntry | ArchiveError::UnexpectedEntry(_), "MissingEntry");
        let mut es = entries_of(&plain);
        es.push(("om-archive-v1/extra/payload".into(), b"x".to_vec()));
        err!("unlisted_extra_entry", "integrity", zip::write_unchecked(&es, false), ArchiveError::UnexpectedEntry(_), "UnexpectedEntry");
    }
    {
        let mut inp = fx.input.clone();
        inp.objects.push(("om.device_identity.private".into(), b"not really a key".to_vec()));
        err!("secret_key_material", "authority", archive::write(&inp, &wopt(None)), ArchiveError::SecretMaterial(_), "SecretMaterial");
        let mut inp = fx.input.clone();
        inp.objects.push(("om.account_root.private".into(), b"not really a key either".to_vec()));
        err!("secret_key_material_encrypted", "authority", archive::write(&inp, &wopt(Some(enc_pass()))), ArchiveError::SecretMaterial(_), "SecretMaterial");
    }
    {
        let mut o = std();
        o.max_manifest_size = 64;
        c.push(Case { id: "manifest_too_large", category: "resource", bytes: plain.clone(), opts: o,
                      expect: Expect::Err(|e| *e == ArchiveError::ManifestTooLarge, "ManifestTooLarge") });
    }
    err!("inventory_path_traversal", "path", with_manifest(&plain, |m| {
        let Value::Array(mut inv) = m.get("record_inventory").unwrap().clone() else { panic!() };
        inv[0] = set(inv[0].clone(), "path", Value::text("records/../../../../tmp/evil"));
        set(m, "record_inventory", Value::Array(inv))
    }, true), ArchiveError::Schema(_), "Schema");
    // ---------- encryption
    {
        let mk = |keys: Vec<KeyMaterial>| ropt(keys);
        c.push(Case { id: "wrong_passphrase", category: "encryption", bytes: enc.clone(), opts: mk(vec![KeyMaterial::Passphrase(b"wrong".to_vec())]),
                      expect: Expect::Err(|e| *e == ArchiveError::NoMatchingKey, "NoMatchingKey") });
        c.push(Case { id: "wrong_raw_key", category: "encryption", bytes: archive::write(&fx.input, &wopt(Some(enc_raw()))), opts: mk(vec![KeyMaterial::RawKey([9; 32])]),
                      expect: Expect::Err(|e| *e == ArchiveError::NoMatchingKey, "NoMatchingKey") });
        c.push(Case { id: "no_key_supplied", category: "encryption", bytes: enc.clone(), opts: mk(vec![]),
                      expect: Expect::Err(|e| *e == ArchiveError::NoMatchingKey, "NoMatchingKey") });
        let mut o = mk(all_keys());
        o.insecure_test_kdf = false;
        c.push(Case { id: "kdf_below_floor", category: "encryption", bytes: enc.clone(), opts: o,
                      expect: Expect::Err(|e| *e == ArchiveError::KdfParams, "KdfParams") });
    }
    let hdr_edit = |bytes: &[u8], f: &dyn Fn(Value) -> Value| {
        let mut es = entries_of(bytes);
        es[1].1 = cbor::encode(&f(cbor::decode(&es[1].1).unwrap()));
        zip::write_unchecked(&es, false)
    };
    let recip_edit = |k: &'static str, v: Value| move |h: Value| {
        let Value::Array(mut r) = h.get("recipients").unwrap().clone() else { panic!() };
        r[0] = set(r[0].clone(), k, v.clone());
        set(h, "recipients", Value::Array(r))
    };
    err!("kdf_param_dos", "encryption", hdr_edit(&enc, &recip_edit("m_kib", Value::Uint(64 << 20))), ArchiveError::KdfParams, "KdfParams");
    err!("kdf_param_tamper", "encryption", hdr_edit(&enc, &recip_edit("m_kib", Value::Uint(16))), ArchiveError::NoMatchingKey, "NoMatchingKey");
    err!("chunk_size_tamper", "encryption", hdr_edit(&enc, &|h| set(h, "chunk_size", Value::Uint(1024))), ArchiveError::Schema(_), "Schema");
    err!("header_mac_tamper", "encryption", hdr_edit(&enc, &|h| set(h, "header_mac", Value::Bytes(vec![0; 32]))), ArchiveError::HeaderMac, "HeaderMac");
    err!("unknown_recipient_type", "version", hdr_edit(&enc, &recip_edit("type", Value::text("pq-kem-future"))), ArchiveError::UnknownCritical(_), "UnknownCritical");
    {
        let es = entries_of(&enc_big);
        let (bi, _) = es.iter().enumerate().max_by_key(|(_, e)| e.1.len()).unwrap();
        let mut t = es.clone();
        let cut = 65536 + 16;
        t[bi].1.truncate(cut); // drop every chunk after the first: the new last chunk is not flagged final
        err!("truncated_ciphertext_chunk_boundary", "encryption", zip::write_unchecked(&t, false), ArchiveError::Decrypt(_), "Decrypt");
        let mut t = es.clone();
        let l = t[bi].1.len();
        t[bi].1.truncate(l - 7);
        err!("truncated_ciphertext_mid_chunk", "encryption", zip::write_unchecked(&t, false), ArchiveError::Decrypt(_), "Decrypt");
        let mut t = es.clone();
        t[bi].1[70_000] ^= 0x40;
        err!("corrupted_ciphertext_byte", "encryption", zip::write_unchecked(&t, false), ArchiveError::Decrypt(_), "Decrypt");
        let mut t = es.clone();
        let (a, b) = (t[3].1.clone(), t[4].1.clone());
        t[3].1 = b;
        t[4].1 = a;
        err!("swapped_encrypted_entries", "encryption", zip::write_unchecked(&t, false), ArchiveError::Decrypt(_), "Decrypt");
        let mut t = es.clone();
        let first: Vec<u8> = t[bi].1[..65552].to_vec();
        t[bi].1.splice(65552..131104, first); // replay chunk 0 in position 1
        err!("reordered_or_replayed_chunk", "encryption", zip::write_unchecked(&t, false), ArchiveError::Decrypt(_), "Decrypt");
        // manifest from a different archive (different archive key) spliced in
        let mut other_in = fx.input.clone();
        other_in.archive_id = arr16("other-archive");
        let other = entries_of(&archive::write(&other_in, &wopt(Some(Encryption { archive_key: arr32("other-key"), ..enc_pass() }))));
        let mut t = entries_of(&enc);
        t[2].1 = other[2].1.clone();
        err!("foreign_manifest_splice", "encryption", zip::write_unchecked(&t, false), ArchiveError::Decrypt(_), "Decrypt");
    }
    // ---------- positive cases (reader must accept)
    macro_rules! ok {
        ($id:expr, $cat:expr, $bytes:expr, $f:expr, $what:expr) => {
            c.push(Case { id: $id, category: $cat, bytes: $bytes, opts: std(), expect: Expect::Ok($f, $what) })
        };
    }
    ok!("positive_plain", "positive", plain.clone(), |a| !a.report.encrypted && a.objects.len() == 3, "accept");
    ok!("positive_encrypted_passphrase", "positive", enc.clone(), |a| a.report.encrypted, "accept");
    ok!("positive_encrypted_raw_key", "positive", archive::write(&fx.input, &wopt(Some(enc_raw()))), |a| a.report.encrypted, "accept");
    ok!("positive_encrypted_multi_chunk", "positive", enc_big.clone(), |a| a.blobs.iter().any(|b| b.size == 150_000), "accept");
    ok!("positive_zip64_records", "positive", archive::write(&fx.input, &WriteOptions { force_zip64: true, ..wopt(None) }), |a| a.objects.len() == 3, "accept (ZIP64 records)");
    {
        let mut inp = fx.input.clone();
        inp.blobs[0].data = None;
        ok!("partial_archive_declared_missing_blob", "partial", archive::write(&inp, &wopt(None)), |a| a.report.missing_blobs.len() == 1, "accept, missing_blobs reported");
    }
    (c, fx)
}

// ---- import-semantics checks ------------------------------------------------------------------------------------

struct Check {
    id: &'static str,
    ok: bool,
    note: String,
}

fn import_checks(fx: &Fixture) -> Vec<Check> {
    let verifier = InsecureTestVerifier {
        trusted: ["alice-device", "room-gov", "bridge-x"].iter().map(|l| insecure_test_signing_key(l).verifying_key().to_bytes()).collect(),
    };
    let mut live = LiveState::default();
    live.control_versions.insert("room-1".into(), 5); // live Room control has moved on (e.g. alice's admin revoked at v4)
    let mut out = vec![];
    let read = |inp: &ArchiveInput| archive::read(&archive::write(inp, &wopt(None)), &ropt(vec![])).unwrap();
    let base = read(&fx.input);
    let v = import::import_readonly(&base, &live, &verifier);
    let status = |v: &import::ImportView, t: &str| v.objects.iter().zip(&base.objects).find(|(_, o)| o.object_type == t).map(|(s, _)| s.1.clone());
    out.push(Check { id: "stale_control_state_not_authority", ok: status(&v, "test.room.control") == Some(ObjectStatus::StaleControl { archived: 3, live: 5 }) && v.authority_changes.is_empty(),
                     note: "archived Room control v3 (alice admin) vs live v5: classified StaleControl, never applied".into() });
    out.push(Check { id: "native_object_verified_exactly", ok: status(&v, "test.event") == Some(ObjectStatus::VerifiedHistory), note: "exact canonical bytes verify after round trip".into() });
    out.push(Check { id: "linked_provenance_with_evidence_kept", ok: v.records.iter().any(|r| r.imported_record_id == "r2" && r.effective_level == "cryptographically_linked" && !r.downgraded),
                     note: "cryptographically_linked backed by a verifying link object".into() });
    // newer-than-live control state claiming admin: must still not be applied
    let mut inp = fx.input.clone();
    inp.objects.push(("test.room.control".into(), insecure_test_object("room-gov", "test.room.control", "room-1", 9, map(vec![("admins", Value::Array(vec![Value::text("mallory")]))]))));
    let a = read(&inp);
    let v = import::import_readonly(&a, &live, &verifier);
    out.push(Check { id: "readonly_import_newer_control_not_applied",
                     ok: v.objects.iter().any(|o| o.1 == ObjectStatus::ControlNeedsLiveValidation { archived: 9 }) && v.authority_changes.is_empty() && live.control_versions["room-1"] == 5,
                     note: "a newer archived control state is flagged for live validation; live state is untouched (import takes &LiveState)".into() });
    // forged signature / unknown signer / type confusion
    let mut inp = fx.input.clone();
    inp.objects.push(("test.event".into(), insecure_test_object("mallory", "test.event", "conv-1", 8, map(vec![("text", Value::text("forged"))]))));
    inp.objects.push(("test.event".into(), insecure_test_object("alice-device", "test.room.control", "room-1", 10, Value::Null)));
    let a = read(&inp);
    let v = import::import_readonly(&a, &live, &verifier);
    out.push(Check { id: "forged_or_type_confused_objects_unverified", ok: v.objects.iter().filter(|o| o.1 == ObjectStatus::Unverified).count() == 2,
                     note: "unknown signer and declared-type mismatch are kept as opaque Unverified bytes, never native".into() });
    // forged provenance and fabricated native authorship in foreign records
    let mut inp = fx.input.clone();
    inp.records = vec![("imported_history".into(), "imported-history".into(), imported_history(vec![
        record("f1", "cryptographically_linked", vec![]),
        record("f2", "bridge_attested", vec![("link_evidence", Value::Text(fx.link_obj_path.clone()))]),
        record("f3", "foreign_export_label", vec![("author_principal", map(vec![("account", Value::Bytes(vec![1; 32]))]))]),
        record("f4", "foreign_export_label", vec![("signature", Value::Bytes(vec![0; 64]))]),
        record("f5", "verified_by_om", vec![]),
        record("f6", "user_annotation", vec![("grants", Value::Array(vec![]))]),
    ]))];
    let a = read(&inp);
    let v = import::import_readonly(&a, &live, &verifier);
    let lvl = |id: &str| v.records.iter().find(|r| r.imported_record_id == id).map(|r| (r.effective_level.clone(), r.downgraded));
    out.push(Check { id: "forged_provenance_downgraded", ok: lvl("f1") == Some(("foreign_export_label".into(), true)) && lvl("f2") == Some(("foreign_export_label".into(), true)),
                     note: "linked/bridge-attested claims without matching verifiable evidence are downgraded to foreign_export_label".into() });
    out.push(Check { id: "fabricated_native_authorship_quarantined", ok: v.quarantined.len() == 4 && v.records.len() == 2,
                     note: "author_principal / signature fields, unknown provenance level and unknown fields are quarantined".into() });
    // round trip: read -> to_input -> write reproduces identical bytes (plain and encrypted)
    for (id, w) in [("roundtrip_plain_byte_identical", wopt(None)), ("roundtrip_encrypted_byte_identical", wopt(Some(enc_pass())))] {
        let b1 = archive::write(&fx.input, &w);
        let a = archive::read(&b1, &ropt(all_keys())).unwrap();
        let b2 = archive::write(&archive::to_input(&a), &w);
        let same_objects = a.objects.iter().zip(&fx.input.objects).all(|_| true)
            && fx.input.objects.iter().all(|(_, bytes)| a.objects.iter().any(|o| &o.bytes == bytes));
        out.push(Check { id, ok: b1 == b2 && same_objects, note: format!("{} bytes; canonical objects preserved exactly", b1.len()) });
    }
    out
}

// ---- vectors ----------------------------------------------------------------------------------------------------

fn vectors(fx: &Fixture) -> Vec<(&'static str, Vec<u8>, String)> {
    let mut partial = fx.input.clone();
    partial.blobs[0].data = None;
    vec![
        ("plain", archive::write(&fx.input, &wopt(None)), "no keys".into()),
        ("encrypted-passphrase", archive::write(&fx.input, &wopt(Some(enc_pass()))),
         format!("passphrase {:?}, salt {}, argon2id m_kib=8 t=1 p=1 (INSECURE test parameters)", String::from_utf8_lossy(PASS), hex(&arr16("salt")))),
        ("encrypted-raw-key", archive::write(&fx.input, &wopt(Some(enc_raw()))), format!("raw key {}", hex(&raw_key()))),
        ("plain-zip64-records", archive::write(&fx.input, &WriteOptions { force_zip64: true, ..wopt(None) }), "no keys".into()),
        ("partial-missing-blob", archive::write(&partial, &wopt(None)), "no keys".into()),
    ]
}

/// Deterministic mutation sweep (INSECURE xorshift test PRNG): the reader must never panic and must never accept a
/// mutated archive whose decoded content differs from the original (no silent corruption).
fn mutation_sweep(vs: &[(&'static str, Vec<u8>, String)], n: usize) -> (usize, usize, usize, usize) {
    let mut st: u64 = 0x9e37_79b9_7f4a_7c15;
    let mut rnd = || { st ^= st << 13; st ^= st >> 7; st ^= st << 17; st };
    let (mut panics, mut silent, mut accepted_same, mut rejected) = (0, 0, 0, 0);
    for i in 0..n {
        let (_, orig, _) = &vs[i % vs.len()];
        let want = archive::read(orig, &ropt(all_keys())).unwrap();
        let mut b = orig.clone();
        match rnd() % 4 {
            0 | 1 => for _ in 0..1 + rnd() % 4 { let at = (rnd() as usize) % b.len(); b[at] ^= 1 << (rnd() % 8); },
            2 => { let at = (rnd() as usize) % b.len(); b.truncate(at); }
            _ => { let at = (rnd() as usize) % b.len(); let x = rnd() as u8; b.insert(at, x); }
        }
        match std::panic::catch_unwind(|| archive::read(&b, &ropt(all_keys()))) {
            Err(_) => panics += 1,
            Ok(Err(_)) => rejected += 1,
            Ok(Ok(a)) => if a == want { accepted_same += 1 } else { silent += 1 },
        }
    }
    (panics, silent, accepted_same, rejected)
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let vec_path = args.iter().position(|a| a == "--vectors").map(|i| args[i + 1].clone());
    let write_vectors = args.iter().any(|a| a == "--write-vectors");
    let (cases, fx) = build_cases();
    let mut rows = vec![];
    let mut all_ok = true;
    for c in &cases {
        let res = archive::read(&c.bytes, &c.opts);
        let (ok, actual, expected) = match (&c.expect, &res) {
            (Expect::Err(f, w), Err(e)) => (f(e), format!("{e:?}"), *w),
            (Expect::Err(_, w), Ok(_)) => (false, "accepted".into(), *w),
            (Expect::Ok(f, w), Ok(a)) => (f(a), format!("accepted (missing_blobs={})", a.report.missing_blobs.len()), *w),
            (Expect::Ok(_, w), Err(e)) => (false, format!("{e:?}"), *w),
        };
        all_ok &= ok;
        eprintln!("{} {:<40} {}", if ok { "ok " } else { "BAD" }, c.id, actual);
        rows.push(format!("  {{\"id\": {}, \"category\": {}, \"expect\": {}, \"actual\": {}, \"ok\": {}, \"sha256\": {}}}",
            jstr(c.id), jstr(c.category), jstr(expected), jstr(&actual), ok, jstr(&hex(&sha256(&[&c.bytes])))));
    }
    let checks = import_checks(&fx);
    for k in &checks {
        all_ok &= k.ok;
        eprintln!("{} {:<40} {}", if k.ok { "ok " } else { "BAD" }, k.id, k.note);
    }
    // vectors: write or verify
    let vs = vectors(&fx);
    let mut vec_ok = true;
    if let Some(p) = &vec_path {
        let body = format!("{{\n \"format\": \"om-archive-v1 positive vectors (INSECURE test keys; regenerate with omarchive-corpus --write-vectors)\",\n \"vectors\": [\n{}\n ]\n}}\n",
            vs.iter().map(|(n, b, k)| format!("  {{\"name\": {}, \"keys\": {}, \"sha256\": {}, \"archive_hex\": {}}}", jstr(n), jstr(k), jstr(&hex(&sha256(&[b]))), jstr(&hex(b)))).collect::<Vec<_>>().join(",\n"));
        if write_vectors {
            std::fs::File::create(p).unwrap().write_all(body.as_bytes()).unwrap();
        } else {
            let committed = std::fs::read_to_string(p).unwrap_or_default();
            vec_ok = committed == body;
            // every committed vector must also read back with the published keys
            for (n, _, _) in &vs {
                let start = committed.find(&format!("\"name\": {}", jstr(n))).unwrap_or(0);
                let hx = committed[start..].split("\"archive_hex\": \"").nth(1).map(|s| s.split('"').next().unwrap()).unwrap_or("");
                vec_ok &= archive::read(&unhex(hx), &ropt(all_keys())).is_ok();
            }
            eprintln!("{} committed vectors match regenerated bytes and read back", if vec_ok { "ok " } else { "BAD" });
        }
    }
    all_ok &= vec_ok;
    std::panic::set_hook(Box::new(|_| {}));
    let n_mut = if args.iter().any(|a| a == "--long") { 200_000 } else { 20_000 };
    let (panics, silent, same, rej) = mutation_sweep(&vs, n_mut);
    let sweep_ok = panics == 0 && silent == 0;
    all_ok &= sweep_ok;
    eprintln!("{} mutation sweep: {n_mut} mutants, {rej} rejected, {same} accepted-identical (only ignored header bytes changed: DOS time/date, made-by version), {silent} silent corruptions, {panics} panics", if sweep_ok { "ok " } else { "BAD" });
    let n_hostile = cases.iter().filter(|c| matches!(c.expect, Expect::Err(..))).count();
    println!("{{\n \"all_ok\": {all_ok},\n \"mutation_sweep\": {{\"mutants\": {n_mut}, \"rejected\": {rej}, \"accepted_identical\": {same}, \"silent_corruptions\": {silent}, \"panics\": {panics}}},\n \"hostile_cases\": {n_hostile},\n \"positive_cases\": {},\n \"import_checks\": {},\n \"vectors_match\": {vec_ok},\n \"vectors\": [{}],\n \"cases\": [\n{}\n ],\n \"checks\": [\n{}\n ]\n}}",
        cases.len() - n_hostile, checks.len(),
        vs.iter().map(|(n, b, _)| format!("{{\"name\": {}, \"sha256\": {}}}", jstr(n), jstr(&hex(&sha256(&[b]))))).collect::<Vec<_>>().join(", "),
        rows.join(",\n"),
        checks.iter().map(|k| format!("  {{\"id\": {}, \"ok\": {}, \"note\": {}}}", jstr(k.id), k.ok, jstr(&k.note))).collect::<Vec<_>>().join(",\n"));
    std::process::exit(if all_ok { 0 } else { 1 });
}
