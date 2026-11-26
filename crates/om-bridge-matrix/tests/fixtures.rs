// SPDX-License-Identifier: MPL-2.0
use om_bridge_matrix::*;
use om_core::bridge_fidelity::{self as fidelity, Carry, Op};
use serde_json::{Value, json};

const ROOM: &str = "!left:matrix.test";

#[test]
fn frozen_timeline_shapes() {
    let fixtures: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/matrix-events.json")).unwrap();
    let parsed: Vec<_> = fixtures
        .iter()
        .map(|raw| {
            observe(ROOM, &serde_json::to_vec(raw).unwrap())
                .unwrap()
                .unwrap()
        })
        .collect();
    assert_eq!(
        parsed.iter().map(|e| e.kind).collect::<Vec<_>>(),
        [
            EventKind::Message,
            EventKind::Edit,
            EventKind::Reaction,
            EventKind::Redaction,
            EventKind::Redaction,
            EventKind::Reply,
            EventKind::Thread,
            EventKind::Message,
            EventKind::Message,
        ]
    );
    assert_eq!(parsed[0].namespace, "matrix");
    assert_eq!(parsed[0].conversation_ref, ROOM);
    assert_eq!(parsed[0].actor_display, "@alice:matrix.test");
    assert_eq!(parsed[0].source_time_ms, Some(1234));
    assert_eq!(
        parsed[0].content,
        Content::Text {
            body: "Hello".into(),
            rich: true
        }
    );
    assert_eq!(
        parsed[1].content,
        Content::Text {
            body: "Updated".into(),
            rich: false
        }
    );
    assert!(
        parsed[1..7]
            .iter()
            .all(|event| event.target_ref.as_deref() == Some("$text"))
    );
    assert!(matches!(&parsed[7].content, Content::Attachment(a) if a.bytes == Some(17)));
    assert_eq!(
        parsed[8].interop_metadata.as_ref().unwrap().fidelity_flags,
        [fidelity::EXACT]
    );
}

#[test]
fn hostile_matrix_input_has_named_refusals() {
    let fixtures: Vec<Value> = serde_json::from_str(include_str!("fixtures/hostile.json")).unwrap();
    for test in fixtures {
        let error = observe(ROOM, test["raw"].as_str().unwrap().as_bytes()).unwrap_err();
        assert_eq!(format!("{error:?}"), test["error"].as_str().unwrap());
    }
    assert_eq!(
        observe(ROOM, &vec![b' '; MAX_EVENT_BYTES + 1]),
        Err(ParseError::ByteLimit)
    );
    let big_value =
        serde_json::to_vec(&json!({"x": "🦊".repeat(MAX_STRING_BYTES / 4 + 1)})).unwrap();
    assert_eq!(observe(ROOM, &big_value), Err(ParseError::StringLimit));
    let big_key = format!("{{\"{}\":true}}", "a".repeat(MAX_STRING_BYTES + 1));
    assert_eq!(
        observe(ROOM, big_key.as_bytes()),
        Err(ParseError::StringLimit)
    );
    assert_eq!(observe(ROOM, &[0xff]), Err(ParseError::InvalidJson));
}

#[test]
fn api_scope_and_cursor_are_explicit() {
    assert!(Endpoint::new("https://user@example.invalid").is_err());
    assert!(Endpoint::new("https://example.invalid/?query").is_err());
    let endpoint = Endpoint::new("https://matrix.test:8448/").unwrap();
    let request = endpoint.sync_request(Some("x/y?&")).unwrap();
    assert_eq!(
        request.url,
        "https://matrix.test:8448/_matrix/client/v3/sync?since=x%2Fy%3F%26"
    );
    assert_eq!(request.method, Method::Get);
    assert!(request.body.is_empty());
    assert_eq!(
        parse_event_response(429, b"{}"),
        Err(ApiError::HttpStatus(429))
    );
    assert_eq!(
        parse_event_response(200, br#"{"event_id":"wrong"}"#),
        Err(ApiError::Response(ParseError::InvalidResponse))
    );
    let batch = parse_sync_response(
        200,
        br#"{"next_batch":"next","rooms":{"join":{
        "!right:matrix.test":{"timeline":{"events":[{}]}},
        "!left:matrix.test":{"timeline":{"limited":true,"events":[{}]}}
    }}}"#,
        ROOM,
    )
    .unwrap();
    assert_eq!(batch.next_batch, "next");
    assert!(batch.limited);
    assert_eq!(batch.events, [Err(ParseError::InvalidEvent)]);
}

#[test]
fn capabilities_feed_core_fidelity() {
    let caps = capabilities();
    assert!(
        caps.rich_text_lossy && caps.deletes_are_requests && caps.polls_are_read_only_summaries
    );
    assert!(caps.carries_interop_metadata);
    assert_eq!(
        caps.support.translate(Op::Text { rich: true }).flags,
        [fidelity::LOSSY_FORMATTING]
    );
    let poll = caps.support.translate(Op::Poll {
        compatible_privacy: true,
    });
    assert_eq!(poll.carry, Carry::ReadOnlySummary);
    assert_eq!(poll.flags, [fidelity::LOSSY_FORMATTING]);
    assert!(!caps.support.translate(Op::Delete).claims_erasure);
}
