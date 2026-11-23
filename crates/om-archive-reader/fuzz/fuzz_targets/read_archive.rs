#![no_main]
use libfuzzer_sys::fuzz_target;
use om_archive_reader::{read_archive, Limits};

fuzz_target!(|input: &[u8]| {
    // Fuzzer-generated archives never gain authority or write files.
    // Tight policy ceilings keep corpus throughput high.
    let limits = Limits {
        max_entries: 1024,
        max_entry_size: 4 * 1024 * 1024,
        max_total_size: 8 * 1024 * 1024,
        max_manifest_size: 1024 * 1024,
    };
    let _ = read_archive(input, &limits);
});
