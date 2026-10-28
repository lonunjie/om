// SPDX-License-Identifier: MPL-2.0
//! Replays the V-001 feasibility corpus through the product crypto: the named selftest checks must all pass and
//! the deterministic vector generator must reproduce the committed corpus byte for byte (frozen wire/crypto
//! constructions; an intentional change needs an ADR and a re-verified corpus, ADR-0165).

use om_core::crypto::{selftest, vectors};
use serde_json::Value;
use std::path::PathBuf;

#[test]
fn selftests_all_pass() {
    let mut rng = om_core::crypto::rng::insecure_test_rng::InsecureTestRng::from_label(b"selftest");
    let checks = selftest::run(&mut rng);
    assert!(checks.len() > 40, "only {} checks ran", checks.len());
    let failed: Vec<_> = checks
        .iter()
        .filter(|c| !c.ok)
        .map(|c| format!("{}: {}", c.name, c.detail))
        .collect();
    assert!(failed.is_empty(), "failed checks: {failed:#?}");
}

#[test]
fn generated_vectors_match_committed_corpus() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../verification/prototypes/v001-crypto/vectors/om-v001-proto-vectors.json");
    let committed: Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let generated = vectors::generate();
    assert_eq!(
        generated, committed,
        "product crypto diverged from the committed V-001 vector corpus"
    );
}
