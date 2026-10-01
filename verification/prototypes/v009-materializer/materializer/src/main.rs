//! Feasibility prototype for V-009. Not product code. Not for production.
//!
//! Usage:
//!   om-v009-materializer check <corpus.json> <out.json>
//!       For every case: materialize every permutation (all when n<=6, else a seeded sample) and
//!       duplicated delivery orders; assert all views are identical; write canonical views.
//!   om-v009-materializer view <corpus.json> <case_id> [i,j,k...]
//!       Print the view for one delivery order (indices into the case's events; duplicates allowed).

mod materialize;

use serde_json::{json, Value};
use std::process::ExitCode;

/// Deterministic test-only PRNG (splitmix64) for permutation sampling. NOT cryptographic.
struct TestPrng(u64);
impl TestPrng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

fn fnv(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325u64, |h, b| (h ^ b as u64).wrapping_mul(0x100_0000_01b3))
}

fn permutations(n: usize) -> Vec<Vec<usize>> {
    // Heap's algorithm, deterministic order.
    let mut a: Vec<usize> = (0..n).collect();
    let mut out = vec![a.clone()];
    let mut c = vec![0usize; n];
    let mut i = 0;
    while i < n {
        if c[i] < i {
            if i % 2 == 0 { a.swap(0, i) } else { a.swap(c[i], i) }
            out.push(a.clone());
            c[i] += 1;
            i = 0;
        } else {
            c[i] = 0;
            i += 1;
        }
    }
    out
}

pub const EXHAUSTIVE_MAX: usize = 6;
pub const SAMPLE_PERMS: usize = 600;

fn delivery_orders(id: &str, n: usize) -> Vec<Vec<usize>> {
    let perms = if n <= EXHAUSTIVE_MAX {
        permutations(n)
    } else {
        let mut r = TestPrng(fnv(id));
        let mut v = vec![(0..n).collect::<Vec<_>>(), (0..n).rev().collect()];
        while v.len() < SAMPLE_PERMS {
            let mut p: Vec<usize> = (0..n).collect();
            for i in (1..n).rev() {
                p.swap(i, (r.next() % (i as u64 + 1)) as usize);
            }
            v.push(p);
        }
        v
    };
    let mut r = TestPrng(fnv(id) ^ 0xD0D0);
    let mut out = vec![];
    for (k, p) in perms.into_iter().enumerate() {
        if k % 3 == 0 && n > 0 {
            // duplication: whole order replayed reversed (every event delivered twice) ...
            let mut d = p.clone();
            d.extend(p.iter().rev());
            out.push(d);
            // ... and one random event re-delivered at a random later position.
            let mut d2 = p.clone();
            let e = p[(r.next() % n as u64) as usize];
            let pos = (r.next() % (n as u64 + 1)) as usize;
            d2.insert(pos, e);
            out.push(d2);
        }
        out.push(p);
    }
    out
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let corpus: Value = serde_json::from_str(&std::fs::read_to_string(&args[2]).expect("read corpus")).expect("corpus json");
    let cases = corpus["cases"].as_array().expect("cases");
    match args[1].as_str() {
        "check" => {
            let mut results = vec![];
            let mut all_ok = true;
            let mut total_orders = 0usize;
            for case in cases {
                let id = case["id"].as_str().unwrap();
                let ci = materialize::case_input(case);
                let events: Vec<&Value> = case["events"].as_array().unwrap().iter().collect();
                let orders = delivery_orders(id, events.len());
                let base = materialize::materialize(&ci, &events);
                let mut divergent: Vec<Value> = vec![];
                for o in &orders {
                    let d: Vec<&Value> = o.iter().map(|&i| events[i]).collect();
                    let v = materialize::materialize(&ci, &d);
                    if v != base && divergent.len() < 3 {
                        divergent.push(json!({"order": o, "view": v}));
                    }
                }
                total_orders += orders.len();
                all_ok &= divergent.is_empty();
                results.push(json!({"id": id, "orders_checked": orders.len(), "converged": divergent.is_empty(), "divergent_orders": divergent, "view": base}));
            }
            // R-239: forwarding transform vectors (source body -> new body).
            let forwards: Vec<Value> = corpus["forwards"].as_array().map(Vec::as_slice).unwrap_or_default().iter().map(|f| {
                let out = materialize::forward_body(f["source"].as_object().expect("source"));
                json!({"id": f["id"], "output": out})
            }).collect();
            let out = json!({"materializer": "om-v009-materializer (Rust)", "total_orders": total_orders, "all_converged": all_ok, "cases": results, "forwards": forwards});
            std::fs::write(&args[3], serde_json::to_string_pretty(&out).unwrap() + "\n").expect("write");
            eprintln!("rust materializer: {} cases, {} delivery orders, all_converged={}", cases.len(), total_orders, all_ok);
            if all_ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        "view" => {
            let case = cases.iter().find(|c| c["id"] == args[3].as_str()).expect("case");
            let ci = materialize::case_input(case);
            let events: Vec<&Value> = case["events"].as_array().unwrap().iter().collect();
            let order: Vec<usize> = match args.get(4) {
                Some(s) => s.split(',').map(|x| x.parse().unwrap()).collect(),
                None => (0..events.len()).collect(),
            };
            let d: Vec<&Value> = order.iter().map(|&i| events[i]).collect();
            println!("{}", serde_json::to_string_pretty(&materialize::materialize(&ci, &d)).unwrap());
            ExitCode::SUCCESS
        }
        _ => ExitCode::FAILURE,
    }
}
