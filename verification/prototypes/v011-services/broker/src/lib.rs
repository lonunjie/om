//! V-011 tool-capability broker feasibility prototype. Not product code. Not for production.
//!
//! Written by a Claude agent from SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md ("Tool use and prompt-injection
//! boundary", "Four data-access modes", "ContextDisclosureBundle", "Automation rules", "Bot-to-bot / agent-to-agent
//! loop safety", invariants 6-8) and SERVICE_CREATION_DISCOVERY_AND_DELEGATED_AGENCY.md (sections 9-10, 13).
//!
//! Modules:
//! - `broker`: the only component with authority. Every tool request from a service passes through `Broker::decide`.
//! - `model`: a stub, deterministic, maximally gullible "LLM" that emits every tool directive it can find in its
//!   inputs (including base64-obfuscated ones and ones inside tool results). Enforcement must not depend on it.
//! - `world`: host state and the append-only effect log.
//! - `oracle`: an independently written checker over the effect log.

pub mod broker;
pub mod model;
pub mod oracle;
pub mod world;
