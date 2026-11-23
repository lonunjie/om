// SPDX-License-Identifier: MPL-2.0
//! Portability slice 2: read-only import (OM_ARCHIVE_V1_SPEC.md section 8, ADR-0263). The container is covered by the
//! reader tests; these build `Archive` values directly so each import rule is exercised on its own.
mod support;
use om_archive_reader::import::*;
use om_archive_reader::{Archive, ArchiveEntry, EntryKind, Limits, read_archive};

// A tiny canonical CBOR writer for text-keyed maps (keys sorted by encoded form by the caller's order).
fn head(major: u8, n: usize) -> Vec<u8> {
    match n {
        0..=23 => vec![major << 5 | n as u8],
        24..=255 => vec![major << 5 | 24, n as u8],
        _ => vec![major << 5 | 25, (n >> 8) as u8, n as u8],
    }
}
fn text(s: &str) -> Vec<u8> {
    [head(3, s.len()), s.as_bytes().to_vec()].concat()
}
fn map(mut pairs: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
    pairs.sort_by_key(|(k, _)| (k.len(), k.to_string()));
    let mut out = head(5, pairs.len());
    for (k, v) in pairs {
        out.extend(text(k));
        out.extend(v);
    }
    out
}
fn array(items: Vec<Vec<u8>>) -> Vec<u8> {
    let mut out = head(4, items.len());
    for i in items {
        out.extend(i);
    }
    out
}
fn base(level: &str) -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("imported_record_id", text("r1")),
        ("source_network_namespace", text("net.example")),
        ("source_export_fingerprint", text("fp")),
        ("display_metadata", map(vec![])),
        ("content", map(vec![])),
        ("attachments", array(vec![])),
        (
            "provenance_level",
            text(Box::leak(level.to_string().into_boxed_str())),
        ),
    ]
}
fn with(extra: Vec<(&'static str, Vec<u8>)>, level: &str) -> Vec<u8> {
    let mut m = base(level);
    m.extend(extra);
    map(m)
}

fn record(items: Vec<Vec<u8>>) -> Vec<u8> {
    array(items)
}
fn archive<'a>(objects: &'a [(&str, &str, Vec<u8>)], records: &'a [Vec<u8>]) -> Archive<'a> {
    let mut entries = Vec::new();
    for (path, ty, bytes) in objects {
        entries.push(ArchiveEntry {
            path: (*path).to_owned(),
            kind: EntryKind::CanonicalObject,
            bytes,
            declared_type: Some((*ty).to_owned()),
        });
    }
    for (i, r) in records.iter().enumerate() {
        entries.push(ArchiveEntry {
            path: format!("records/{i:04}.cbor"),
            kind: EntryKind::Record,
            bytes: r,
            declared_type: Some("imported_history".into()),
        });
    }
    Archive {
        archive_id: [0; 16],
        entries,
        missing_blobs: vec!["blobs/aa".into()],
    }
}

struct Live(Vec<(&'static str, u64)>);
impl LiveControl for Live {
    fn live_version(&self, s: &str) -> Option<u64> {
        self.0.iter().find(|(k, _)| *k == s).map(|(_, v)| *v)
    }
}
/// Accepts objects whose bytes start with "ok"; the byte after names what they are.
struct Verifier;
impl ObjectVerifier for Verifier {
    fn verify(&self, _t: &str, b: &[u8]) -> Option<Verified> {
        if !b.starts_with(b"ok") {
            return None;
        }
        Some(match b.get(2) {
            Some(b'c') => Verified {
                control: Some(("room1".into(), u64::from(b[3]))),
                evidence: None,
            },
            Some(b'l') => Verified {
                control: None,
                evidence: Some(Evidence::ForeignIdentityLink),
            },
            Some(b'b') => Verified {
                control: None,
                evidence: Some(Evidence::BridgeAttestation),
            },
            _ => Verified {
                control: None,
                evidence: None,
            },
        })
    }
}

#[test]
fn objects_are_classified_against_live_state_and_never_applied() {
    let objs = [
        ("canonical/objects/a", "RoomControl", b"ok-plain".to_vec()),
        ("canonical/objects/b", "RoomControl", b"okc\x02".to_vec()),
        ("canonical/objects/c", "RoomControl", b"okc\x07".to_vec()),
        ("canonical/objects/d", "RoomControl", b"okc\x05".to_vec()),
        ("canonical/objects/e", "RoomControl", b"forged".to_vec()),
    ];
    let a = archive(&objs, &[]);
    let v = import_readonly(&a, &Live(vec![("room1", 5)]), &Verifier);
    let st: Vec<_> = v.objects.iter().map(|(_, s)| s.clone()).collect();
    assert_eq!(
        st,
        vec![
            ObjectStatus::VerifiedHistory,
            ObjectStatus::StaleControl {
                archived: 2,
                live: 5
            },
            ObjectStatus::ControlNeedsLiveValidation { archived: 7 },
            ObjectStatus::ControlNeedsLiveValidation { archived: 5 },
            ObjectStatus::Unverified,
        ]
    );
    assert_eq!(v.missing_blobs, vec!["blobs/aa".to_string()]);
    // control state unknown to live state is never applied either
    let v = import_readonly(&a, &Live(vec![]), &Verifier);
    assert!(matches!(
        v.objects[1].1,
        ObjectStatus::ControlNeedsLiveValidation { archived: 2 }
    ));
}

#[test]
fn records_claiming_native_authorship_are_quarantined() {
    for field in [
        "author_principal",
        "author_account_id",
        "author_endpoint_id",
        "event_id",
        "conversation_id",
        "signature",
    ] {
        let r = record(vec![with(vec![(field, text("x"))], "foreign_export_label")]);
        let recs = [r];
        let v = import_readonly(&archive(&[], &recs), &Live(vec![]), &Verifier);
        assert!(v.records.is_empty(), "{field}");
        assert_eq!(
            v.quarantined[0].2,
            Quarantine::NativeAuthorshipClaim(field.into())
        );
    }
}

#[test]
fn unknown_fields_missing_fields_and_unknown_levels_are_quarantined() {
    let unknown = with(vec![("surprise", text("x"))], "foreign_export_label");
    let mut missing = base("foreign_export_label");
    missing.retain(|(k, _)| *k != "content");
    let missing = map(missing);
    let bad_level = with(vec![], "totally_trusted");
    let recs = [record(vec![unknown, missing, bad_level, vec![0xf6]])];
    let v = import_readonly(&archive(&[], &recs), &Live(vec![]), &Verifier);
    assert!(v.records.is_empty());
    let q: Vec<_> = v.quarantined.iter().map(|(_, _, q)| q.clone()).collect();
    assert_eq!(
        q,
        vec![
            Quarantine::UnknownField("surprise".into()),
            Quarantine::MissingField("content"),
            Quarantine::UnknownProvenance,
            Quarantine::NotAMap,
        ]
    );
}

#[test]
fn strong_provenance_needs_verified_evidence_inside_the_archive() {
    let objs = [
        (
            "canonical/objects/link",
            "ForeignIdentityLink",
            b"okl".to_vec(),
        ),
        (
            "canonical/objects/att",
            "BridgeAttestation",
            b"okb".to_vec(),
        ),
        (
            "canonical/objects/fake",
            "ForeignIdentityLink",
            b"nope".to_vec(),
        ),
    ];
    let linked = |ev: &str| {
        with(
            vec![(
                "link_evidence",
                text(Box::leak(ev.to_string().into_boxed_str())),
            )],
            "cryptographically_linked",
        )
    };
    let attested = |ev: &str| {
        with(
            vec![(
                "link_evidence",
                text(Box::leak(ev.to_string().into_boxed_str())),
            )],
            "bridge_attested",
        )
    };
    let recs = [record(vec![
        linked("canonical/objects/link"),         // backed
        linked("canonical/objects/fake"),         // evidence does not verify
        linked("canonical/objects/att"),          // evidence of the wrong kind
        linked("canonical/objects/missing"),      // names nothing
        with(vec![], "cryptographically_linked"), // no evidence at all
        attested("canonical/objects/att"),        // backed
        attested("canonical/objects/link"),       // wrong kind
        with(vec![], "user_annotation"),          // needs none
        with(vec![], "foreign_export_label"),
    ])];
    let v = import_readonly(&archive(&objs, &recs), &Live(vec![]), &Verifier);
    let eff: Vec<_> = v
        .records
        .iter()
        .map(|r| (r.effective_level.as_str(), r.downgraded))
        .collect();
    assert_eq!(
        eff,
        vec![
            ("cryptographically_linked", false),
            ("foreign_export_label", true),
            ("foreign_export_label", true),
            ("foreign_export_label", true),
            ("foreign_export_label", true),
            ("bridge_attested", false),
            ("foreign_export_label", true),
            ("user_annotation", false),
            ("foreign_export_label", false),
        ]
    );
    assert!(v.quarantined.is_empty());
}

#[test]
fn the_real_plain_vector_imports_without_panics_and_yields_no_authority() {
    let bytes = support::plain();
    let archive = read_archive(&bytes, &Limits::default()).unwrap();
    let v = import_readonly(&archive, &Live(vec![]), &Verifier);
    // the vector's objects are not ones this verifier accepts: all unverified, nothing becomes native
    assert!(
        v.objects
            .iter()
            .all(|(_, s)| *s == ObjectStatus::Unverified || *s == ObjectStatus::VerifiedHistory)
    );
    // ImportView has no field that could carry authority; this only keeps the claim visible to a reader of the test
    let ImportView {
        objects: _,
        records: _,
        quarantined: _,
        missing_blobs: _,
    } = v;
}
