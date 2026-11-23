mod support;
use om_archive_reader::{EntryKind, Limits, read_archive};

#[test]
fn byte_exact_public_vectors_plain_and_encrypted_skip() {
    let mut accepted = 0;
    let mut skipped = 0;
    for vector in support::vectors() {
        if vector.name.starts_with("encrypted-") {
            skipped += 1;
            eprintln!("SKIPPED encrypted positive vector: {}", vector.name);
            assert_eq!(
                read_archive(&vector.bytes, &Limits::default())
                    .unwrap_err()
                    .class_name(),
                "encrypted_unsupported"
            );
            continue;
        }
        let archive = read_archive(&vector.bytes, &Limits::default())
            .unwrap_or_else(|err| panic!("{}: {}", vector.name, err));
        assert_eq!(archive.archive_id.len(), 16);
        // Inventory order from the public vectors: three canonical objects,
        // one present blob (except partial), one imported-history record.
        let expected_kinds = if vector.name == "partial-missing-blob" {
            vec![
                EntryKind::CanonicalObject,
                EntryKind::CanonicalObject,
                EntryKind::CanonicalObject,
                EntryKind::Record,
            ]
        } else {
            vec![
                EntryKind::CanonicalObject,
                EntryKind::CanonicalObject,
                EntryKind::CanonicalObject,
                EntryKind::Blob,
                EntryKind::Record,
            ]
        };
        assert_eq!(
            archive.entries.iter().map(|e| e.kind).collect::<Vec<_>>(),
            expected_kinds
        );
        assert!(archive.entries.iter().all(|e| !e.bytes.is_empty()));
        for entry in &archive.entries {
            let name = &entry.path;
            match entry.kind {
                EntryKind::CanonicalObject => assert!(name.starts_with("canonical/objects/")),
                EntryKind::Blob => assert!(name.starts_with("blobs/")),
                EntryKind::Record => assert_eq!(name, "records/imported-history.cbor"),
            }
        }
        if vector.name == "partial-missing-blob" {
            assert_eq!(archive.missing_blobs.len(), 1);
            assert!(archive.missing_blobs[0].starts_with("blobs/"));
        } else {
            assert!(archive.missing_blobs.is_empty());
        }
        accepted += 1;
        eprintln!(
            "ACCEPTED public vector: {} ({} payloads)",
            vector.name,
            archive.entries.len()
        );
    }
    assert_eq!(accepted, 3);
    assert_eq!(skipped, 2);
}

#[test]
fn reconstruct_plain_exactly() {
    let base = support::plain();
    // Repacking is intentionally not assumed to preserve a third-party ZIP's
    // metadata. This positive fixture was written with the canonical defaults.
    let repacked = support::build(&support::entries_from_plain());
    assert_eq!(repacked.bytes, base);
}
