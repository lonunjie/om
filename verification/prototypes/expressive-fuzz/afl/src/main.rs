// SPDX-License-Identifier: MPL-2.0
//! AFL++ persistent-mode driver over `om_core::expressive::fuzz_entry` (V-012, ADR-0251). One binary, the entry point is
//! picked by the `OM_AFL_TARGET` environment variable, so a run of one target never shares coverage state with another.
//! The input is cut at the same maximum length the libFuzzer targets use. A panic is a finding (the `afl` crate turns it
//! into an abort that AFL++ records under `crashes/`).

use om_core::expressive::fuzz_entry as entry;

type Target = fn(&[u8]) -> Result<(), &'static str>;

const TARGETS: [(&str, Target, usize); 7] = [
    ("webp", entry::webp, 65536),
    ("facts", entry::facts, 65536),
    ("asset_ref", entry::asset_ref, 65536),
    ("event", entry::event, 65536),
    ("release", entry::release, 65536),
    ("control_state", entry::control_state, 65536),
    ("pack_file", entry::pack_file, 262144),
];

fn main() {
    let name = std::env::var("OM_AFL_TARGET").unwrap_or_default();
    let Some(&(_, run, max_len)) = TARGETS.iter().find(|(n, _, _)| *n == name) else {
        eprintln!(
            "OM_AFL_TARGET must be one of {:?}",
            TARGETS.iter().map(|t| t.0).collect::<Vec<_>>()
        );
        std::process::exit(2);
    };
    afl::fuzz!(|data: &[u8]| {
        let _ = run(&data[..data.len().min(max_len)]);
    });
}
