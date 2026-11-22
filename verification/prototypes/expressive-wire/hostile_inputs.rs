// SPDX-License-Identifier: Apache-2.0
//! Offline hostile-header corpus and property fuzzing against the product parser.

use om_core::expressive::{Frame, Kind, validate_facts, walk_webp};
use proptest::prelude::*;

// A small lossless WebP header. Pixel bytes are opaque to the walker.
const STILL: &[u8] = b"RIFF\x12\0\0\0WEBPVP8L\x06\0\0\0\x2f\0\0\0\0\0";

const KINDS: [Kind; 4] = [
    Kind::Sticker,
    Kind::CustomEmoji,
    Kind::ReactionAsset,
    Kind::LoopAnimation,
];

fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut out = tag.to_vec();
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    if !payload.len().is_multiple_of(2) {
        out.push(0);
    }
    out
}

fn riff(chunks: &[u8]) -> Vec<u8> {
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&((chunks.len() + 4) as u32).to_le_bytes());
    out.extend_from_slice(b"WEBP");
    out.extend_from_slice(chunks);
    out
}

fn animation(payload: &[u8]) -> Vec<u8> {
    let mut chunks = chunk(b"VP8X", &[2, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    chunks.extend(chunk(b"ANIM", &[0; 6]));
    for _ in 0..2 {
        let mut frame = [0; 16].to_vec();
        frame[12] = 34;
        frame.extend_from_slice(payload);
        chunks.extend(chunk(b"ANMF", &frame));
    }
    riff(&chunks)
}

#[test]
fn hostile_chunk_corpus() {
    let good = animation(&STILL[12..]);
    assert_eq!(walk_webp(Kind::Sticker, STILL).unwrap().frames, 1);
    assert_eq!(walk_webp(Kind::LoopAnimation, &good).unwrap().frames, 2);
    let check = |bytes: &[u8]| assert!(walk_webp(Kind::LoopAnimation, bytes).is_err());
    for i in 0..good.len() {
        check(&good[..i]);
        if i >= 12 {
            check(&riff(&good[12..i]));
        }
    }
    for len in [1u32, 5, 7, u32::MAX] {
        let mut overrun = good.clone();
        overrun[16..20].copy_from_slice(&len.to_le_bytes());
        check(&overrun);
    }
    // An apparent nested container, nested frame, or overlapping size must be refused.
    check(&animation(&chunk(b"RIFF", &good)));
    check(&animation(&chunk(b"ANMF", &good)));
    let mut overlap = STILL[12..].to_vec();
    overlap.extend_from_slice(&STILL[12..]);
    check(&animation(&overlap));
    let mut wrong_frame_size = STILL[12..].to_vec();
    wrong_frame_size[9] = 1;
    check(&animation(&wrong_frame_size));
    check(&riff(&good[30..]));
}

#[test]
fn every_profile_at_and_over_its_limits() {
    for (kind, canvas, bytes, ms, frames) in [
        (Kind::Sticker, 512, 1 << 20, 3000, 90usize),
        (Kind::CustomEmoji, 256, 1 << 19, 3000, 90),
        (Kind::ReactionAsset, 256, 1 << 19, 3000, 90),
        (Kind::LoopAnimation, 1024, 6 << 20, 10000, 300),
    ] {
        let at = (0..frames)
            .map(|i| Frame {
                x: 0,
                y: 0,
                width: canvas,
                height: canvas,
                duration_ms: 33 + u32::from(i < frames / 3),
            })
            .collect::<Vec<_>>();
        let validate = |size, w, h, fs| validate_facts(kind, size, w, h, true, fs);
        let got = validate(bytes, canvas, canvas, &at).unwrap();
        assert_eq!(got.duration_ms, Some(ms));
        assert_eq!(got.total_pixels, canvas * canvas * frames as u32);
        assert_eq!(
            validate(bytes + 1, canvas, canvas, &at),
            Err("encoded_size")
        );
        assert_eq!(validate(bytes, canvas + 1, canvas, &at), Err("canvas"));
        assert_eq!(validate(bytes, canvas, canvas + 1, &at), Err("canvas"));
        let mut over = at.clone();
        over.push(at[0]);
        assert_eq!(validate(bytes, canvas, canvas, &over), Err("frames"));
        let mut over = at.clone();
        over[0].duration_ms += 1;
        assert_eq!(validate(bytes, canvas, canvas, &over), Err("duration"));
        let mut over = at.clone();
        over[0].duration_ms -= 1;
        assert_eq!(validate(bytes, canvas, canvas, &over), Err("frame_rate"));
        let mut over = at.clone();
        over[0].width += 1;
        assert_eq!(validate(bytes, canvas, canvas, &over), Err("frame_rect"));
        let mut over = at.clone();
        over[0].height += 1;
        assert_eq!(validate(bytes, canvas, canvas, &over), Err("frame_rect"));
    }
    assert_eq!(
        walk_webp(Kind::LoopAnimation, STILL),
        Err("animation_required")
    );
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 4096,
        failure_persistence: None,
        ..ProptestConfig::default()
    })]

    #[test]
    fn arbitrary_and_structured_mutations_stay_bounded(
        raw in proptest::collection::vec(any::<u8>(), 0..768),
        edits in proptest::collection::vec((any::<usize>(), any::<u8>()), 0..32),
    ) {
        let mut seeded = animation(&STILL[12..]);
        for (index, byte) in edits {
            let len = seeded.len();
            seeded[index % len] = byte;
        }
        for bytes in [&raw, &seeded] {
            for kind in KINDS {
                if let Ok(facts) = walk_webp(kind, bytes) {
                    prop_assert_eq!(facts.encoded_size, bytes.len());
                    prop_assert!((1..=1024).contains(&facts.width));
                    prop_assert!((1..=1024).contains(&facts.height));
                    prop_assert!((1..=300).contains(&facts.frames));
                    prop_assert!(facts.total_pixels <= 314_572_800);
                }
            }
        }
    }
}

#[test]
fn product_budget_agrees_with_frozen_container_vectors() {
    let doc: serde_json::Value = serde_json::from_reader(flate2::read::GzDecoder::new(
        &include_bytes!("../../vectors/expressive-wire-vectors.json.gz")[..],
    ))
    .unwrap();
    for row in doc["container"].as_array().unwrap() {
        let kind = match row["resource_profile"].as_str().unwrap() {
            "sticker.v1" => Kind::Sticker,
            "custom-emoji.v1" => Kind::CustomEmoji,
            "loop-animation.v1" => Kind::LoopAnimation,
            _ => unreachable!(),
        };
        let facts = &row["facts"];
        let get = |key: &str| facts[key].as_u64().unwrap() as u32;
        let frames: Vec<_> = facts["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| {
                let get = |idx: usize| r[idx].as_u64().unwrap() as u32;
                Frame {
                    x: get(0),
                    y: get(1),
                    width: get(2),
                    height: get(3),
                    duration_ms: get(4),
                }
            })
            .collect();
        let animated = frames.len() > 1 || frames.iter().any(|frame| frame.duration_ms != 0);
        let result = validate_facts(
            kind,
            get("encoded_size") as usize,
            get("canvas_w"),
            get("canvas_h"),
            animated,
            &frames,
        );
        if row["expect"] == "valid" {
            assert!(result.is_ok(), "{}: {result:?}", row["id"]);
        } else {
            assert_eq!(
                result.unwrap_err(),
                row["rule"].as_str().unwrap(),
                "{}",
                row["id"]
            );
        }
    }
}
