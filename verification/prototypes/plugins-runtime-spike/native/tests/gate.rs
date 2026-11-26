// SPDX-License-Identifier: MPL-2.0
//! Real Component Model components through the product plugin host policy (`om_core::plugin_host`), on the native
//! runtime: the imports a component declares are read by Wasmtime itself and gated at instantiation, and fuel exhaustion
//! on a real looping component terminates the plugin. Non-production spike code (V-010). Covers the import gate and the
//! fuel path on Wasmtime only; the browser path is measured separately and no independent review has been done.
use om_core::plugin_host::{Budgets, Capability, HostConfig, PluginError, PluginHost};
use std::collections::BTreeSet;
use wasmtime::component::Component;
use wasmtime::{Config, Engine, Store};

fn engine() -> Engine {
    let mut c = Config::new();
    c.consume_fuel(true);
    Engine::new(&c).unwrap()
}

/// A component importing the named instances (each exporting one function), as a real binary.
fn component(engine: &Engine, imports: &[&str]) -> Component {
    let mut wat = String::from("(component\n");
    for i in imports {
        wat.push_str(&format!(
            "  (import \"{i}\" (instance (export \"f\" (func))))\n"
        ));
    }
    wat.push_str(")\n");
    Component::new(engine, wat).unwrap()
}

/// The import names Wasmtime reads from the compiled component.
fn declared_imports(engine: &Engine, c: &Component) -> Vec<String> {
    c.component_type()
        .imports(engine)
        .map(|(n, _)| n.to_string())
        .collect()
}

fn host(isolated: bool) -> PluginHost {
    PluginHost::new(HostConfig {
        site_isolation: isolated,
        budgets: Budgets {
            max_memory_bytes: 16 << 20,
            max_fuel: 1_000_000,
        },
    })
}

/// Gate: every declared import must be one of the registered capabilities and granted.
fn gate(
    h: &mut PluginHost,
    imports: &[String],
    grants: &BTreeSet<Capability>,
) -> Result<(), PluginError> {
    let mut names = Vec::new();
    for i in imports {
        names.push(
            Capability::from_wit_import(i)
                .ok_or(PluginError::UnknownCapability)?
                .name(),
        );
    }
    h.instantiate(&names, grants)
}

fn grants(c: &[Capability]) -> BTreeSet<Capability> {
    c.iter().copied().collect()
}

#[test]
fn a_granted_capability_only_component_passes_the_gate() {
    let e = engine();
    let c = component(&e, &["om:plugin/ui-panel@0.1.0"]);
    let imports = declared_imports(&e, &c);
    assert_eq!(imports, vec!["om:plugin/ui-panel@0.1.0"]);
    assert_eq!(
        gate(&mut host(true), &imports, &grants(&[Capability::UiPanel])),
        Ok(())
    );
}

#[test]
fn ambient_wasi_imports_are_refused() {
    let e = engine();
    for wasi in [
        "wasi:filesystem/types@0.2.0",
        "wasi:sockets/tcp@0.2.0",
        "wasi:cli/environment@0.2.0",
    ] {
        let c = component(&e, &[wasi]);
        assert_eq!(
            gate(&mut host(true), &declared_imports(&e, &c), &grants(&[])),
            Err(PluginError::UnknownCapability),
            "{wasi}"
        );
    }
}

#[test]
fn an_ungranted_send_import_is_refused_even_next_to_a_granted_one() {
    let e = engine();
    let c = component(
        &e,
        &[
            "om:plugin/ui-panel@0.1.0",
            "om:plugin/conversation-send-message@0.1.0",
        ],
    );
    assert_eq!(
        gate(
            &mut host(true),
            &declared_imports(&e, &c),
            &grants(&[Capability::UiPanel])
        ),
        Err(PluginError::CapabilityNotGranted)
    );
}

#[test]
fn a_content_plugin_is_refused_where_the_platform_cannot_isolate_it() {
    let e = engine();
    let c = component(&e, &["om:plugin/content-read-current-conversation@0.1.0"]);
    let g = grants(&[Capability::ContentReadCurrentConversation]);
    assert_eq!(
        gate(&mut host(false), &declared_imports(&e, &c), &g),
        Err(PluginError::HostPolicyDenied)
    );
    assert_eq!(gate(&mut host(true), &declared_imports(&e, &c), &g), Ok(()));
}

#[test]
fn a_real_looping_component_runs_out_of_fuel_and_the_host_terminates_it() {
    let e = engine();
    let wat = r#"(component
        (core module $m (func (export "run") (loop $l (br $l))))
        (core instance $i (instantiate $m))
        (func (export "run") (canon lift (core func $i "run"))))"#;
    let c = Component::new(&e, wat).unwrap();
    let mut store = Store::new(&e, ());
    store.set_fuel(1_500_000).unwrap(); // above the host's 1,000,000 budget
    let linker = wasmtime::component::Linker::<()>::new(&e);
    let inst = linker.instantiate(&mut store, &c).unwrap();
    let run = inst.get_typed_func::<(), ()>(&mut store, "run").unwrap();
    assert!(
        run.call(&mut store, ()).is_err(),
        "the loop traps on fuel exhaustion"
    );
    let used = 1_500_000 - store.get_fuel().unwrap();
    assert_eq!(used, 1_500_000, "the whole fuel allowance was consumed");
    let mut h = host(true);
    // the runtime reports the consumption it measured; above the host's configured budget the plugin is terminated
    h.begin_invocation().unwrap();
    assert_eq!(h.account(0, used), Err(PluginError::ResourceBudgetExceeded));
    assert!(h.is_terminated());
}
