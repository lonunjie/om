// SPDX-License-Identifier: MPL-2.0
//! Link-and-run proof for the OpenMLS engine inside the wasm module (ADR-0187, a proposed delegated draft). Compiled
//! only with `groups-openmls`. Not part of the product ABI: one export, `om_groups_selftest`, returns 0 when two devices
//! can create a group, add one by Welcome, exchange an application message, round-trip a snapshot and remove the
//! other, drawing entropy and time only through the host imports. Delete this file to revert.

use om_core::crypto::rng::RngCore;
use om_core::groups::mls::{CryptoEnv, GroupCrypto, GroupCryptoLoader};
use om_core::groups::openmls::{OpenMlsGroupCrypto, OpenMlsLoader};

/// The engine does not read `CryptoEnv` (ADR-0187 decision 3); this stub only satisfies the signature.
struct Unused;
impl RngCore for Unused {
    fn next_u32(&mut self) -> u32 {
        0
    }
    fn next_u64(&mut self) -> u64 {
        0
    }
    fn fill_bytes(&mut self, _: &mut [u8]) {}
}

pub fn run() -> u32 {
    let mut r = Unused;
    let env = &mut CryptoEnv {
        rng: &mut r,
        now_ms: 0,
    };
    let gid = [7u8; 24];
    let (Ok(mut a), Ok(mut b)) = (
        OpenMlsGroupCrypto::new(&[1; 16]),
        OpenMlsGroupCrypto::new(&[2; 16]),
    ) else {
        return 1;
    };
    if a.create_group(env, &gid, b"ctx").is_err() {
        return 2;
    }
    let Ok(kp) = b.key_package(env) else { return 3 };
    let Ok(p) = a.add_commit(env, &gid, &[kp.bytes]) else {
        return 4;
    };
    if a.merge_pending(&gid).is_err() {
        return 5;
    }
    let Some(w) = p.welcome else { return 6 };
    if b.process_welcome(env, &w).is_err() {
        return 7;
    }
    let Ok(ct) = a.encrypt_application(env, &gid, b"hello") else {
        return 8;
    };
    match b.decrypt_application(&gid, &ct) {
        Ok(d) if d.plaintext == b"hello" => {}
        _ => return 9,
    }
    let snap = a.snapshot();
    let Ok(a2) = OpenMlsLoader::restore(&snap) else {
        return 10;
    };
    if a2.snapshot() != snap {
        return 11;
    }
    let Ok(rm) = a.remove_commit(env, &gid, &[vec![2; 16]]) else {
        return 12;
    };
    if a.merge_pending(&gid).is_err() || rm.resulting_epoch != 2 {
        return 13;
    }
    0
}
