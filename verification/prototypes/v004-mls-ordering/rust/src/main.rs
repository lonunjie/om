// SPDX-License-Identifier: Apache-2.0
// Feasibility prototype for V-004. Not product code. Not for production.
fn main() {
    match om_v004_mls_ordering::run_all() {
        Ok(v) => println!("{}", serde_json::to_string_pretty(&v).unwrap()),
        Err(e) => {
            eprintln!("FAIL: {e}");
            std::process::exit(1);
        }
    }
}
