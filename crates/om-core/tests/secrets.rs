// SPDX-License-Identifier: MPL-2.0
//! Secret hygiene for the persisted profile record (audit M3): private delivery/request seeds are never printed and
//! are wiped when dropped or explicitly zeroized; the stored JSON shape is unchanged.

use om_core::state::{DeliveryKeyRec, InviteRec, PubKey, SecretHex};
use zeroize::Zeroize;

const SEED: &str = "5ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7e75ec7";

fn pk() -> PubKey {
    PubKey {
        key_id: "00".repeat(16),
        public_key: "11".repeat(32),
        route_binding: "22".repeat(32),
        expires_at: 1,
    }
}

#[test]
fn private_seeds_are_redacted_in_debug_output_of_every_record_that_holds_one() {
    let key = DeliveryKeyRec {
        public: pk(),
        seed: SecretHex::from(SEED.to_string()),
    };
    let inv = InviteRec {
        operation_id: "op".into(),
        invite_id: "ab".repeat(16),
        link: "om:invite/x".into(),
        expires_at: 1,
        mailbox: "cd".repeat(16),
        request_seed: SecretHex::from(SEED.to_string()),
        request_digest: "ef".repeat(32),
        revoked: false,
        used: false,
    };
    for dbg in [
        format!("{key:?}"),
        format!("{inv:?}"),
        format!("{:?}", key.seed),
    ] {
        assert!(!dbg.contains(SEED), "seed leaked into Debug output: {dbg}");
    }
}

#[test]
fn a_secret_wipes_in_place_and_its_storage_shape_is_a_plain_hex_string() {
    let mut s = SecretHex::from(SEED.to_string());
    assert_eq!(serde_json::to_string(&s).unwrap(), format!("\"{SEED}\""));
    let back: SecretHex = serde_json::from_str(&format!("\"{SEED}\"")).unwrap();
    assert_eq!(back, s);
    s.zeroize();
    assert!(s.is_empty(), "zeroize must clear the hex text");
    assert_ne!(back, s);
}
