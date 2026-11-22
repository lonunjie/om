// SPDX-License-Identifier: MPL-2.0
//! Byte-slice entry points over the owner-recovery codecs and the link-ceremony frames, for external fuzzers and the
//! seed-corpus replay test (V-012). Compiled only with the `fuzz-entry` feature, which a `production` build refuses
//! (R-072). Reached as `om_core::recovery_fuzz_entry`.
//!
//! Every function takes arbitrary bytes, returns `Ok(())` when the input was accepted and a stable refusal code
//! otherwise, and panics only when a stated invariant fails: that panic is the finding. The first byte of most inputs
//! picks a case, so one libFuzzer binary covers one group of related parsers. Keys, scopes and nonces used by the
//! structured cases are fixed synthetic values.

use super::recovery_peer::parse_notice;
use super::registry;
use crate::crypto::cbor::{self, V};
use crate::crypto::recovery_approval as ra;
use crate::crypto::sig::ds_hash;
use crate::recovery::{RecoveryPolicy, RecoveryRequest, RecoveryVeto};
use crate::recovery_record::{
    self as rr, BUCKETS, ProviderHome, RecordKeys, RecoveryKit, RecoveryRecord, RecoveryRedirect,
    RelationshipSnapshot,
};
use crate::recovery_sheet::{self as sheet, OwnerCheckpoint, Parsed, Sheet};

pub use super::ceremony::fuzz_entry::link;

/// Sealed link-ceremony and recovery-approval frames: byte 0 picks the ceremony (even) or the approval (odd) purpose, the
/// rest is the frame plaintext (`len || body || zeros`) that is sealed under fixed synthetic keys and opened again.
pub fn frame(input: &[u8]) -> Outcome {
    let (mode, rest) = split_mode(input)?;
    let mut v = Vec::with_capacity(rest.len() + 1);
    v.push(5 + (mode & 1));
    v.extend_from_slice(rest);
    link(&v)
}

/// The continuation chunker of ADR-0195 (`cont_chunks`), sizes taken from the input.
pub fn continuation(input: &[u8]) -> Outcome {
    let mut v = Vec::with_capacity(input.len() + 1);
    v.push(7);
    v.extend_from_slice(input);
    link(&v)
}

/// Every target, by the name of its libFuzzer binary.
pub const TARGETS: [(&str, Entry); 8] = [
    ("sheet", sheet),
    ("record", record),
    ("kit", kit),
    ("approval", approval),
    ("notice", notice),
    ("link", link),
    ("frame", frame),
    ("continuation", continuation),
];

type Outcome = Result<(), &'static str>;

/// One entry point.
pub type Entry = fn(&[u8]) -> Outcome;

fn split_mode(input: &[u8]) -> Result<(u8, &[u8]), &'static str> {
    input.split_first().map(|(m, r)| (*m, r)).ok_or("short")
}

fn text(rest: &[u8]) -> String {
    String::from_utf8_lossy(rest).into_owned()
}

// ------------------------------------------------------------------------------------------------- sheet

fn check_sheet(s: &Sheet) {
    assert!(
        sheet::valid_host(&s.host),
        "an accepted sheet has a valid host"
    );
    assert_eq!(s.is_full(), s.root().is_some());
    assert!(!s.place().is_full());
    assert!(!s.authority().is_empty());
    let want = Some(s);
    for form in [s.to_compact(), s.to_sheet_form(), s.to_qr()] {
        match sheet::parse(&form) {
            Ok(Parsed::Sheet(again)) => {
                assert_eq!(Some(&again), want, "a sheet reparses to itself")
            }
            _ => panic!("a sheet's own text form is refused"),
        }
    }
    let home = ProviderHome::new(&s.host, s.port, Some(s.scope));
    assert!(
        sheet::check_redirect(&home, &[], 0).is_ok() || s.port == Some(0),
        "a valid sheet place passes the redirect host rules"
    );
}

/// Recovery sheet: the typed and scanned text forms, the binary layout (raw and with a valid whole-string check
/// appended, so mutations reach the field rules), the owner checkpoint, and the live-typing helpers.
pub fn sheet(input: &[u8]) -> Outcome {
    let (mode, rest) = split_mode(input)?;
    match mode % 6 {
        0 => {
            let t = text(rest);
            let _ = sheet::suggest_correction(&t);
            let _ = sheet::first_bad_line(&t);
            match sheet::parse(&t).map_err(|_| "sheet_refused")? {
                Parsed::Sheet(s) => check_sheet(&s),
                Parsed::Legacy(_) => {}
            }
            Ok(())
        }
        1 => {
            let s = Sheet::from_bytes(rest).map_err(|_| "sheet_refused")?;
            assert_eq!(s.to_bytes(), rest, "binary sheet is canonical");
            check_sheet(&s);
            Ok(())
        }
        2 => {
            let mut b = rest.to_vec();
            let chk = ds_hash(sheet::LBL_SHEET_CHECK, &b);
            b.extend_from_slice(&chk[..4]);
            let s = Sheet::from_bytes(&b).map_err(|_| "sheet_refused")?;
            assert_eq!(s.to_bytes(), b, "binary sheet is canonical");
            check_sheet(&s);
            Ok(())
        }
        3 => {
            let c = OwnerCheckpoint::from_bytes(rest).ok_or("checkpoint_refused")?;
            assert_eq!(c.to_bytes(), rest, "checkpoint is canonical");
            Ok(())
        }
        4 => {
            let t = format!("{}{}", sheet::QR_PREFIX, crate::flows::b64url(rest));
            let _ = sheet::first_bad_line(&t);
            match sheet::parse(&t).map_err(|_| "sheet_refused")? {
                Parsed::Sheet(s) => check_sheet(&s),
                Parsed::Legacy(_) => {}
            }
            Ok(())
        }
        _ => {
            // A redirect place from loose fields: hops, visited list and the new place.
            let hops = usize::from(*rest.first().unwrap_or(&0)) % 6;
            let port = match rest.get(1).unwrap_or(&0) % 3 {
                0 => None,
                1 => Some(0),
                _ => Some(u16::from_be_bytes([
                    *rest.get(2).unwrap_or(&0),
                    *rest.get(3).unwrap_or(&1),
                ])),
            };
            let host = text(rest.get(4..).unwrap_or(&[]));
            let home = ProviderHome::new(&host, port, Some([9; 32]));
            let visited = vec![(host.clone(), [9u8; 32])];
            assert!(
                sheet::check_redirect(&home, &visited, 0).is_err(),
                "a loop is refused"
            );
            sheet::check_redirect(&home, &[], hops)
                .map(|_| ())
                .map_err(|_| "redirect_refused")
        }
    }
}

// ------------------------------------------------------------------------------------------------ record

fn fixed_record_keys() -> (RecordKeys, [u8; 32]) {
    let scope = [0x5c; 32];
    (RecordKeys::derive(&[0x42; 32], &scope), scope)
}

/// Recovery record (type 36), redirect (37), the envelope (35), the relationship snapshot and its second layer.
pub fn record(input: &[u8]) -> Outcome {
    let (mode, rest) = split_mode(input)?;
    let (keys, scope) = fixed_record_keys();
    match mode % 7 {
        0 => {
            let r = RecoveryRecord::from_bytes(rest).map_err(|_| "record_refused")?;
            let _ = r.verify();
            // Re-sealing what parsed must not panic and must read back the same record.
            if let Ok(p) = r.to_padded_bytes() {
                let again = RecoveryRecord::from_bytes(&p).map_err(|_| "reread_refused")?;
                assert_eq!(again, r, "a parsed record re-reads equal after padding");
            }
            Ok(())
        }
        1 => {
            let r = RecoveryRedirect::from_bytes(rest).map_err(|_| "redirect_refused")?;
            let p = r.to_padded_bytes().map_err(|_| "pad_refused")?;
            assert_eq!(RecoveryRedirect::from_bytes(&p).ok(), Some(r));
            Ok(())
        }
        2 => rr::read_plaintext(rest)
            .map(|_| ())
            .map_err(|_| "plaintext_refused"),
        3 => {
            let _ = rr::open_envelope(&keys, &scope, rest);
            Err("envelope_refused")
        }
        4 => {
            // Seal the input itself when it is exactly a bucket long: open must hand back the same bytes, and the
            // typed read must agree with reading the plaintext directly.
            if !BUCKETS.contains(&rest.len()) {
                return Err("not_a_bucket");
            }
            let env =
                rr::seal_envelope(&keys, &scope, [3; 12], rest).map_err(|_| "seal_refused")?;
            let opened = rr::open_envelope(&keys, &scope, &env).map_err(|_| "open_refused")?;
            assert_eq!(opened, rest);
            assert_eq!(
                rr::open(&keys, &scope, &env).is_ok(),
                rr::read_plaintext(rest).is_ok()
            );
            Ok(())
        }
        5 => {
            let s = RelationshipSnapshot::from_bytes(rest).map_err(|_| "snapshot_refused")?;
            let again =
                RelationshipSnapshot::from_bytes(&s.to_bytes()).map_err(|_| "reread_refused")?;
            assert_eq!(again, s, "a parsed snapshot re-reads equal");
            Ok(())
        }
        _ => {
            let _ = rr::open_snapshot_layer(&keys.snapshot_key, &scope, 3 << 20, rest);
            Err("layer_refused")
        }
    }
}

// ------------------------------------------------------------------------------------------------- kit

/// The kit (type 38) and the frozen registry's pre-decode check. Byte 0 of the second case picks one registered
/// object (sorted by name) so every frozen type, including the recovery objects, shares one target. The provisional
/// certified kit of ADR-0246 has no parser in the product (its type is unregistered), so it is refused here by the
/// registry as an unknown or mismatched type.
pub fn kit(input: &[u8]) -> Outcome {
    let (mode, rest) = split_mode(input)?;
    if mode % 2 == 0 {
        let k = RecoveryKit::from_bytes(rest).map_err(|_| "kit_refused")?;
        let again = RecoveryKit::from_bytes(&k.to_bytes()).map_err(|_| "reread_refused")?;
        assert!(again == k, "a parsed kit re-reads equal");
        assert_eq!(k.locator(), rr::locator_of(&k.read_cap));
        let keys = RecordKeys::from_kit(&k);
        assert_eq!(keys.locator, k.locator());
        Ok(())
    } else {
        let mut names: Vec<&String> = registry().types.keys().collect();
        names.sort();
        let (&pick, body) = rest.split_first().ok_or("short")?;
        let name = names[usize::from(pick) % names.len()];
        let lenient = pick & 0x80 != 0;
        let a = registry().check(name, body, lenient).map(|_| ());
        assert_eq!(
            a,
            registry().check(name, body, lenient).map(|_| ()),
            "registry check is deterministic"
        );
        a.map_err(|_| "registry_refused")
    }
}

// ---------------------------------------------------------------------------------------------- approval

fn assembler_case(rest: &[u8]) -> Outcome {
    // A descriptor made consistent from the input, a small slice size, and the input as the evidence.
    let size = 1 + usize::from(*rest.first().unwrap_or(&7)) % 64;
    let data = rest.get(1..).unwrap_or(&[]);
    if data.is_empty() || data.len() > size * ra::MAX_CHUNKS {
        return Err("shape_refused");
    }
    let digest = ra::evidence_digest(data);
    let d = ra::Descriptor {
        request_digest: [1; 32],
        parent: [2; 32],
        target: [3; 32],
        controller: [4; 16],
        child: [5; 32],
        evidence_digest: digest,
        evidence_len: data.len() as u64,
        chunks: data.len().div_ceil(size) as u64,
    };
    let chunks: Vec<ra::Chunk> = ra::split(data, size)
        .into_iter()
        .enumerate()
        .map(|(i, data)| ra::Chunk {
            digest,
            index: i as u64,
            data,
        })
        .collect();
    // Deterministic shuffle from the input; flip one byte of one chunk when the input says so.
    let seed = ds_hash("om-fuzz-assembler", data);
    let mut order: Vec<usize> = (0..chunks.len()).collect();
    for i in (1..order.len()).rev() {
        order.swap(i, usize::from(seed[i % 32]) % (i + 1));
    }
    let damaged = (seed[31] & 3 == 0).then(|| usize::from(seed[30]) % chunks.len());
    let mut a = ra::Assembler::with_size(d, size);
    for i in order {
        let mut c = chunks[i].clone();
        if damaged == Some(i) {
            let l = c.data.len();
            c.data[usize::from(seed[29]) % l] ^= 1;
        }
        let r = a.add(&c);
        assert!(r.is_ok(), "a well-shaped chunk is taken: {r:?}");
        assert_eq!(a.add(&c), Ok(()), "an identical duplicate is a no-op");
    }
    match (a.assemble(), damaged) {
        (Some(out), None) => {
            assert_eq!(out, data);
            Ok(())
        }
        (None, Some(_)) => Err("digest_refused"),
        (got, damaged) => panic!(
            "assembly disagrees with the damage flag: {} {damaged:?}",
            got.is_some()
        ),
    }
}

/// Recovery approval ceremony: the offer link, the five message types and the chunk assembler.
pub fn approval(input: &[u8]) -> Outcome {
    let (mode, rest) = split_mode(input)?;
    match mode % 8 {
        0 => {
            let link = text(rest);
            let o = ra::decode_offer(&link).map_err(|_| "offer_refused")?;
            assert_eq!(ra::encode_offer(&o).as_deref(), Some(link.trim()));
            Ok(())
        }
        1 => {
            let p = ra::Proposal::from_bytes(rest).ok_or("proposal_refused")?;
            assert_eq!(p.to_bytes(), rest);
            Ok(())
        }
        2 => {
            let d = ra::Descriptor::from_bytes(rest).ok_or("descriptor_refused")?;
            assert_eq!(d.to_bytes(), rest);
            // Any announced descriptor builds an assembler of bounded size.
            let a = ra::Assembler::new(d);
            assert!(a.indexes().is_empty());
            Ok(())
        }
        3 => {
            let (r, raw) = ra::Reply::from_bytes(rest).ok_or("reply_refused")?;
            assert_eq!(r.to_bytes(), rest);
            assert_eq!(r.descriptor.to_bytes(), raw);
            Ok(())
        }
        4 => {
            let c = ra::Chunk::from_bytes(rest).ok_or("chunk_refused")?;
            assert_eq!(c.to_bytes(), rest);
            Ok(())
        }
        5 => {
            let e = ra::Evidence::from_bytes(rest).ok_or("evidence_refused")?;
            assert_eq!(e.to_bytes(), rest);
            Ok(())
        }
        6 => assembler_case(rest),
        _ => {
            let (a, b) = (
                rest.get(..rest.len() / 2).unwrap_or(&[]),
                rest.get(rest.len() / 2..).unwrap_or(&[]),
            );
            let _ = ra::context(1, a, b, a);
            let _ = ra::keys(1, a, b, a, b);
            let _ = ra::mailbox(b);
            Err("derivation_ran")
        }
    }
}

// -------------------------------------------------------------------------------------------------- notice

/// The recovery notice, its dossier, the signed-object bodies (policy, request, veto) and the drained inbox item.
pub fn notice(input: &[u8]) -> Outcome {
    let (mode, rest) = split_mode(input)?;
    match mode % 6 {
        0 => parse_notice(rest).map(|_| ()).ok_or("notice_refused"),
        1 => {
            let v = cbor::decode(rest).map_err(|_| "cbor_refused")?;
            let list = V::A(vec![v.clone()]);
            let _ = super::recovery_peer::parse_dossier(Some(&v));
            super::recovery_peer::parse_dossier(Some(&list))
                .map(|_| ())
                .ok_or("dossier_refused")
        }
        2 => {
            let p = RecoveryPolicy::from_body(rest).map_err(|_| "policy_refused")?;
            let body = p.body();
            let again = RecoveryPolicy::from_body(&body).map_err(|_| "reread_refused")?;
            assert_eq!(again.body(), body, "a parsed policy re-encodes stably");
            let _ = again.hash();
            Ok(())
        }
        3 => {
            let r = RecoveryRequest::from_body(rest).map_err(|_| "request_refused")?;
            assert_eq!(RecoveryRequest::from_body(&r.body()).ok(), Some(r));
            Ok(())
        }
        4 => {
            let v = RecoveryVeto::from_body(rest).map_err(|_| "veto_refused")?;
            assert_eq!(RecoveryVeto::from_body(&v.body()).ok(), Some(v));
            Ok(())
        }
        _ => super::recovery_inbox::fuzz_inbox_item(rest),
    }
}
