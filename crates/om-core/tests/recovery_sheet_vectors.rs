// SPDX-License-Identifier: MPL-2.0
//! Replays `verification/vectors/recovery-sheet-vectors.json.gz` (ADR-0200 section 9) through Core. The vectors come from an
//! independent Python encoder/parser (hashlib + base64): layouts, both checks, text forms, the QR form, every layout
//! rejection, the legacy 35-byte form, the one-substitution correction and the redirect rules.

use flate2::read::GzDecoder;
use om_core::recovery_record::ProviderHome;
use om_core::recovery_sheet::*;
use serde_json::Value as J;
use std::io::Read;

fn corpus() -> J {
    let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/vectors/recovery-sheet-vectors.json.gz");
    let raw = std::fs::read(p).expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn hx(v: &J) -> Vec<u8> {
    om_core::crypto::unhex(v.as_str().unwrap()).unwrap()
}

fn error_name(e: SheetError) -> String {
    match e {
        SheetError::LineCheck(n) => format!("line_check:{n}"),
        e => e.name().to_string(),
    }
}

#[test]
fn layouts_checks_and_every_text_form() {
    let c = corpus();
    for s in c["sheets"].as_array().unwrap() {
        let id = s["id"].as_str().unwrap();
        let scope: [u8; 32] = hx(&s["scope"]).try_into().unwrap();
        let port = (s["port"].as_u64().unwrap() != 0).then(|| s["port"].as_u64().unwrap() as u16);
        let host = s["host"].as_str().unwrap();
        let sheet = if s["kind"] == 1 {
            Sheet::full(hx(&s["root"]).try_into().unwrap(), scope, port, host).unwrap()
        } else {
            Sheet::place_only(scope, port, host).unwrap()
        };
        assert_eq!(sheet.to_bytes(), hx(&s["binary_hex"]), "{id}: binary");
        assert_eq!(
            sheet.to_bytes()[sheet.to_bytes().len() - 4..],
            hx(&s["check_hex"])[..],
            "{id}: check"
        );
        assert_eq!(
            sheet.to_compact(),
            s["compact"].as_str().unwrap(),
            "{id}: compact"
        );
        assert_eq!(
            sheet.to_sheet_form(),
            s["sheet_form"].as_str().unwrap(),
            "{id}: sheet form"
        );
        assert_eq!(sheet.to_qr(), s["qr"].as_str().unwrap(), "{id}: qr");
        for text in [
            s["compact"].as_str().unwrap().to_string(),
            s["sheet_form"].as_str().unwrap().to_string(),
            s["qr"].as_str().unwrap().to_string(),
            s["compact"].as_str().unwrap().to_lowercase(),
        ] {
            assert_eq!(parse(&text), Ok(Parsed::Sheet(sheet.clone())), "{id}");
        }
        assert_eq!(sheet.is_full(), s["kind"] == 1);
    }
    for l in c["line_checks"].as_array().unwrap() {
        assert_eq!(
            line_check(
                l["index"].as_u64().unwrap() as usize,
                l["payload"].as_str().unwrap()
            ),
            l["check"].as_str().unwrap()
        );
    }
}

#[test]
fn legacy_form_is_accepted_and_never_produced() {
    let c = corpus();
    let l = &c["legacy"];
    let root: [u8; 32] = hx(&l["root"]).try_into().unwrap();
    for t in [l["text"].as_str().unwrap(), l["grouped"].as_str().unwrap()] {
        assert_eq!(parse(t), Ok(Parsed::Legacy(LegacyKey(root))));
    }
    assert_eq!(
        parse(l["bad_check"].as_str().unwrap()),
        Err(SheetError::Check)
    );
}

#[test]
fn every_rejection_names_its_reason() {
    let c = corpus();
    let negs = c["negatives"].as_array().unwrap();
    assert!(negs.len() >= 45, "{}", negs.len());
    for n in negs {
        let r = parse(n["text"].as_str().unwrap());
        let got = r.as_ref().err().map(|e| error_name(*e));
        if n["id"] == "version-2" {
            // Version 2 now exists (ADR-0203): a version-2 string with no checkpoint is still refused, as a missing one.
            assert_eq!(got.as_deref(), Some("length"), "{}", n["id"]);
            continue;
        }
        assert_eq!(got.as_deref(), n["error"].as_str(), "{}", n["id"]);
    }
}

#[test]
fn one_substitution_correction() {
    let c = corpus();
    for k in c["corrections"].as_array().unwrap() {
        assert_eq!(
            suggest_correction(k["typed"].as_str().unwrap()).as_deref(),
            k["corrected"].as_str(),
            "{}",
            k["id"]
        );
    }
    for f in c["first_bad_line"].as_array().unwrap() {
        assert_eq!(
            first_bad_line(f["text"].as_str().unwrap()).map(|n| n as u64),
            f["line"].as_u64()
        );
    }
}

#[test]
fn redirect_rules() {
    let c = corpus();
    for r in c["redirect"].as_array().unwrap() {
        let scope = r["scope"]
            .as_str()
            .map(|s| hx(&J::String(s.into())).try_into().unwrap());
        let place = ProviderHome::new(
            r["host"].as_str().unwrap(),
            r["port"].as_u64().map(|p| p as u16),
            scope,
        );
        let visited: Vec<(String, [u8; 32])> = r["visited"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    v[0].as_str().unwrap().to_string(),
                    hx(&v[1]).try_into().unwrap(),
                )
            })
            .collect();
        let got = match check_redirect(&place, &visited, r["hops"].as_u64().unwrap() as usize) {
            Ok(()) => "ok",
            Err(RedirectRefusal::TooManyHops) => "too_many_hops",
            Err(RedirectRefusal::Host) => "host",
            Err(RedirectRefusal::NoScope) => "no_scope",
            Err(RedirectRefusal::Visited) => "visited",
        };
        assert_eq!(got, r["expect"].as_str().unwrap(), "{}", r["id"]);
    }
}
