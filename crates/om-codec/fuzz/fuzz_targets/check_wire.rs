#![no_main]
use libfuzzer_sys::fuzz_target;
use om_codec::schema::Registry;
use std::sync::OnceLock;

/// The W4 object types added for services, bridges, plugins and Spaces, plus the envelope and event they ride in.
const TYPES: &[&str] = &[
    "ServicePrincipal",
    "ServiceControlState",
    "ServiceDefinition",
    "ServiceInstallation",
    "RepresentationGrant",
    "ContextDisclosureBundle",
    "BridgePrincipal",
    "BridgeControlState",
    "BridgeMapping",
    "InteropTrace",
    "ForeignProvenance",
    "ForeignIdentityLink",
    "BridgeImportedConversationEvent",
    "ConversationEvent",
    "SpaceControlState",
    "SpaceDelegatedIntent",
    "PublicationControlState",
    "PublicationEvent",
];

fn reg() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(Registry::load_w4)
}

fuzz_target!(|input: &[u8]| {
    // First byte picks the type; the decoder must never panic, and accepted bytes must be canonical.
    let Some((&sel, rest)) = input.split_first() else { return };
    let t = TYPES[sel as usize % TYPES.len()];
    if let Ok(v) = reg().check(t, rest, false) {
        assert_eq!(om_codec::cbor::encode(&v), rest, "accepted a non-canonical encoding for {t}");
    }
});
