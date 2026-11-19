// SPDX-License-Identifier: Apache-2.0
//! Offline recovery wire evidence. This crate is outside the product workspace.

#[cfg(test)]
mod kit;

#[cfg(test)]
mod relationship;

#[cfg(test)]
mod tests {
    use om_core::crypto::cbor::{V, encode};
    use om_core::crypto::kdf::hkdf_sha256;
    use om_core::crypto::recovery_approval as ra;
    use om_core::crypto::sig::ds_hash;
    use om_core::crypto::{hex, unhex};
    use serde_json::{Value, json};

    pub(super) const GOLDEN: &str = include_str!("../../../vectors/recovery-extension-v1.json");
    const PURPOSE: &str = "om.controller_change.v1";

    pub(super) fn bytes(data: impl AsRef<[u8]>) -> V {
        V::B(data.as_ref().to_vec())
    }

    pub(super) fn string(data: &str) -> V {
        V::T(data.into())
    }

    fn transcript(operation: u64) -> Value {
        let offer = ra::Offer {
            expiry_s: 1_800_000_000,
            secret: std::array::from_fn(|i| i as u8),
            a_cap: vec![0xa1; 24],
            b_recv: vec![0xb2; 24],
            uri: "https://home.example".into(),
        };
        let origin = [0x31; 16];
        let link = ra::encode_offer(&offer).unwrap();
        let token = link.strip_prefix(ra::OFFER_PREFIX).unwrap();
        let binding = encode(&V::A(vec![
            string(PURPOSE),
            V::U(operation),
            bytes([0x11; 32]),
            bytes([0x22; 32]),
            bytes(origin),
            bytes([0x42; 16]),
            bytes([0x53; 32]),
            bytes([0x64; 32]),
        ]));
        let route = ra::context(
            offer.expiry_s.into(),
            offer.uri.as_bytes(),
            &offer.a_cap,
            &offer.b_recv,
        );
        let context = encode(&V::A(vec![bytes(&binding), bytes(route)]));
        let id = ds_hash(ra::LBL_KDF, &context);
        let key = |purpose| {
            let mut key = [0; 32];
            let info = encode(&V::A(vec![string(ra::LBL_KDF), string(purpose)]));
            hkdf_sha256(&id, &offer.secret, &info, &mut key);
            key
        };
        let proof = encode(&V::A(vec![string(PURPOSE), bytes(id), bytes(&binding)]));
        json!({
            "link": format!("om:controller-change/{operation}/{}/{token}", hex(&origin)),
            "binding": hex(&binding),
            "ceremony_id": hex(&id),
            "keys": {
                "req": hex(&key("req")),
                "resp": hex(&key("resp")),
                "code": hex(&key("code")),
                "bind": hex(&key("bind"))
            },
            "proof": hex(&proof),
            "code": ra::code_digits(&key("code"), &proof)
        })
    }

    #[test]
    fn cc3_01_transcript_golden() {
        let legacy: Value =
            serde_json::from_str(include_str!("../../../vectors/controller-change-v1.json"))
                .unwrap();
        assert_eq!(transcript(0), legacy);
        let golden: Value = serde_json::from_str(GOLDEN).unwrap();
        let op3 = transcript(3);
        assert_eq!(op3, golden["CC3-01"]);
        let binding = |fixture: &Value| unhex(fixture["binding"].as_str().unwrap()).unwrap();
        let (old, new) = (binding(&legacy), binding(&op3));
        let differences: Vec<_> = old.iter().zip(&new).filter(|(a, b)| a != b).collect();
        assert_eq!(differences, vec![(&0, &3)]);
    }
}
