#![no_main]
use libfuzzer_sys::fuzz_target;
use om_codec::schema::Registry;
use std::sync::OnceLock;

fn reg() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::load_w1)
}

fuzz_target!(|input: &[u8]| {
    // The LAN frame check must never panic, and an accepted frame must be the canonical encoding of what it decoded to.
    if let Ok(v) = reg().check_lan_frame(input, 131_068) {
        assert_eq!(om_codec::cbor::encode(&v), input, "accepted a non-canonical LAN frame");
    }
});
