// SPDX-License-Identifier: MPL-2.0
#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    let _ = om_core::expressive::fuzz_entry::release(data);
});
