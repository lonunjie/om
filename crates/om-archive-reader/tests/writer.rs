// SPDX-License-Identifier: MPL-2.0
//! Portability slice 3: the plain writer, staged selective export and the report. The strongest check is that the writer
//! reproduces the prototype's plain and partial vectors byte for byte from the archives' own contents.
mod support;
use om_archive_reader::import::*;
use om_archive_reader::writer::*;
use om_archive_reader::{EntryKind, Limits, read_archive};
use std::collections::{BTreeMap, BTreeSet};

fn unhex32(s: &str) -> [u8; 32] {
    support::unhex(s).try_into().unwrap()
}
/// `export_scope` of the vectors: {"conversations": ["conv-1"]}.
fn scope() -> Vec<u8> {
    let mut v = vec![0xa1, 0x60 | 13];
    v.extend(b"conversations");
    v.extend([0x81, 0x60 | 6]);
    v.extend(b"conv-1");
    v
}
fn meta(id: [u8; 16]) -> ManifestMeta {
    ManifestMeta {
        archive_id: id,
        export_scope: scope(),
        source_account_hint: Some(None),
        included_conversations: Some(vec!["conv-1".into()]),
        generated_at_hint: 1_790_000_000,
        exporter_implementation: Some("omarchive-prototype/0".into()),
        provenance_profiles: vec!["om-native".into(), "foreign-import".into()],
    }
}
const TYPES: [&str; 3] = [
    "test.event",
    "test.room.control",
    "test.foreign_identity_link",
];

/// Rebuilds the items of a vector archive from what the reader returns.
fn items_of(bytes: &[u8], blob_present: bool) -> ([u8; 16], Vec<Item>) {
    let a = read_archive(bytes, &Limits::default()).unwrap();
    let mut items = Vec::new();
    let mut ti = 0;
    for e in &a.entries {
        match e.kind {
            EntryKind::CanonicalObject => {
                items.push(Item::Object {
                    object_type: e.declared_type.clone().unwrap(),
                    bytes: e.bytes.to_vec(),
                });
                ti += 1;
            }
            EntryKind::Blob => items.push(Item::Blob {
                blob_id: unhex32(e.path.strip_prefix("blobs/").unwrap()),
                media_type: "image/webp".into(),
                bytes: Some(e.bytes.to_vec()),
                declared: None,
            }),
            EntryKind::Record => items.push(Item::Record {
                record_type: e.declared_type.clone().unwrap(),
                stem: "imported-history".into(),
                bytes: e.bytes.to_vec(),
            }),
        }
    }
    assert_eq!(ti, TYPES.len());
    if !blob_present {
        // the omitted blob: its id and declared content come from the sibling full archive
        let full = support::vectors()
            .into_iter()
            .find(|v| v.name == "plain")
            .unwrap();
        let fa = read_archive(&full.bytes, &Limits::default()).unwrap();
        let b = fa
            .entries
            .iter()
            .find(|e| e.kind == EntryKind::Blob)
            .unwrap();
        let h: [u8; 32] = sha2::Sha256::digest_array(b.bytes);
        items.push(Item::Blob {
            blob_id: unhex32(b.path.strip_prefix("blobs/").unwrap()),
            media_type: "image/webp".into(),
            bytes: None,
            declared: Some((b.bytes.len() as u64, h)),
        });
    }
    (a.archive_id, items)
}
use sha2::Digest as _;
trait DigestArray {
    fn digest_array(b: &[u8]) -> [u8; 32];
}
impl DigestArray for sha2::Sha256 {
    fn digest_array(b: &[u8]) -> [u8; 32] {
        sha2::Sha256::digest(b).into()
    }
}

#[test]
fn the_writer_reproduces_the_prototype_plain_and_partial_vectors_byte_for_byte() {
    for (name, present) in [("plain", true), ("partial-missing-blob", false)] {
        let v = support::vectors()
            .into_iter()
            .find(|v| v.name == name)
            .unwrap();
        let (id, items) = items_of(&v.bytes, present);
        let out = write_archive(&meta(id), &items).unwrap();
        assert_eq!(out, v.bytes, "{name}");
    }
}

#[test]
fn written_archives_are_read_back_exactly_and_item_order_does_not_matter() {
    let v = support::vectors()
        .into_iter()
        .find(|v| v.name == "plain")
        .unwrap();
    let (id, mut items) = items_of(&v.bytes, true);
    items.reverse();
    let out = write_archive(&meta(id), &items).unwrap();
    assert_eq!(out, v.bytes);
    let a = read_archive(&out, &Limits::default()).unwrap();
    assert_eq!(a.entries.len(), 5);
}

#[test]
fn a_secret_type_cannot_be_written() {
    let v = support::vectors()
        .into_iter()
        .find(|v| v.name == "plain")
        .unwrap();
    let (id, mut items) = items_of(&v.bytes, true);
    for t in SECRET_TYPES {
        items.push(Item::Object {
            object_type: t.into(),
            bytes: b"key".to_vec(),
        });
        assert_eq!(
            write_archive(&meta(id), &items),
            Err(WriteError::SecretType(t.into()))
        );
        items.pop();
    }
}

#[test]
fn more_than_65534_entries_use_zip64_and_the_reader_accepts_them() {
    let items: Vec<Item> = (0..65_600u32)
        .map(|i| Item::Record {
            record_type: "note".into(),
            stem: format!("r{i}"),
            bytes: vec![0x80],
        })
        .collect();
    let out = write_archive(&meta([1; 16]), &items).unwrap();
    let a = read_archive(&out, &Limits::default()).unwrap();
    assert_eq!(a.entries.len(), 65_600);
}

#[derive(Default)]
struct Sink {
    staged: BTreeMap<String, Vec<u8>>,
    published: Option<Vec<u8>>,
    fail_after: Option<usize>,
}
impl Staging for Sink {
    type Error = &'static str;
    fn stage(&mut self, path: &str, bytes: &[u8]) -> Result<(), &'static str> {
        if self.fail_after.is_some_and(|n| self.staged.len() >= n) {
            return Err("disk full");
        }
        self.staged.insert(path.into(), bytes.to_vec());
        Ok(())
    }
    fn is_durable(&self, path: &str) -> bool {
        self.staged.contains_key(path)
    }
    fn publish(&mut self, a: &[u8]) -> Result<(), &'static str> {
        self.published = Some(a.to_vec());
        Ok(())
    }
    fn discard(&mut self) {
        self.staged.clear();
        self.published = None;
    }
}

fn cand(item: Item, conv: Option<&str>, time: Option<u64>) -> Candidate {
    Candidate {
        item,
        conversation: conv.map(str::to_owned),
        time,
        disappearing: false,
        view_once: false,
    }
}
fn obj(t: &str, b: &[u8]) -> Item {
    Item::Object {
        object_type: t.into(),
        bytes: b.to_vec(),
    }
}

#[test]
fn selection_and_exclusions_are_reported_and_never_leak() {
    let mut vo = cand(obj("test.event", b"view-once"), Some("a"), Some(5));
    vo.view_once = true;
    let mut gone = cand(obj("test.event", b"disappearing"), Some("a"), Some(5));
    gone.disappearing = true;
    let cands = vec![
        cand(obj("test.event", b"in"), Some("a"), Some(5)),
        cand(obj("test.event", b"other-conv"), Some("b"), Some(5)),
        cand(obj("test.event", b"too-old"), Some("a"), Some(1)),
        cand(obj("om.device_vault", b"secret"), Some("a"), Some(5)),
        vo,
        gone,
        cand(
            Item::Blob {
                blob_id: [7; 32],
                media_type: "image/png".into(),
                bytes: Some(vec![1, 2]),
                declared: None,
            },
            Some("a"),
            Some(5),
        ),
    ];
    let sel = Selection {
        conversations: Some(BTreeSet::from(["a".to_string()])),
        from: Some(3),
        to: None,
        include_blobs: false,
    };
    let job = ExportJob::plan(meta([2; 16]), cands, &sel);
    let r = &job.report;
    assert_eq!((r.objects, r.blobs, r.records), (1, 0, 0));
    assert_eq!(r.excluded[&Excluded::SecretType], 1);
    assert_eq!(r.excluded[&Excluded::ViewOnce], 1);
    assert_eq!(r.excluded[&Excluded::Disappearing], 1);
    assert_eq!(r.excluded[&Excluded::NotSelected], 3);
    let mut sink = Sink::default();
    job.stage_all(&mut sink).unwrap();
    job.finish(&mut sink).unwrap();
    let out = sink.published.unwrap();
    let a = read_archive(&out, &Limits::default()).unwrap();
    assert_eq!(a.entries.len(), 1);
    assert!(!out.windows(6).any(|w| w == b"secret") && !out.windows(9).any(|w| w == b"view-once"));
}

#[test]
fn an_interrupted_export_never_publishes_and_resumes_or_discards() {
    let cands = vec![
        cand(obj("test.event", b"one"), None, None),
        cand(obj("test.event", b"two"), None, None),
        cand(obj("test.event", b"three"), None, None),
    ];
    let job = ExportJob::plan(meta([3; 16]), cands, &Selection::default());
    let mut sink = Sink {
        fail_after: Some(1),
        ..Default::default()
    };
    assert_eq!(
        job.stage_all(&mut sink),
        Err(ExportError::Staging("disk full"))
    );
    // the manifest is never published ahead of its payload
    assert!(matches!(
        job.finish(&mut sink),
        Err(ExportError::NotDurable(_))
    ));
    assert!(sink.published.is_none());
    // resume: the space came back, only the missing items are staged
    sink.fail_after = None;
    let before = sink.staged.len();
    job.stage_all(&mut sink).unwrap();
    assert_eq!(sink.staged.len(), 3);
    assert!(before < 3);
    job.finish(&mut sink).unwrap();
    assert!(sink.published.is_some());
    // or discard everything
    sink.discard();
    assert!(sink.staged.is_empty() && sink.published.is_none());
}

#[test]
fn a_partial_export_reports_its_omitted_blobs_and_the_round_trip_imports() {
    let cands = vec![
        cand(obj("test.event", b"e"), None, None),
        cand(
            Item::Blob {
                blob_id: [9; 32],
                media_type: "image/png".into(),
                bytes: None,
                declared: Some((10, [4; 32])),
            },
            None,
            None,
        ),
    ];
    let job = ExportJob::plan(
        meta([4; 16]),
        cands,
        &Selection {
            include_blobs: true,
            ..Default::default()
        },
    );
    assert_eq!(job.report.omitted_blobs.len(), 1);
    let mut sink = Sink::default();
    job.stage_all(&mut sink).unwrap();
    job.finish(&mut sink).unwrap();
    let a = read_archive(sink.published.as_ref().unwrap(), &Limits::default()).unwrap();
    assert_eq!(a.missing_blobs.len(), 1);
    // and the read-only import of what we wrote yields no authority and nothing native
    struct L;
    impl LiveControl for L {
        fn live_version(&self, _: &str) -> Option<u64> {
            None
        }
    }
    struct V;
    impl ObjectVerifier for V {
        fn verify(&self, _: &str, _: &[u8]) -> Option<Verified> {
            None
        }
    }
    let view = import_readonly(&a, &L, &V);
    assert_eq!(view.objects[0].1, ObjectStatus::Unverified);
}
