// SPDX-License-Identifier: MPL-2.0
//! RNG providers. Production code would only ever see `system_rng()`.
//!
//! The deterministic generator lives in `insecure_test_rng`, exists only with the
//! `insecure-test-rng` cargo feature, and cannot be compiled together with `production`
//! (a sketch of R-072 "production build provably excludes dev/test CryptoProvider").

pub use rand_core::{CryptoRng, RngCore};

#[cfg(all(feature = "production", feature = "insecure-test-rng"))]
compile_error!("R-072: the `production` build must not include the `insecure-test-rng` provider");

/// OS/host CSPRNG (getrandom). On wasm32-unknown-unknown the embedder supplies it.
pub type SystemRng = rand_core::UnwrapErr<rand_core::OsRng>;

pub fn system_rng() -> SystemRng {
    rand_core::UnwrapErr(rand_core::OsRng)
}

pub fn fill<R: RngCore + ?Sized>(rng: &mut R, n: usize) -> Vec<u8> {
    let mut v = vec![0u8; n];
    rng.fill_bytes(&mut v);
    v
}

pub fn arr32<R: RngCore + ?Sized>(rng: &mut R) -> [u8; 32] {
    let mut a = [0u8; 32];
    rng.fill_bytes(&mut a);
    a
}

pub fn arr64<R: RngCore + ?Sized>(rng: &mut R) -> [u8; 64] {
    let mut a = [0u8; 64];
    rng.fill_bytes(&mut a);
    a
}

#[cfg(feature = "insecure-test-rng")]
pub mod insecure_test_rng {
    //! INSECURE: seeded ChaCha20 for reproducible test vectors only.
    use rand_chacha::rand_core::SeedableRng;
    use rand_core::{CryptoRng, RngCore};

    pub struct InsecureTestRng(rand_chacha::ChaCha20Rng);

    impl InsecureTestRng {
        /// Seed = SHA-256("OM-V001-INSECURE-TEST-RNG" || label).
        pub fn from_label(label: &[u8]) -> Self {
            let seed = crate::crypto::sha256(&[b"OM-V001-INSECURE-TEST-RNG", label]);
            InsecureTestRng(rand_chacha::ChaCha20Rng::from_seed(seed))
        }
    }

    impl RngCore for InsecureTestRng {
        fn next_u32(&mut self) -> u32 {
            self.0.next_u32()
        }
        fn next_u64(&mut self) -> u64 {
            self.0.next_u64()
        }
        fn fill_bytes(&mut self, dst: &mut [u8]) {
            self.0.fill_bytes(dst)
        }
    }
    // Marked CryptoRng only so it can drive the same code paths; the name is the warning.
    impl CryptoRng for InsecureTestRng {}
}
