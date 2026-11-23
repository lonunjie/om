// SPDX-License-Identifier: MPL-2.0
//! Portability slice 6: hostile foreign input is rejected or neutralized and no record gains OM identity or provenance.
use om_archive_reader::foreign::*;

/// A tiny line-based test adapter: `actor<TAB>text` per line, header `#chat`.
struct Lines;
impl ImportAdapter for Lines {
    fn namespace(&self) -> &str {
        "testchat"
    }
    fn detect(&self, head: &[u8]) -> bool {
        head.starts_with(b"#chat\n")
    }
    fn parse_bounded(&self, input: &[u8], b: &Bounds) -> Result<Vec<Draft>, AdapterError> {
        let s = std::str::from_utf8(input).map_err(|_| AdapterError::Rejected("not utf-8"))?;
        let mut out = Vec::new();
        for line in s.lines().skip(1) {
            if out.len() > b.max_records {
                return Err(AdapterError::Rejected("too many records"));
            }
            let (actor, text) = line
                .split_once('\t')
                .ok_or(AdapterError::Rejected("bad line"))?;
            out.push(Draft {
                source_network_namespace: "testchat".into(),
                source_conversation_ref: None,
                source_event_ref: None,
                source_actor_ref: Some(actor.into()),
                display_name: Some(actor.into()),
                text: text.into(),
                blob_refs: vec![],
            });
        }
        Ok(out)
    }
    fn enumerate_blobs(&self, d: &[Draft]) -> Vec<String> {
        d.iter().flat_map(|x| x.blob_refs.clone()).collect()
    }
    fn emit_warnings(&self) -> Vec<String> {
        vec!["reactions are not carried".into()]
    }
}

fn small() -> Bounds {
    Bounds {
        max_input_bytes: 1000,
        max_records: 3,
        max_text_bytes: 10,
        max_blobs: 2,
    }
}

#[test]
fn records_carry_the_lowest_provenance_and_a_report() {
    let (recs, rep) = run(&Lines, b"#chat\nann\thello\nbob\thi\n", &Bounds::DEFAULT).unwrap();
    assert_eq!(recs.len(), 2);
    assert!(recs.iter().all(|r| r.provenance_level == LEVEL));
    assert_eq!(recs[0].imported_record_id, "testchat-1");
    assert_eq!((rep.carried, rep.refused, rep.degraded), (2, 0, 0));
    assert_eq!(rep.warnings, vec!["reactions are not carried"]);
}

#[test]
fn unrecognized_oversized_and_overlong_input_is_rejected() {
    assert_eq!(
        run(&Lines, b"not a chat", &Bounds::DEFAULT).err(),
        Some(AdapterError::NotRecognized)
    );
    assert_eq!(
        run(&Lines, &vec![b'x'; 2000], &small()).err(),
        Some(AdapterError::Rejected("input too large"))
    );
    let many = b"#chat\na\t1\nb\t2\nc\t3\nd\t4\n";
    assert!(matches!(
        run(&Lines, many, &small()),
        Err(AdapterError::Rejected(_))
    ));
    assert!(matches!(
        run(&Lines, b"#chat\nno-tab-here\n", &Bounds::DEFAULT),
        Err(AdapterError::Rejected(_))
    ));
    assert!(matches!(
        run(&Lines, b"#chat\n\xff\xfe\n", &Bounds::DEFAULT),
        Err(AdapterError::Rejected(_))
    ));
}

#[test]
fn hostile_text_is_neutralized_truncated_and_never_gains_identity() {
    let input = "#chat\nev\u{202E}il\tpay\u{0007}load that is far too long\n\u{1}\t\n"
        .as_bytes()
        .to_vec();
    let (recs, rep) = run(&Lines, &input, &small()).unwrap();
    assert_eq!(recs.len(), 1, "an empty record is refused");
    assert_eq!(rep.refused, 1);
    assert_eq!(rep.degraded, 1);
    let r = &recs[0];
    assert!(r.draft.text.len() <= 10);
    assert!(!r.draft.text.contains('\u{0007}'));
    assert!(
        !r.draft
            .display_name
            .as_deref()
            .unwrap()
            .contains('\u{202E}')
    );
    // a Draft has no event id, author, conversation or signature field: the type carries none to set
    assert_eq!(r.provenance_level, "foreign_export_label");
}
