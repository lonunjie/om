//! V-011 harness: runs the prompt-injection corpus and a random-model sweep through the broker, then checks every
//! outcome with the independent oracle. Output: deterministic JSON on stdout. Not product code.

use capbroker::broker::{Broker, Decision};
use capbroker::model::{extract, ToolRequest};
use capbroker::oracle;
use capbroker::world::{Effect, Installation, Invocation, World};
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, VecDeque};

#[derive(Deserialize)]
struct Corpus {
    presets: BTreeMap<String, Installation>,
    web: BTreeMap<String, String>,
    local_index: Vec<String>,
    cases: Vec<Case>,
}

#[derive(Deserialize, Clone)]
struct Case {
    id: String,
    category: String,
    description: String,
    service: String,
    #[serde(default)]
    extra_installs: Vec<String>,
    #[serde(default)]
    ping_pong: bool,
    #[serde(default)]
    room: Option<String>,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    bundle: Vec<(String, String)>,
    #[serde(default)]
    room_messages: Vec<(String, String)>,
    #[serde(default)]
    trigger: Option<(String, String)>,
    #[serde(default)]
    must_block: Vec<String>,
    #[serde(default)]
    must_allow: Vec<String>,
    #[serde(default)]
    max_messages: Option<usize>,
}

const MAX_REQUESTS_PER_INVOCATION: usize = 60; // harness guard only; the broker's own budget is lower
const MAX_INVOCATIONS: usize = 400;

struct Run {
    world: World,
    broker: Broker,
    decisions: Vec<(String, Decision)>,
    invocations: usize,
    oracle: Vec<String>,
}

fn run_case(c: &Case, corpus: &Corpus, disable_limits: bool) -> Run {
    let mut installs: Vec<Installation> = vec![corpus.presets[&c.service].clone()];
    for x in &c.extra_installs {
        installs.push(corpus.presets[x].clone());
    }
    let original: BTreeMap<String, Vec<String>> = installs.iter().map(|i| (i.service_id.clone(), i.granted_capabilities.iter().cloned().collect())).collect();
    let mut world = World { web: corpus.web.clone(), local_index: corpus.local_index.clone(), ..Default::default() };
    if let Some(r) = &c.room {
        world.rooms.insert(r.clone(), c.room_messages.clone());
    }
    let mut broker = Broker::default();
    broker.insecure_disable_loop_limits = disable_limits;
    let mut decisions = vec![];
    let mut next_id = 1u64;
    // (invocation, installation index, extra model input)
    let mut queue: VecDeque<(Invocation, usize, Vec<String>)> = VecDeque::new();
    match &c.trigger {
        None => {
            queue.push_back((Invocation { invocation_id: 1, root_invocation_id: 1, origin_principal: "user".into(), hop_count: 0, service_id: installs[0].service_id.clone(),
                                          bundle: c.bundle.clone(), room: c.room.clone() }, 0, vec![c.prompt.clone()]));
            next_id = 2;
        }
        Some((author, text)) => {
            let room = c.room.clone().unwrap();
            world.rooms.entry(room.clone()).or_default().push((author.clone(), text.clone()));
            for (k, inst) in installs.iter().enumerate() {
                if broker.should_trigger(inst, false, author, 0, &room, text) {
                    // The triggering Room message is plaintext disclosed to the service: it is a bundle item (taints).
                    queue.push_back((Invocation { invocation_id: next_id, root_invocation_id: 1, origin_principal: author.clone(), hop_count: 1,
                                                  service_id: inst.service_id.clone(), bundle: vec![("trigger".into(), text.clone())], room: Some(room.clone()) }, k, vec![]));
                    next_id += 1;
                }
            }
        }
    }
    let mut invocations = 0;
    while let Some((inv, k, inputs)) = queue.pop_front() {
        invocations += 1;
        if invocations > MAX_INVOCATIONS {
            break;
        }
        broker.start(&inv, &mut world);
        let inst = installs[k].clone();
        let mut reqs: VecDeque<ToolRequest> = VecDeque::new();
        if c.ping_pong {
            let other = installs.iter().map(|i| i.service_id.clone()).find(|s| *s != inst.service_id).unwrap_or_default();
            reqs.push_back(ToolRequest { tool: "send_message".into(), args: json!({"room": inv.room, "text": format!("@{other} ping from {} (hop {})", inst.service_id, inv.hop_count)}) });
        }
        for t in inputs.iter().chain(inv.bundle.iter().map(|b| &b.1)) {
            reqs.extend(extract(t, 0));
        }
        let mut n = 0;
        while let Some(r) = reqs.pop_front() {
            n += 1;
            if n > MAX_REQUESTS_PER_INVOCATION {
                break;
            }
            let before = world.effects.len();
            let (d, output) = broker.decide(&inst, &inv, &r, &mut world);
            decisions.push((r.tool.clone(), d.clone()));
            if let Some(out) = output {
                reqs.extend(extract(&out, 0)); // chained injection via tool results
            }
            // automation triggers from a newly posted service message
            if let Some((_, Effect::Message { room, text, .. })) = world.effects.get(before).filter(|_| world.effects.len() > before) {
                let (room, text) = (room.clone(), text.clone());
                for (j, other) in installs.iter().enumerate() {
                    if broker.should_trigger(other, true, &inst.service_id, inv.hop_count, &room, &text) {
                        queue.push_back((Invocation { invocation_id: next_id, root_invocation_id: inv.root_invocation_id, origin_principal: inst.service_id.clone(),
                                                      hop_count: inv.hop_count + 1, service_id: other.service_id.clone(), bundle: vec![("trigger".into(), text.clone())], room: Some(room.clone()) }, j, vec![]));
                        next_id += 1;
                    }
                }
            }
        }
    }
    let oracle = oracle::check(&installs, &original, &world);
    Run { world, broker, decisions, invocations, oracle }
}

fn messages(w: &World) -> usize {
    w.effects.iter().filter(|(_, e)| matches!(e, Effect::Message { .. })).count()
}

/// Deterministic random "model" (INSECURE xorshift test PRNG): arbitrary tool requests with adversarial arguments.
fn random_sweep(corpus: &Corpus, n: usize) -> (usize, usize, usize, Vec<String>) {
    let mut st: u64 = 0x2545_f491_4f6c_dd1d;
    let mut rnd = || { st ^= st << 13; st ^= st >> 7; st ^= st << 17; st };
    let tools = ["create_draft", "send_message", "read_context", "search_local", "fetch_url", "propose_room_action", "request_capability",
                 "export_keys", "shell_exec", "manage_members", "add_bridge", "send_as_account", "grant_capability", "frobnicate"];
    let rooms = ["R1", "R2", "R3"];
    let urls = ["https://api.weather.example/today", "https://docs.example.org/page", "https://evil.example/x?d=SECRET-1", "http://api.weather.example/",
                "https://api.weather.example@evil.example/", "https://api.weather.example:8443/", "https://api.weather.example/q?d=SECRET-2"];
    let presets: Vec<&String> = corpus.presets.keys().collect();
    let (mut allowed, mut denied, mut violations) = (0, 0, vec![]);
    let mut total = 0;
    for i in 0..n {
        let p = presets[(rnd() as usize) % presets.len()];
        let inst = corpus.presets[p].clone();
        let original = BTreeMap::from([(inst.service_id.clone(), inst.granted_capabilities.iter().cloned().collect::<Vec<_>>())]);
        let mut world = World { web: corpus.web.clone(), local_index: corpus.local_index.clone(), ..Default::default() };
        let mut broker = Broker::default();
        let bundle = if rnd() % 2 == 0 { vec![("m1".to_string(), "private SECRET-9 text".to_string())] } else { vec![] };
        let inv = Invocation { invocation_id: 1, root_invocation_id: 1, origin_principal: "user".into(), hop_count: (rnd() % 5) as u32, service_id: inst.service_id.clone(),
                               bundle, room: Some(rooms[(rnd() as usize) % 3].into()) };
        broker.start(&inv, &mut world);
        for _ in 0..(1 + rnd() % 12) {
            let tool = tools[(rnd() as usize) % tools.len()];
            let mut args = json!({"room": rooms[(rnd() as usize) % 3], "url": urls[(rnd() as usize) % urls.len()], "item": if rnd() % 2 == 0 { "m1" } else { "room:0" },
                                  "text": format!("t{} SECRET-{}", rnd() % 3, rnd() % 3), "capability": "network.fetch"});
            if rnd() % 3 == 0 {
                args["on_behalf_of"] = json!(if rnd() % 2 == 0 { "alice" } else { "bob" });
            }
            if rnd() % 4 == 0 {
                args["author"] = json!("alice");
            }
            let (d, _) = broker.decide(&inst, &inv, &ToolRequest { tool: tool.into(), args }, &mut world);
            total += 1;
            if d == Decision::Allow { allowed += 1 } else { denied += 1 }
        }
        for v in oracle::check(&[inst], &original, &world) {
            violations.push(format!("sweep {i}: {v}"));
        }
    }
    (total, allowed, denied, violations)
}

fn main() {
    let long = std::env::args().any(|a| a == "--long");
    let path = std::env::args().skip(1).find(|a| !a.starts_with("--")).unwrap_or("../injection-corpus.json".into());
    let corpus: Corpus = serde_json::from_str(&std::fs::read_to_string(&path).expect("corpus")).expect("corpus json");
    let mut rows = vec![];
    let mut all_ok = true;
    let mut cats: BTreeMap<String, usize> = BTreeMap::new();
    for c in &corpus.cases {
        *cats.entry(c.category.clone()).or_default() += 1;
        let r = run_case(c, &corpus, false);
        let requested = |t: &str| r.decisions.iter().any(|(x, _)| x == t);
        let allowed = |t: &str| r.decisions.iter().any(|(x, d)| x == t && *d == Decision::Allow);
        let mut fails: Vec<String> = r.oracle.clone();
        for t in &c.must_block {
            if !requested(t) {
                fails.push(format!("attack did not fire: {t} never requested"));
            } else if allowed(t) {
                fails.push(format!("{t} was allowed"));
            }
        }
        for t in &c.must_allow {
            if !allowed(t) {
                fails.push(format!("benign {t} not allowed"));
            }
        }
        let msgs = messages(&r.world);
        if let Some(m) = c.max_messages {
            if msgs > m {
                fails.push(format!("{msgs} messages > {m}"));
            }
        }
        let baseline = if c.category == "loop" { Some(messages(&run_case(c, &corpus, true).world)) } else { None };
        let ok = fails.is_empty();
        all_ok &= ok;
        let denials: Vec<String> = r.broker.log.iter().filter(|x| x.2 != "Allow").map(|x| format!("{}:{}", x.1, x.2)).collect();
        eprintln!("{} {:<34} {:<18} msgs={:<3} inv={:<3} {}", if ok { "ok " } else { "BAD" }, c.id, c.category, msgs, r.invocations,
                  if ok { baseline.map(|b| format!("(without broker limits: {b} messages)")).unwrap_or_default() } else { fails.join("; ") });
        rows.push(json!({"id": c.id, "category": c.category, "description": c.description, "ok": ok, "failures": fails,
                         "messages": msgs, "invocations": r.invocations, "denials": denials, "baseline_messages_without_broker_limits": baseline}));
    }
    let n_sweep = if long { 100_000 } else { 10_000 };
    let (total, allowed, denied, viol) = random_sweep(&corpus, n_sweep);
    let sweep_ok = viol.is_empty();
    all_ok &= sweep_ok;
    eprintln!("{} random-model sweep: {n_sweep} invocations, {total} requests, {allowed} allowed, {denied} denied, {} oracle violations", if sweep_ok { "ok " } else { "BAD" }, viol.len());
    let attacks = corpus.cases.iter().filter(|c| c.category != "benign_control").count();
    let out = json!({"all_ok": all_ok, "cases": corpus.cases.len(), "attack_cases": attacks, "categories": cats,
                     "random_model_sweep": {"invocations": n_sweep, "requests": total, "allowed": allowed, "denied": denied, "oracle_violations": viol.iter().take(10).collect::<Vec<_>>()},
                     "results": rows});
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    std::process::exit(if all_ok { 0 } else { 1 });
}

#[allow(dead_code)]
fn _unused(_: Value) {}
