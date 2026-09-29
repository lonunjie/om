// SPDX-License-Identifier: MPL-2.0
//! Clean-room implementation of the V1 presentation extensions for V-005 independent-implementation evidence
//! (ADR-0167/ADR-0168). Written from MESSENGER_BASELINE_V1.md ("Rich-list", "Voice-note" and "Static location-card"
//! semantics), the om-numeric-fields registry and ADR-0155 layering, not from the Python/Node harnesses.
//!
//! Layering (ADR-0155): unknown extension -> critical rejects the object / non-critical is ignored; registered wire
//! criticality mismatch -> reject_object; wrong owning object or payload-schema violation -> ignore_extension; semantic
//! failure -> ignore_extension; otherwise accept_extension.
//!
//! Usage: v005-extensions <registry om-numeric-fields.json> <adr0148 vectors> <adr0150 vectors>
//! Prints one JSON object: {vector_id: {"verdict", "error", "cbor_hex"}} for both corpora.

use serde_json::{json, Map, Value as J};
use std::collections::BTreeMap;

// ------------------------------------------------------------------ canonical CBOR (RFC 8949 §4.2.1)
#[derive(Clone, Debug)]
enum C {
    U(u64),
    N(u64), // negative: value = -1 - n
    B(Vec<u8>),
    T(String),
    A(Vec<C>),
    M(Vec<(C, C)>),
    Bool(bool),
}

fn head(major: u8, n: u64, out: &mut Vec<u8>) {
    let m = major << 5;
    if n < 24 {
        out.push(m | n as u8);
    } else if n <= 0xff {
        out.push(m | 24);
        out.push(n as u8);
    } else if n <= 0xffff {
        out.push(m | 25);
        out.extend_from_slice(&(n as u16).to_be_bytes());
    } else if n <= 0xffff_ffff {
        out.push(m | 26);
        out.extend_from_slice(&(n as u32).to_be_bytes());
    } else {
        out.push(m | 27);
        out.extend_from_slice(&n.to_be_bytes());
    }
}

fn enc(v: &C, out: &mut Vec<u8>) {
    match v {
        C::U(n) => head(0, *n, out),
        C::N(n) => head(1, *n, out),
        C::B(b) => {
            head(2, b.len() as u64, out);
            out.extend_from_slice(b)
        }
        C::T(s) => {
            head(3, s.len() as u64, out);
            out.extend_from_slice(s.as_bytes())
        }
        C::A(a) => {
            head(4, a.len() as u64, out);
            for x in a {
                enc(x, out)
            }
        }
        C::M(m) => {
            // deterministic: sort entries by the bytewise order of the encoded key
            let mut ents: Vec<(Vec<u8>, &C)> = m
                .iter()
                .map(|(k, v)| {
                    let mut kb = Vec::new();
                    enc(k, &mut kb);
                    (kb, v)
                })
                .collect();
            ents.sort_by(|a, b| a.0.cmp(&b.0));
            head(5, ents.len() as u64, out);
            for (kb, v) in ents {
                out.extend_from_slice(&kb);
                enc(v, out)
            }
        }
        C::Bool(b) => out.push(if *b { 0xf5 } else { 0xf4 }),
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{:02x}", x)).collect()
}

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap()).collect()
}

/// JSON test-vector notation -> CBOR value: {"$map": [[k, v], ...]}, {"$bytes": "hex"}, ints, strings, bools, arrays.
fn from_json(v: &J) -> C {
    match v {
        J::Bool(b) => C::Bool(*b),
        J::Number(n) => {
            if let Some(u) = n.as_u64() {
                C::U(u)
            } else {
                C::N((-1 - n.as_i64().unwrap()) as u64)
            }
        }
        J::String(s) => C::T(s.clone()),
        J::Array(a) => C::A(a.iter().map(from_json).collect()),
        J::Object(o) => {
            if let Some(J::Array(ps)) = o.get("$map") {
                C::M(ps.iter().map(|p| (from_json(&p[0]), from_json(&p[1]))).collect())
            } else if let Some(J::String(h)) = o.get("$bytes") {
                C::B(unhex(h))
            } else {
                panic!("unsupported vector notation: {v}")
            }
        }
        J::Null => panic!("null is never a valid OM value"),
    }
}

// ------------------------------------------------------------------ registry
struct Ext {
    object: String,
    critical: bool,
}

fn registry(path: &str) -> BTreeMap<String, Ext> {
    let r: J = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    r["extensions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e["name"].as_str().unwrap().to_string(),
                Ext { object: e["object"].as_str().unwrap().to_string(), critical: e["critical"].as_bool().unwrap() },
            )
        })
        .collect()
}

type Verdict = (&'static str, Option<&'static str>);
const OK: Verdict = ("accept_extension", None);
fn ignore(e: &'static str) -> Verdict {
    ("ignore_extension", Some(e))
}

/// Integer-keyed payload map -> key/value table, or the schema error.
fn int_map(p: &C) -> Result<BTreeMap<u64, C>, &'static str> {
    match p {
        C::M(m) => {
            let mut out = BTreeMap::new();
            for (k, v) in m {
                match k {
                    C::U(n) => {
                        out.insert(*n, v.clone());
                    }
                    _ => return Err("wrong_type"),
                }
            }
            Ok(out)
        }
        _ => Err("wrong_type"),
    }
}

fn uint(v: Option<&C>) -> Option<u64> {
    match v {
        Some(C::U(n)) => Some(*n),
        _ => None,
    }
}

// ------------------------------------------------------------------ voice note (VoiceNoteV1Extension)
fn voice(p: &BTreeMap<u64, C>, ctx: &Map<String, J>) -> Verdict {
    if p.keys().any(|k| *k > 1) {
        return ignore("unknown_field");
    }
    let idx = match p.get(&0) {
        None => return ignore("missing_field"),
        Some(C::U(n)) if *n <= 31 => *n,
        _ => return ignore("attachment_index_schema"),
    };
    if let Some(w) = p.get(&1) {
        match w {
            C::B(b) if (16..=256).contains(&b.len()) => {}
            _ => return ignore("waveform_bounds"),
        }
    }
    if ctx.get("event_kind").and_then(J::as_str).unwrap_or("message.create") != "message.create" {
        return ignore("event_kind");
    }
    let atts = ctx.get("attachments").and_then(J::as_array).cloned().unwrap_or_default();
    let Some(a) = atts.get(idx as usize) else { return ignore("attachment_index") };
    if !a.get("media_type").and_then(J::as_str).map_or(false, |m| m.starts_with("audio/")) {
        return ignore("audio_attachment");
    }
    if a.get("duration_ms").and_then(J::as_u64).is_none() {
        return ignore("duration_ms");
    }
    OK
}

// ------------------------------------------------------------------ location card (LocationCardV1Extension)
fn e7(x: i64) -> String {
    let sign = if x < 0 { "-" } else { "" };
    let a = x.unsigned_abs();
    format!("{sign}{}.{:07}", a / 10_000_000, a % 10_000_000)
}

/// Canonical-form geo tokens in `text` (MESSENGER_BASELINE_V1 as clarified by ADR-0168 F-17):
/// "geo:" [-] 1*DIGIT "." 7DIGIT "," [-] 1*DIGIT "." 7DIGIT, not preceded by an ASCII letter, digit or one of
/// "_.+-", and not followed by a digit or ".".
fn geo_tokens(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(off) = text[i..].find("geo:") {
        let s = i + off;
        i = s + 4;
        if s > 0 {
            let c = b[s - 1];
            if c.is_ascii_alphanumeric() || b"_.+-".contains(&c) {
                continue;
            }
        }
        let num = |mut j: usize| -> Option<usize> {
            if j < b.len() && b[j] == b'-' {
                j += 1;
            }
            let d0 = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j == d0 || j >= b.len() || b[j] != b'.' {
                return None;
            }
            j += 1;
            let f0 = j;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j - f0 != 7 {
                return None;
            }
            Some(j)
        };
        let Some(j) = num(s + 4) else { continue };
        if j >= b.len() || b[j] != b',' {
            continue;
        }
        let Some(k) = num(j + 1) else { continue };
        if k < b.len() && (b[k].is_ascii_digit() || b[k] == b'.') {
            continue;
        }
        out.push(text[s..k].to_string());
    }
    out
}

fn location(p: &BTreeMap<u64, C>, ctx: &Map<String, J>) -> Verdict {
    if p.keys().any(|k| *k > 3) {
        return ignore("unknown_field");
    }
    let (Some(lat), Some(lon)) = (p.get(&0), p.get(&1)) else { return ignore("missing_field") };
    let lat = match uint(Some(lat)) {
        Some(n) if n <= 1_800_000_000 => n as i64 - 900_000_000,
        _ => return ignore("lat_bounds"),
    };
    let lon = match uint(Some(lon)) {
        Some(n) if n <= 3_600_000_000 => n as i64 - 1_800_000_000,
        _ => return ignore("lon_bounds"),
    };
    if let Some(a) = p.get(&2) {
        if !matches!(uint(Some(a)), Some(n) if (1..=1_000_000).contains(&n)) {
            return ignore("accuracy_bounds");
        }
    }
    if let Some(l) = p.get(&3) {
        if !matches!(l, C::T(s) if (1..=256).contains(&s.len())) {
            return ignore("label_bounds");
        }
    }
    if ctx.get("event_kind").and_then(J::as_str).unwrap_or("message.create") != "message.create" {
        return ignore("event_kind");
    }
    // the fallback that is currently materialized (after an accepted edit: the winning replacement text)
    let text = ctx.get("fallback").and_then(J::as_str).unwrap_or("");
    let ours = format!("geo:{},{}", e7(lat), e7(lon));
    let toks = geo_tokens(text);
    if !toks.iter().any(|t| *t == ours) {
        return ignore("fallback_mismatch");
    }
    if toks.iter().any(|t| *t != ours) {
        return ignore("fallback_ambiguous");
    }
    OK
}

// ------------------------------------------------------------------ ADR-0148 corpus driver
fn adr0148(reg: &BTreeMap<String, Ext>, v: &J) -> (Verdict, String) {
    let o = v.as_object().unwrap();
    let id = o["extension_id"].as_str().unwrap();
    let wire_critical = o["critical"].as_bool().unwrap();
    let mut payload = from_json(&o["payload"]);
    if let Some(n) = o.get("repeat_waveform_bytes").and_then(J::as_u64) {
        if let C::M(ref mut m) = payload {
            for (k, val) in m.iter_mut() {
                if matches!(k, C::U(1)) {
                    if let C::B(b) = val {
                        *val = C::B(b.repeat(n as usize));
                    }
                }
            }
        }
    }
    let entry = C::M(vec![(C::T(id.into()), C::M(vec![(C::U(0), C::Bool(wire_critical)), (C::U(1), payload.clone())]))]);
    let mut bytes = Vec::new();
    enc(&entry, &mut bytes);
    let ctx = o.get("context").and_then(J::as_object).cloned().unwrap_or_default();
    let verdict = match reg.get(id) {
        None => {
            if wire_critical {
                ("reject_object", Some("unknown_critical_extension"))
            } else {
                ("accept_ignored", None)
            }
        }
        Some(e) if e.critical != wire_critical => ("reject_object", Some("criticality_mismatch")),
        Some(e) if o["owning_object"].as_str().unwrap() != e.object => ignore("wrong_extension_object"),
        Some(_) => match int_map(&payload) {
            Err(err) => ignore(err),
            Ok(p) => match id {
                "om.voice-note.v1" => voice(&p, &ctx),
                "om.location-card.v1" => location(&p, &ctx),
                _ => ("accept_ignored", None),
            },
        },
    };
    (verdict, hex(&bytes))
}

// ------------------------------------------------------------------ rich list (RichListV1Extension)
fn rich_list(reg: &BTreeMap<String, Ext>, v: &J) -> (Verdict, String) {
    let o = v.as_object().unwrap();
    let wire_critical = o["critical"].as_bool().unwrap();
    // build the wire payload from the vector description
    let empty = Map::new();
    let item_extra = o.get("item_extra").and_then(J::as_object).unwrap_or(&empty);
    let payload_extra = o.get("payload_extra").and_then(J::as_object).unwrap_or(&empty);
    let items = o["items"].as_array().unwrap();
    let mut pm: Vec<(C, C)> = Vec::new();
    if !o.get("missing_items").and_then(J::as_bool).unwrap_or(false) {
        let arr = items
            .iter()
            .map(|it| {
                let mut m = vec![
                    (C::U(0), from_json(&it["block_index"])),
                    (C::U(1), from_json(&it["depth"])),
                    (C::U(2), from_json(&it["style"])),
                ];
                if let Some(ord) = it.get("ordinal") {
                    m.push((C::U(3), from_json(ord)));
                }
                for (k, x) in item_extra {
                    m.push((C::U(k.parse().unwrap()), from_json(x)));
                }
                C::M(m)
            })
            .collect();
        pm.push((C::U(0), C::A(arr)));
    }
    for (k, x) in payload_extra {
        pm.push((C::U(k.parse().unwrap()), from_json(x)));
    }
    let payload = C::M(pm);
    let entry = C::M(vec![(
        C::T("om.rich-list.v1".into()),
        C::M(vec![(C::U(0), C::Bool(wire_critical)), (C::U(1), payload.clone())]),
    )]);
    let mut bytes = Vec::new();
    enc(&entry, &mut bytes);
    (rich_list_verdict(reg, o, &payload), hex(&bytes))
}

fn rich_list_verdict(reg: &BTreeMap<String, Ext>, o: &Map<String, J>, payload: &C) -> Verdict {
    let e = &reg["om.rich-list.v1"];
    if o["critical"].as_bool().unwrap() != e.critical {
        return ("reject_object", Some("criticality_mismatch"));
    }
    if o["owning_object"].as_str().unwrap() != e.object {
        return ignore("wrong_extension_object");
    }
    // ---- registered payload schema (whole enhancement ignored on any violation)
    let p = match int_map(payload) {
        Ok(p) => p,
        Err(err) => return ignore(err),
    };
    if p.keys().any(|k| *k != 0) {
        return ignore("unknown_field");
    }
    let Some(C::A(items)) = p.get(&0) else { return ignore("missing_field") };
    if !(1..=256).contains(&items.len()) {
        return ignore("item_count_schema");
    }
    let mut parsed: Vec<(u64, u64, u64, Option<u64>)> = Vec::new();
    for it in items {
        let m = match int_map(it) {
            Ok(m) => m,
            Err(err) => return ignore(err),
        };
        if m.keys().any(|k| *k > 3) {
            return ignore("unknown_field");
        }
        let bi = match uint(m.get(&0)) {
            Some(n) if n <= 4095 => n,
            _ => return ignore("block_index_schema"),
        };
        let depth = match uint(m.get(&1)) {
            Some(n) if n <= 3 => n,
            _ => return ignore("depth_schema"),
        };
        let style = match uint(m.get(&2)) {
            Some(n) if n == 1 || n == 2 => n,
            _ => return ignore("style_schema"),
        };
        let ordinal = match m.get(&3) {
            None => None,
            Some(x) => match uint(Some(x)) {
                Some(n) if (1..=9999).contains(&n) => Some(n),
                _ => return ignore("ordinal_schema"),
            },
        };
        parsed.push((bi, depth, style, ordinal));
    }
    // ---- semantic layer
    let kind = o.get("event_kind").and_then(J::as_str).unwrap_or("message.create");
    if kind != "message.create" && kind != "message.edit" {
        return ignore("event_kind");
    }
    if o.get("rendered_event").and_then(J::as_str).unwrap_or("self") != "self" {
        return ignore("annotation_not_for_rendered_text");
    }
    let block_count = o["block_count"].as_u64().unwrap();
    let overrides = o.get("block_overrides").and_then(J::as_object).cloned().unwrap_or_default();
    let mut prev: Option<(u64, u64)> = None;
    for &(bi, depth, style, ordinal) in &parsed {
        if let Some((pb, _)) = prev {
            if bi <= pb {
                return ignore("block_index_order");
            }
        }
        if bi >= block_count {
            return ignore("block_reference");
        }
        // canonical fallback marker: two spaces per depth, then "- " (unordered) or "<ordinal>. " (ordered)
        let marker = format!(
            "{}{}",
            "  ".repeat(depth as usize),
            if style == 1 { "- ".to_string() } else { format!("{}. ", ordinal.map_or(String::new(), |n| n.to_string())) }
        );
        let (bkind, btext, bmarked) = match overrides.get(&bi.to_string()) {
            Some(b) => (
                b["kind"].as_str().unwrap().to_string(),
                b["text"].as_str().unwrap_or("").to_string(),
                b["marked"].as_bool().unwrap_or(false),
            ),
            None => ("paragraph".into(), format!("{marker}item {bi}"), false),
        };
        if bkind != "paragraph" {
            return ignore("block_kind");
        }
        let starts_run = prev.map_or(true, |(pb, _)| bi != pb + 1);
        if starts_run && depth != 0 {
            return ignore("depth_run");
        }
        if let (false, Some((_, pd))) = (starts_run, prev) {
            if depth > pd + 1 {
                return ignore("depth_transition");
            }
        }
        if (style == 2) != ordinal.is_some() {
            return ignore("ordinal_semantics");
        }
        if bmarked || !btext.starts_with(&marker) {
            return ignore("fallback_prefix");
        }
        prev = Some((bi, depth));
    }
    OK
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let reg = registry(&args[1]);
    let mut out = Map::new();
    for (path, f) in [(&args[2], adr0148 as fn(&BTreeMap<String, Ext>, &J) -> (Verdict, String)), (&args[3], rich_list)] {
        let vs: J = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
        for v in vs.as_array().unwrap() {
            let ((verdict, error), cbor) = f(&reg, v);
            out.insert(v["id"].as_str().unwrap().into(), json!({"verdict": verdict, "error": error, "cbor_hex": cbor}));
        }
    }
    println!("{}", J::Object(out));
}
