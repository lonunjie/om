// SPDX-License-Identifier: MPL-2.0
//! V-010 feasibility spike (non-production): can a Component Model plugin run under a fuel budget and a memory cap, with
//! no ambient imports? Prints one line per case. Also writes the hostile core module the browser spike reuses.
use std::time::Instant;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine, ResourceLimiter, Store};

const CORE: &str = r#"
(module
  (memory (export "memory") 1)
  (func (export "ok") (result i32) i32.const 42)
  (func (export "spin") (loop $l br $l))
  (func (export "grow") (result i32)
    (local $n i32)
    (loop $l
      (local.set $n (memory.grow (i32.const 16)))
      (br_if $l (i32.ne (local.get $n) (i32.const -1))))
    (local.get $n)))
"#;

struct Limits {
    max_bytes: usize,
    denied: bool,
}
impl ResourceLimiter for Limits {
    fn memory_growing(&mut self, _cur: usize, desired: usize, _max: Option<usize>) -> anyhow::Result<bool> {
        if desired > self.max_bytes {
            self.denied = true;
            return Ok(false);
        }
        Ok(true)
    }
    fn table_growing(&mut self, _c: usize, d: usize, _m: Option<usize>) -> anyhow::Result<bool> {
        Ok(d <= 10_000)
    }
}

fn main() -> anyhow::Result<()> {
    let mut cfg = Config::new();
    cfg.consume_fuel(true);
    let engine = Engine::new(&cfg)?;
    // a component wrapping the core module, exporting three functions with no imports
    let comp_wat = format!(
        r#"(component
  (core module $m {CORE_BODY}
  (core instance $i (instantiate $m))
  (func (export "ok") (result s32) (canon lift (core func $i "ok")))
  (func (export "spin") (canon lift (core func $i "spin")))
  (func (export "grow") (result s32) (canon lift (core func $i "grow")))
)"#,
        CORE_BODY = CORE.trim().strip_prefix("(module").unwrap()
    );
    // the browser spike reuses the same module but takes its memory as an import, so the host sets the cap
    let browser = CORE.replace("(memory (export \"memory\") 1)", "(import \"env\" \"memory\" (memory 1))");
    std::fs::write("hostile-browser.wasm", wat_bytes(&browser))?;
    let t = Instant::now();
    let component = Component::new(&engine, &comp_wat)?;
    println!("compile_ms={}", t.elapsed().as_millis());
    let linker: Linker<Limits> = Linker::new(&engine);
    for case in ["ok", "spin", "grow"] {
        let mut store = Store::new(&engine, Limits { max_bytes: 4 << 20, denied: false });
        store.limiter(|s| s);
        store.set_fuel(5_000_000)?;
        let t = Instant::now();
        let inst = linker.instantiate(&mut store, &component)?;
        let inst_ms = t.elapsed().as_micros();
        let t = Instant::now();
        let res = match case {
            "ok" => inst.get_typed_func::<(), (i32,)>(&mut store, "ok")?.call(&mut store, ()).map(|r| format!("{}", r.0)),
            "spin" => inst.get_typed_func::<(), ()>(&mut store, "spin")?.call(&mut store, ()).map(|_| "returned".into()),
            _ => inst.get_typed_func::<(), (i32,)>(&mut store, "grow")?.call(&mut store, ()).map(|r| format!("{}", r.0)),
        };
        let used = 5_000_000 - store.get_fuel()?;
        println!(
            "case={case} instantiate_us={inst_ms} run_us={} fuel_used={used} memory_denied={} result={}",
            t.elapsed().as_micros(),
            store.data().denied,
            match res {
                Ok(v) => format!("ok:{v}"),
                Err(e) => format!("trap:{}", e.root_cause().to_string().lines().next().unwrap_or("")),
            }
        );
    }
    // an import the host does not provide must fail to link, so there is no ambient authority
    let needs_import = Component::new(
        &engine,
        r#"(component (import "net" (func $f)) (core func $cf (canon lower (func $f)))
          (core module $m (import "e" "f" (func)) (func (export "go") call 0))
          (core instance $i (instantiate $m (with "e" (instance (export "f" (func $cf))))))
          (func (export "go") (canon lift (core func $i "go"))))"#,
    )?;
    let mut store = Store::new(&engine, Limits { max_bytes: 1 << 20, denied: false });
    store.set_fuel(1000)?;
    println!("unprovided_import_links={}", linker.instantiate(&mut store, &needs_import).is_ok());
    Ok(())
}

fn wat_bytes(w: &str) -> Vec<u8> {
    wat::parse_str(w).expect("wat")
}
