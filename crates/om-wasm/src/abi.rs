// SPDX-License-Identifier: MPL-2.0
//! The raw ABI and host imports. Every `unsafe` block in the crate is in this file and justified inline.

use crate::bridge::{Bridge, system_bridge};
use std::cell::RefCell;

#[cfg(target_arch = "wasm32")]
mod host {
    // SAFETY: the host module `om_host` provides these two functions with exactly these signatures (the TypeScript
    // loader refuses to instantiate otherwise); neither retains the pointer past the call.
    #[link(wasm_import_module = "om_host")]
    unsafe extern "C" {
        #[link_name = "random_fill"]
        fn random_fill_raw(ptr: *mut u8, len: usize);
        #[link_name = "now_ms"]
        safe fn now_ms_raw() -> f64;
    }
    pub fn now_ms() -> u64 {
        let t = now_ms_raw();
        if t.is_finite() && t > 0.0 {
            t as u64
        } else {
            0
        }
    }
    /// getrandom 0.3 "custom" backend (selected for this target in .cargo/config.toml).
    ///
    /// SAFETY: getrandom guarantees `dest..dest+len` is a valid, exclusively borrowed writable buffer; it is passed
    /// straight to the host's `crypto.getRandomValues`, which only writes within it.
    #[unsafe(no_mangle)]
    unsafe extern "Rust" fn __getrandom_v03_custom(
        dest: *mut u8,
        len: usize,
    ) -> Result<(), getrandom::Error> {
        unsafe { random_fill_raw(dest, len) };
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod host {
    pub fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static BRIDGE: RefCell<Option<Bridge<Rng>>> = const { RefCell::new(None) };
}

type Rng = om_core::crypto::rng::SystemRng;

/// Reserve an input buffer of `len` bytes and return its address for the host to fill (at most 16 MiB).
#[unsafe(no_mangle)]
pub extern "C" fn om_in_alloc(len: u32) -> *mut u8 {
    INPUT.with(|b| {
        let mut b = b.borrow_mut();
        let n = (len as usize).min(16 << 20);
        b.clear();
        b.resize(n, 0);
        b.as_mut_ptr()
    })
}

/// Address of the response of the last `om_call`.
#[unsafe(no_mangle)]
pub extern "C" fn om_out_ptr() -> *const u8 {
    OUTPUT.with(|b| b.borrow().as_ptr())
}

/// Run the command in the input buffer; returns the response length (the response is in the output buffer).
#[unsafe(no_mangle)]
pub extern "C" fn om_call(len: u32) -> u32 {
    let request: Vec<u8> = INPUT.with(|b| {
        let b = b.borrow();
        b[..(len as usize).min(b.len())].to_vec()
    });
    let response = BRIDGE.with(|slot| {
        let mut slot = slot.borrow_mut();
        let bridge = slot.get_or_insert_with(|| system_bridge(host::now_ms));
        bridge.handle(&request)
    });
    OUTPUT.with(|o| {
        let mut o = o.borrow_mut();
        *o = response;
        o.len() as u32
    })
}
