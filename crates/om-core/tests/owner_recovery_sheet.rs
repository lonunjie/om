// SPDX-License-Identifier: MPL-2.0
//! Sheet version 2 and the owner checkpoint (ADR-0203 section 1), replayed against the independent vectors in
//! `verification/vectors/owner-recovery-vectors.json.gz` and against the refusals the ADR lists.

use flate2::read::GzDecoder;
use om_core::recovery_sheet::*;
use serde_json::Value as J;
use std::io::Read;

fn corpus() -> J {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/vectors/owner-recovery-vectors.json.gz");
    let raw = std::fs::read(p).expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn unhex(s: &str) -> Vec<u8> {
    om_core::crypto::unhex(s).unwrap()
}

fn vector_sheet() -> (Vec<u8>, J) {
    let c = corpus();
    (
        unhex(c["sheet"]["binary"].as_str().unwrap()),
        c["sheet"].clone(),
    )
}

#[test]
fn the_vector_sheet_parses_in_every_form_and_round_trips() {
    let (bin, v) = vector_sheet();
    let s = Sheet::from_bytes(&bin).expect("version 2 sheet");
    assert!(s.is_full());
    assert_eq!(s.host, "home.example");
    let cp = s.checkpoint.expect("checkpoint");
    assert_eq!(cp.generation, 1);
    assert_eq!(s.to_bytes(), bin, "exact bytes");
    assert_eq!(s.to_compact(), v["compact"].as_str().unwrap());
    assert_eq!(s.to_qr(), v["qr"].as_str().unwrap());
    for text in [s.to_compact(), s.to_sheet_form(), s.to_qr()] {
        match parse(&text).unwrap() {
            Parsed::Sheet(p) => assert_eq!(p, s),
            other => panic!("{other:?}"),
        }
    }
    // The place alone never carries the checkpoint.
    assert_eq!(s.place().checkpoint, None);
    assert_eq!(s.place().to_bytes()[0], 1);
}

fn with_check(body: &[u8]) -> Vec<u8> {
    let mut b = body.to_vec();
    let chk = om_core::crypto::sig::ds_hash(LBL_SHEET_CHECK, body);
    b.extend_from_slice(&chk[..4]);
    b
}

#[test]
fn malformed_version_2_sheets_use_nothing() {
    let (bin, _) = vector_sheet();
    let body = &bin[..bin.len() - 4];
    // missing checkpoint
    let host_end = 2 + 32 + 32 + 2 + 1 + 12;
    assert_eq!(
        Sheet::from_bytes(&with_check(&body[..host_end])),
        Err(SheetError::Length)
    );
    // trailing bytes after the checkpoint
    let mut t = body.to_vec();
    t.extend_from_slice(&[0; 9]);
    assert_eq!(
        Sheet::from_bytes(&with_check(&t)),
        Err(SheetError::Trailing)
    );
    // an extension key inside the checkpoint (non-canonical for v1 of the object)
    let mut e = body[..host_end].to_vec();
    let cp = om_core::crypto::cbor::V::M(vec![
        (0, om_core::crypto::cbor::V::U(39)),
        (1, om_core::crypto::cbor::V::U(1)),
        (2, om_core::crypto::cbor::V::B(vec![1; 32])),
        (3, om_core::crypto::cbor::V::U(1)),
        (
            4,
            om_core::crypto::cbor::V::M(vec![
                (0, om_core::crypto::cbor::V::U(1)),
                (1, om_core::crypto::cbor::V::B(vec![2; 32])),
            ]),
        ),
        (23, om_core::crypto::cbor::V::U(0)),
    ]);
    e.extend_from_slice(&om_core::crypto::cbor::encode(&cp));
    assert!(matches!(
        Sheet::from_bytes(&with_check(&e)),
        Err(SheetError::Checkpoint | SheetError::Trailing)
    ));
    // an unsupported version, and the place-only kind under version 2
    let mut v3 = body.to_vec();
    v3[0] = 3;
    assert_eq!(
        Sheet::from_bytes(&with_check(&v3)),
        Err(SheetError::Version)
    );
    let mut k = body.to_vec();
    k[1] = 2;
    assert_eq!(Sheet::from_bytes(&with_check(&k)), Err(SheetError::Kind));
    // a flipped checkpoint byte fails the whole-sheet check
    let mut f = bin.clone();
    let n = f.len();
    f[n - 10] ^= 1;
    assert_eq!(Sheet::from_bytes(&f), Err(SheetError::Check));
}

#[test]
fn a_version_1_reader_refuses_a_version_2_sheet() {
    // An old client knows only version 1 and its layout: the same bytes read as version 1 do not parse.
    let (bin, _) = vector_sheet();
    let mut as_v1 = bin[..bin.len() - 4].to_vec();
    as_v1[0] = 1;
    assert!(Sheet::from_bytes(&with_check(&as_v1)).is_err());
}

#[test]
fn legacy_and_version_1_stay_readable() {
    let s = Sheet::full([5; 32], [6; 32], None, "home.example.net").unwrap();
    assert_eq!(s.to_bytes()[0], 1);
    assert!(matches!(parse(&s.to_compact()), Ok(Parsed::Sheet(p)) if p.checkpoint.is_none()));
    let longest = Sheet::full_v2(
        [1; 32],
        [2; 32],
        Some(8443),
        &format!(
            "{}.{}.{}.{}",
            "a".repeat(63),
            "b".repeat(63),
            "c".repeat(63),
            "d".repeat(61)
        ),
        OwnerCheckpoint {
            account_id: [3; 32],
            generation: u64::MAX,
            acs_hash: [4; 32],
        },
    )
    .unwrap();
    assert_eq!(longest.to_bytes().len(), 2 + 32 + 32 + 2 + 1 + 253 + 90 + 4);
    assert_eq!(longest.to_bytes().len(), 416);
    for text in [
        longest.to_compact(),
        longest.to_sheet_form(),
        longest.to_qr(),
    ] {
        assert!(matches!(parse(&text), Ok(Parsed::Sheet(p)) if p == longest));
    }
}
