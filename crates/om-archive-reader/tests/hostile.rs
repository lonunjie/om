mod support;
use om_archive_reader::{ArchiveError, Limits, read_archive};
use support::{build, build_extra, entries_from_plain, find, set16, set32};

struct Case {
    name: String,
    bytes: Vec<u8>,
    error: &'static str,
    limits: Limits,
}
fn add(cases: &mut Vec<Case>, name: impl Into<String>, bytes: Vec<u8>, error: &'static str) {
    cases.push(Case {
        name: name.into(),
        bytes,
        error,
        limits: Limits::default(),
    });
}
fn limit(
    cases: &mut Vec<Case>,
    name: &str,
    bytes: Vec<u8>,
    error: &'static str,
    change: impl FnOnce(&mut Limits),
) {
    let mut limits = Limits::default();
    change(&mut limits);
    cases.push(Case {
        name: name.into(),
        bytes,
        error,
        limits,
    });
}
fn manifest_edit(edit: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
    let mut entries = entries_from_plain();
    edit(&mut entries[1].1);
    build(&entries).bytes
}
fn payload_edit(edit: impl FnOnce(&mut Vec<(String, Vec<u8>)>)) -> Vec<u8> {
    let mut entries = entries_from_plain();
    edit(&mut entries);
    build(&entries).bytes
}

// Deterministic generator; no runtime file I/O. All cases are freshly built
// and not merely a pre-existing hostile corpus.
fn hostile_cases() -> Vec<Case> {
    let mut cases = Vec::new();
    let entries = entries_from_plain();
    let base = build(&entries);
    assert!(read_archive(&base.bytes, &Limits::default()).is_ok());

    for (name, bad) in [
        ("dot-dot", "om-archive-v1/a/../z"),
        ("single-dot", "om-archive-v1/a/./z"),
        ("absolute", "/om-archive-v1/escape"),
        ("backslash", "om-archive-v1/a\\b"),
        ("drive-letter", "C:/om-archive-v1/a"),
        ("unicode-dot", "om-archive-v1/a\u{2024}b"),
        ("unicode-slash", "om-archive-v1/a\u{ff0f}b"),
        ("upper-case", "om-archive-v1/Canonical/obj"),
        ("nul-byte", "om-archive-v1/a\0b"),
        ("directory-entry", "om-archive-v1/a/"),
        ("empty-segment", "om-archive-v1/a//b"),
    ] {
        add(
            &mut cases,
            name,
            payload_edit(|e| e[2].0 = bad.into()),
            "Zip(Path)",
        );
    }
    add(
        &mut cases,
        "overlong-name",
        payload_edit(|e| {
            e[2].0 = format!("om-archive-v1/{}", "x".repeat(129));
        }),
        "Zip(Path)",
    );
    add(
        &mut cases,
        "duplicate-name",
        payload_edit(|e| {
            let duplicate = e[1].0.clone();
            e[2].0 = duplicate;
        }),
        "Zip(Duplicate)",
    );

    for (name, mode) in [
        ("symlink", 0o120777),
        ("directory-mode", 0o040755),
        ("device-mode", 0o020600),
    ] {
        let mut z = build(&entries);
        set32(&mut z.bytes, z.central[2] + 38, (mode as u32) << 16);
        add(&mut cases, name, z.bytes, "Zip(Mode)");
    }
    let mut z = build(&entries);
    set32(&mut z.bytes, z.central[2] + 38, (0o100644u32 << 16) | 0x10);
    add(&mut cases, "dos-directory-bit", z.bytes, "Zip(Mode)");

    let mut z = build(&entries);
    set16(&mut z.bytes, z.locals[2] + 8, 8);
    set16(&mut z.bytes, z.central[2] + 10, 8);
    add(
        &mut cases,
        "deflate-bomb-rejected-before-inflate",
        z.bytes,
        "Zip(Compression)",
    );
    let mut z = build(&entries);
    set32(&mut z.bytes, z.central[1] + 42, 0);
    add(
        &mut cases,
        "overlapping-local-headers",
        z.bytes,
        "Zip(Structure)",
    );
    let mut z = build(&entries);
    set32(&mut z.bytes, z.central[2] + 20, 0x7fff_ffff);
    set32(&mut z.bytes, z.central[2] + 24, 0x7fff_ffff);
    set32(&mut z.bytes, z.locals[2] + 18, 0x7fff_ffff);
    set32(&mut z.bytes, z.locals[2] + 22, 0x7fff_ffff);
    limit(
        &mut cases,
        "declared-huge-size",
        z.bytes,
        "Zip(SizeLimit)",
        |l| l.max_entry_size = 1_000_000,
    );
    limit(
        &mut cases,
        "count-limit",
        base.bytes.clone(),
        "Zip(CountLimit)",
        |l| l.max_entries = 6,
    );
    limit(
        &mut cases,
        "entry-size-limit",
        base.bytes.clone(),
        "Zip(SizeLimit)",
        |l| l.max_entry_size = 150,
    );
    limit(
        &mut cases,
        "total-size-limit",
        base.bytes.clone(),
        "Zip(TotalSizeLimit)",
        |l| l.max_total_size = 1500,
    );
    let mut z = build(&entries);
    set16(&mut z.bytes, z.eocd + 8, 999);
    set16(&mut z.bytes, z.eocd + 10, 999);
    add(
        &mut cases,
        "entry-count-plausibility",
        z.bytes,
        "Zip(Structure)",
    );

    let mut z = build(&entries);
    z.bytes.push(0);
    add(&mut cases, "trailing-data", z.bytes, "Zip(Structure)");
    let mut z = b"prefix".to_vec();
    z.extend_from_slice(&base.bytes);
    add(&mut cases, "prepended-polyglot", z, "Zip(Structure)");
    let mut z = build(&entries);
    set16(&mut z.bytes, z.eocd + 20, 1);
    add(&mut cases, "eocd-comment", z.bytes, "Zip(Structure)");
    let mut z = build(&entries);
    set16(&mut z.bytes, z.eocd + 4, 1);
    add(&mut cases, "multi-disk", z.bytes, "Zip(Structure)");
    add(
        &mut cases,
        "truncated",
        base.bytes[..11].to_vec(),
        "Zip(Truncated)",
    );
    add(&mut cases, "empty", vec![], "Zip(Truncated)");
    for (name, flag) in [
        ("descriptor", 0x0808),
        ("pkware-encryption", 0x0801),
        ("strong-encryption", 0x0840),
    ] {
        let mut z = build(&entries);
        set16(&mut z.bytes, z.locals[2] + 6, flag);
        set16(&mut z.bytes, z.central[2] + 8, flag);
        add(&mut cases, name, z.bytes, "Zip(Flags)");
    }
    let mut z = build(&entries);
    z.bytes[z.data[2]] ^= 1;
    add(&mut cases, "crc-mismatch", z.bytes, "Zip(Crc)");
    let mut z = build(&entries);
    z.bytes[z.locals[2] + 30] = b'X';
    add(
        &mut cases,
        "local-central-name-mismatch",
        z.bytes,
        "Zip(Mismatch)",
    );
    let mut z = build(&entries);
    set32(&mut z.bytes, z.locals[2] + 14, 1234);
    add(
        &mut cases,
        "local-central-crc-mismatch",
        z.bytes,
        "Zip(Mismatch)",
    );
    let mut z = build(&entries);
    set16(&mut z.bytes, z.locals[2] + 4, 46);
    set16(&mut z.bytes, z.central[2] + 6, 46);
    add(
        &mut cases,
        "version-needed-too-high",
        z.bytes,
        "Zip(Structure)",
    );
    add(
        &mut cases,
        "unknown-extra",
        build_extra(&entries, Some(2)).bytes,
        "Zip(Extra)",
    );

    add(
        &mut cases,
        "missing-format",
        payload_edit(|e| e[0].0 = "om-archive-v1/notformat".into()),
        "NotOmArchive",
    );
    add(
        &mut cases,
        "format-future-major",
        payload_edit(|e| e[0].1 = b"om-archive-v2\n".to_vec()),
        "UnsupportedVersion",
    );
    add(
        &mut cases,
        "wrong-layout",
        payload_edit(|e| e[1].0 = "om-archive-v1/manifest2.cbor".into()),
        "UnexpectedLayout",
    );
    add(
        &mut cases,
        "manifest-future-major",
        manifest_edit(|m| {
            let p = find(m, b"om-archive-v1");
            m[p + 12] = b'2';
        }),
        "UnsupportedVersion",
    );

    add(
        &mut cases,
        "cbor-map-unsorted",
        manifest_edit(|m| {
            let first = m[1..29].to_vec();
            let second = m[29..41].to_vec();
            m.splice(1..41, second.into_iter().chain(first));
        }),
        "Cbor",
    );
    add(
        &mut cases,
        "cbor-duplicate-key",
        manifest_edit(|m| {
            let pos = find(m, b"extensions");
            m[pos..pos + 10].copy_from_slice(b"archive_id");
        }),
        "Cbor",
    );
    add(
        &mut cases,
        "cbor-float",
        manifest_edit(|m| *m = vec![0xfb, 0, 0, 0, 0, 0, 0, 0, 0]),
        "Cbor",
    );
    add(
        &mut cases,
        "cbor-indefinite",
        manifest_edit(|m| m[0] = 0xbf),
        "Cbor",
    );
    add(
        &mut cases,
        "cbor-trailing",
        manifest_edit(|m| m.push(0x00)),
        "Cbor",
    );
    add(
        &mut cases,
        "cbor-nonminimal-length",
        manifest_edit(|m| {
            m.splice(0..1, [0xb8, 15]);
        }),
        "Cbor",
    );
    add(
        &mut cases,
        "cbor-deep",
        manifest_edit(|m| {
            *m = vec![0x81; 17];
            m.push(0xf6);
        }),
        "Cbor",
    );
    add(
        &mut cases,
        "unknown-critical",
        manifest_edit(|m| {
            let pos = find(m, b"critical_extensions") + b"critical_extensions".len();
            assert_eq!(m[pos], 0x80);
            m.splice(pos..pos + 1, [0x81, 0x61, b'x']);
        }),
        "UnknownCritical",
    );
    add(
        &mut cases,
        "unknown-manifest-key",
        manifest_edit(|m| {
            let p = find(m, b"export_scope");
            m[p + 11] = b'z';
        }),
        "Schema",
    );
    add(
        &mut cases,
        "root-mismatch",
        manifest_edit(|m| {
            let pos = find(m, b"integrity_root") + b"integrity_root".len();
            assert_eq!(&m[pos..pos + 2], &[0x58, 0x20]);
            m[pos + 2] ^= 0x80;
        }),
        "IntegrityRoot",
    );
    add(
        &mut cases,
        "entry-hash-mismatch",
        payload_edit(|e| e[2].1[0] ^= 1),
        "EntryHash",
    );
    add(
        &mut cases,
        "entry-size-mismatch",
        payload_edit(|e| e[2].1.push(0)),
        "EntrySize",
    );
    add(
        &mut cases,
        "removed-payload",
        payload_edit(|e| {
            e.remove(2);
        }),
        "MissingEntry",
    );
    add(
        &mut cases,
        "unlisted-payload",
        payload_edit(|e| {
            e.push(("om-archive-v1/records/unlisted.cbor".into(), vec![0x80]));
        }),
        "UnexpectedEntry",
    );
    limit(
        &mut cases,
        "manifest-size-limit",
        base.bytes.clone(),
        "ManifestSizeLimit",
        |l| l.max_manifest_size = 50,
    );
    add(
        &mut cases,
        "inventory-path-traversal",
        manifest_edit(|m| {
            let p = find(m, b"records/imported-history.cbor");
            m[p] = b'/';
        }),
        "Schema",
    );
    add(
        &mut cases,
        "secret-material",
        manifest_edit(|m| {
            let p = find(m, b"test.event");
            assert_eq!(m[p - 1], 0x6a);
            m.splice(
                p - 1..p + 10,
                std::iter::once(0x6f).chain(b"om.device_vault".iter().copied()),
            );
        }),
        "SecretMaterial",
    );

    let forced = support::vectors()
        .into_iter()
        .find(|v| v.name == "plain-zip64-records")
        .unwrap()
        .bytes;
    let mut bad = forced;
    let locator = bad.len() - 22 - 20;
    set32(&mut bad, locator + 16, 2);
    add(&mut cases, "zip64-multidisk-locator", bad, "Zip(Structure)");
    cases
}

#[test]
fn generated_hostile_plain_archive_classes() {
    let cases = hostile_cases();
    assert!(cases.len() >= 45, "hostile coverage regressed");
    for case in cases {
        let result = std::panic::catch_unwind(|| read_archive(&case.bytes, &case.limits));
        assert!(result.is_ok(), "panic: {}", case.name);
        let err: ArchiveError = result.unwrap().unwrap_err();
        assert_eq!(err.class_name(), case.error, "hostile case {}", case.name);
    }
}
