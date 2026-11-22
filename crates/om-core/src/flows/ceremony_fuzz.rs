// SPDX-License-Identifier: MPL-2.0
//! Byte-slice entry points over the link-ceremony frames and the continuation form (ADR-0192, ADR-0195), for external
//! fuzzers and the seed-corpus replay test (V-012). Compiled only with the `fuzz-entry` feature, which a `production`
//! build refuses (R-072). Reached through `crate::recovery_fuzz_entry::link`.
//!
//! The frames are sealed, so a fuzzer cannot pass the AEAD by chance. These entry points therefore also seal the input
//! themselves under fixed synthetic keys and check the opener against an independent restatement of the framing rule
//! (length prefix, zero padding), then run the field readers over the opened body. Every function returns `Ok(())` when
//! the input was accepted and a stable refusal code otherwise, and panics only when a stated invariant fails.

use super::*;
use crate::crypto::recovery_approval as ra;

type Outcome = Result<(), &'static str>;

/// Fixed synthetic ceremony context; nothing here is a real key.
fn fixed_keys(approval: bool) -> CeremonyKeys {
    let (a_cap, b_recv) = ([0xa1u8; 24], [0xb2u8; 24]);
    if approval {
        ra::keys(
            1_790_000_300,
            b"om-test://home",
            &[0x11; 16],
            &a_cap,
            &b_recv,
        )
    } else {
        lc::keys(
            1_790_000_300,
            b"om-test://home",
            &[0x11; 16],
            &a_cap,
            &b_recv,
        )
    }
}

/// Seal `frame` (exactly one bucket's plaintext length) as `seal_msg_ad` does, with a fixed nonce and filler.
fn seal_frame(
    keys: &CeremonyKeys,
    role: u8,
    ad: fn(&[u8; 32], u8) -> Vec<u8>,
    frame: &[u8],
) -> Option<Vec<u8>> {
    let nonce = [7u8; 24];
    let ct = XChaCha20Poly1305::new(keys.for_role(role).into())
        .encrypt(
            (&nonce).into(),
            Payload {
                msg: frame,
                aad: &ad(&keys.id, role),
            },
        )
        .ok()?;
    Some(wrap_envelope(
        &nonce,
        ct,
        [3u8; 32],
        vec![5u8; ENC_LEN],
        1_790_000_300,
    ))
}

/// The framing rule restated: `len (u32 big endian) || body || zeros`, nothing else.
fn expected_body(frame: &[u8]) -> Option<Vec<u8>> {
    let n = u32::from_be_bytes(frame.get(..4)?.try_into().ok()?) as usize;
    let end = n.checked_add(4)?;
    let body = frame.get(4..end)?;
    frame[end..].iter().all(|b| *b == 0).then(|| body.to_vec())
}

/// Run the field readers a ceremony body reaches, over bytes that opened.
fn read_body(body: &[u8]) {
    let Ok(v) = cbor::decode(body) else { return };
    assert_eq!(
        cbor::encode(&v),
        body,
        "strict decode re-encodes to its input"
    );
    let _ = v.get(0).and_then(|x| x.u());
    let _ = history_index(&v, 4, &[]);
    let _ = history_index(&v, 32, &[0, 1, 2]);
    let _ = notice_state(body);
    let _ = notice_device(body);
    let _ = chain_head(body);
}

fn frame_case(input: &[u8], approval: bool) -> Outcome {
    let keys = fixed_keys(approval);
    let ad: fn(&[u8; 32], u8) -> Vec<u8> = if approval { ra::aead_ad } else { lc::aead_ad };
    // The smallest bucket whose plaintext holds the input; longer input is cut, shorter is zero padded.
    let frame_len = BUCKETS
        .iter()
        .map(|b| b - NONCE_LEN - TAG_LEN)
        .find(|n| *n >= input.len())
        .unwrap_or(BUCKETS[BUCKETS.len() - 1] - NONCE_LEN - TAG_LEN);
    let mut frame = vec![0u8; frame_len];
    let take = input.len().min(frame_len);
    frame[..take].copy_from_slice(&input[..take]);
    let sealed = seal_frame(&keys, ROLE_REQ, ad, &frame).ok_or("seal_refused")?;
    let want = expected_body(&frame);
    let got = open_msg_ad(&keys, ROLE_REQ, &sealed, ad);
    assert_eq!(got, want, "the opener disagrees with the framing rule");
    // The other direction's key and associated data never open it.
    assert!(
        open_msg_ad(&keys, ROLE_RESP, &sealed, ad).is_none(),
        "a frame opened under the wrong role"
    );
    match got {
        Some(body) => {
            read_body(&body);
            Ok(())
        }
        None => Err("frame_refused"),
    }
}

/// `cont_chunks` (ADR-0195): the sizes come from the input, the content is a pattern. Checks the chunk count bound and
/// that the chunks, read back in order, carry exactly the states and credentials that went in.
fn continuation_case(input: &[u8]) -> Outcome {
    // Sizes come from the input; a 60,000 byte item (the spill case) is allowed for the first few items only, which keeps
    // one execution well inside the 1 second bound under a sanitizer.
    let size = |b: u8, big: bool| {
        if b == 255 && big {
            60_000
        } else {
            usize::from(b) * 8
        }
    };
    let n_states = usize::from(*input.first().unwrap_or(&0)) % (MAX_STATES + 1);
    let n_creds = usize::from(*input.get(1).unwrap_or(&0)) % (MAX_CREDS + 1);
    let mut at = 2;
    let mut next = |tag: u8, i: usize| {
        let b = *input.get(at).unwrap_or(&1);
        at += 1;
        vec![tag.wrapping_add(i as u8); size(b, at <= 8)]
    };
    let states: Vec<Vec<u8>> = (0..n_states).map(|i| next(0x10, i)).collect();
    let creds: Vec<Vec<u8>> = (0..n_creds).map(|i| next(0x80, i)).collect();
    let Some(chunks) = cont_chunks(&states, &creds) else {
        return Err("too_many_chunks");
    };
    assert!(
        chunks.len() <= CONT_MAX_CHUNKS,
        "chunk count over the bound"
    );
    let (mut got_states, mut got_creds) = (vec![], vec![]);
    for (i, c) in chunks.iter().enumerate() {
        let v = cbor::decode(c).expect("a chunk decodes");
        assert_eq!(v.get(0).and_then(|x| x.u()).ok(), Some(M_CHAIN));
        assert_eq!(
            v.get(1).and_then(|x| x.u()).ok(),
            Some(i as u64),
            "chunk index"
        );
        for s in v.get(2).and_then(|x| x.a()).expect("states") {
            got_states.push(s.b().expect("state bytes").to_vec());
        }
        for s in v.get(3).and_then(|x| x.a()).expect("creds") {
            got_creds.push(s.b().expect("credential bytes").to_vec());
        }
    }
    assert_eq!(got_states, states, "states lost, reordered or altered");
    assert_eq!(got_creds, creds, "credentials lost, reordered or altered");
    Ok(())
}

fn text(rest: &[u8]) -> String {
    String::from_utf8_lossy(rest).into_owned()
}

/// Link-ceremony and continuation frames. The first byte picks the case.
pub fn link(input: &[u8]) -> Outcome {
    let Some((&mode, rest)) = input.split_first() else {
        return Err("short");
    };
    match mode % 8 {
        0 => {
            let link = text(rest);
            let parts = decode_link(&link).map_err(|_| "link_refused")?;
            assert_eq!(parts.secret.len(), 16);
            assert!((16..=64).contains(&parts.b_recv.len()));
            // A parsed link survives a re-encode and decodes to the same parts.
            let again = encode_link(
                parts.expiry_s,
                &parts.secret,
                &parts.a_cap,
                &parts.b_recv,
                &parts.uri,
            )
            .expect("a parsed link re-encodes");
            let p2 = decode_link(&again).map_err(|_| "reencoded link refused")?;
            assert_eq!(
                (p2.expiry_s, &p2.secret, &p2.a_cap, &p2.b_recv, &p2.uri),
                (
                    parts.expiry_s,
                    &parts.secret,
                    &parts.a_cap,
                    &parts.b_recv,
                    &parts.uri
                )
            );
            Ok(())
        }
        1 => {
            let t = intro::parse_link_token(rest).ok_or("token_refused")?;
            assert!(!t.services.is_empty() && t.services.len() <= intro::MAX_LINK_SERVICES);
            Ok(())
        }
        2 => super::controller_change::fuzz_parse_link(&text(rest)),
        3 => {
            let link = text(rest);
            let o = ra::decode_offer(&link).map_err(|_| "offer_refused")?;
            assert_eq!(ra::encode_offer(&o).as_deref(), Some(link.trim()));
            Ok(())
        }
        4 => {
            // Arbitrary bytes as a queue item: refused before any plaintext exists.
            let _ = open_msg(&fixed_keys(false), ROLE_REQ, rest);
            let _ = open_msg_ad(&fixed_keys(true), ROLE_RESP, rest, ra::aead_ad);
            Err("frame_refused")
        }
        5 => frame_case(rest, false),
        6 => frame_case(rest, true),
        _ => continuation_case(rest),
    }
}
