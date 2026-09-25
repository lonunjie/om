// SPDX-License-Identifier: Apache-2.0
//! wasm32-unknown-unknown entry points for a JS host (Node/V8; same engine class as a PWA).
//! Feasibility prototype for V-001. Not product code. Not for production.
//!
//! Host imports (module "om_host"): `random_fill(ptr, len)` backed by crypto.getRandomValues,
//! `now_ns() -> f64` backed by performance.now(). No wasm-bindgen: a raw, auditable ABI.

use om_v001_proto::rng::insecure_test_rng::InsecureTestRng;
use serde_json::json;

#[link(wasm_import_module = "om_host")]
extern "C" {
    fn random_fill(ptr: *mut u8, len: usize);
    fn now_ns() -> f64;
}

/// getrandom 0.3 "custom" backend (selected by .cargo/config.toml for this target).
#[no_mangle]
unsafe extern "Rust" fn __getrandom_v03_custom(dest: *mut u8, len: usize) -> Result<(), getrandom::Error> {
    random_fill(dest, len);
    Ok(())
}

static mut OUT: Vec<u8> = Vec::new();
static mut IN: Vec<u8> = Vec::new();

fn set_out(b: Vec<u8>) -> u32 {
    unsafe {
        OUT = b;
        let o = &*core::ptr::addr_of!(OUT);
        o.len() as u32
    }
}

#[no_mangle]
pub extern "C" fn om_out_ptr() -> *const u8 {
    unsafe { (*core::ptr::addr_of!(OUT)).as_ptr() }
}

/// Reserve an input buffer of `len` bytes and return its address for the host to fill.
#[no_mangle]
pub extern "C" fn om_in_alloc(len: u32) -> *mut u8 {
    unsafe {
        IN = vec![0u8; len as usize];
        (*core::ptr::addr_of_mut!(IN)).as_mut_ptr()
    }
}

/// cmd 1 = selftest JSON, 2 = bench(a, b) JSON, 3 = vectors JSON,
/// 4 = persist step `a` on the input buffer (world bytes; ignored for step 0) -> new world bytes,
/// 5 = system-RNG smoke test (host entropy reaches the Rust CSPRNG interface),
/// 6 = number of persist steps, 7 = draft-ietf-hpke-pq-05 KATs over the input buffer (vector JSON).
/// Returns output length; 0xFFFF_FFFF on error (message in output buffer).
#[no_mangle]
pub extern "C" fn om_run(cmd: u32, a: u32, b: u32) -> u32 {
    let out = match cmd {
        1 => {
            let mut rng = InsecureTestRng::from_label(b"selftest");
            let checks = om_v001_proto::otk::run(&mut rng);
            let ok = checks.iter().all(|c| c.ok);
            let arr: Vec<_> = checks.iter().map(|c| json!({"name": c.name, "ok": c.ok, "detail": c.detail})).collect();
            Ok(serde_json::to_vec_pretty(&json!({"checks": arr, "all_ok": ok})).unwrap())
        }
        2 => {
            let mut rng = InsecureTestRng::from_label(b"bench");
            let now = || unsafe { (now_ns()) as u64 };
            Ok(serde_json::to_vec_pretty(&om_v001_proto::bench::run(&mut rng, &now, a as usize, b as usize)).unwrap())
        }
        3 => Ok(serde_json::to_vec_pretty(&om_v001_proto::vectors::generate()).unwrap()),
        4 => {
            let input = unsafe { (*core::ptr::addr_of!(IN)).clone() };
            om_v001_proto::persist::step(a as usize, if a == 0 { None } else { Some(&input) }).map_err(|e| e.into_bytes())
        }
        5 => {
            use om_v001_proto::rng::RngCore;
            let mut r = om_v001_proto::rng::system_rng();
            let (x, y) = (r.next_u64(), r.next_u64());
            Ok(format!("{{\"distinct\": {}}}", x != y).into_bytes())
        }
        6 => Ok(om_v001_proto::persist::n_steps().to_string().into_bytes()),
        7 => {
            let input = unsafe { (*core::ptr::addr_of!(IN)).clone() };
            let r = om_v001_proto::hpke::check_kats(&String::from_utf8_lossy(&input));
            Ok(serde_json::to_vec_pretty(&r).unwrap())
        }
        _ => Err(b"unknown command".to_vec()),
    };
    match out {
        Ok(v) => set_out(v),
        Err(e) => {
            set_out(e);
            u32::MAX
        }
    }
}
