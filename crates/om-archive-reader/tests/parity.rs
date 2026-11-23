// SPDX-License-Identifier: MPL-2.0
//! Parity of the product reader with the V-014 prototype reader on the prototype's hostile corpus. The prototype writes its
//! cases with `omarchive-corpus --dump-cases <file>`; set `OM_V014_CASES` to that file (it is about 90 MB, so it is not
//! committed). Without it the test does nothing, so the normal suite stays fast. Every hostile case the prototype rejects
//! must be rejected by the product reader with the same error class; the encrypted-profile cases must be refused as
//! unsupported (the product reader is plain-only, ADR-0263); positive cases must be accepted.
use om_archive_reader::{Limits, read_archive};

#[test]
fn product_reader_matches_the_prototype_on_every_case() {
    let Ok(path) = std::env::var("OM_V014_CASES") else {
        eprintln!("OM_V014_CASES not set: parity check skipped");
        return;
    };
    let cases: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let (mut checked, mut same, mut diffs) = (0, 0, Vec::new());
    let (mut exact, mut mapped, mut refused_by_design) = (0, 0, 0);
    for c in cases.as_array().unwrap() {
        let id = c["id"].as_str().unwrap();
        let bytes = decode_hex(c["hex"].as_str().unwrap());
        let limits = Limits {
            max_entries: c["max_entries"].as_u64().unwrap(),
            max_entry_size: c["max_entry_size"].as_u64().unwrap(),
            max_total_size: c["max_total_size"].as_u64().unwrap(),
            max_manifest_size: c["max_manifest_size"].as_u64().unwrap(),
        };
        let want = c["expect"].as_str().unwrap();
        let got = read_archive(&bytes, &limits);
        checked += 1;
        let ok = match (c["kind"].as_str().unwrap(), &got) {
            ("ok", Ok(_)) => {
                exact += 1;
                true
            }
            // the plain-only product reader refuses encrypted archives by design (ADR-0263)
            ("ok", Err(e)) if is_encrypted(id) && e.class_name() == "encrypted_unsupported" => {
                refused_by_design += 1;
                true
            }
            ("err", Err(e)) => {
                let name = e.class_name();
                if name == want {
                    exact += 1;
                    true
                } else if name == "encrypted_unsupported"
                    && (c["category"] == "encryption" || is_encrypted(id))
                {
                    refused_by_design += 1;
                    true
                } else if class_matches(want, name) {
                    mapped += 1;
                    true
                } else {
                    false
                }
            }
            _ => false,
        };
        if ok {
            same += 1;
        } else {
            let g = match &got {
                Ok(_) => "accepted".to_string(),
                Err(e) => e.class_name().to_string(),
            };
            diffs.push(format!("{id}: prototype {want}, product {g}"));
        }
    }
    eprintln!(
        "parity: {same} of {checked} cases agree ({exact} identical class, {mapped} renamed class, {refused_by_design} encrypted refused by design)"
    );
    for d in &diffs {
        eprintln!("DIFF {d}");
    }
    assert!(diffs.is_empty(), "{} differing cases", diffs.len());
}

fn decode_hex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
        .collect()
}

fn is_encrypted(id: &str) -> bool {
    id.contains("encrypted")
        || id.contains("recipient")
        || id.contains("passphrase")
        || id.contains("raw_key")
}

/// The product reader names some classes differently from the prototype. Each pair below is the same refusal under
/// another name; a pair missing here is reported as a difference.
fn class_matches(proto: &str, product: &str) -> bool {
    if proto == "Zip(any)" {
        return product.starts_with("Zip(");
    }
    if proto.starts_with("Cbor(") {
        return product == "Cbor";
    }
    let pairs: &[(&str, &[&str])] = &[
        ("Zip(BadName)", &["Zip(Path)"]),
        ("Zip(NotRegularFile)", &["Zip(Mode)"]),
        ("Zip(DuplicateName)", &["Zip(Duplicate)"]),
        ("Zip(UnsupportedMethod)", &["Zip(Compression)"]),
        ("Zip(NonContiguous)", &["Zip(Structure)"]),
        ("Zip(EntryTooLarge)", &["Zip(SizeLimit)"]),
        ("Zip(TooManyEntries)", &["Zip(CountLimit)"]),
        ("Zip(Truncated)", &["Zip(Structure)", "Zip(Truncated)"]),
        ("Zip(TrailingOrPrependedData)", &["Zip(Structure)"]),
        (
            "Zip(TrailingOrPrependedData|CommentPresent)",
            &["Zip(Structure)"],
        ),
        ("Zip(MultiDisk)", &["Zip(Structure)"]),
        ("Zip(NotZip)", &["Zip(Truncated)", "Zip(Structure)"]),
        ("Zip(DataDescriptor)", &["Zip(Flags)"]),
        ("Zip(Encrypted)", &["Zip(Flags)"]),
        ("Zip(HeaderMismatch)", &["Zip(Mismatch)"]),
        ("Zip(ExtraField)", &["Zip(Extra)"]),
        ("Zip(TotalTooLarge)", &["Zip(TotalSizeLimit)"]),
        ("ManifestTooLarge", &["ManifestSizeLimit"]),
    ];
    pairs
        .iter()
        .any(|(a, bs)| *a == proto && bs.contains(&product))
}
