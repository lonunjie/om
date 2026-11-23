mod support;
use om_archive_reader::{Limits, read_archive};
use std::collections::BTreeSet;
use std::time::{Duration, Instant};

// Fixed xorshift64* state. No system randomness, files, or network.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn choose(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}
fn u16le(s: &[u8], p: usize) -> usize {
    u16::from_le_bytes([s[p], s[p + 1]]) as usize
}
fn u32le(s: &[u8], p: usize) -> usize {
    u32::from_le_bytes([s[p], s[p + 1], s[p + 2], s[p + 3]]) as usize
}
fn u64le(s: &[u8], p: usize) -> usize {
    u64::from_le_bytes(s[p..p + 8].try_into().unwrap()) as usize
}

// Derive offsets from the original (trusted) positive fixtures, not mutated bytes.
// "Ignored" is precisely DOS local/central time/date and the made-by low byte.
fn offset_classes(s: &[u8]) -> (Vec<usize>, BTreeSet<usize>) {
    let mut protected = Vec::new();
    let mut ignored = BTreeSet::new();
    let mut p = 0;
    while &s[p..p + 4] == b"PK\x03\x04" {
        protected.extend(p..p + 4); // signature cannot change
        ignored.extend(p + 10..p + 14);
        let name = u16le(s, p + 26);
        let extra = u16le(s, p + 28);
        let mut length = u32le(s, p + 18);
        if length == u32::MAX as usize {
            length = u64le(s, p + 30 + name + 4); // local ZIP64 uncompressed size
        }
        let data_start = p + 30 + name + extra;
        protected.extend(data_start..data_start + length);
        p = data_start + length;
    }
    while p + 46 <= s.len() && &s[p..p + 4] == b"PK\x01\x02" {
        protected.extend(p..p + 4);
        protected.extend(p + 16..p + 20); // CRC32
        ignored.insert(p + 4);
        ignored.extend(p + 12..p + 16);
        p += 46 + u16le(s, p + 28) + u16le(s, p + 30) + u16le(s, p + 32);
    }
    if p + 4 <= s.len() && &s[p..p + 4] == b"PK\x06\x06" {
        ignored.insert(p + 12); // ZIP64 EOCD made-by low byte
    }
    assert!(!protected.is_empty());
    assert!(!ignored.is_empty());
    (protected, ignored)
}

#[test]
fn deterministic_twenty_thousand_mutants_no_panic_no_unbound_acceptance() {
    let bases: Vec<_> = support::vectors()
        .into_iter()
        .filter(|v| !v.name.starts_with("encrypted-"))
        .collect();
    assert_eq!(bases.len(), 3);
    let offsets: Vec<_> = bases.iter().map(|v| offset_classes(&v.bytes)).collect();
    let mut rng = Rng(0x4f4d_4152_4348_4956);
    let mut accepted = 0usize;
    for n in 0..20_000 {
        let i = n % bases.len();
        let original = &bases[i].bytes;
        let (protected, ignored) = &offsets[i];
        let mut mutant = original.clone();
        let flip = |b: &mut [u8], idx: usize, mask: u8| b[idx] ^= mask;
        let mode = rng.choose(6);
        match mode {
            0 => {
                let p = protected[rng.choose(protected.len())];
                flip(&mut mutant, p, 1 << rng.choose(8));
            }
            1 => {
                for _ in 0..3 {
                    let p = protected[rng.choose(protected.len())];
                    flip(&mut mutant, p, 1 << rng.choose(8));
                }
            }
            2 => {
                let n = rng.choose(mutant.len());
                mutant.truncate(n);
            }
            3 => {
                mutant.push(rng.next() as u8);
            }
            4 => {
                let p = *ignored.iter().nth(rng.choose(ignored.len())).unwrap();
                flip(&mut mutant, p, 1 << rng.choose(8));
            }
            _ => {
                let p = protected[rng.choose(protected.len())];
                flip(&mut mutant, p, 1 << rng.choose(8));
                let q = *ignored.iter().nth(rng.choose(ignored.len())).unwrap();
                flip(&mut mutant, q, 1 << rng.choose(8));
            }
        }
        assert!(mutant.len() < 64 * 1024, "size bound: mutant {n}");
        let start = Instant::now();
        let outcome = std::panic::catch_unwind(|| read_archive(&mutant, &Limits::default()));
        assert!(
            start.elapsed() < Duration::from_secs(2),
            "time bound: mutant {n}"
        );
        let result = outcome.unwrap_or_else(|_| panic!("panic on mutant {n}"));
        if result.is_ok() {
            accepted += 1;
            assert_eq!(
                mutant.len(),
                original.len(),
                "accepted length-changing mutant {n}"
            );
            for (pos, (a, b)) in original.iter().zip(mutant.iter()).enumerate() {
                assert!(
                    a == b || ignored.contains(&pos),
                    "accepted mutation at non-ignored byte {pos}, mutant {n}, vector {}",
                    bases[i].name
                );
            }
        }
    }
    assert!(
        accepted > 0,
        "the ignored-byte acceptance path was never exercised"
    );
    eprintln!("mutation sweep: 20000 mutations, {accepted} metadata-only accepted");
}
