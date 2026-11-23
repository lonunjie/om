// SPDX-License-Identifier: MPL-2.0
//! `om-core`: the OM Core (core_kernel, ADR-0169).
//!
//! * [`crypto`]: OM-Crypto-v1 (PQXDH + Double Ratchet, dual Ed25519/ML-DSA-65 device binding, PQ/T HPKE).
//! * [`api`]: the typed intent/query/outcome/view boundary the UI sees.
//! * [`engine`]: the sans-IO state machine ([`Core`]). It owns no storage and no network: see [`host`].
//! * [`runtime`] (test builds): an in-memory loopback provider and a synchronous driver for tests.
#![forbid(unsafe_code)]

#[cfg(all(
    feature = "production",
    any(
        feature = "insecure-test-rng",
        feature = "test-loopback",
        feature = "fuzz-entry"
    )
))]
compile_error!(
    "R-072: a `production` build must not include the insecure test RNG, the loopback test provider or the fuzz entry points"
);

pub mod api;
pub mod attach;
pub mod backup;
pub mod bridge_control;
pub mod bridge_provenance;
pub mod calls;
pub mod control;
pub mod crypto;
pub mod direct;
pub mod engine;
pub mod events;
pub mod expressive;
mod flows;
pub mod groups;
pub mod host;
pub mod intro;
pub mod lan;
pub mod lan_route;
pub mod present;
pub mod publication;
pub mod recovery;
pub mod recovery_record;
pub mod recovery_sheet;
pub mod relayset;
pub mod room_moderation;
pub mod room_topics;
pub mod route;
pub mod service_authorship;
pub mod service_control;
pub mod service_invocation;
pub mod space_binding;
pub mod space_control;
pub mod state;

#[cfg(any(test, feature = "test-loopback"))]
pub mod runtime;

/// Byte-slice entry points over the owner-recovery codecs and the link-ceremony frames, for external fuzzers (V-012).
/// Compiled only with the `fuzz-entry` feature, which a `production` build refuses (R-072).
#[cfg(feature = "fuzz-entry")]
pub use flows::recovery_fuzz as recovery_fuzz_entry;

pub use engine::Core;
pub use flows::{
    CONFIRMATION_TTL_MS, FIRST_CONTACT_MAX_CHAIN, FIRST_CONTACT_PER_DAY, FIRST_CONTACT_PER_HOUR,
    HOUR_MS, MAX_NOTICE_BYTES, build_notice, request_key_digest,
};
