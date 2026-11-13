// SPDX-License-Identifier: MIT OR Apache-2.0
//! OM plumbing shim for `web-time` (ADR-0187, proposed; contains no cryptography). Replaces the upstream crate through
//! `[patch.crates-io]` so OpenMLS (feature `js`) gets `SystemTime` and `Instant` on `wasm32-unknown-unknown` without
//! wasm-bindgen: the clock is the host import `om_host.now_ms` that `om-wasm` already declares, so the module's
//! import list stays exactly `om_host.random_fill` and `om_host.now_ms`.
//!
//! * Everywhere except `wasm32-unknown-unknown` this crate re-exports `std::time`, as upstream does.
//! * On `wasm32-unknown-unknown`, `SystemTime` wraps `std::time::SystemTime` (whose arithmetic and serde form work
//!   on that target; only `now()` is unsupported) and replaces `now()` with the host clock. Because it serializes
//!   through the inner std value, it has the std form (`secs_since_epoch`, `nanos_since_epoch`) that OpenMLS persists
//!   in its state, so a snapshot written on native restores on wasm and the reverse.
//! * `Instant` there is built from the same host clock (millisecond resolution, not guaranteed monotonic).
//!   OpenMLS uses `Instant` in tests only.
//!
//! The wrapper is compiled on every target (module [`host_clock`]) so a native test pins its behaviour.

pub use std::time::Duration;
pub use std::time::SystemTimeError;

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
pub use std::time::{Instant, SystemTime, UNIX_EPOCH};

#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
pub use host_clock::{Instant, SystemTime, UNIX_EPOCH};

/// The clock source: the host import on wasm, `std` elsewhere (used only by the tests of [`host_clock`]).
#[cfg(all(target_arch = "wasm32", target_os = "unknown"))]
mod source {
    // SAFETY: the host module `om_host` provides `now_ms` with exactly this signature (the TypeScript loader refuses
    // to instantiate otherwise); it takes no pointer and has no side effect. `om-wasm` declares the identical import.
    #[link(wasm_import_module = "om_host")]
    unsafe extern "C" {
        #[link_name = "now_ms"]
        safe fn now_ms_raw() -> f64;
    }
    pub fn now_ms() -> f64 {
        now_ms_raw()
    }
}

#[cfg(not(all(target_arch = "wasm32", target_os = "unknown")))]
mod source {
    pub fn now_ms() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as f64)
            .unwrap_or(0.0)
    }
}

pub mod host_clock {
    use super::source::now_ms;
    use std::ops::{Add, AddAssign, Sub, SubAssign};
    use std::time::{Duration, SystemTimeError};

    /// `SystemTime` whose `now()` is the host clock. Never panics: a non-finite or negative host value reads as the epoch.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct SystemTime(std::time::SystemTime);

    pub const UNIX_EPOCH: SystemTime = SystemTime(std::time::UNIX_EPOCH);

    fn host_duration() -> Duration {
        let ms = now_ms();
        if ms.is_finite() && ms > 0.0 {
            Duration::from_millis(ms as u64)
        } else {
            Duration::ZERO
        }
    }

    impl SystemTime {
        pub const UNIX_EPOCH: SystemTime = UNIX_EPOCH;
        pub fn now() -> SystemTime {
            SystemTime(std::time::UNIX_EPOCH + host_duration())
        }
        pub fn duration_since(&self, earlier: SystemTime) -> Result<Duration, SystemTimeError> {
            self.0.duration_since(earlier.0)
        }
        pub fn elapsed(&self) -> Result<Duration, SystemTimeError> {
            SystemTime::now().duration_since(*self)
        }
        pub fn checked_add(&self, d: Duration) -> Option<SystemTime> {
            self.0.checked_add(d).map(SystemTime)
        }
        pub fn checked_sub(&self, d: Duration) -> Option<SystemTime> {
            self.0.checked_sub(d).map(SystemTime)
        }
    }
    impl Add<Duration> for SystemTime {
        type Output = SystemTime;
        fn add(self, d: Duration) -> SystemTime {
            SystemTime(self.0 + d)
        }
    }
    impl AddAssign<Duration> for SystemTime {
        fn add_assign(&mut self, d: Duration) {
            self.0 += d;
        }
    }
    impl Sub<Duration> for SystemTime {
        type Output = SystemTime;
        fn sub(self, d: Duration) -> SystemTime {
            SystemTime(self.0 - d)
        }
    }
    impl SubAssign<Duration> for SystemTime {
        fn sub_assign(&mut self, d: Duration) {
            self.0 -= d;
        }
    }
    impl From<std::time::SystemTime> for SystemTime {
        fn from(t: std::time::SystemTime) -> Self {
            SystemTime(t)
        }
    }

    #[cfg(feature = "serde")]
    impl serde::Serialize for SystemTime {
        fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0.serialize(s)
        }
    }
    #[cfg(feature = "serde")]
    impl<'de> serde::Deserialize<'de> for SystemTime {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            std::time::SystemTime::deserialize(d).map(SystemTime)
        }
    }

    /// Host-clock `Instant`: milliseconds since the host epoch as a `Duration`.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(Duration);

    impl Instant {
        pub fn now() -> Instant {
            Instant(host_duration())
        }
        pub fn duration_since(&self, earlier: Instant) -> Duration {
            self.0.saturating_sub(earlier.0)
        }
        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            self.duration_since(earlier)
        }
        pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
            self.0.checked_sub(earlier.0)
        }
        pub fn elapsed(&self) -> Duration {
            Instant::now().duration_since(*self)
        }
        pub fn checked_add(&self, d: Duration) -> Option<Instant> {
            self.0.checked_add(d).map(Instant)
        }
        pub fn checked_sub(&self, d: Duration) -> Option<Instant> {
            self.0.checked_sub(d).map(Instant)
        }
    }
    impl Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, d: Duration) -> Instant {
            Instant(self.0 + d)
        }
    }
    impl Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, d: Duration) -> Instant {
            Instant(self.0 - d)
        }
    }
    impl Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, o: Instant) -> Duration {
            self.duration_since(o)
        }
    }
}

#[cfg(all(test, feature = "serde"))]
mod tests {
    use super::host_clock::SystemTime;
    use std::time::{Duration, UNIX_EPOCH};

    #[test]
    fn serde_form_equals_std() {
        let std_t = UNIX_EPOCH + Duration::new(1_800_000_000, 123_000_000);
        let ours = SystemTime::from(std_t);
        assert_eq!(serde_json::to_string(&ours).unwrap(), serde_json::to_string(&std_t).unwrap());
        // a std-written value reads back as ours and the reverse
        let back: SystemTime = serde_json::from_str(&serde_json::to_string(&std_t).unwrap()).unwrap();
        assert_eq!(back, ours);
        let back_std: std::time::SystemTime = serde_json::from_str(&serde_json::to_string(&ours).unwrap()).unwrap();
        assert_eq!(back_std, std_t);
        assert!(serde_json::to_string(&ours).unwrap().contains("secs_since_epoch"));
    }

    #[test]
    fn now_is_after_2020_and_arithmetic_does_not_panic() {
        let n = SystemTime::now();
        assert!(n.duration_since(SystemTime::UNIX_EPOCH).unwrap() > Duration::from_secs(1_577_836_800));
        let later = n + Duration::from_secs(5);
        assert_eq!(later.duration_since(n).unwrap(), Duration::from_secs(5));
        assert!(n.duration_since(later).is_err());
    }
}
