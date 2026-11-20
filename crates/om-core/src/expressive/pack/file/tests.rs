// SPDX-License-Identifier: MPL-2.0

use super::*;
use crate::attach::{AttachmentRef, ciphertext_digest, seal};
use crate::expressive::{Kind, Presentation};

fn golden(index: usize) -> (Vec<u8>, serde_json::Value) {
    let doc: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../../../verification/vectors/pack-file-vectors.json"
    ))
    .unwrap();
    let row = doc["vectors"][index].clone();
    (unhex(row["hex"].as_str().unwrap()).unwrap(), row)
}

fn set(value: &mut Value, key: u64, next: Value) {
    let Value::Map(fields) = value else {
        panic!("map fixture")
    };
    if let Some((_, field)) = fields.iter_mut().find(|(k, _)| *k == Value::Uint(key)) {
        *field = next;
    } else {
        fields.push((Value::Uint(key), next));
    }
}

fn field(value: &Value, key: u64) -> &Value {
    let Value::Map(fields) = value else {
        panic!("map fixture")
    };
    &fields
        .iter()
        .find(|(k, _)| *k == Value::Uint(key))
        .unwrap()
        .1
}

fn successor(previous: &[u8], sequence: u64) -> Vec<u8> {
    let V::M(mut fields) = cbor::decode(previous).unwrap() else {
        panic!("release fixture")
    };
    fields.retain(|(k, _)| ![3, 4, 11].contains(k));
    fields.push((3, V::U(sequence)));
    fields.push((4, sig::digest(&ds_hash(RELEASE_HASH, previous))));
    cbor::encode(&V::M(fields))
}

fn relink(release: &[u8], previous: &[u8]) -> Vec<u8> {
    let V::M(mut fields) = cbor::decode(release).unwrap() else {
        panic!("release fixture")
    };
    fields.retain(|(k, _)| *k != 4);
    fields.push((4, sig::digest(&ds_hash(RELEASE_HASH, previous))));
    cbor::encode(&V::M(fields))
}

fn array_field(value: &mut Value, key: u64) -> &mut Vec<Value> {
    let Value::Map(fields) = value else {
        panic!("map fixture")
    };
    let (_, Value::Array(values)) = fields
        .iter_mut()
        .find(|(k, _)| *k == Value::Uint(key))
        .unwrap()
    else {
        panic!("array fixture")
    };
    values
}

fn decoded(data: &[u8]) -> Value {
    om_codec::decode(
        data,
        Limits {
            max_string: PACK_FILE_MAX as u64,
            ..Limits::OM
        },
        PACK_FILE_MAX,
    )
    .unwrap()
}

fn refuse(value: &Value, reason: Error) {
    let state = Packs::default();
    assert_eq!(
        state.review_file(&om_codec::encode(value)).err(),
        Some(reason)
    );
    assert!(state.is_empty());
}

fn activate(packs: &mut Packs, file: &[u8]) {
    let mut result = packs.review_file(file).unwrap();
    let preview = result.preview.clone();
    let count = result
        .begin(
            packs,
            &preview.release_id,
            preview.control_digest.as_deref(),
        )
        .unwrap();
    for _ in 0..count {
        result.stage().unwrap();
    }
    result.activate(packs).unwrap();
}

#[test]
fn golden_files_redecode_and_bind_exact_heads() {
    let (genesis, _) = golden(0);
    for i in 0..3 {
        let (bytes, row) = golden(i);
        let mut packs = Packs::default();
        if i == 2 {
            assert_eq!(
                packs.review_file(&bytes).err(),
                Some("pack_needs_earlier_version")
            );
            activate(&mut packs, &genesis);
        }
        assert_eq!(om_codec::encode(&decoded(&bytes)), bytes);
        let r = packs.review_file(&bytes).unwrap();
        assert_eq!(r.preview.title, "Little faces");
        assert_eq!(Some(r.preview.sequence.into()), row["sequence"].as_u64());
        assert_eq!(
            Some(r.preview.release_id.as_str()),
            row["releaseId"].as_str()
        );
        assert_eq!(
            r.preview.control_digest.as_deref(),
            row["controlDigest"].as_str()
        );
        activate(&mut packs, &bytes);
        assert_eq!(packs.cache_digests().len(), 1);
    }
}

#[test]
fn rejects_untrusted_envelopes_and_manifest_chains() {
    let (genesis_bytes, _) = golden(0);
    let (update_bytes, _) = golden(1);
    let genesis = decoded(&genesis_bytes);
    let update = decoded(&update_bytes);
    let mut variant = genesis.clone();
    set(
        &mut variant,
        1,
        Value::Text("om.content-pack-file.v2".into()),
    );
    refuse(&variant, "pack_file_format");
    for key in [5, 6] {
        let mut variant = genesis.clone();
        set(&mut variant, key, Value::Array(vec![]));
        refuse(&variant, "pack_file_shape");
    }
    let mut not_shortest = genesis_bytes.clone();
    not_shortest.splice(1..2, [0x18, 1]);
    assert_eq!(
        Packs::default().review_file(&not_shortest).err(),
        Some("noncanonical_int")
    );
    let mut variant = genesis.clone();
    set(&mut variant, 2, Value::Bytes(vec![0xa0]));
    assert!(
        Packs::default()
            .review_file(&om_codec::encode(&variant))
            .is_err()
    );
    let mut variant = genesis.clone();
    let Value::Bytes(envelope) = &mut array_field(&mut variant, 3)[0] else {
        unreachable!()
    };
    *envelope.last_mut().unwrap() ^= 1;
    refuse(&variant, "bad_signature");

    let mut variant = update.clone();
    array_field(&mut variant, 5).clear();
    refuse(&variant, "pack_predecessors");
    let mut variant = update.clone();
    let predecessors = array_field(&mut variant, 5);
    predecessors.push(predecessors[0].clone());
    refuse(&variant, "sequence_skip");
    let mut variant = update.clone();
    array_field(&mut variant, 3).reverse();
    assert!(
        Packs::default()
            .review_file(&om_codec::encode(&variant))
            .is_err()
    );
    let mut variant = update.clone();
    array_field(&mut variant, 3).remove(0);
    assert!(
        Packs::default()
            .review_file(&om_codec::encode(&variant))
            .is_err()
    );
    let mut variant = update;
    // Replace the genesis control commitment with the target's commitment.
    let states = array_field(&mut variant, 3);
    states[0] = states[1].clone();
    assert!(
        Packs::default()
            .review_file(&om_codec::encode(&variant))
            .is_err()
    );
}

#[test]
fn predecessor_manifests_cannot_skip_reorder_or_substitute_a_release() {
    let (bytes, _) = golden(1);
    let mut update = decoded(&bytes);
    let Value::Bytes(second) = field(&update, 2).clone() else {
        panic!("release fixture")
    };
    let first = array_field(&mut update, 5)[0].clone();
    let third = successor(&second, 3);
    let mut missing = update.clone();
    set(&mut missing, 2, Value::Bytes(third.clone()));
    refuse(&missing, "sequence_skip");

    let mut reordered = missing;
    set(
        &mut reordered,
        5,
        Value::Array(vec![Value::Bytes(second.clone()), first.clone()]),
    );
    refuse(&reordered, "sequence_skip");

    let mut extra = reordered;
    set(
        &mut extra,
        5,
        Value::Array(vec![
            first.clone(),
            Value::Bytes(second.clone()),
            Value::Bytes(second.clone()),
        ]),
    );
    refuse(&extra, "sequence_skip");

    let Value::Bytes(first) = first else {
        panic!("release fixture")
    };
    let V::M(mut substituted) = cbor::decode(&first).unwrap() else {
        panic!("release fixture")
    };
    substituted.iter_mut().find(|(k, _)| *k == 5).unwrap().1 = V::T("Different title".into());
    let substituted = cbor::encode(&V::M(substituted));
    set(&mut update, 2, Value::Bytes(relink(&second, &substituted)));
    set(
        &mut update,
        5,
        Value::Array(vec![Value::Bytes(substituted)]),
    );
    // The altered manifests link to one another, but the authentic genesis state commits to the original.
    refuse(&update, "release_id_mismatch");
}

#[test]
fn chain_counts_are_bounded_for_both_arrays() {
    let (bytes, _) = golden(1);
    for key in [3, 5] {
        let mut file = decoded(&bytes);
        let states = array_field(&mut file, key);
        states.resize(64, Value::Bytes(vec![]));
        assert!(File::decode(&om_codec::encode(&file)).is_ok());
        array_field(&mut file, key).push(Value::Bytes(vec![]));
        refuse(&file, "pack_chain_limit");
    }
}

fn still_bytes(size: usize) -> Vec<u8> {
    assert!(size >= 26 && size.is_multiple_of(2));
    let mut bytes = vec![0; size];
    bytes[..4].copy_from_slice(b"RIFF");
    bytes[4..8].copy_from_slice(&((size - 8) as u32).to_le_bytes());
    bytes[8..16].copy_from_slice(b"WEBPVP8L");
    bytes[16..20].copy_from_slice(&((size - 20) as u32).to_le_bytes());
    bytes[20] = 0x2f; // 1x1 lossless header. The header walker leaves pixel data opaque.
    bytes
}

fn item(id: u8, size: usize, width: u32) -> (V, Value) {
    item_with_plaintext(id, &still_bytes(size), width)
}

fn item_with_plaintext(id: u8, plaintext: &[u8], width: u32) -> (V, Value) {
    let blob_id = [id; 32];
    let key = [id.wrapping_add(1); 32];
    let ciphertext = seal(&key, &blob_id, plaintext);
    let digest = ciphertext_digest(&ciphertext);
    let a = AssetRef {
        attachment: AttachmentRef {
            blob_id,
            key,
            media_type: "image/webp".into(),
            plaintext_size: plaintext.len() as u64,
            ciphertext_digest: digest,
            locations: vec![],
            filename: None,
            read_cap: None,
            width: Some(width),
            height: Some(1),
            duration_ms: None,
        },
        presentation: Presentation::webp(Kind::Sticker, "Item".into(), None),
    };
    let entry = Value::Map(vec![
        (Value::Text("bytes".into()), Value::Bytes(ciphertext)),
        (
            Value::Text("digest".into()),
            decoded(&cbor::encode(&sig::digest(&digest))),
        ),
    ]);
    (
        V::M(vec![
            (0, V::T(format!("item-{id}"))),
            (1, V::U(1)),
            (2, a.value()),
            (5, V::T("Item".into())),
        ]),
        entry,
    )
}

fn unsigned(items: Vec<V>, blobs: Vec<Value>, title: &str, extra: Option<usize>) -> Value {
    let mut fields = vec![
        (0, V::U(45)),
        (1, V::U(1)),
        (2, V::B(vec![9; 32])),
        (3, V::U(1)),
        (5, V::T(title.into())),
        (7, V::A(vec![V::U(1)])),
        (8, V::A(items)),
        (9, V::M(vec![])),
    ];
    if let Some(size) = extra {
        fields.push((
            23,
            V::TM(vec![(
                "example.padding".into(),
                V::M(vec![(0, V::Bool(false)), (1, V::B(vec![0; size]))]),
            )]),
        ));
    }
    Value::Map(vec![
        (Value::Uint(1), Value::Text(FORMAT.into())),
        (Value::Uint(2), Value::Bytes(cbor::encode(&V::M(fields)))),
        (Value::Uint(3), Value::Array(vec![])),
        (Value::Uint(4), Value::Array(blobs)),
    ])
}

#[test]
fn rejects_excess_missing_duplicate_unlisted_and_invalid_blobs() {
    let (a, one) = item(1, 26, 1);
    let (b, two) = item(2, 26, 1);
    let (c, three) = item(3, 26, 2);
    let good = unsigned(
        vec![a.clone(), b.clone()],
        vec![one.clone(), two.clone()],
        "Good",
        None,
    );
    assert!(
        Packs::default()
            .review_file(&om_codec::encode(&good))
            .is_ok()
    );
    refuse(
        &unsigned(
            vec![a.clone()],
            vec![one.clone(), two.clone()],
            "Extra",
            None,
        ),
        "pack_blob_count",
    );
    refuse(
        &unsigned(
            vec![a.clone(), b.clone()],
            vec![one.clone()],
            "Missing",
            None,
        ),
        "cache_incomplete",
    );
    refuse(
        &unsigned(
            vec![a.clone(), b],
            vec![one.clone(), one.clone()],
            "Duplicate",
            None,
        ),
        "blob_duplicate",
    );
    refuse(
        &unsigned(vec![a.clone()], vec![two], "Unlisted", None),
        "pack_blob_unlisted",
    );
    let mut corrupt = one;
    let Value::Map(fields) = &mut corrupt else {
        unreachable!()
    };
    let Value::Bytes(bytes) = &mut fields[0].1 else {
        unreachable!()
    };
    bytes[0] ^= 1;
    refuse(
        &unsigned(vec![a], vec![corrupt], "Digest", None),
        "invalid_cached_blob",
    );
    refuse(
        &unsigned(vec![c], vec![three], "Claims", None),
        "invalid_cached_blob",
    );
    let mut oversized_canvas = still_bytes(26);
    oversized_canvas[21..25].copy_from_slice(&512u32.to_le_bytes()); // coded width becomes 513
    let (bad, named) = item_with_plaintext(4, &oversized_canvas, 1);
    refuse(
        &unsigned(vec![bad], vec![named], "Canvas", None),
        "invalid_cached_blob",
    );
}

#[test]
fn declared_overhead_accepts_exactly_64_kib_and_rejects_one_more() {
    let (manifest, blob) = item(1, 26, 1);
    let base = |n| {
        om_codec::encode(&unsigned(
            vec![manifest.clone()],
            vec![blob.clone()],
            "Overhead",
            Some(n),
        ))
    };
    let start = 60_000;
    let n = start + 26 + OVERHEAD as usize - base(start).len();
    let exact = base(n);
    assert_eq!(exact.len(), 26 + OVERHEAD as usize);
    assert!(Packs::default().review_file(&exact).is_ok());
    assert_eq!(
        Packs::default().review_file(&base(n + 1)).err(),
        Some("size_limit")
    );
}

#[test]
fn absolute_file_size_accepts_exactly_32_mib_and_rejects_one_more() {
    const MAX_STICKER: usize = 1 << 20;
    let (mut items, mut blobs): (Vec<_>, Vec<_>) =
        (0..31).map(|id| item(id, MAX_STICKER, 1)).unzip();
    let (last, last_blob) = item(31, MAX_STICKER, 1);
    items.push(last);
    blobs.push(last_blob);
    let first_size =
        om_codec::encode(&unsigned(items.clone(), blobs.clone(), "Boundary", None)).len();
    let excess = first_size - PACK_FILE_MAX;
    let (last, last_blob) = item(31, MAX_STICKER - (excess & !1), 1);
    items[31] = last;
    blobs[31] = last_blob;
    let mut exact = om_codec::encode(&unsigned(
        items,
        blobs,
        if excess % 2 == 0 {
            "Boundary"
        } else {
            "Boundar"
        },
        None,
    ));
    assert_eq!(exact.len(), PACK_FILE_MAX);
    assert!(Packs::default().review_file(&exact).is_ok());
    exact.push(0);
    assert_eq!(
        Packs::default().review_file(&exact).err(),
        Some("size_limit")
    );
}
