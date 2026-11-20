// SPDX-License-Identifier: MPL-2.0
//! Expressive-media portion of the metadata trace test plan (`METADATA_TRACE_TEST_PLAN.md`, V-015 method applied to
//! V-012 scope): record every call a provider role receives while a content pack is sent as an ordinary attachment,
//! received, reviewed and installed, and while stickers, inline emoji and reactions are sent. The pass rule is the
//! plan's: no observation beyond what the descriptors in `spec/metadata/w0-metadata-exposure-descriptors.json` claim,
//! and no signal that an ordinary attachment deposit does not already produce.
//!
//! Capture here is the loopback provider's complete view: every `NetCall` Core issues (taken from the outbox before it
//! is performed), the relay deposit log and the BlobStore contents. Detectors search that material literally, in raw,
//! hex, base64 and base64url form, for every string and byte string inside the pack and for the seeded identifiers.
//! This is the Core-level capture. Browser, TLS, CDN and push captures of the plan need real builds and stay open.

mod common;

use om_core::api::*;
use om_core::attach::Meta;
use om_core::crypto::cbor::{self, V};
use om_core::crypto::{hex, unhex};
use om_core::expressive::{
    BlobPolicy, CachePolicy, Install, Kind, Presentation, Release, SendFrom, Target,
};
use om_core::host::{NetCall, Step};
use om_core::runtime::{LoopbackNetwork, Runtime};
use std::collections::BTreeMap;

const PACK_FILE_VECTORS: &str =
    include_str!("../../../verification/vectors/pack-file-vectors.json");
const DESCRIPTORS: &str =
    include_str!("../../../spec/metadata/w0-metadata-exposure-descriptors.json");

struct Pair {
    net: LoopbackNetwork,
    alice: Runtime,
    bob: Runtime,
    a: ProfileExecutionContext,
    b: ProfileExecutionContext,
    a_conv: String,
    b_conv: String,
}

fn pair() -> Pair {
    let net = LoopbackNetwork::default();
    let mut alice = Runtime::new("alice", &net);
    let mut bob = Runtime::new("bob", &net);
    alice.core.enable_expressive_media();
    bob.core.enable_expressive_media();
    let a = common::create_profile(&mut alice, "p-a", "Alice");
    let b = common::create_profile(&mut bob, "p-b", "Bob");
    alice.dispatch(&a, "inv", IntentBody::CreateInvite);
    alice.flush_net(&a);
    let link = alice
        .query::<Option<InviteView>>(Query::GetInvite {
            ctx: a.clone(),
            operation_id: "inv".into(),
        })
        .unwrap()
        .link;
    bob.dispatch(&b, "acc", IntentBody::AcceptInvitation { link });
    settle_observed(&mut alice, &a, &mut bob, &b, &mut Vec::new());
    let intros: Vec<IntroductionRequest> = alice.query(Query::GetIntroductions { ctx: a.clone() });
    alice.dispatch(
        &a,
        "i",
        IntentBody::AcceptIntroduction {
            request_id: intros[0].id.clone(),
        },
    );
    settle_observed(&mut alice, &a, &mut bob, &b, &mut Vec::new());
    let conv = |rt: &Runtime, c: &ProfileExecutionContext| {
        let l: Vec<ConversationSummary> = rt.query(Query::GetConversationList { ctx: c.clone() });
        assert_eq!(l[0].relationship, RelationshipStateCode::Active);
        l[0].id.clone()
    };
    let (a_conv, b_conv) = (conv(&alice, &a), conv(&bob, &b));
    Pair {
        net,
        alice,
        bob,
        a,
        b,
        a_conv,
        b_conv,
    }
}

/// The runtime's `pump`, with every call recorded before it reaches the provider.
fn pump_observed(
    rt: &mut Runtime,
    ctx: &ProfileExecutionContext,
    seen: &mut Vec<NetCall>,
) -> usize {
    let tick = rt.core.notice_tick(ctx).expect("notice tick");
    rt.settle(&tick);
    let ops = rt.core.net_ops(ctx).expect("net ops");
    let n = ops.len();
    for op in ops {
        seen.push(op.call.clone());
        let result = rt.net.perform(&op.call);
        let step = rt.core.net_result(ctx, op.id, result).expect("net result");
        rt.settle(&step);
    }
    n + rt.poll(ctx)
}

fn settle_observed(
    alice: &mut Runtime,
    a: &ProfileExecutionContext,
    bob: &mut Runtime,
    b: &ProfileExecutionContext,
    seen: &mut Vec<NetCall>,
) {
    for _ in 0..40 {
        let moved = pump_observed(alice, a, seen) + pump_observed(bob, b, seen);
        if moved == 0 {
            return;
        }
    }
    panic!("did not settle");
}

fn settle(p: &mut Pair, seen: &mut Vec<NetCall>) {
    settle_observed(&mut p.alice, &p.a, &mut p.bob, &p.b, seen);
}

fn kinds(calls: &[NetCall]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for call in calls {
        let name = serde_json::to_value(call).unwrap()["call"]
            .as_str()
            .unwrap()
            .to_string();
        *out.entry(name).or_default() += 1;
    }
    out
}

fn send_file(p: &mut Pair, op: &str, plaintext: &[u8], media_type: &str, filename: &str) -> String {
    let prepared = p
        .alice
        .prepare_attachment(
            &p.a,
            plaintext,
            Meta {
                media_type: media_type.into(),
                filename: Some(filename.into()),
                ..Default::default()
            },
        )
        .unwrap();
    p.alice.dispatch(
        &p.a,
        op,
        IntentBody::SendAttachments {
            conversation_id: p.a_conv.clone(),
            draft: Draft {
                text: "here".into(),
                reply_to: None,
            },
            attachments: vec![AttachmentInput {
                blob_id: prepared.blob_id.clone(),
                voice_note: None,
            }],
        },
    );
    prepared.blob_id
}

fn incoming_ids(p: &Pair) -> Vec<String> {
    let view: ConversationView = p.bob.query(Query::GetConversationView {
        ctx: p.b.clone(),
        conversation_id: p.b_conv.clone(),
    });
    view.timeline
        .iter()
        .map(|t| match t {
            TimelineItem::Outgoing { message_id, .. }
            | TimelineItem::Incoming { message_id, .. } => message_id.clone(),
        })
        .collect()
}

// ------------------------------------------------------------------------------------------------ detectors

fn b64(data: &[u8], url: bool) -> String {
    let table: &[u8; 64] = if url {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_"
    } else {
        b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/"
    };
    let mut out = String::new();
    for chunk in data.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | u32::from(*b) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(table[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    !needle.is_empty() && hay.windows(needle.len()).any(|w| w == needle)
}

/// Every encoding in which a value can reach a provider: raw, hex, base64 and base64url (padding-free, and each
/// of the three alignments a substring of a longer base64 string can take).
fn forms(value: &[u8]) -> Vec<Vec<u8>> {
    let mut out = vec![
        value.to_vec(),
        hex(value).into_bytes(),
        hex(value).to_uppercase().into_bytes(),
    ];
    for url in [false, true] {
        for skip in 0..3 {
            let mut shifted = vec![0u8; skip];
            shifted.extend_from_slice(value);
            let text = b64(&shifted, url);
            // Drop the characters that mix in the padding bytes at the front and the partial group at the back.
            let start = skip + usize::from(skip > 0);
            let end = text.len().saturating_sub(2);
            if start + 8 < end {
                out.push(text.as_bytes()[start..end].to_vec());
            }
        }
    }
    out
}

fn walk(v: &V, out: &mut Vec<Vec<u8>>) {
    match v {
        V::T(s) if s.len() >= 4 => out.push(s.as_bytes().to_vec()),
        V::B(b) if b.len() >= 16 => out.push(b.clone()),
        V::A(items) => items.iter().for_each(|i| walk(i, out)),
        V::M(fields) => fields.iter().for_each(|(_, i)| walk(i, out)),
        V::TM(fields) => fields.iter().for_each(|(_, i)| walk(i, out)),
        _ => {}
    }
}

/// The golden genesis pack file and every string and byte string of its manifest, as needles. Registry constants
/// that every pack carries (media type, profile and extension identifiers) are not pack-specific and are skipped.
fn golden_pack() -> (Vec<u8>, Vec<Vec<u8>>) {
    let doc: serde_json::Value = serde_json::from_str(PACK_FILE_VECTORS).unwrap();
    let file = unhex(doc["vectors"][0]["hex"].as_str().unwrap()).unwrap();
    let V::M(fields) = cbor::decode(&file).unwrap() else {
        panic!("pack file is a map")
    };
    let manifest = fields
        .iter()
        .find_map(|(k, v)| (*k == 2).then(|| v.b().unwrap().to_vec()))
        .unwrap();
    let mut needles = Vec::new();
    walk(&cbor::decode(&manifest).unwrap(), &mut needles);
    needles.retain(|n| !n.starts_with(b"om.expressive.") && !n.starts_with(b"image/"));
    needles.sort();
    needles.dedup();
    assert!(needles.len() >= 6, "pack-specific needles: {needles:?}");
    (file, needles)
}

/// Everything any provider role received, as one list of byte strings.
fn provider_view(net: &LoopbackNetwork, calls: &[NetCall]) -> Vec<Vec<u8>> {
    let mut view = net.all_deposited_bytes();
    view.extend(net.blob_ciphertexts());
    for call in calls {
        view.push(serde_json::to_vec(call).unwrap());
    }
    view
}

fn assert_absent(view: &[Vec<u8>], needles: &[Vec<u8>], what: &str) {
    for needle in needles {
        for form in forms(needle) {
            for seen in view {
                assert!(
                    !contains(seen, &form),
                    "{what}: {:?} reached a provider",
                    String::from_utf8_lossy(needle)
                );
            }
        }
    }
}

// ------------------------------------------------------------------------------------------------ the pack path

#[test]
fn pack_send_receive_and_install_look_like_an_ordinary_attachment_to_every_provider() {
    let (pack, needles) = golden_pack();
    let mut p = pair();

    // Control: an ordinary attachment of the same length and the same message text.
    let filler: Vec<u8> = (0..pack.len()).map(|i| (i * 131 % 251) as u8).collect();
    let mut control = Vec::new();
    let setup = p.net.all_deposited_bytes().len();
    send_file(&mut p, "control", &filler, "application/zip", "archive.zip");
    settle(&mut p, &mut control);
    let control_blobs = p.net.blob_ciphertexts();
    let control_deposits: Vec<usize> = p.net.all_deposited_bytes()[setup..]
        .iter()
        .map(Vec::len)
        .collect();

    // Treatment: the pack file as an ordinary attachment of media type application/vnd.om.content-pack.
    let (blobs_before, deposits_before) = (p.net.blob_count(), p.net.deposit_count());
    let mut sent = Vec::new();
    send_file(
        &mut p,
        "pack",
        &pack,
        "application/vnd.om.content-pack",
        "cats.ompack",
    );
    settle(&mut p, &mut sent);

    // 1. Same call classes, same number of each, as the control send.
    let control_sends: Vec<NetCall> = control.clone();
    assert!(!control_sends.is_empty());
    assert_eq!(
        kinds(&sent),
        kinds(&control_sends),
        "pack send added or dropped a provider call class"
    );
    assert_eq!(
        p.net.blob_count() - blobs_before,
        1,
        "one BlobStore object per pack file"
    );
    let pack_deposits = p.net.deposit_count() - deposits_before;
    assert_eq!(pack_deposits, control_deposits.len() as u64);

    // 2. Provider-visible sizes match the control: same ciphertext length, same deposit lengths.
    let all_blobs = p.net.blob_ciphertexts();
    let pack_blob = all_blobs
        .iter()
        .find(|b| !control_blobs.contains(b))
        .expect("pack blob");
    assert_eq!(pack_blob.len(), control_blobs[0].len());
    let deposits: Vec<usize> = p.net.all_deposited_bytes().iter().map(Vec::len).collect();
    assert_eq!(
        &deposits[setup + control_deposits.len()..],
        &control_deposits[..],
        "deposit lengths differ from control"
    );

    // 3. Receiving: the only call is the ordinary blob fetch the person's tap causes.
    let id = incoming_ids(&p).pop().unwrap();
    let plan = p.bob.core.attachment_fetch(&p.b, &id, 0).unwrap();
    assert!(matches!(plan, NetCall::GetBlob { .. }));
    let fetched = match p.net.perform(&plan) {
        om_core::host::NetResult::Bytes { bytes } => bytes,
        other => panic!("{other:?}"),
    };
    let opened = p.bob.core.open_attachment(&p.b, &id, 0, &fetched).unwrap();
    assert_eq!(opened.plaintext, pack);

    // 4. Review and install make no provider call at all.
    let (deposits_before, blobs_before) = (p.net.deposit_count(), p.net.blob_put_count());
    let (ticket, preview) = p
        .bob
        .core
        .review_pack_file(&p.b, &opened.plaintext)
        .unwrap()
        .unwrap();
    let count = p
        .bob
        .core
        .begin_pack_install(
            &p.b,
            ticket,
            &preview.release_id,
            preview.control_digest.as_deref(),
        )
        .unwrap()
        .unwrap();
    for _ in 0..count {
        p.bob.core.stage_pack_blob(&p.b, ticket).unwrap().unwrap();
    }
    let (ok, commit) = p.bob.core.commit_pack_install(&p.b, ticket).unwrap();
    ok.unwrap();
    assert!(p.bob.settle(&Step {
        commit,
        handle: None
    }));
    assert!(
        p.bob.core.net_ops(&p.b).unwrap().is_empty(),
        "install queued provider work"
    );
    let mut after_install = Vec::new();
    settle(&mut p, &mut after_install);
    assert!(
        after_install.is_empty(),
        "install caused provider calls: {:?}",
        kinds(&after_install)
    );
    assert_eq!(p.net.deposit_count(), deposits_before);
    assert_eq!(p.net.blob_put_count(), blobs_before);

    // 5. Nothing from inside the pack, no pack or release identifier, no file name and no seeded identifier reached a
    //    provider, in any encoding.
    let mut all_calls = control;
    all_calls.extend(sent);
    all_calls.push(plan);
    let view = provider_view(&p.net, &all_calls);
    assert_absent(&view, &needles, "pack contents");
    let release = unhex(&preview.release_id).unwrap();
    let mut seeded = vec![
        release,
        b"cats.ompack".to_vec(),
        b"application/vnd.om.content-pack".to_vec(),
    ];
    seeded.push(p.a_conv.as_bytes().to_vec());
    seeded.push(p.b_conv.as_bytes().to_vec());
    for conv in [&p.a_conv, &p.b_conv] {
        if let Ok(raw) = unhex(conv) {
            seeded.push(raw);
        }
    }
    assert_absent(&view, &seeded, "identifiers and file metadata");
}

// ------------------------------------------------------------------------------ stickers, inline emoji, reactions

fn pack_id(kind: Kind) -> [u8; 32] {
    [0x70 + kind as u8; 32]
}
const WEBP_HEADER: &[u8] = b"RIFF\x12\0\0\0WEBPVP8L\x06\0\0\0\x2f\0\0\0\0\0";
const STRINGS: [&str; 4] = ["Local pack", "Item label", "Accessible label", "Fallback"];

fn local_pack(kind: Kind) -> (Vec<u8>, Vec<u8>) {
    use om_core::attach::{AttachmentRef, ciphertext_digest, seal};
    use om_core::expressive::AssetRef;
    let (blob_id, key) = ([12; 32], [23; 32]);
    let ct = seal(&key, &blob_id, WEBP_HEADER);
    let asset = AssetRef {
        attachment: AttachmentRef {
            blob_id,
            key,
            media_type: "image/webp".into(),
            plaintext_size: WEBP_HEADER.len() as u64,
            ciphertext_digest: ciphertext_digest(&ct),
            locations: vec![],
            filename: None,
            read_cap: None,
            width: Some(1),
            height: Some(1),
            duration_ms: None,
        },
        presentation: Presentation::webp(kind, STRINGS[2].into(), Some(STRINGS[3].into())),
    };
    let manifest = cbor::encode(&V::M(vec![
        (0, V::U(45)),
        (1, V::U(1)),
        (2, V::B(pack_id(kind).to_vec())),
        (3, V::U(1)),
        (5, V::T(STRINGS[0].into())),
        (7, V::A(vec![V::U(4)])),
        (
            8,
            V::A(vec![V::M(vec![
                (0, V::T("item".into())),
                (1, V::U(kind as u64)),
                (2, asset.value()),
                (5, V::T(STRINGS[1].into())),
            ])]),
        ),
        (9, V::M(vec![])),
    ]));
    (manifest, ct)
}

fn prepare(p: &mut Pair, kind: Kind, policy: BlobPolicy, install: bool) -> PreparedAttachment {
    let (manifest, ct) = local_pack(kind);
    if install {
        let id = Release::parse(&manifest).unwrap().id();
        let (ok, commit) = p
            .alice
            .core
            .install_content_pack(
                &p.a,
                Install {
                    release: &manifest,
                    expected_release: id,
                    confirmed_release: id,
                    control: None,
                    policy: CachePolicy::Lazy,
                    favourites: &[],
                    cached: &[],
                },
            )
            .unwrap();
        ok.unwrap();
        assert!(p.alice.settle(&Step {
            commit,
            handle: None
        }));
    }
    let (prepared, step) = p
        .alice
        .core
        .prepare_expressive(&p.a, pack_id(kind), "item", &ct, policy)
        .unwrap();
    assert!(p.alice.settle(&step));
    let prepared = prepared.unwrap();
    p.net.stage_blob(
        &unhex(&prepared.blob_id).unwrap(),
        prepared.ciphertext.clone(),
    );
    prepared
}

fn send_expressive(p: &mut Pair, op: &str, target: Target, asset: SendFrom) {
    p.alice.dispatch(
        &p.a,
        op,
        IntentBody::SendExpressive {
            conversation_id: p.a_conv.clone(),
            target,
            asset,
        },
    );
}

fn prepared(blob: &PreparedAttachment) -> SendFrom {
    SendFrom::Prepared {
        blob_id: blob.blob_id.clone(),
    }
}

#[test]
fn sticker_inline_emoji_and_reaction_sends_add_no_provider_call_class() {
    let mut p = pair();
    // The ordinary baseline: a plain attachment message and a plain text message.
    let mut baseline = Vec::new();
    send_file(&mut p, "plain", &[7; 300], "image/png", "photo.png");
    settle(&mut p, &mut baseline);
    let allowed: Vec<String> = kinds(&baseline).into_keys().collect();
    assert!(allowed.iter().any(|k| k == "put_blob") && allowed.iter().any(|k| k == "deposit"));

    let sticker = prepare(&mut p, Kind::Sticker, BlobPolicy::Fresh, true);
    let emoji = prepare(&mut p, Kind::CustomEmoji, BlobPolicy::Fresh, true);
    let reaction = prepare(&mut p, Kind::ReactionAsset, BlobPolicy::Fresh, true);
    let mut seen = Vec::new();
    send_expressive(&mut p, "sticker", Target::Attachment, prepared(&sticker));
    settle(&mut p, &mut seen);
    send_expressive(&mut p, "inline", Target::Inline, prepared(&emoji));
    settle(&mut p, &mut seen);
    let mid = incoming_ids(&p).pop().unwrap();
    send_expressive(
        &mut p,
        "react",
        Target::Reaction {
            message_id: mid,
            key: "k1".into(),
        },
        prepared(&reaction),
    );
    settle(&mut p, &mut seen);

    for class in kinds(&seen).keys() {
        assert!(
            allowed.contains(class),
            "expressive sends caused a new call class: {class}"
        );
    }
    // Each send is one deposit; no send installed, listed or looked up anything at a provider.
    assert_eq!(kinds(&seen).get("deposit").copied().unwrap_or(0), 3);

    let mut needles: Vec<Vec<u8>> = STRINGS.iter().map(|s| s.as_bytes().to_vec()).collect();
    for kind in [Kind::Sticker, Kind::CustomEmoji, Kind::ReactionAsset] {
        needles.push(pack_id(kind).to_vec());
    }
    needles.push(b"item".to_vec());
    let mut calls = baseline;
    calls.extend(seen);
    assert_absent(
        &provider_view(&p.net, &calls),
        &needles,
        "pack item metadata",
    );
}

// ----------------------------------------------------------------------------------------- shared-blob linkage

/// The BlobStore-visible object ids, in upload order, and how many uploads it saw.
fn store_view(p: &Pair) -> (usize, u64) {
    (p.net.blob_count(), p.net.blob_put_count())
}

#[test]
fn shared_blob_linkage_is_the_only_added_signal_and_carries_its_disclosure_line() {
    // The claim under test, from the descriptors: only the BlobStore can see the linkage, and only sometimes.
    let doc: serde_json::Value = serde_json::from_str(DESCRIPTORS).unwrap();
    for role in doc["descriptors"].as_array().unwrap() {
        let expected = if role["role"] == "blob_store" {
            "sometimes"
        } else {
            "no_by_design"
        };
        assert_eq!(
            role["exposures"]["shared_blob_linkage"], expected,
            "{}",
            role["role"]
        );
    }
    assert!(doc["notes"].as_array().unwrap().iter().any(|n| {
        let n = n.as_str().unwrap();
        n.starts_with("shared_blob_linkage")
            && n.contains("opt-in")
            && n.contains("can link those conversations")
    }));

    // The default carries no disclosure line, and the opt-in carries the line naming who can link what.
    assert!(BlobPolicy::Fresh.metadata_exposure().is_none());
    let line = BlobPolicy::SharedBlob
        .metadata_exposure()
        .expect("linkage line present");
    assert!(
        line.contains("BlobStore") && line.contains("link conversations by BlobID"),
        "{line}"
    );

    // Default: every send of the same asset uploads a new object, so the store sees unrelated BlobIDs.
    let mut p = pair();
    let first = prepare(&mut p, Kind::Sticker, BlobPolicy::Fresh, true);
    let second = prepare(&mut p, Kind::Sticker, BlobPolicy::Fresh, false);
    assert_ne!(first.blob_id, second.blob_id);
    let mut calls = Vec::new();
    send_expressive(&mut p, "one", Target::Attachment, prepared(&first));
    send_expressive(&mut p, "two", Target::Attachment, prepared(&second));
    settle(&mut p, &mut calls);
    assert_eq!(store_view(&p), (2, 2));
    let ciphertexts = p.net.blob_ciphertexts();
    assert_ne!(
        ciphertexts[0], ciphertexts[1],
        "fresh sends must not share ciphertext"
    );

    // Opt-in: the second send reuses the first object, which is exactly the linkage the line discloses. Nothing
    // else changes: still one deposit per send and no new call class.
    let mut p = pair();
    let shared = prepare(&mut p, Kind::Sticker, BlobPolicy::SharedBlob, true);
    let mut calls = Vec::new();
    send_expressive(&mut p, "one", Target::Attachment, prepared(&shared));
    settle(&mut p, &mut calls);
    let mid = incoming_ids(&p).pop().unwrap();
    let sender_view: ConversationView = p.alice.query(Query::GetConversationView {
        ctx: p.a.clone(),
        conversation_id: p.a_conv.clone(),
    });
    let sent_id = match &sender_view.timeline[0] {
        TimelineItem::Outgoing { message_id, .. } | TimelineItem::Incoming { message_id, .. } => {
            message_id.clone()
        }
    };
    let _ = mid;
    let conversation_id = p.a_conv.clone();
    send_expressive(
        &mut p,
        "two",
        Target::Attachment,
        SendFrom::Shared {
            conversation_id,
            message_id: sent_id,
        },
    );
    settle(&mut p, &mut calls);
    assert_eq!(
        store_view(&p),
        (1, 1),
        "the store saw one object for two sends"
    );
    assert_eq!(kinds(&calls).get("deposit").copied().unwrap_or(0), 2);
    assert_eq!(kinds(&calls).get("put_blob").copied().unwrap_or(0), 1);
    assert!(
        kinds(&calls)
            .keys()
            .all(|k| matches!(k.as_str(), "deposit" | "put_blob" | "ack")),
        "{:?}",
        kinds(&calls)
    );
}

// ------------------------------------------------------------------------------------------------ detector control

#[test]
fn the_detector_finds_a_planted_value_in_every_encoding_and_alignment() {
    let secret: Vec<u8> = (0..32).map(|i| (i * 37 + 11) as u8).collect();
    let found = |hay: &[u8]| forms(&secret).iter().any(|f| contains(hay, f));
    assert!(found(&[b"xx".as_slice(), &secret, b"yy"].concat()));
    assert!(found(format!("zz{}zz", hex(&secret)).as_bytes()));
    assert!(found(
        format!("zz{}zz", hex(&secret).to_uppercase()).as_bytes()
    ));
    for url in [false, true] {
        for pad in 0..3usize {
            let mut raw = vec![0xa5; pad];
            raw.extend_from_slice(&secret);
            raw.extend_from_slice(b"tail");
            assert!(
                found(b64(&raw, url).as_bytes()),
                "base64 (url={url}) with {pad} leading bytes"
            );
        }
    }
    assert!(!found(b"nothing of the secret here"));
}
