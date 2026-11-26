#![no_main]
use libfuzzer_sys::fuzz_target;
use om_bridge_matrix::{observe, parse_event_response, parse_sync_response};

fuzz_target!(|input: &[u8]| {
    // Foreign input is untrusted: none of these parsers may panic on any bytes.
    let _ = observe("!room:example.test", input);
    let _ = parse_event_response(200, input);
    let _ = parse_event_response(429, input);
    let _ = parse_sync_response(200, input, "!room:example.test");
});
