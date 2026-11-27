// SPDX-License-Identifier: MPL-2.0
//! Two Matrix bridge devices and an OM Room model against a disposable Synapse homeserver.
//! The transport maps the adapter's HTTPS origin to plain HTTP on loopback. TLS is not exercised.
//!
//! Set OM_MATRIX_LOOPBACK to http://127.0.0.1:8008 and supply the three OM_MATRIX_PW_*
//! variables for alice, bridgea and bridgeb on om-test.invalid. The test creates two rooms
//! on that server. Run with --nocapture to see the trace table.

use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fmt::Write as _;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use om_bridge_matrix::*;
use om_codec::{Value as Cbor, cbor::encode};
use om_core::{
    bridge_control::{BridgeChain, BridgeState},
    bridge_fidelity::{
        self as fidelity, FOREIGN_ACTOR_UNVERIFIED as ACTOR, ORIGIN_UNVERIFIED as ORIGIN,
    },
    bridge_provenance::{ImportMapping, RoomProvenance, origin_fingerprint},
    bridge_room::{ADD_BRIDGE, BOTH, BridgeRoom, KIND_BRIDGE, Mapping},
    service_control::{ACTIVE, Endpoint as BridgeEndpoint},
};
use serde_json::{Value, json};

const ORIGIN_URL: &str = "https://om-test.invalid";
const LOOPBACK: &str = "http://127.0.0.1:8008";
const API: &str = "/_matrix/client/v3";
const LEFT: usize = 0;
const RIGHT: usize = 1;
const BRIDGES: [User; 2] = [User::BridgeA, User::BridgeB];
const OM_ROOM: [u8; 32] = [42; 32];
// Fixture value shared only by the two modeled OM endpoints.
const ROOM_KEY: [u8; 32] = [43; 32];
const MAX_HTTP_BODY: usize = 2 * MAX_SYNC_BYTES;

#[derive(Clone, Copy)]
enum User {
    Alice,
    BridgeA,
    BridgeB,
}

impl User {
    fn credentials(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Alice => ("alice", "@alice:om-test.invalid", "OM_MATRIX_PW_ALICE"),
            Self::BridgeA => (
                "bridgea",
                "@bridgea:om-test.invalid",
                "OM_MATRIX_PW_BRIDGEA",
            ),
            Self::BridgeB => (
                "bridgeb",
                "@bridgeb:om-test.invalid",
                "OM_MATRIX_PW_BRIDGEB",
            ),
        }
    }

    fn mxid(self) -> &'static str {
        self.credentials().1
    }
}

fn component(value: &str) -> String {
    let mut encoded = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            write!(encoded, "%{byte:02X}").unwrap();
        }
    }
    encoded
}

fn remaining(deadline: Instant) -> io::Result<Duration> {
    deadline
        .checked_duration_since(Instant::now())
        .filter(|left| !left.is_zero())
        .ok_or_else(|| io::Error::new(io::ErrorKind::TimedOut, "60 s test deadline"))
}

struct TimedSocket(TcpStream, Instant);

impl Read for TimedSocket {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.0.set_read_timeout(Some(remaining(self.1)?))?;
        self.0.read(buf)
    }
}

impl Write for TimedSocket {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.set_write_timeout(Some(remaining(self.1)?))?;
        self.0.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

struct Response {
    status: u16,
    body: Vec<u8>,
}

impl Response {
    fn json(self, operation: &str) -> Value {
        assert_eq!(self.status, 200, "{operation} HTTP status");
        serde_json::from_slice(&self.body)
            .unwrap_or_else(|_| panic!("{operation} returned invalid JSON"))
    }
}

fn http_line(reader: &mut impl BufRead) -> String {
    let mut line = String::new();
    reader
        .take(8192)
        .read_line(&mut line)
        .expect("HTTP read failed");
    line.strip_suffix("\r\n")
        .expect("incomplete HTTP line")
        .into()
}

fn read_chunked(reader: &mut impl BufRead) -> Vec<u8> {
    let mut body = Vec::new();
    loop {
        let line = http_line(reader);
        let size = usize::from_str_radix(line.split(';').next().unwrap().trim(), 16)
            .expect("invalid HTTP chunk length");
        if size == 0 {
            while !http_line(reader).is_empty() {}
            return body;
        }
        let offset = body.len();
        assert!(size <= MAX_HTTP_BODY - offset, "HTTP body too large");
        body.resize(offset + size, 0);
        reader
            .read_exact(&mut body[offset..])
            .expect("HTTP chunk read failed");
        assert!(
            http_line(reader).is_empty(),
            "missing HTTP chunk terminator"
        );
    }
}

struct Loopback {
    deadline: Instant,
}

impl Loopback {
    fn exchange(&self, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> Response {
        assert!(path.starts_with("/_matrix/") && path.bytes().all(|b| (33..=126).contains(&b)));
        let addr = SocketAddr::from(([127, 0, 0, 1], 8008));
        let stream = TcpStream::connect_timeout(&addr, remaining(self.deadline).unwrap())
            .expect("connect to loopback Synapse");
        let mut stream = TimedSocket(stream, self.deadline);
        let mut headers = Vec::new();
        write!(
            headers,
            "{method} {path} HTTP/1.1\r\nHost: om-test.invalid\r\nConnection: close\r\n"
        )
        .expect("HTTP write failed");
        if let Some(token) = token {
            write!(headers, "Authorization: Bearer {token}\r\n").expect("HTTP write failed");
        }
        if method != "GET" {
            write!(
                headers,
                "Content-Type: application/json\r\nContent-Length: {}\r\n",
                body.len()
            )
            .expect("HTTP write failed");
        }
        headers.extend_from_slice(b"\r\n");
        headers.extend_from_slice(body);
        stream.write_all(&headers).expect("HTTP write failed");

        let mut reader = BufReader::new(stream);
        let status: u16 = http_line(&mut reader)
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse().ok())
            .expect("invalid HTTP status");
        let mut length = None;
        let mut chunked = false;
        loop {
            let line = http_line(&mut reader);
            if line.is_empty() {
                break;
            }
            let (name, value) = line.split_once(':').expect("invalid HTTP header");
            if name.eq_ignore_ascii_case("Content-Length") {
                length = Some(value.trim().parse::<usize>().expect("invalid HTTP length"));
            } else if name.eq_ignore_ascii_case("Transfer-Encoding") {
                assert!(
                    value.trim().eq_ignore_ascii_case("chunked"),
                    "unsupported HTTP transfer encoding"
                );
                chunked = true;
            }
        }
        let body = if chunked {
            read_chunked(&mut reader)
        } else if let Some(length) = length {
            assert!(length <= MAX_HTTP_BODY, "HTTP body too large");
            let mut body = vec![0; length];
            reader.read_exact(&mut body).expect("HTTP body read failed");
            body
        } else {
            let mut body = Vec::new();
            reader
                .take((MAX_HTTP_BODY + 1) as u64)
                .read_to_end(&mut body)
                .expect("HTTP body read failed");
            assert!(body.len() <= MAX_HTTP_BODY, "HTTP body too large");
            body
        };
        Response { status, body }
    }

    fn request(&self, method: &str, path: &str, token: Option<&str>, body: &[u8]) -> Response {
        loop {
            let response = self.exchange(method, path, token, body);
            if response.status != 429 {
                return response;
            }
            // An explicit rate refusal is safe to retry with identical bytes.
            let delay = serde_json::from_slice::<Value>(&response.body)
                .ok()
                .and_then(|error| error["retry_after_ms"].as_u64())
                .unwrap_or(250);
            let retry_at = Instant::now() + Duration::from_millis(delay.clamp(50, 60_000));
            assert!(
                retry_at < self.deadline,
                "Synapse rate limit exceeds the test deadline"
            );
            while let Ok(left) = remaining(retry_at) {
                thread::sleep(left.min(Duration::from_millis(500)));
            }
        }
    }

    fn login(&self, user: User) -> String {
        let (name, mxid, variable) = user.credentials();
        let password = env::var(variable).unwrap_or_else(|_| panic!("set {variable}"));
        assert!(!password.is_empty(), "set {variable}");
        let body = serde_json::to_vec(&json!({
            "type": "m.login.password",
            "identifier": {"type": "m.id.user", "user": name},
            "password": password,
        }))
        .expect("login JSON encoding failed");
        let result = self
            .request("POST", &format!("{API}/login"), None, &body)
            .json("login");
        assert!(
            result["user_id"].as_str() == Some(mxid),
            "unexpected login identity for {name}"
        );
        let token = string_field(&result, "access_token");
        assert!(
            !token.is_empty() && token.bytes().all(|b| (33..=126).contains(&b)),
            "invalid access token format"
        );
        token.into()
    }
}

fn string_field<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key]
        .as_str()
        .unwrap_or_else(|| panic!("missing Matrix string field {key}"))
}

type History = BTreeMap<String, Value>;

struct Trace {
    scenario: &'static str,
    hops: String,
    flags: Vec<u8>,
    verdict: String,
}

struct Homeserver {
    client: Loopback,
    tokens: [String; 3],
    rooms: [String; 2],
    expected: [BTreeSet<String>; 2],
    txn: u64,
    trace: Vec<Trace>,
}

impl Homeserver {
    fn new(deadline: Instant) -> Self {
        let client = Loopback { deadline };
        let tokens = [User::Alice, User::BridgeA, User::BridgeB].map(|user| client.login(user));
        let mut server = Self {
            client,
            tokens,
            rooms: Default::default(),
            expected: Default::default(),
            txn: 0,
            trace: Vec::new(),
        };
        for scope in [LEFT, RIGHT] {
            let created = server.json(
                User::Alice,
                "POST",
                "/createRoom",
                Some(json!({
                    "preset": "private_chat", "visibility": "private",
                    "invite": BRIDGES.map(User::mxid),
                })),
            );
            let room = string_field(&created, "room_id");
            server.rooms[scope] = room.into();
            let room = component(room);
            for bridge in BRIDGES {
                let joined = server.json(
                    bridge,
                    "POST",
                    &format!("/rooms/{room}/join"),
                    Some(json!({})),
                );
                assert_eq!(joined["room_id"], server.rooms[scope]);
            }
            let state = server.json(User::Alice, "GET", &format!("/rooms/{room}/state"), None);
            let state = state.as_array().expect("room state array");
            assert!(
                !state
                    .iter()
                    .any(|event| event["type"] == "m.room.encryption")
            );
            assert!(
                state
                    .iter()
                    .any(|event| event["type"] == "m.room.join_rules"
                        && event["content"]["join_rule"] == "invite")
            );
        }
        assert_ne!(server.rooms[LEFT], server.rooms[RIGHT]);
        server
    }

    fn json(&self, user: User, method: &str, path: &str, body: Option<Value>) -> Value {
        let body = body
            .map(|value| serde_json::to_vec(&value).unwrap())
            .unwrap_or_default();
        self.client
            .request(
                method,
                &format!("{API}{path}"),
                Some(&self.tokens[user as usize]),
                &body,
            )
            .json(path)
    }

    fn adapter_request(&self, peer: usize, request: &Request) -> Response {
        let path = request
            .url
            .strip_prefix(ORIGIN_URL)
            .expect("unexpected adapter origin");
        let method = match request.method {
            Method::Get => "GET",
            Method::Put => "PUT",
        };
        self.client.request(
            method,
            path,
            Some(&self.tokens[BRIDGES[peer] as usize]),
            &request.body,
        )
    }

    fn remember(&mut self, scope: usize, event: &str) {
        assert!(
            self.expected[scope].insert(event.into()),
            "unexpected reused Matrix event ID"
        );
    }

    fn put(&mut self, user: User, scope: usize, route: &str, content: &Value) -> String {
        self.txn += 1;
        let room = component(&self.rooms[scope]);
        let path = format!("{API}/rooms/{room}/{route}/fixture-{}", self.txn);
        let body = serde_json::to_vec(content).unwrap();
        let response = self
            .client
            .request("PUT", &path, Some(&self.tokens[user as usize]), &body);
        let reference = parse_event_response(response.status, &response.body).unwrap();
        self.remember(scope, &reference);
        reference
    }

    fn send(&mut self, user: User, scope: usize, event_type: &str, content: Value) -> String {
        self.put(
            user,
            scope,
            &format!("send/{}", component(event_type)),
            &content,
        )
    }

    fn message(&mut self, scope: usize, body: &str) -> String {
        self.send(User::Alice, scope, "m.room.message", text_content(body))
    }

    fn history(&self) -> [History; 2] {
        [LEFT, RIGHT].map(|scope| {
            let room = component(&self.rooms[scope]);
            let path = format!("/rooms/{room}/messages?dir=b&limit=100");
            let page = self.json(User::Alice, "GET", &path, None);
            let events = page["chunk"].as_array().expect("Matrix messages chunk");
            assert!(
                events.len() < 100,
                "history exceeded the fresh-room fixture limit"
            );
            let mut history = BTreeMap::new();
            for event in events {
                assert_ne!(event["type"], "m.room.encrypted");
                if matches!(
                    event["type"].as_str(),
                    Some("m.room.message" | "m.reaction" | "m.room.redaction")
                ) {
                    let reference = string_field(event, "event_id");
                    assert!(history.insert(reference.into(), event.clone()).is_none());
                }
            }
            let found: BTreeSet<String> = history.keys().cloned().collect();
            assert_eq!(found, self.expected[scope], "room {scope} history");
            history
        })
    }

    fn record(
        &mut self,
        scenario: &'static str,
        before: Option<u64>,
        after: Option<u64>,
        flags: &[u8],
        verdict: String,
    ) {
        let display = |hop: Option<u64>| hop.map_or_else(|| "-".into(), |hop| hop.to_string());
        self.trace.push(Trace {
            scenario,
            hops: format!("{} -> {}", display(before), display(after)),
            flags: flags.to_vec(),
            verdict,
        });
    }

    fn print_trace(&self) {
        println!(
            "| scenario | hop counts (wire -> OM) | flags (numeric) | dedup verdict / evidence |"
        );
        println!("| --- | --- | --- | --- |");
        for row in &self.trace {
            println!(
                "| {} | {} | {:?} | {} |",
                row.scenario, row.hops, row.flags, row.verdict
            );
        }
    }
}

fn endpoint() -> Endpoint {
    Endpoint::new(ORIGIN_URL).unwrap()
}

fn event_id(n: u8) -> EventId {
    [n; 16]
}

fn text(body: &str, rich: bool) -> Rendering {
    Rendering::Message {
        message: Message::Text {
            body: body.into(),
            rich,
        },
        relation: None,
    }
}

fn native(n: u8, rendering: Rendering) -> ForeignRendering {
    ForeignRendering {
        event_id: event_id(n),
        epoch: 1,
        author: RenderingAuthor::Native,
        rendering,
    }
}

fn text_content(body: &str) -> Value {
    json!({"msgtype":"m.text", "body":body})
}

fn edit(target: &str) -> Value {
    json!({"msgtype":"m.text","body":"* Corrected",
        "m.new_content":{"msgtype":"m.text","body":"Corrected"},
        "m.relates_to":{"rel_type":"m.replace","event_id":target}})
}

fn reaction(target: &str) -> Value {
    json!({"m.relates_to":{"rel_type":"m.annotation","event_id":target,"key":"👍"}})
}

struct Peer {
    adapter: MatrixAdapter,
    binding: Binding,
    mapping: ImportMapping,
    chain: BridgeChain,
    cursor: Option<String>,
    path_preserves_metadata: bool,
}

impl Peer {
    fn admit(room: &mut BridgeRoom, id: u8, foreign: &str) -> Self {
        let bridge = [id; 32];
        let endpoint_id = vec![id; 16];
        let policy = Binding {
            incarnation: [id; 16],
            room: OM_ROOM,
            matrix_room: foreign.into(),
            bridge,
            endpoint_id: endpoint_id.clone(),
            admitted_epoch: 1,
        };
        let ep = BridgeEndpoint {
            id: endpoint_id.clone(),
            state: ACTIVE,
            profile: 1,
            key: vec![id; 40],
            mode: 0,
            host_ref: None,
        };
        let controller = (1, vec![99; 32]);
        let head = BridgeState {
            service_id: bridge,
            version: 1,
            prev: None,
            controllers: [controller.clone()].into(),
            threshold: 1,
            endpoints: [(endpoint_id, ep)].into(),
            hash: [id; 32],
        };
        let mut chain = BridgeChain::new(bridge);
        chain.accept(head.clone(), &[controller]).unwrap();
        room.admit(
            &caps(),
            &head,
            Mapping {
                incarnation: policy.incarnation,
                namespace: NAMESPACE.into(),
                foreign_scope: "scope".into(),
                allowed_classes: [1, 2, 3, 4, 8].into(),
                direction: BOTH,
                active: true,
            },
            &[],
            1,
        )
        .unwrap();
        Self {
            adapter: MatrixAdapter::new(endpoint(), policy.clone()).unwrap(),
            binding: policy,
            chain,
            cursor: None,
            path_preserves_metadata: true,
            mapping: ImportMapping {
                bridge,
                room: OM_ROOM.to_vec(),
                namespace: NAMESPACE.into(),
                foreign_scope: foreign.into(),
                direction: BOTH,
                active: true,
            },
        }
    }
}

fn caps() -> BTreeSet<String> {
    [ADD_BRIDGE.to_string()].into()
}

fn access<'a>(
    room: &'a BridgeRoom,
    mapping: &'a ImportMapping,
    epoch: u64,
    preserves: bool,
) -> RoomAccess<'a> {
    RoomAccess {
        room,
        mapping,
        epoch,
        now: 1,
        observed_at: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
        matrix_plaintext_allowed: true,
        edits_allowed: true,
        relay_bridged: false,
        path_preserves_metadata: preserves,
    }
}

fn map(fields: impl IntoIterator<Item = (u64, Cbor)>) -> Cbor {
    Cbor::Map(
        fields
            .into_iter()
            .map(|(k, v)| (Cbor::Uint(k), v))
            .collect(),
    )
}

fn rich(body: &str) -> Cbor {
    let inline = map([(0, Cbor::Uint(1)), (1, Cbor::Text(body.into()))]);
    let block = map([(0, Cbor::Uint(1)), (1, Cbor::Array(vec![inline]))]);
    map([(0, Cbor::Uint(1)), (1, Cbor::Array(vec![block]))])
}

fn message_body(body: &str, relation: Option<Relation<EventId>>) -> Cbor {
    let mut fields = vec![(0, rich(body)), (1, Cbor::Array(vec![]))];
    match relation {
        Some(Relation::Reply(id)) => fields.push((2, Cbor::Bytes(id.to_vec()))),
        Some(Relation::Thread(id)) => fields.push((5, Cbor::Bytes(id.to_vec()))),
        None => {}
    }
    map(fields)
}

fn event_body(binding: &Binding, id: EventId, import: &PreparedImport) -> Vec<u8> {
    let (kind, body) = match import.operation() {
        ImportedAction::Message {
            message: Message::Text { body, .. },
            relation,
        } => (1, message_body(body, *relation)),
        ImportedAction::Edit { target, body } => {
            (2, map([(0, Cbor::Bytes(target.to_vec())), (1, rich(body))]))
        }
        ImportedAction::Reaction { target, key } => (
            4,
            map([
                (0, Cbor::Bytes(target.to_vec())),
                (1, Cbor::Text(key.clone())),
                (2, Cbor::Bool(true)),
            ]),
        ),
        ImportedAction::UnavailableTarget { .. } => (1, message_body("Target unavailable", None)),
        ImportedAction::RedactionRequest { .. } => (1, message_body("Redaction requested", None)),
        ImportedAction::Message {
            message: Message::Attachment(_),
            ..
        } => panic!("this model has no media store"),
    };
    encode(&map([
        (0, Cbor::Uint(10)),
        (1, Cbor::Uint(1)),
        (2, Cbor::Uint(1)),
        (3, Cbor::Bytes(id.to_vec())),
        (4, Cbor::Bytes(OM_ROOM.to_vec())),
        (
            5,
            map([
                (0, Cbor::Uint(KIND_BRIDGE)),
                (1, Cbor::Bytes(import.author().to_vec())),
            ]),
        ),
        (6, Cbor::Bytes(binding.endpoint_id.clone())),
        (
            7,
            map([
                (0, Cbor::Uint(1000)),
                (1, Cbor::Uint(0)),
                (2, Cbor::Bytes(binding.endpoint_id.clone())),
            ]),
        ),
        (8, Cbor::Uint(kind)),
        (9, Cbor::Uint(1)),
        (11, body),
        (14, import.provenance().clone()),
    ]))
}

struct Materialized {
    id: EventId,
    author: BridgeId,
    actor: String,
    operation: ImportedAction,
    trace: InteropTrace,
    flags: Vec<u8>,
}

struct Harness<'a> {
    server: &'a mut Homeserver,
    scopes: [usize; 2],
    room: BridgeRoom,
    provenance: RoomProvenance,
    peers: [Peer; 2],
    imported: BTreeMap<String, Materialized>,
    drops: BTreeMap<(usize, String), ImportDrop>,
    epoch: u64,
}

impl<'a> Harness<'a> {
    fn new(server: &'a mut Homeserver, scopes: [usize; 2]) -> Self {
        let mut room = BridgeRoom::new(None, 20, 60);
        let peers = [
            Peer::admit(&mut room, 11, &server.rooms[scopes[0]]),
            Peer::admit(&mut room, 12, &server.rooms[scopes[1]]),
        ];
        let mut harness = Self {
            server,
            scopes,
            room,
            provenance: RoomProvenance::new(ROOM_KEY),
            peers,
            imported: BTreeMap::new(),
            drops: BTreeMap::new(),
            epoch: 1,
        };
        for i in [0, 1] {
            // Seed the cursor after joining. This binding imports only future timeline events.
            let _ = harness.sync_batch(i);
        }
        harness
    }

    fn prepare(&self, i: usize, source: &ForeignRendering) -> Result<PreparedEmission, ExportDrop> {
        let peer = &self.peers[i];
        let context = access(
            &self.room,
            &peer.mapping,
            self.epoch,
            peer.path_preserves_metadata,
        );
        peer.adapter.emit(source, &context, &ROOM_KEY)
    }

    fn request(&self, i: usize, prepared: &PreparedEmission) -> Response {
        let peer = &self.peers[i];
        let context = access(
            &self.room,
            &peer.mapping,
            self.epoch,
            peer.path_preserves_metadata,
        );
        let request = peer.adapter.request(prepared, &context).unwrap();
        self.server.adapter_request(i, request)
    }

    fn confirm(&mut self, i: usize, prepared: &PreparedEmission, response: Response) -> String {
        let reference = self.peers[i]
            .adapter
            .confirm_emit(prepared, response.status, &response.body)
            .unwrap();
        self.server.remember(self.scopes[i], &reference);
        reference
    }

    fn publish(&mut self, i: usize, prepared: &PreparedEmission) -> String {
        let response = self.request(i, prepared);
        self.confirm(i, prepared, response)
    }

    fn sync_batch(&mut self, i: usize) -> SyncBatch {
        let peer = &mut self.peers[i];
        let incremental = peer.cursor.is_some();
        // Matrix /sync defaults to timeout=0. Send the adapter's URL unchanged.
        let request = endpoint().sync_request(peer.cursor.as_deref()).unwrap();
        let response = self.server.adapter_request(i, &request);
        let batch = parse_sync_response(response.status, &response.body, &peer.binding.matrix_room)
            .unwrap();
        assert!(
            !incremental || !batch.limited,
            "limited incremental Matrix timeline for bridge {i}"
        );
        peer.cursor = Some(batch.next_batch.clone());
        batch
    }

    fn sync(&mut self, scenario: &'static str, expected: [&[&str]; 2]) {
        // Serialize Room materialization and commit before the second adapter consumes its batch.
        for (i, expected) in expected.into_iter().enumerate() {
            let mut pending: BTreeSet<_> = expected.iter().copied().collect();
            assert_eq!(pending.len(), expected.len());
            loop {
                assert!(
                    Instant::now() < self.server.client.deadline,
                    "{scenario}: timed out waiting for {} event(s) on bridge {i}",
                    pending.len()
                );
                for parsed in self.sync_batch(i).events {
                    let event =
                        parsed.unwrap_or_else(|error| panic!("{scenario}: bridge {i}: {error:?}"));
                    assert!(
                        pending.remove(event.event_ref.as_str()),
                        "{scenario}: unexpected sync event {} on bridge {i}: {:?}",
                        event.event_ref,
                        self.server.history()[self.scopes[i]].get(&event.event_ref)
                    );
                    self.consume(scenario, i, event);
                }
                if pending.is_empty() {
                    break;
                }
                thread::sleep(
                    remaining(self.server.client.deadline)
                        .unwrap()
                        .min(Duration::from_millis(50)),
                );
            }
        }
    }

    fn consume(&mut self, scenario: &'static str, i: usize, event: ForeignEvent) {
        let peer = &mut self.peers[i];
        let context = access(
            &self.room,
            &peer.mapping,
            self.epoch,
            peer.path_preserves_metadata,
        );
        let id = event_id(100 + u8::try_from(self.imported.len()).unwrap());
        let before = event
            .interop_metadata
            .as_ref()
            .map(|meta| meta.trace.hop_count);
        let candidate = match peer.adapter.prepare_import(
            &event,
            id,
            &context,
            &self.provenance,
            &ROOM_KEY,
            None,
        ) {
            Ok(candidate) => candidate,
            Err(drop) => {
                let flags = event
                    .interop_metadata
                    .as_ref()
                    .map_or(&[][..], |meta| meta.fidelity_flags.as_slice());
                self.server
                    .record(scenario, before, None, flags, format!("{drop:?}"));
                assert!(self.drops.insert((i, event.event_ref), drop).is_none());
                return;
            }
        };
        let body = event_body(&peer.binding, id, &candidate);
        let checked = peer
            .adapter
            .check_import(&candidate, &body, &context, &peer.chain, &self.provenance)
            .unwrap();
        assert_eq!(candidate.author(), peer.binding.bridge);
        let imported = Materialized {
            id,
            author: candidate.author(),
            actor: event.actor_ref,
            operation: candidate.operation().clone(),
            trace: candidate.trace().clone(),
            flags: candidate.flags().to_vec(),
        };
        let observed = &imported.trace;
        let fingerprint = observed.origin_fingerprint;
        let verified = !imported.flags.contains(&ORIGIN);
        let verdict = if verified {
            "materialized, verified"
        } else {
            "materialized, unverified"
        };
        let rendering = ForeignRendering {
            event_id: imported.id,
            epoch: self.epoch,
            author: RenderingAuthor::Bridged {
                bridge: imported.author,
                trace: observed.clone(),
                origin_verified: verified,
            },
            rendering: text("Room retained the import", false),
        };
        self.server.record(
            scenario,
            before,
            Some(observed.hop_count),
            &imported.flags,
            verdict.into(),
        );
        assert!(self.imported.insert(event.event_ref, imported).is_none());
        peer.adapter
            .commit_import(checked, &mut self.provenance)
            .unwrap();
        assert_eq!(self.provenance.is_materialized(&fingerprint), verified);
        for bridge in [0, 1] {
            let expected = if bridge == i {
                ExportDrop::OwnImport
            } else {
                ExportDrop::RelayDenied
            };
            assert!(
                matches!(self.prepare(bridge, &rendering), Err(drop) if drop == expected),
                "{scenario}: default export policy did not refuse bridge {bridge}"
            );
        }
    }

    fn assert_drop(&self, i: usize, event: &str, expected: ImportDrop) {
        assert_eq!(self.drops.get(&(i, event.into())), Some(&expected));
    }
}

fn metadata(body: &[u8]) -> InteropMetadata {
    let content: Value = serde_json::from_slice(body).unwrap();
    serde_json::from_value(content[INTEROP_FIELD].clone()).unwrap()
}

fn retry_round_trip(h: &mut Harness<'_>) {
    const SCENARIO: &str = "round trip and retry";
    let om = native(1, text("Hello", true));
    let a = h.prepare(0, &om).unwrap();
    let b = h.prepare(1, &om).unwrap();
    assert_eq!(a.flags(), &[fidelity::LOSSY_FORMATTING]);
    assert_eq!(b.flags(), a.flags());
    assert_ne!(a.transaction_id(), b.transaction_id());
    assert_eq!(
        a.transaction_id(),
        h.prepare(0, &om).unwrap().transaction_id()
    );

    let peer = &h.peers[0];
    let context = access(&h.room, &peer.mapping, h.epoch, true);
    let original_request = peer.adapter.request(&a, &context).unwrap();
    let expected_metadata = metadata(&original_request.body);
    let persisted = serde_json::to_vec(&a).unwrap();
    let retry: PreparedEmission = serde_json::from_slice(&persisted).unwrap();
    assert!(peer.adapter.request(&retry, &context).unwrap() == original_request);

    // Synapse accepts the first send. Withhold its acknowledgement from the adapter.
    let first = h.request(0, &a);
    let first_id = parse_event_response(first.status, &first.body).unwrap();
    assert_eq!(h.peers[0].adapter.records().count(), 0);
    let left = h.publish(0, &retry);
    assert_eq!(first_id, left);
    let right = h.publish(1, &b);
    assert_ne!(left, right);

    let history = h.server.history();
    for (scope, id, user) in [(LEFT, &left, User::BridgeA), (RIGHT, &right, User::BridgeB)] {
        let event = &history[scope][id];
        assert_eq!(event["sender"], user.mxid());
        assert_eq!(event["content"]["body"], "Hello");
        assert!(event["content"].get("formatted_body").is_none());
    }
    let observed: InteropMetadata =
        serde_json::from_value(history[LEFT][&left]["content"][INTEROP_FIELD].clone()).unwrap();
    assert_eq!(
        observed, expected_metadata,
        "stored InteropTrace field changed"
    );
    assert_eq!(observed.trace.hop_count, 1);
    assert_eq!(observed.trace.hop_limit, 3);
    assert_eq!(
        observed.trace.traversed_bridge_ids,
        [h.peers[0].binding.bridge]
    );
    h.server.record(
        SCENARIO,
        Some(observed.trace.hop_count),
        None,
        &observed.fidelity_flags,
        "send endpoint preserved InteropTrace intact".into(),
    );

    let peer = &mut h.peers[0];
    let records = serde_json::to_vec(&peer.adapter.records().cloned().collect::<Vec<_>>()).unwrap();
    let mut restored = MatrixAdapter::new(endpoint(), peer.binding.clone()).unwrap();
    restored
        .restore_records(&peer.binding, serde_json::from_slice(&records).unwrap())
        .unwrap();
    peer.adapter = restored;

    let reply = h.server.send(
        User::Alice,
        LEFT,
        "m.room.message",
        json!({
            "msgtype": "m.text", "body": "Reply",
            "m.relates_to": {"m.in_reply_to": {"event_id": left}},
        }),
    );
    h.sync(SCENARIO, [&[&left, &reply], &[&right]]);
    h.assert_drop(0, &left, ImportDrop::Echo);
    h.assert_drop(1, &right, ImportDrop::Echo);
    let got = &h.imported[&reply];
    assert_eq!(got.actor, User::Alice.mxid());
    assert_eq!(got.flags, [ACTOR]);
    assert!(matches!(got.operation, ImportedAction::Message {
        relation: Some(Relation::Reply(id)), ..
    } if id == event_id(1)));
    assert_eq!(
        h.server.history()[LEFT][&reply]["content"]["m.relates_to"]["m.in_reply_to"]["event_id"],
        left
    );
}

fn reflection_and_stripping(h: &mut Harness<'_>) {
    const REFLECTION: &str = "reflection with a fresh ID";
    const STRIPPED: &str = "metadata-stripping relay";
    let prepared = h
        .prepare(0, &native(2, text("Relay origin", false)))
        .unwrap();
    let source = h.publish(0, &prepared);
    let content = h.server.history()[LEFT][&source]["content"].clone();
    assert!(content.get(INTEROP_FIELD).is_some());

    // An authenticated Matrix user models a foreign relay by posting copied content.
    let copy = h
        .server
        .send(User::Alice, LEFT, "m.room.message", content.clone());
    assert_ne!(copy, source);
    h.sync(REFLECTION, [&[&source, &copy], &[]]);
    h.assert_drop(0, &source, ImportDrop::Echo);
    h.assert_drop(0, &copy, ImportDrop::Reflection);
    assert!(!h.imported.contains_key(&copy));
    assert_eq!(h.server.history()[LEFT][&copy]["content"], content);

    h.peers[1].path_preserves_metadata = false;
    let mut stripped = content;
    assert!(
        stripped
            .as_object_mut()
            .unwrap()
            .remove(INTEROP_FIELD)
            .is_some()
    );
    let copy = h
        .server
        .send(User::Alice, RIGHT, "m.room.message", stripped.clone());
    assert_ne!(copy, source);
    h.sync(STRIPPED, [&[], &[&copy]]);
    assert_eq!(h.server.history()[RIGHT][&copy]["content"], stripped);
    let got = &h.imported[&copy];
    assert_eq!(got.flags, [ACTOR, ORIGIN]);
    let observed = origin_fingerprint(&ROOM_KEY, NAMESPACE, &copy);
    assert_eq!(got.trace.origin_fingerprint, observed);
    assert_eq!(got.trace.traversed_bridge_ids, [h.peers[1].binding.bridge]);
    assert_eq!(got.trace.hop_count, 1);
    assert!(!h.provenance.is_materialized(&observed));
    h.peers[1].path_preserves_metadata = true;
}

fn dedup_on_recomputed_fingerprints(h: &mut Harness<'_>) {
    const SCENARIO: &str = "shared Matrix IDs";
    let source = h.server.message(LEFT, "Same text");
    let other = h.server.message(LEFT, "Same text");
    assert_ne!(source, other);
    h.sync(SCENARIO, [&[&source, &other], &[&source, &other]]);
    let history = h.server.history();
    assert_eq!(
        history[LEFT][&source]["content"],
        history[LEFT][&other]["content"]
    );
    assert_eq!(
        history[LEFT][&source]["sender"],
        history[LEFT][&other]["sender"]
    );
    assert_eq!(h.imported.len(), 2);
    for id in [&source, &other] {
        h.assert_drop(1, id, ImportDrop::Duplicate);
        let got = &h.imported[id];
        let recomputed = origin_fingerprint(&ROOM_KEY, NAMESPACE, id);
        assert_eq!(got.flags, [ACTOR]);
        assert_eq!(got.trace.origin_fingerprint, recomputed);
        assert!(h.provenance.is_materialized(&recomputed));
    }
    assert_ne!(
        h.imported[&source].trace.origin_fingerprint,
        h.imported[&other].trace.origin_fingerprint
    );
}

fn fresh_id_and_last_hop(h: &mut Harness<'_>) {
    const SCENARIO: &str = "R2 fresh ID and R1 terminal hop";
    let source = h.server.message(LEFT, "Foreign origin");
    h.sync(SCENARIO, [&[&source], &[]]);
    let original = h.imported[&source].trace.clone();
    assert!(h.provenance.is_materialized(&original.origin_fingerprint));

    let mut relay = text_content("Foreign origin");
    relay[INTEROP_FIELD] = serde_json::to_value(InteropMetadata {
        trace: original.clone(),
        fidelity_flags: vec![fidelity::EXACT],
    })
    .unwrap();
    let relay_id = h
        .server
        .send(User::Alice, RIGHT, "m.room.message", relay.clone());
    let mut exhausted = relay.clone();
    let mut spent = original.clone();
    spent.traversed_bridge_ids.push([77; 32]);
    spent.hop_count = 2;
    exhausted[INTEROP_FIELD]["trace"] = serde_json::to_value(&spent).unwrap();
    assert_eq!(spent.hop_count + 1, spent.hop_limit);
    let exhausted_id = h
        .server
        .send(User::Alice, RIGHT, "m.room.message", exhausted.clone());

    let history = h.server.history();
    assert_eq!(
        history[LEFT][&source]["content"],
        text_content("Foreign origin")
    );
    assert_eq!(history[RIGHT][&relay_id]["content"], relay);
    assert_eq!(history[RIGHT][&exhausted_id]["content"], exhausted);
    assert_ne!(relay_id, source);
    assert_ne!(exhausted_id, relay_id);

    h.sync(SCENARIO, [&[], &[&relay_id, &exhausted_id]]);
    let got = &h.imported[&relay_id];
    assert_eq!(got.flags, [ACTOR, ORIGIN]);
    assert_eq!(got.trace.hop_count, 2);
    assert_eq!(
        got.trace.traversed_bridge_ids,
        [h.peers[0].binding.bridge, h.peers[1].binding.bridge]
    );
    assert_ne!(got.trace.origin_fingerprint, original.origin_fingerprint);
    assert_eq!(
        got.trace.origin_fingerprint,
        origin_fingerprint(&ROOM_KEY, NAMESPACE, &relay_id)
    );
    assert!(!h.provenance.is_materialized(&got.trace.origin_fingerprint));
    assert!(h.provenance.is_materialized(&original.origin_fingerprint));
    h.assert_drop(1, &exhausted_id, ImportDrop::HopLimit);
    assert!(!h.imported.contains_key(&exhausted_id));
}

fn assert_relation(event: &Value, event_type: &str, rel_type: &str, target: &str) {
    assert_eq!(event["type"], event_type);
    assert_eq!(event["content"]["m.relates_to"]["rel_type"], rel_type);
    assert_eq!(event["content"]["m.relates_to"]["event_id"], target);
}

fn assert_redaction(event: &Value, target: &str) {
    assert_eq!(event["type"], "m.room.redaction");
    // The target is top-level in older room versions and inside content in newer ones.
    let modern = event["content"].get("redacts");
    let legacy = event.get("redacts");
    assert_eq!(modern.or(legacy).and_then(Value::as_str), Some(target));
    if let (Some(modern), Some(legacy)) = (modern, legacy) {
        assert_eq!(modern, legacy);
    }
}

fn exact_targets(h: &mut Harness<'_>, unmapped: &str) {
    const SCENARIO: &str = "edit, reaction, thread and redaction targets";
    let source = h.server.message(LEFT, "Original");
    let edit_id = h
        .server
        .send(User::Alice, LEFT, "m.room.message", edit(&source));
    let reaction_id = h
        .server
        .send(User::Alice, LEFT, "m.reaction", reaction(&source));
    let missing_edit = h
        .server
        .send(User::Alice, LEFT, "m.room.message", edit(unmapped));
    let missing_reaction = h
        .server
        .send(User::Alice, LEFT, "m.reaction", reaction(unmapped));
    let wrong_actor = h
        .server
        .send(User::BridgeB, LEFT, "m.room.message", edit(&source));
    h.sync(
        SCENARIO,
        [
            &[
                &source,
                &edit_id,
                &reaction_id,
                &missing_edit,
                &missing_reaction,
                &wrong_actor,
            ],
            &[],
        ],
    );

    let history = h.server.history();
    assert_eq!(history[LEFT][&source]["content"]["body"], "Original");
    assert_eq!(
        history[LEFT][unmapped]["content"]["body"],
        "Before the sync cursor"
    );
    for id in [&edit_id, &wrong_actor] {
        assert_relation(&history[LEFT][id], "m.room.message", "m.replace", &source);
    }
    assert_eq!(history[LEFT][&wrong_actor]["sender"], User::BridgeB.mxid());
    assert_relation(
        &history[LEFT][&reaction_id],
        "m.reaction",
        "m.annotation",
        &source,
    );
    assert_relation(
        &history[LEFT][&missing_edit],
        "m.room.message",
        "m.replace",
        unmapped,
    );
    assert_relation(
        &history[LEFT][&missing_reaction],
        "m.reaction",
        "m.annotation",
        unmapped,
    );
    assert!(!h.imported.contains_key(unmapped));

    let target = h.imported[&source].id;
    let edited = &h.imported[&edit_id];
    assert_eq!(
        edited.operation,
        ImportedAction::Edit {
            target,
            body: "Corrected".into()
        }
    );
    assert_eq!(edited.flags, [ACTOR]);
    let reacted = &h.imported[&reaction_id];
    assert_eq!(
        reacted.operation,
        ImportedAction::Reaction {
            target,
            key: "👍".into()
        }
    );
    assert_eq!(reacted.flags, [ACTOR]);
    let fallback = &h.imported[&missing_edit];
    assert_eq!(
        fallback.flags,
        [
            fidelity::REPLY_TARGET_UNAVAILABLE,
            fidelity::EDIT_AS_NEW_MESSAGE,
            ACTOR
        ]
    );
    let corrected = ImportedAction::Message {
        message: Message::Text {
            body: "Edited version: Corrected".into(),
            rich: false,
        },
        relation: None,
    };
    assert_eq!(fallback.operation, corrected);
    assert_eq!(h.imported[&wrong_actor].actor, User::BridgeB.mxid());
    assert_eq!(
        h.imported[&wrong_actor].flags,
        [fidelity::EDIT_AS_NEW_MESSAGE, ACTOR]
    );
    assert_eq!(h.imported[&wrong_actor].operation, corrected);
    assert_eq!(
        h.imported[&missing_reaction].flags,
        [fidelity::REPLY_TARGET_UNAVAILABLE, ACTOR]
    );
    assert_eq!(
        h.imported[&missing_reaction].operation,
        ImportedAction::UnavailableTarget {
            kind: EventKind::Reaction
        }
    );

    let base_id = event_id(3);
    let base = h.prepare(1, &native(3, text("OM source", false))).unwrap();
    let base = h.publish(1, &base);
    let actions = [
        (
            Rendering::Edit {
                target: base_id,
                body: "Updated".into(),
                rich: false,
            },
            "m.room.message",
            "m.replace",
        ),
        (
            Rendering::Reaction {
                target: base_id,
                key: "👍".into(),
            },
            "m.reaction",
            "m.annotation",
        ),
        (
            Rendering::Message {
                message: Message::Text {
                    body: "Thread".into(),
                    rich: false,
                },
                relation: Some(Relation::Thread(base_id)),
            },
            "m.room.message",
            "m.thread",
        ),
    ];
    let mut echoes = vec![base.clone()];
    for (index, (rendering, event_type, relation)) in actions.into_iter().enumerate() {
        let prepared = h
            .prepare(1, &native(4 + u8::try_from(index).unwrap(), rendering))
            .unwrap();
        assert_eq!(prepared.flags(), &[fidelity::EXACT]);
        let id = h.publish(1, &prepared);
        assert_relation(&h.server.history()[RIGHT][&id], event_type, relation, &base);
        echoes.push(id);
    }
    let echo_ids: Vec<_> = echoes.iter().map(String::as_str).collect();
    h.sync(SCENARIO, [&[], &echo_ids]);
    for id in &echoes {
        h.assert_drop(1, id, ImportDrop::Echo);
    }

    for missing in [
        Rendering::Edit {
            target: event_id(99),
            body: "Missing".into(),
            rich: false,
        },
        Rendering::Reaction {
            target: event_id(99),
            key: "👍".into(),
        },
        Rendering::Redact {
            target: event_id(99),
            reason: None,
        },
    ] {
        assert!(
            matches!(h.prepare(1, &native(7, missing)), Err(ExportDrop::Fidelity(flags))
            if flags == [fidelity::REPLY_TARGET_UNAVAILABLE])
        );
    }
    assert_eq!(
        h.server.history()[RIGHT][&base]["content"]["body"],
        "OM source"
    );

    let redact = h
        .prepare(
            1,
            &native(
                7,
                Rendering::Redact {
                    target: base_id,
                    reason: None,
                },
            ),
        )
        .unwrap();
    assert_eq!(redact.flags(), &[fidelity::EXACT]);
    let peer = &h.peers[1];
    let context = access(&h.room, &peer.mapping, h.epoch, true);
    let request = peer.adapter.request(&redact, &context).unwrap();
    assert!(request.url.contains("/redact/"));
    let sent = metadata(&request.body);
    let redaction = h.publish(1, &redact);
    let requested = h.server.put(
        User::Alice,
        LEFT,
        &format!("redact/{}", component(&source)),
        &json!({"reason":"Requested"}),
    );
    let history = h.server.history();
    assert_redaction(&history[RIGHT][&redaction], &base);
    assert_redaction(&history[LEFT][&requested], &source);
    assert_eq!(history[LEFT][&requested]["content"]["reason"], "Requested");
    assert!(history[RIGHT][&base]["content"].get("body").is_none());
    assert!(history[LEFT][&source]["content"].get("body").is_none());
    assert_eq!(
        history[LEFT][unmapped]["content"]["body"],
        "Before the sync cursor"
    );

    let evidence = match history[RIGHT][&redaction]["content"].get(INTEROP_FIELD) {
        Some(value) => {
            assert_eq!(
                value,
                &serde_json::to_value(&sent).unwrap(),
                "stored redaction InteropTrace changed"
            );
            "redaction endpoint preserved InteropTrace intact"
        }
        None => "redaction endpoint stripped InteropTrace",
    };
    h.server.record(
        SCENARIO,
        Some(sent.trace.hop_count),
        None,
        &sent.fidelity_flags,
        evidence.into(),
    );
    h.sync(SCENARIO, [&[&requested], &[&redaction]]);
    h.assert_drop(1, &redaction, ImportDrop::Echo);
    assert_eq!(h.imported[&requested].flags, [ACTOR]);
    assert_eq!(
        h.imported[&requested].operation,
        ImportedAction::RedactionRequest {
            target,
            reason: Some("Requested".into())
        }
    );
}

fn opt_in_relay(h: &mut Harness<'_>) {
    const SCENARIO: &str = "opt-in relay preserves lineage";
    let source = h.server.message(LEFT, "Opt-in relay");
    h.sync(SCENARIO, [&[&source], &[]]);
    let original = &h.imported[&source];
    let input = ForeignRendering {
        event_id: original.id,
        epoch: h.epoch,
        author: RenderingAuthor::Bridged {
            bridge: original.author,
            trace: original.trace.clone(),
            origin_verified: true,
        },
        rendering: text("Opt-in relay", false),
    };
    let peer = &h.peers[1];
    let mut context = access(&h.room, &peer.mapping, h.epoch, false);
    context.relay_bridged = true;
    assert!(matches!(
        peer.adapter.emit(&input, &context, &ROOM_KEY),
        Err(ExportDrop::RelayDenied)
    ));
    context.path_preserves_metadata = true;
    let approved = peer.adapter.emit(&input, &context, &ROOM_KEY).unwrap();
    assert_eq!(approved.flags(), &[fidelity::EXACT]);
    let request = peer.adapter.request(&approved, &context).unwrap();
    let response = h.server.adapter_request(1, request);
    let from_trace = original.trace.clone();
    let from_author = original.author;
    let relayed = h.confirm(1, &approved, response);

    let history = h.server.history();
    assert_eq!(history[LEFT][&source]["content"]["body"], "Opt-in relay");
    let content = &history[RIGHT][&relayed]["content"];
    assert_eq!(content["body"], "Opt-in relay");
    let meta: InteropMetadata = serde_json::from_value(content[INTEROP_FIELD].clone()).unwrap();
    assert_eq!(meta.trace.origin_fingerprint, from_trace.origin_fingerprint);
    assert_eq!(
        meta.trace.traversed_bridge_ids,
        [from_author, h.peers[1].binding.bridge]
    );
    assert_eq!(meta.trace.hop_count, 2);
    assert_eq!(meta.trace.hop_limit, 3);
    assert_eq!(meta.fidelity_flags, [fidelity::EXACT]);

    let reflected = h
        .server
        .send(User::Alice, LEFT, "m.room.message", content.clone());
    h.sync(SCENARIO, [&[&reflected], &[&relayed]]);
    h.assert_drop(0, &reflected, ImportDrop::Reflection);
    h.assert_drop(1, &relayed, ImportDrop::Echo);
    assert_eq!(h.server.history()[LEFT][&reflected]["content"], *content);
}

fn inert_content(h: &mut Harness<'_>) {
    const SCENARIO: &str = "inert file and poll summary";
    let file = h
        .prepare(
            0,
            &native(
                8,
                Rendering::Message {
                    message: Message::Attachment(Attachment {
                        kind: AttachmentKind::File,
                        name: "notes.txt".into(),
                        caption: Some("Meeting notes".into()),
                        mxc: "mxc://om-test.invalid/opaque".into(),
                        bytes: Some(23),
                    }),
                    relation: None,
                },
            ),
        )
        .unwrap();
    assert_eq!(file.flags(), &[fidelity::EXACT]);
    let file = h.publish(0, &file);
    let poll = h
        .prepare(
            0,
            &native(
                9,
                Rendering::PollSummary {
                    body: "Choose a date".into(),
                },
            ),
        )
        .unwrap();
    assert_eq!(poll.flags(), &[fidelity::LOSSY_FORMATTING]);
    let poll = h.publish(0, &poll);

    let history = h.server.history();
    let content = &history[LEFT][&file]["content"];
    assert_eq!(content["msgtype"], "m.file");
    assert_eq!(content["filename"], "notes.txt");
    assert_eq!(content["body"], "Meeting notes");
    assert_eq!(content["url"], "mxc://om-test.invalid/opaque");
    assert_eq!(history[LEFT][&poll]["type"], "m.room.message");
    assert_eq!(
        history[LEFT][&poll]["content"]["body"],
        "Poll summary (read-only): Choose a date"
    );
    h.sync(SCENARIO, [&[&file, &poll], &[]]);
    for id in [&file, &poll] {
        h.assert_drop(0, id, ImportDrop::Echo);
    }
}

fn removal(h: &mut Harness<'_>) {
    const SCENARIO: &str = "removal and pending retry";
    let prepared = h
        .prepare(1, &native(10, text("Prepared prior to removal", false)))
        .unwrap();
    let pending: PreparedEmission =
        serde_json::from_slice(&serde_json::to_vec(&prepared).unwrap()).unwrap();
    let historical = ForeignRendering {
        epoch: 0,
        ..native(11, text("Before admission", false))
    };
    assert!(matches!(
        h.prepare(1, &historical),
        Err(ExportDrop::OutsideMembership)
    ));

    let in_flight = h
        .prepare(1, &native(13, text("Accepted before removal", false)))
        .unwrap();
    let unconfirmed = h.request(1, &in_flight);
    let accepted = parse_event_response(unconfirmed.status, &unconfirmed.body).unwrap();
    h.server.remember(RIGHT, &accepted);
    assert_eq!(
        h.server.history()[RIGHT][&accepted]["content"]["body"],
        "Accepted before removal"
    );
    let peer = &h.peers[1];
    let context = access(&h.room, &peer.mapping, h.epoch, true);
    assert!(peer.adapter.request(&pending, &context).is_ok());
    h.room.remove(&caps(), &peer.binding.bridge).unwrap();
    h.epoch += 1;
    let context = access(&h.room, &peer.mapping, h.epoch, true);
    assert!(matches!(
        peer.adapter.request(&pending, &context),
        Err(ExportDrop::Inactive)
    ));
    assert!(matches!(
        peer.adapter.request(&in_flight, &context),
        Err(ExportDrop::Inactive)
    ));
    let future = ForeignRendering {
        epoch: h.epoch,
        ..native(12, text("After removal", false))
    };
    assert!(matches!(h.prepare(1, &future), Err(ExportDrop::Inactive)));
    h.server.record(
        SCENARIO,
        None,
        None,
        pending.flags(),
        "retry and future export refused: Inactive".into(),
    );

    let inbound = h.server.message(RIGHT, "Arrives after removal");
    h.sync(SCENARIO, [&[], &[&accepted, &inbound]]);
    h.assert_drop(1, &accepted, ImportDrop::Inactive);
    h.assert_drop(1, &inbound, ImportDrop::Inactive);
    assert!(!h.imported.contains_key(&inbound));
    let history = h.server.history();
    assert_eq!(
        history[RIGHT][&accepted]["content"]["body"],
        "Accepted before removal"
    );
    assert_eq!(
        history[RIGHT][&inbound]["content"]["body"],
        "Arrives after removal"
    );
    assert!(history[RIGHT].values().all(|event| {
        !matches!(
            event["content"]["body"].as_str(),
            Some("Prepared prior to removal" | "Before admission" | "After removal")
        )
    }));
}

#[test]
fn real_homeserver_round_trip() {
    let loopback = match env::var("OM_MATRIX_LOOPBACK") {
        Ok(value) => value,
        Err(env::VarError::NotPresent) => {
            println!("skipped: OM_MATRIX_LOOPBACK not set");
            return;
        }
        Err(env::VarError::NotUnicode(_)) => panic!("invalid OM_MATRIX_LOOPBACK"),
    };
    assert!(loopback == LOOPBACK, "set OM_MATRIX_LOOPBACK to {LOOPBACK}");
    let mut server = Homeserver::new(Instant::now() + Duration::from_secs(60));
    {
        let mut same_scope = Harness::new(&mut server, [LEFT, LEFT]);
        dedup_on_recomputed_fingerprints(&mut same_scope);
    }

    // Synapse may reject relations to nonexistent events. This real predecessor is outside
    // the next binding's import cursor, so it exists in Matrix but has no exact OM target.
    let unmapped = server.message(LEFT, "Before the sync cursor");
    assert_eq!(
        server.history()[LEFT][&unmapped]["content"]["body"],
        "Before the sync cursor"
    );
    let mut two_scopes = Harness::new(&mut server, [LEFT, RIGHT]);
    retry_round_trip(&mut two_scopes);
    reflection_and_stripping(&mut two_scopes);
    fresh_id_and_last_hop(&mut two_scopes);
    exact_targets(&mut two_scopes, &unmapped);
    opt_in_relay(&mut two_scopes);
    inert_content(&mut two_scopes);
    removal(&mut two_scopes);
    assert!(
        Instant::now() < server.client.deadline,
        "60 s test deadline"
    );
    server.print_trace();
}
