// SPDX-License-Identifier: MPL-2.0
//! Byte-slice entry points over the expressive-media parsers, for external fuzzers and the seed-corpus replay test
//! (V-012, ADR-0251). Compiled only with the `fuzz-entry` feature, which a `production` build refuses (R-072).
//!
//! Every function takes arbitrary bytes, returns `Ok(())` when the input was accepted and the stable refusal code
//! otherwise, and panics only when a stated invariant fails: that panic is the finding. No function allocates in
//! proportion to anything but the input length.

use super::pack::{CachePolicy, Install, Packs, Release};
use super::{AssetRef, Error, Frame, Kind, decode_event, validate_facts, walk_webp};

const KINDS: [Kind; 4] = [
    Kind::Sticker,
    Kind::CustomEmoji,
    Kind::ReactionAsset,
    Kind::LoopAnimation,
];

/// Split `[u16 big-endian length][head][tail]`. A length past the end takes the whole remainder.
fn split(input: &[u8]) -> (&[u8], &[u8]) {
    let Some((len, rest)) = input.split_first_chunk::<2>() else {
        return (input, &[]);
    };
    let at = usize::from(u16::from_be_bytes(*len)).min(rest.len());
    rest.split_at(at)
}

/// ADR-0251 target (a): the WebP RIFF header and chunk walker with the profile and budget validator.
pub fn webp(input: &[u8]) -> Result<(), Error> {
    let mut accepted = false;
    for kind in KINDS {
        let first = walk_webp(kind, input);
        assert_eq!(first, walk_webp(kind, input), "walker is not deterministic");
        if let Ok(facts) = first {
            let limits = kind.limits();
            assert_eq!(facts.encoded_size, input.len());
            assert!((1..=limits.canvas).contains(&facts.width));
            assert!((1..=limits.canvas).contains(&facts.height));
            assert!((1..=limits.frames).contains(&facts.frames));
            assert!(facts.total_pixels <= limits.canvas * limits.canvas * limits.frames);
            if let Some(ms) = facts.duration_ms {
                assert!(ms <= limits.duration);
            }
            accepted = true;
        }
    }
    if accepted {
        Ok(())
    } else {
        Err("webp_refused")
    }
}

/// ADR-0251 target (a), budget half: header facts supplied independently of any bytes.
/// Layout: `kind:u8, size:u32, width:u32, height:u32, animated:u8, then frames of 5 x u32` (little endian).
pub fn facts(input: &[u8]) -> Result<(), Error> {
    let u32_at = |at: usize| {
        input
            .get(at..at + 4)
            .map_or(0, |b| u32::from_le_bytes(b.try_into().unwrap()))
    };
    let Some(&kind) = input.first() else {
        return Err("short");
    };
    let kind = KINDS[usize::from(kind) % KINDS.len()];
    let frames: Vec<Frame> = input
        .get(14..)
        .unwrap_or_default()
        .as_chunks::<20>()
        .0
        .iter()
        .map(|f| {
            let n = |i: usize| u32::from_le_bytes(f[i * 4..i * 4 + 4].try_into().unwrap());
            Frame {
                x: n(0),
                y: n(1),
                width: n(2),
                height: n(3),
                duration_ms: n(4),
            }
        })
        .collect();
    let animated = input.get(13).is_some_and(|b| b & 1 == 1);
    let result = validate_facts(
        kind,
        u32_at(1) as usize,
        u32_at(5),
        u32_at(9),
        animated,
        &frames,
    );
    let limits = kind.limits();
    if let Ok(f) = result {
        assert!(f.width <= limits.canvas && f.height <= limits.canvas);
        assert!(f.frames <= limits.frames);
        assert!(f.total_pixels <= limits.canvas * limits.canvas * limits.frames);
        assert!(f.encoded_size <= limits.bytes);
    }
    result.map(|_| ())
}

/// ADR-0251 target (b): `ExpressiveAssetRef` and its header-claim check against supplied plaintext.
/// Layout: `[u16 length][ExpressiveAssetRef][candidate plaintext]`.
pub fn asset_ref(input: &[u8]) -> Result<(), Error> {
    let (bytes, plaintext) = split(input);
    let asset = AssetRef::parse(bytes)?;
    // Total accessors and the header-claim check must not panic on any accepted reference.
    let _ = asset.presentation.fallback();
    let first = asset.validate_plaintext(plaintext);
    assert_eq!(
        first,
        asset.validate_plaintext(plaintext),
        "not deterministic"
    );
    Ok(())
}

/// ADR-0251 target (b), inline and reaction half: a conversation event under the expressive grammar. The W0
/// grammar is run beside it only to check that it neither panics nor changes its answer.
pub fn event(input: &[u8]) -> Result<(), Error> {
    let on = decode_event(input, true).map(|_| ());
    assert_eq!(
        on,
        decode_event(input, true).map(|_| ()),
        "not deterministic"
    );
    let off = decode_event(input, false).map(|_| ());
    assert_eq!(
        off,
        decode_event(input, false).map(|_| ()),
        "not deterministic"
    );
    on
}

/// ADR-0251 target (c): the `ContentPack` manifest and `ContentPackRelease` parser.
pub fn release(input: &[u8]) -> Result<(), Error> {
    let first = Release::parse(input)?;
    let second = Release::parse(input).map_err(|_| "release_not_deterministic")?;
    assert_eq!(first.id(), second.id());
    assert_eq!(first.pack_id(), second.pack_id());
    Ok(())
}

/// ADR-0251 target (d): the `ContentPackControlState` envelope parser and the lineage verifier, driven through
/// the installation path. Layout: `[u16 length][ContentPackRelease][ContentPackControlState envelope]`.
pub fn control_state(input: &[u8]) -> Result<(), Error> {
    let (manifest, envelope) = split(input);
    let release = Release::parse(manifest)?;
    let mut packs = Packs::default();
    packs.install(Install {
        release: manifest,
        expected_release: release.id(),
        confirmed_release: release.id(),
        control: Some(envelope),
        policy: CachePolicy::Lazy,
        favourites: &[],
        cached: &[],
    })
}

/// The portable pack-file codec and its predecessor-chain walk (ADR-0254), driven through review, the
/// confirmation-bound staging steps and activation. A refused file must leave the inventory untouched.
pub fn pack_file(input: &[u8]) -> Result<(), Error> {
    let mut packs = Packs::default();
    let mut review = match packs.review_file(input) {
        Ok(review) => review,
        Err(reason) => {
            assert!(packs.is_empty(), "a refused file changed the inventory");
            return Err(reason);
        }
    };
    let preview = review.preview.clone();
    assert!(
        review.stage().is_err(),
        "staging was possible before confirmation"
    );
    let count = review.begin(
        &packs,
        &preview.release_id,
        preview.control_digest.as_deref(),
    )?;
    for _ in 0..count {
        review.stage()?;
    }
    assert!(review.stage().is_err(), "more blobs staged than announced");
    review.activate(&mut packs)?;
    assert!(!packs.is_empty());
    Ok(())
}

/// Every entry point by target name, so runners and the replay test share one table.
/// One byte-slice entry point.
pub type Entry = fn(&[u8]) -> Result<(), Error>;

pub const TARGETS: [(&str, Entry); 7] = [
    ("webp", webp),
    ("facts", facts),
    ("asset_ref", asset_ref),
    ("event", event),
    ("release", release),
    ("control_state", control_state),
    ("pack_file", pack_file),
];
