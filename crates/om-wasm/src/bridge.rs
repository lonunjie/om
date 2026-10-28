// SPDX-License-Identifier: MPL-2.0
//! The JSON command layer over [`om_core::Core`]. Pure safe Rust and host-independent, so it is unit-tested natively.

use om_core::Core;
use om_core::api::{HomeSummary, Intent, ProfileExecutionContext, Query};
use om_core::crypto::rng::{self, CryptoRng, RngCore};
use om_core::host::{Clock, CoreError, Inbound, NetResult, Record};
use om_core::route::RouteOffer;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(
    tag = "cmd",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
enum Command {
    Init {
        home: HomeSummary,
    },
    OpenProfile {
        profile_id: String,
        records: Vec<Record>,
    },
    CloseProfile {
        profile_id: String,
    },
    ContextFor {
        profile_id: String,
    },
    Dispatch {
        intent: Intent,
    },
    FinishCommit {
        commit_id: u64,
        ok: bool,
    },
    NetOps {
        ctx: ProfileExecutionContext,
    },
    NetResult {
        ctx: ProfileExecutionContext,
        net_id: u64,
        result: NetResult,
    },
    PollTargets {
        ctx: ProfileExecutionContext,
    },
    Inbound {
        ctx: ProfileExecutionContext,
        mailbox: String,
        items: Vec<Inbound>,
    },
    Query {
        query: Query,
    },
    PendingOperations {
        ctx: ProfileExecutionContext,
    },
    Outcome {
        ctx: ProfileExecutionContext,
        operation_id: String,
    },
    /// Routes the provider adapter allocated for mailboxes Core has not created yet. Answers how many Core holds.
    SupplyRoutes {
        routes: Vec<RouteOffer>,
    },
}

pub struct HostClock<F: Fn() -> u64>(pub F);
impl<F: Fn() -> u64> Clock for HostClock<F> {
    fn now_ms(&self) -> u64 {
        (self.0)()
    }
}

/// One Core instance behind the JSON API. `Bridge::new` takes the RNG and clock so the native tests can inject
/// deterministic ones; the wasm entry point passes the system RNG.
pub struct Bridge<R: RngCore + CryptoRng> {
    make_rng: Option<Box<dyn FnOnce() -> R>>,
    clock: Option<Box<dyn Fn() -> u64>>,
    core: Option<Core<R>>,
}

impl<R: RngCore + CryptoRng> Bridge<R> {
    pub fn new(make_rng: impl FnOnce() -> R + 'static, clock: impl Fn() -> u64 + 'static) -> Self {
        Bridge {
            make_rng: Some(Box::new(make_rng)),
            clock: Some(Box::new(clock)),
            core: None,
        }
    }

    /// Handle one request; always returns a JSON response, never panics on malformed input.
    pub fn handle(&mut self, request: &[u8]) -> Vec<u8> {
        let out = match serde_json::from_slice::<Command>(request) {
            Err(e) => err("invalid", &format!("malformed command: {e}")),
            Ok(cmd) => match self.run(cmd) {
                Ok(v) => json!({ "ok": v }),
                Err(e) => core_err(&e),
            },
        };
        serde_json::to_vec(&out)
            .unwrap_or_else(|_| br#"{"err":{"code":"internal","message":"encode"}}"#.to_vec())
    }

    fn core(&mut self) -> Result<&mut Core<R>, CoreError> {
        self.core
            .as_mut()
            .ok_or(CoreError::Invalid("not initialised"))
    }

    fn run(&mut self, cmd: Command) -> Result<Value, CoreError> {
        let enc = |v: &dyn erased::Ser| v.to_value();
        match cmd {
            Command::Init { home } => {
                if self.core.is_some() {
                    return Err(CoreError::Invalid("already initialised"));
                }
                let (Some(make_rng), Some(clock)) = (self.make_rng.take(), self.clock.take())
                else {
                    return Err(CoreError::Invalid("not initialisable"));
                };
                self.core = Some(Core::new(make_rng(), HostClock(clock), home));
                Ok(Value::Null)
            }
            Command::OpenProfile {
                profile_id,
                records,
            } => Ok(enc(&self.core()?.open_profile(&profile_id, &records)?)),
            Command::CloseProfile { profile_id } => {
                self.core()?.close_profile(&profile_id);
                Ok(Value::Null)
            }
            Command::ContextFor { profile_id } => {
                Ok(enc(&self.core()?.context_for(&profile_id)?))
            }
            Command::Dispatch { intent } => Ok(enc(&self.core()?.dispatch(&intent)?)),
            Command::FinishCommit { commit_id, ok } => {
                Ok(enc(&self.core()?.finish_commit(commit_id, ok)?))
            }
            Command::NetOps { ctx } => Ok(enc(&self.core()?.net_ops(&ctx)?)),
            Command::NetResult {
                ctx,
                net_id,
                result,
            } => Ok(enc(&self.core()?.net_result(&ctx, net_id, result)?)),
            Command::PollTargets { ctx } => Ok(enc(&self.core()?.poll_targets(&ctx)?)),
            Command::Inbound {
                ctx,
                mailbox,
                items,
            } => {
                let mailbox =
                    om_core::crypto::unhex(&mailbox).map_err(|_| CoreError::Invalid("mailbox"))?;
                Ok(enc(&self.core()?.inbound(&ctx, &mailbox, &items)?))
            }
            Command::Query { query } => self.core()?.query(&query),
            Command::PendingOperations { ctx } => {
                Ok(enc(&self.core()?.pending_operations(&ctx)?))
            }
            Command::Outcome { ctx, operation_id } => {
                Ok(enc(&self.core()?.outcome(&ctx, &operation_id)?))
            }
            Command::SupplyRoutes { routes } => Ok(json!(self.core()?.supply_routes(routes))),
        }
    }
}

/// The production constructor: system CSPRNG (host entropy on wasm) and a host clock.
pub fn system_bridge(clock: impl Fn() -> u64 + 'static) -> Bridge<rng::SystemRng> {
    Bridge::new(rng::system_rng, clock)
}

fn err(code: &str, message: &str) -> Value {
    json!({ "err": { "code": code, "message": message } })
}

fn core_err(e: &CoreError) -> Value {
    let code = match e {
        CoreError::ProfileAccess(_) => "profile_access",
        CoreError::OperationIdConflict => "operation_id_conflict",
        CoreError::Busy => "busy",
        CoreError::CorruptState(_) => "corrupt_state",
        CoreError::NotFound(_) => "not_found",
        CoreError::Invalid(_) => "invalid",
    };
    err(code, &e.to_string())
}

mod erased {
    pub trait Ser {
        fn to_value(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
        }
    }
}
