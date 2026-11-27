// SPDX-License-Identifier: MPL-2.0
#![cfg(feature = "scope-engines")]

use om_archive_reader::{ArchiveError, EntryKind, Limits, ZipError, read_archive};
use om_core::crypto::{hex, rng::insecure_test_rng::InsecureTestRng, unhex};
use om_wasm::bridge::Bridge;
use serde_json::{Value, json};

fn session() -> impl FnMut(Value) -> Value {
    let mut bridge = Bridge::<InsecureTestRng>::new(
        || panic!("portability must not need entropy"),
        || panic!("portability must not read the clock"),
    );
    move |request| {
        let input = json!({"cmd": "portability", "request": request});
        let envelope: Value =
            serde_json::from_slice(&bridge.handle(&serde_json::to_vec(&input).unwrap())).unwrap();
        envelope
            .get("ok")
            .unwrap_or_else(|| panic!("{envelope}"))
            .clone()
    }
}

fn fixture(name: &str) -> String {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../om-archive-reader/tests/fixtures/om-archive-v1-vectors.json"
    ))
    .unwrap();
    vectors["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|vector| vector["name"] == name)
        .unwrap()["archive_hex"]
        .as_str()
        .unwrap()
        .to_owned()
}

fn readonly(archive_hex: &str, live_control: Value, verdicts: Value) -> Value {
    json!({
        "op": "import_readonly", "archiveHex": archive_hex, "liveControl": live_control,
        "verdicts": verdicts,
    })
}

fn candidate(item: Value) -> Value {
    json!({"item": item, "disappearing": false, "viewOnce": false})
}

#[test]
fn exports_the_plain_vector_byte_for_byte_and_reports_exclusions() {
    let mut call = session();
    let plain = fixture("plain");
    let bytes = unhex(&plain).unwrap();
    let archive = read_archive(&bytes, &Limits::default()).unwrap();
    let mut candidates: Vec<_> = archive.entries.iter().map(|entry| candidate(match entry.kind {
        EntryKind::CanonicalObject => json!({
            "kind": "object", "objectType": entry.declared_type, "bytesHex": hex(entry.bytes),
        }),
        EntryKind::Blob => json!({
            "kind": "blob", "blobId": entry.path.strip_prefix("blobs/").unwrap(),
            "mediaType": "image/webp", "bytesHex": hex(entry.bytes),
        }),
        EntryKind::Record => json!({
            "kind": "record", "recordType": entry.declared_type, "stem": "imported-history",
            "bytesHex": hex(entry.bytes),
        }),
    })).collect();
    candidates.push(candidate(
        json!({"kind": "object", "objectType": "om.device_vault", "bytesHex": "01"}),
    ));
    let mut only_once =
        candidate(json!({"kind": "object", "objectType": "test.event", "bytesHex": "02"}));
    only_once["viewOnce"] = json!(true);
    candidates.push(only_once);
    let mut disappearing =
        candidate(json!({"kind": "object", "objectType": "test.event", "bytesHex": "03"}));
    disappearing["disappearing"] = json!(true);
    candidates.push(disappearing);

    let mut request = json!({
        "op": "export_plan", "candidates": candidates, "selection": {"includeBlobs": true},
        "meta": {
            "archiveId": hex(&archive.archive_id),
            "exportScopeHex": "a16d636f6e766572736174696f6e738166636f6e762d31",
            "sourceAccountHint": null, "includedConversations": ["conv-1"],
            "generatedAtHint": 1_790_000_000, "exporterImplementation": "omarchive-prototype/0",
            "provenanceProfiles": ["om-native", "foreign-import"],
        },
    });
    let plan = call(request.clone());
    assert_eq!(
        plan["report"],
        json!({
            "objects": 3, "blobs": 1, "records": 1, "omittedBlobs": [],
            "excluded": {"secret_type": 1, "view_once": 1, "disappearing": 1},
        })
    );
    request["op"] = json!("export");
    let output = call(request);
    assert_eq!(output["report"], plan["report"]);
    assert_eq!(output["archiveHex"], plain);
    let inspected = call(json!({"op": "inspect", "archiveHex": output["archiveHex"]}));
    assert_eq!(inspected["manifest"]["objects"], 3);
    assert_eq!(
        inspected["modes"],
        json!({
            "read_only": "available", "same_account": "requires_live_validation",
        })
    );
    let view = call(readonly(
        output["archiveHex"].as_str().unwrap(),
        json!([]),
        json!([]),
    ));
    assert_eq!(view["objects"].as_array().unwrap().len(), 3);
    assert!(view["records"].is_array());
}

#[test]
fn matches_reader_refusals_and_imports_partial_and_zip64() {
    let mut call = session();
    let registry = call(json!({"op": "registry"}));
    let codes = &registry["errorCodes"];
    for name in ["encrypted-passphrase", "encrypted-raw-key"] {
        let archive_hex = fixture(name);
        let reason = read_archive(&unhex(&archive_hex).unwrap(), &Limits::default()).unwrap_err();
        assert_eq!(reason, ArchiveError::EncryptedUnsupported);
        let expected = json!({
            "refused": "encrypted_unsupported", "code": codes["encrypted_unsupported"],
            "reason": reason.class_name(),
        });
        assert_eq!(
            call(json!({"op": "inspect", "archiveHex": archive_hex})),
            expected
        );
        assert_eq!(call(readonly(&archive_hex, json!([]), json!([]))), expected);
    }

    let mut bytes = unhex(&fixture("plain")).unwrap();
    // Change the data in the first local ZIP entry and leave its CRC intact.
    let format_data = 30 + usize::from(u16::from_le_bytes([bytes[26], bytes[27]]));
    bytes[format_data] ^= 1;
    let reason = read_archive(&bytes, &Limits::default()).unwrap_err();
    assert_eq!(reason, ArchiveError::Zip(ZipError::Crc));
    let archive_hex = hex(&bytes);
    let expected = json!({
        "refused": "integrity_failure", "code": codes["integrity_failure"], "reason": reason.class_name(),
    });
    assert_eq!(
        call(json!({"op": "inspect", "archiveHex": archive_hex})),
        expected
    );
    assert_eq!(call(readonly(&archive_hex, json!([]), json!([]))), expected);

    for (name, missing) in [("partial-missing-blob", 1), ("plain-zip64-records", 0)] {
        let archive_hex = fixture(name);
        let inspected = call(json!({"op": "inspect", "archiveHex": archive_hex}));
        let view = call(readonly(&archive_hex, json!([]), json!([])));
        assert_eq!(inspected["manifest"]["blobs"], 1);
        assert_eq!(view["missingBlobs"].as_array().unwrap().len(), missing);
        assert_eq!(view["missingBlobs"], inspected["manifest"]["missingBlobs"]);
    }
}

#[test]
fn control_status_uses_each_calls_live_facts_and_never_applies_archive_state() {
    let mut call = session();
    let plain = fixture("plain");
    let bytes = unhex(&plain).unwrap();
    let archive = read_archive(&bytes, &Limits::default()).unwrap();
    let control = archive
        .entries
        .iter()
        .find(|entry| entry.declared_type.as_deref() == Some("test.room.control"))
        .unwrap();
    let digest = control.path.strip_prefix("canonical/objects/").unwrap();
    let verdict = json!({
        "objectType": "test.room.control", "sha256": digest,
        "verified": {"control": {"stream": "room1", "version": "18446744073709551614"}},
    });
    let before = readonly(
        &plain,
        json!([{"stream": "room1", "version": "18446744073709551615"}]),
        json!([verdict]),
    );
    let index = archive
        .entries
        .iter()
        .position(|entry| entry.path == control.path)
        .unwrap();
    let stale = call(before.clone());
    assert_eq!(
        stale["objects"][index]["status"],
        json!({
            "kind": "stale_control", "archived": "18446744073709551614", "live": "18446744073709551615",
        })
    );
    assert_eq!(stale.as_object().unwrap().len(), 4);

    let mut older_live = before.clone();
    older_live["liveControl"][0]["version"] = json!(1);
    let newer = call(older_live);
    assert_eq!(
        newer["objects"][index]["status"]["kind"],
        "control_needs_live_validation"
    );
    let mut mismatched_verdict = before.clone();
    mismatched_verdict["verdicts"][0]["objectType"] = json!("test.event");
    assert_eq!(
        call(mismatched_verdict)["objects"][index]["status"]["kind"],
        "unverified"
    );
    assert_eq!(call(before), stale);

    let mut rehydrate = json!({
        "op": "rehydrate_plan", "view": newer,
        "items": [
            {"id": "history", "viewOnce": false, "device": "revoked"},
            {"id": "gone", "viewOnce": false, "expiry": 7},
            {"id": "later", "viewOnce": false, "expiry": 9},
            {"id": "view-once", "viewOnce": true},
        ],
        "context": {
            "authorizedDevice": false, "accountMatches": false, "now": 7,
            "canEnforceExpiry": false, "liveControl": [], "revokedDevices": ["revoked"],
        },
    });
    assert_eq!(call(rehydrate.clone())["refused"], "unauthorized_device");
    rehydrate["context"]["authorizedDevice"] = json!(true);
    assert_eq!(call(rehydrate.clone())["refused"], "wrong_account");
    rehydrate["context"]["accountMatches"] = json!(true);
    let plan = call(rehydrate);
    assert_eq!(
        plan,
        json!({
            "import": ["history"], "scheduleDeletion": [], "droppedExpired": ["gone"],
            "refusedViewOnce": ["view-once"], "skippedUnenforceable": ["later"],
            "historyOnlyRevokedDevice": ["history"], "needsLiveValidation": [control.path],
            "staleControl": [], "freshSessionsRequired": true,
        })
    );
}

#[test]
fn archived_native_authorship_claim_is_quarantined() {
    let mut call = session();
    let output = call(json!({
        "op": "export",
        "candidates": [candidate(json!({
            "kind": "record", "recordType": "imported_history", "stem": "claim",
            // A single CBOR record with the forbidden field "signature".
            "bytesHex": "81a1697369676e617475726540",
        }))],
        "selection": {"includeBlobs": false},
        "meta": {
            "archiveId": "00".repeat(16), "exportScopeHex": "a0",
            "generatedAtHint": 0, "provenanceProfiles": [],
        },
    }));
    let view = call(readonly(
        output["archiveHex"].as_str().unwrap(),
        json!([]),
        json!([]),
    ));
    assert_eq!(
        view,
        json!({
            "objects": [], "records": [], "missingBlobs": [],
            "quarantined": [{"path": "records/claim.cbor", "index": 0, "reason": "native_authorship_claim"}],
        })
    );
}
