// SPDX-License-Identifier: Apache-2.0
// Feasibility prototype for V-019. Not product code. Not for production.
fn main() {
    match om_v019_hybrid_pq_migration::run_all() {
        Ok(v) => {
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
            if v["vectors_failed"].as_u64() != Some(0) {
                eprintln!("FAIL: {} vector(s) failed", v["vectors_failed"]);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }
}
