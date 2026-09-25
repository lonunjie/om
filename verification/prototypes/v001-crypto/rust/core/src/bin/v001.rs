// SPDX-License-Identifier: Apache-2.0
//! V-001 prototype CLI (native and wasm32-wasip1). Not for production: requires the
//! `insecure-test-rng` feature. All output is deterministic JSON on stdout, except `bench`.

use om_v001_proto::rng::insecure_test_rng::InsecureTestRng;
use om_v001_proto::{otk, persist, vectors};
use serde_json::{json, Value};
use std::process::ExitCode;

fn print(v: &Value) {
    println!("{}", serde_json::to_string_pretty(v).unwrap());
}

fn selftest() -> (Value, bool) {
    let mut rng = InsecureTestRng::from_label(b"selftest");
    let checks = otk::run(&mut rng);
    let ok = checks.iter().all(|c| c.ok);
    let arr: Vec<Value> = checks.iter().map(|c| json!({"name": c.name, "ok": c.ok, "detail": c.detail})).collect();
    (json!({"checks": arr, "all_ok": ok}), ok)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let cmd = args.get(1).map(String::as_str).unwrap_or("");
    match cmd {
        "selftest" => {
            let (v, ok) = selftest();
            print(&v);
            if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        "vectors" => {
            print(&vectors::generate());
            ExitCode::SUCCESS
        }
        "persist-nsteps" => {
            println!("{}", persist::n_steps());
            ExitCode::SUCCESS
        }
        "persist-step" => {
            let dir = &args[2];
            let k: usize = args[3].parse().expect("step");
            let path = format!("{dir}/world.cbor");
            let prev = if k == 0 { None } else { Some(std::fs::read(&path).expect("read world")) };
            match persist::step(k, prev.as_deref()) {
                Ok(bytes) => {
                    std::fs::write(&path, &bytes).expect("write world");
                    let mut s = persist::summary(&bytes).unwrap();
                    s["step"] = json!(k);
                    println!("{}", serde_json::to_string(&s).unwrap());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("persist step {k} FAILED: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "bench" => {
            let si: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100);
            let mi: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(600);
            let start = std::time::Instant::now();
            let now = move || start.elapsed().as_nanos() as u64;
            let mut rng = InsecureTestRng::from_label(b"bench");
            print(&om_v001_proto::bench::run(&mut rng, &now, si, mi));
            ExitCode::SUCCESS
        }
        "acvp" => {
            let kg = std::fs::read_to_string(&args[2]).expect("keyGen file");
            let ed = std::fs::read_to_string(&args[3]).expect("encapDecap file");
            let r = om_v001_proto::acvp::run(&kg, &ed);
            let ok = r["all_passed"].as_bool().unwrap_or(false);
            print(&r);
            if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        "hpke-kat" => {
            let v = std::fs::read_to_string(&args[2]).expect("draft-ietf-hpke-pq-05 test-vectors.json");
            let r = om_v001_proto::hpke::check_kats(&v);
            let ok = r["all_passed"].as_bool().unwrap_or(false);
            print(&r);
            if ok { ExitCode::SUCCESS } else { ExitCode::FAILURE }
        }
        _ => {
            eprintln!("usage: v001 selftest|vectors|persist-nsteps|persist-step DIR K|bench [SETUP MSGS]|acvp KEYGEN.json ENCAPDECAP.json|hpke-kat VECTORS.json");
            ExitCode::from(2)
        }
    }
}
