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
    any(feature = "insecure-test-rng", feature = "test-loopback")
))]
compile_error!(
    "R-072: a `production` build must not include the insecure test RNG or the loopback test provider"
);

pub mod api;
pub mod control;
pub mod crypto;
pub mod engine;
pub mod events;
mod flows;
pub mod host;
pub mod state;

#[cfg(any(test, feature = "test-loopback"))]
pub mod runtime;

pub use engine::Core;
