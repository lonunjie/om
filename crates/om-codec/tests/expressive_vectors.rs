// SPDX-License-Identifier: MPL-2.0
//! Replays the ADR-0252 expressive-media wire vectors (`verification/vectors/expressive-wire-vectors.json.gz`) through the
//! product codec under the slot overlay (`Registry::load_w1_expressive`): verdict, exact error class, byte-identical
//! re-encoding, encode-from-value equality, the derived signature and hash inputs (the ContentPackReleaseID and the
//! ContentPackControlState signature inputs), and the W0 receiver view (no overlay). It then evaluates every semantic rule
//! the registry and the CDDL cannot express with a checker written in this file (the header-only claims and container
//! budget validator, event, release and control-state rules, lineage, pack_ref and the emission gate), which must agree
//! with the Python checker (`verification/prototypes/expressive-wire/checks.py`) on every case.
//! The two prototype codecs (Rust v005, Node) agree on the same file (expressive-wire/run.sh). Verification evidence only:
//! no production decode path loads the overlay registry.

use flate2::read::GzDecoder;
use om_codec::cbor::{Value, encode, from_diag, unhex};
use om_codec::derive::{derived, ds_hash};
use om_codec::schema::Registry;
use serde_json::Value as J;
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::io::Read;
use std::path::PathBuf;

fn repo(path: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(path)
}

fn expand_hex(h: &J) -> Vec<u8> {
    match h {
        J::String(s) => unhex(s),
        J::Array(parts) => {
            let mut out = Vec::new();
            for p in parts {
                match p {
                    J::String(s) => out.extend(unhex(s)),
                    J::Object(o) => {
                        let b = unhex(o["fill"].as_str().unwrap())[0];
                        out.extend(std::iter::repeat_n(b, o["len"].as_u64().unwrap() as usize));
                    }
                    _ => panic!("bad hex segment"),
                }
            }
            out
        }
        _ => panic!("bad hex"),
    }
}

fn corpus() -> J {
    let raw = std::fs::read(repo("verification/vectors/expressive-wire-vectors.json.gz"))
        .expect("vectors present");
    let mut s = String::new();
    GzDecoder::new(&raw[..]).read_to_string(&mut s).unwrap();
    serde_json::from_str(&s).unwrap()
}

fn media() -> J {
    serde_json::from_str(
        &std::fs::read_to_string(repo("spec/registries/om-expressive-media.json")).unwrap(),
    )
    .unwrap()
}

fn pick(x: &J, keys: &[&str]) -> Vec<(String, J)> {
    keys.iter()
        .filter_map(|k| x.get(*k).map(|v| ((*k).to_string(), v.clone())))
        .collect()
}

// ------------------------------------------------------------------ small Value helpers
fn field(v: &Value, k: u64) -> Option<&Value> {
    v.get(k)
}
fn uint(v: Option<&Value>) -> Option<u64> {
    match v {
        Some(Value::Uint(n)) => Some(*n),
        _ => None,
    }
}
fn text(v: Option<&Value>) -> Option<&str> {
    match v {
        Some(Value::Text(s)) => Some(s),
        _ => None,
    }
}
fn bytes(v: Option<&Value>) -> Option<&[u8]> {
    match v {
        Some(Value::Bytes(b)) => Some(b),
        _ => None,
    }
}
fn array(v: Option<&Value>) -> &[Value] {
    match v {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}
fn digest_value(v: Option<&Value>) -> Vec<u8> {
    bytes(v.and_then(|d| d.get(1))).unwrap().to_vec()
}
fn scalars(s: &str) -> usize {
    s.chars().count()
}

type Verdict = (String, String);
fn vd(v: &str, r: &str) -> Verdict {
    (v.to_string(), r.to_string())
}

// ------------------------------------------------------------------ the checker
struct Rules {
    m: J,
}

impl Rules {
    fn new() -> Rules {
        Rules { m: media() }
    }
    fn lim(&self, k: &str) -> usize {
        self.m["text_limits_scalar_values"][k].as_u64().unwrap() as usize
    }
    fn rp(&self, kind: u64) -> &J {
        let name = [
            "sticker",
            "custom_emoji",
            "reaction_asset",
            "loop_animation",
        ][kind as usize - 1];
        let id = self.m["kind_resource_profile"][name].as_str().unwrap();
        self.m["resource_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["id"] == id)
            .unwrap()
    }
    fn known_profile(&self, id: &str) -> bool {
        self.m["media_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == id)
    }

    // Test-only builder mirroring the claim fields one to one.
    #[allow(clippy::too_many_arguments)]
    fn claims(
        &self,
        kind: u64,
        profile: &str,
        media_type: &str,
        size: u64,
        w: Option<u64>,
        h: Option<u64>,
        d: Option<u64>,
    ) -> Verdict {
        if !self.known_profile(profile) {
            return vd("placeholder", "unknown_profile");
        }
        if media_type != "image/webp" {
            return vd("invalid", "media_type");
        }
        let rp = self.rp(kind);
        if size > rp["max_encoded_bytes"].as_u64().unwrap() {
            return vd("invalid", "size_over");
        }
        let (Some(w), Some(h)) = (w, h) else {
            return vd("invalid", "dimensions_missing");
        };
        let canvas = rp["max_canvas"].as_u64().unwrap();
        if w > canvas {
            return vd("invalid", "width_over");
        }
        if h > canvas {
            return vd("invalid", "height_over");
        }
        match d {
            None if rp["animated"] == "required" => return vd("invalid", "duration_required"),
            Some(d) if d > rp["max_duration_ms"].as_u64().unwrap() => {
                return vd("invalid", "duration_over");
            }
            _ => {}
        }
        vd("valid", "-")
    }

    fn texts(&self, alt: Option<&str>, fallback: Option<&str>, item: Option<&str>) -> Verdict {
        if alt.is_some_and(|s| scalars(s) > self.lim("alt_text")) {
            return vd("invalid", "alt_text_scalars");
        }
        if fallback.is_some_and(|s| scalars(s) > self.lim("fallback_text")) {
            return vd("invalid", "fallback_text_scalars");
        }
        if item.is_some_and(|s| scalars(s) > self.lim("item_id")) {
            return vd("invalid", "item_id_scalars");
        }
        vd("valid", "-")
    }

    fn asset(&self, a: &Value) -> Verdict {
        let item = field(a, 15).and_then(|p| text(p.get(2)));
        let t = self.texts(text(field(a, 13)), text(field(a, 14)), item);
        if t.0 != "valid" {
            return t;
        }
        self.claims(
            uint(field(a, 11)).unwrap(),
            text(field(a, 12)).unwrap(),
            text(field(a, 1)).unwrap(),
            uint(field(a, 2)).unwrap(),
            uint(field(a, 7)),
            uint(field(a, 8)),
            uint(field(a, 9)),
        )
    }

    fn inlines<'a>(&self, rich: &'a Value, out: &mut Vec<&'a Value>) {
        fn walk<'a>(i: &'a Value, out: &mut Vec<&'a Value>) {
            if uint(i.get(0)) == Some(4) {
                out.push(i);
            }
            for c in array(i.get(4)) {
                walk(c, out);
            }
        }
        for b in array(rich.get(1)) {
            if matches!(uint(b.get(0)), Some(1 | 2)) {
                for i in array(b.get(1)) {
                    walk(i, out);
                }
            }
        }
    }

    fn event(&self, ev: &Value) -> Verdict {
        let kind = uint(field(ev, 8)).unwrap();
        let body = field(ev, 11).unwrap();
        let caps: Vec<&str> = array(field(ev, 10))
            .iter()
            .filter_map(|c| match c {
                Value::Text(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        if kind == 4
            && let Some(a) = field(body, 3)
        {
            if field(body, 2) != Some(&Value::Bool(true)) {
                return vd("invalid", "reaction_present_false");
            }
            if !matches!(uint(field(a, 11)), Some(2 | 3)) {
                return vd("invalid", "reaction_kind");
            }
            if !caps.contains(&"om.expressive.reaction-asset.v1") {
                return vd("invalid", "reaction_capability");
            }
            let v = self.asset(a);
            if v.0 != "valid" {
                return v;
            }
        }
        let rich = match kind {
            1 => field(body, 0),
            2 => field(body, 1),
            _ => None,
        };
        if let Some(rich) = rich {
            let mut found = Vec::new();
            self.inlines(rich, &mut found);
            for x in found {
                let a = field(x, 6).unwrap();
                if uint(field(a, 11)) != Some(2) {
                    return vd("invalid", "inline_kind");
                }
                if !caps.contains(&"om.expressive.inline-emoji.v1") {
                    return vd("invalid", "inline_capability");
                }
                let v = self.asset(a);
                if v.0 != "valid" {
                    return v;
                }
            }
        }
        let Some(exts) = field(ev, 23) else {
            return vd("valid", "-");
        };
        let Some(entry) = exts.get_text("om.expressive.asset.v1") else {
            return vd("valid", "-");
        };
        let value = entry.get(1).unwrap();
        if kind != 1 {
            return vd("ignored", "ext_not_create");
        }
        let schema_ok = matches!(value, Value::Map(_))
            && uint(value.get(0)).is_some_and(|n| n <= 31)
            && uint(value.get(1)).is_some_and(|k| (1..=4).contains(&k))
            && text(value.get(2)).is_some_and(|s| (1..=64).contains(&s.len()))
            && text(value.get(3)).is_some_and(|s| (1..=2048).contains(&s.len()));
        if !schema_ok {
            return vd("ignored", "ext_schema");
        }
        let atts = array(field(body, 1));
        let Some(att) = atts.get(uint(value.get(0)).unwrap() as usize) else {
            return vd("ignored", "ext_index");
        };
        let t = self.texts(
            text(value.get(3)),
            text(value.get(4)),
            value.get(5).and_then(|p| text(p.get(2))),
        );
        if t.0 != "valid" {
            return vd("ignored", &t.1);
        }
        let v = self.claims(
            uint(value.get(1)).unwrap(),
            text(value.get(2)).unwrap(),
            text(att.get(1)).unwrap(),
            uint(att.get(2)).unwrap(),
            uint(att.get(7)),
            uint(att.get(8)),
            uint(att.get(9)),
        );
        if v.0 == "invalid" {
            vd("ignored", &v.1)
        } else {
            v
        }
    }

    fn release(&self, r: &Value) -> Verdict {
        let seq = uint(field(r, 3)).unwrap();
        if (seq == 1) != field(r, 4).is_none() {
            return vd("invalid", "sequence_previous");
        }
        if field(r, 11).is_some() && seq != 1 {
            return vd("invalid", "forked_from_sequence");
        }
        if scalars(text(field(r, 5)).unwrap()) > self.lim("title") {
            return vd("invalid", "title_scalars");
        }
        if text(field(r, 6)).is_some_and(|s| scalars(s) > self.lim("description")) {
            return vd("invalid", "description_scalars");
        }
        if matches!(field(r, 10), Some(Value::Map(m)) if m.is_empty()) {
            return vd("invalid", "licensing_empty");
        }
        let mut seen = HashSet::new();
        for it in array(field(r, 8)) {
            let id = text(it.get(0)).unwrap();
            if scalars(id) > self.lim("item_id") {
                return vd("invalid", "item_id_scalars");
            }
            if !seen.insert(id.to_string()) {
                return vd("invalid", "item_id_duplicate");
            }
            let a = it.get(2).unwrap();
            if uint(it.get(1)) != uint(a.get(11)) {
                return vd("invalid", "item_kind");
            }
            if [5u64, 10, 15, 16].iter().any(|k| a.get(*k).is_some()) {
                return vd("invalid", "asset_forbidden_key");
            }
            if scalars(text(it.get(5)).unwrap()) > self.lim("accessibility_label") {
                return vd("invalid", "label_scalars");
            }
            if array(it.get(4))
                .iter()
                .any(|k| matches!(k, Value::Text(s) if scalars(s) > self.lim("keyword")))
            {
                return vd("invalid", "keyword_scalars");
            }
            if array(it.get(3))
                .iter()
                .any(|k| matches!(k, Value::Text(s) if scalars(s) > self.lim("emoji_association")))
            {
                return vd("invalid", "emoji_scalars");
            }
            let v = self.asset(a);
            if v.0 == "invalid" {
                return v;
            }
        }
        vd("valid", "-")
    }
}

trait GetText {
    fn get_text(&self, k: &str) -> Option<&Value>;
}
impl GetText for Value {
    fn get_text(&self, k: &str) -> Option<&Value> {
        match self {
            Value::Map(m) => m
                .iter()
                .find(|(kk, _)| matches!(kk, Value::Text(s) if s == k))
                .map(|(_, v)| v),
            _ => None,
        }
    }
}

fn hash_of(label: &str, body: &[u8]) -> Vec<u8> {
    Sha256::digest(ds_hash(label, body)).to_vec()
}
fn release_id(release: &Value) -> Vec<u8> {
    hash_of("OM-CONTENT-PACK-RELEASE-HASH-v1", &encode(release))
}
fn key_id(profile: u64, public_key: &[u8]) -> Vec<u8> {
    let body = encode(&Value::Array(vec![
        Value::Uint(profile),
        Value::Bytes(public_key.to_vec()),
    ]));
    hash_of("OM-CONTENT-PACK-CONTROLLER-KEY-ID-v1", &body)[..16].to_vec()
}

fn state(s: &Value, sigs: &[(Vec<u8>, u64, u64)], parent: Option<&Value>) -> Verdict {
    let version = uint(field(s, 3)).unwrap();
    if (version == 1) != field(s, 4).is_none() {
        return vd("invalid", "version_genesis");
    }
    let ctrls = array(field(s, 5));
    let ids: Vec<&[u8]> = ctrls.iter().map(|c| bytes(c.get(0)).unwrap()).collect();
    let keys: Vec<&[u8]> = ctrls.iter().map(|c| bytes(c.get(2)).unwrap()).collect();
    if ids.iter().collect::<HashSet<_>>().len() != ids.len()
        || keys.iter().collect::<HashSet<_>>().len() != keys.len()
    {
        return vd("invalid", "controllers_duplicate");
    }
    if ctrls.iter().any(|c| {
        key_id(uint(c.get(1)).unwrap(), bytes(c.get(2)).unwrap()) != bytes(c.get(0)).unwrap()
    }) {
        return vd("invalid", "controller_key_id");
    }
    if uint(field(s, 6)).unwrap() as usize > ctrls.len() {
        return vd("invalid", "threshold");
    }
    let mut auth = ctrls;
    let mut threshold = uint(field(s, 6)).unwrap() as usize;
    if let Some(p) = parent {
        if version != uint(field(p, 3)).unwrap() + 1 {
            return vd("invalid", "version_step");
        }
        if field(s, 2) != field(p, 2) {
            return vd("invalid", "pack_id_changed");
        }
        if digest_value(field(s, 4)) != hash_of("OM-CONTENT-PACK-CONTROL-HASH-v1", &encode(p)) {
            return vd("invalid", "previous_state_hash");
        }
        let (ps, pr) = (
            uint(field(p, 8)).unwrap(),
            uint(field(p, 7).and(Some(&Value::Null)).or(None)).unwrap_or(0),
        );
        let _ = pr;
        let same = field(s, 7) == field(p, 7);
        let want = if same { ps } else { ps + 1 };
        if uint(field(s, 8)).unwrap() != want {
            return vd("invalid", "release_step");
        }
        auth = array(field(p, 5));
        threshold = uint(field(p, 6)).unwrap() as usize;
    }
    if sigs.iter().any(|(_, _, role)| *role != 1) {
        return vd("invalid", "signature_role");
    }
    let find = |id: &[u8]| auth.iter().find(|c| bytes(c.get(0)).unwrap() == id);
    if sigs.iter().any(|(id, _, _)| find(id).is_none()) {
        return vd("invalid", "signer_unknown");
    }
    if sigs
        .iter()
        .map(|(id, _, _)| id)
        .collect::<HashSet<_>>()
        .len()
        != sigs.len()
    {
        return vd("invalid", "signer_duplicate");
    }
    if sigs
        .iter()
        .any(|(id, prof, _)| uint(find(id).unwrap().get(1)).unwrap() != *prof)
    {
        return vd("invalid", "signature_profile");
    }
    if sigs.len() < threshold {
        return vd("invalid", "signature_threshold");
    }
    vd("valid", "-")
}

fn envelope_sigs(env: &Value) -> Vec<(Vec<u8>, u64, u64)> {
    array(env.get(2))
        .iter()
        .map(|s| {
            (
                bytes(s.get(0)).unwrap().to_vec(),
                uint(s.get(1)).unwrap(),
                uint(s.get(2)).unwrap(),
            )
        })
        .collect()
}

fn check_vd(id: &str, got: Verdict, v: &J, bad: &mut Vec<String>) {
    let want = (
        v["verdict"]
            .as_str()
            .unwrap_or(v["expect"].as_str().unwrap_or(""))
            .to_string(),
        v["rule"].as_str().unwrap_or("").to_string(),
    );
    if got != want {
        bad.push(format!("{id}: got {got:?}, want {want:?}"));
    }
}

// ------------------------------------------------------------------ tests
#[test]
fn object_vectors_replay_exactly_under_the_slot_overlay() {
    let reg = Registry::load_w1_expressive();
    let rules = Rules::new();
    let doc = corpus();
    let vectors = doc["vectors"].as_array().unwrap();
    assert!(
        vectors.len() >= 240,
        "corpus unexpectedly small: {}",
        vectors.len()
    );
    let mut by_id = std::collections::HashMap::new();
    let mut bad = Vec::new();
    let (mut accepted, mut rejected, mut semantic) = (0, 0, 0);
    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let name = v["object"].as_str().unwrap();
        let bytes_ = expand_hex(&v["hex"]);
        match (
            v["expect"].as_str().unwrap(),
            reg.check(name, &bytes_, false),
        ) {
            ("reject", Err(e)) => {
                rejected += 1;
                if Some(e) != v["error"].as_str() {
                    bad.push(format!("{id}: error {e} != {}", v["error"]));
                }
            }
            ("accept", Ok(val)) => {
                accepted += 1;
                if encode(&val) != bytes_ {
                    bad.push(format!("{id}: re-encode differs"));
                }
                if let Some(dv) = v.get("value")
                    && encode(&from_diag(dv)) != bytes_
                {
                    bad.push(format!("{id}: value does not encode to the bytes"));
                }
                let got: Vec<J> = derived(&reg, name, &val, &bytes_);
                let want = v["derived"].as_array().unwrap();
                let strip = |x: &J| pick(x, &["kind", "hex", "input_hex", "sha256"]);
                let (g, w): (Vec<_>, Vec<_>) = (
                    got.iter().map(strip).collect(),
                    want.iter().map(strip).collect(),
                );
                if g != w {
                    bad.push(format!("{id}: derived signature/hash inputs differ"));
                }
                by_id.insert(id.to_string(), val);
            }
            (want, got) => bad.push(format!(
                "{id}: expected {want}, got {:?}",
                got.map(|_| "accept")
            )),
        }
    }
    // semantic rules, on the values the product codec decoded from the bytes
    for v in vectors {
        let id = v["id"].as_str().unwrap();
        let Some(s) = v.get("semantic") else { continue };
        semantic += 1;
        let val = &by_id[id];
        let got = match v["object"].as_str().unwrap() {
            "ConversationEvent" => rules.event(val),
            "ContentPackRelease" => rules.release(val),
            _ => {
                let body = om_codec::cbor::decode(
                    bytes(val.get(1)).unwrap(),
                    om_codec::Limits::OM,
                    1 << 20,
                )
                .unwrap();
                let parent = v.get("parent").map(|p| {
                    let pv = &by_id[p.as_str().unwrap()];
                    om_codec::cbor::decode(bytes(pv.get(1)).unwrap(), om_codec::Limits::OM, 1 << 20)
                        .unwrap()
                });
                state(&body, &envelope_sigs(val), parent.as_ref())
            }
        };
        check_vd(id, got, s, &mut bad);
    }
    assert!(
        bad.is_empty(),
        "{} mismatches, first: {:#?}",
        bad.len(),
        &bad[..bad.len().min(10)]
    );
    assert!(
        accepted >= 90 && rejected >= 140 && semantic >= 90,
        "accepted {accepted} rejected {rejected} semantic {semantic}"
    );
}

/// A W0 receiver (the frozen registry without the overlay) refuses ReactionSet key 3 and inline type 4 and accepts the
/// non-critical extension and every W0-valid body, with exactly the error classes the vectors name.
#[test]
fn w0_receivers_refuse_the_slots() {
    let reg = Registry::load();
    let doc = corpus();
    let mut bad = Vec::new();
    let (mut refused, mut accepted) = (0, 0);
    for v in doc["vectors"].as_array().unwrap() {
        let Some(want) = v.get("w0_expect").and_then(|x| x.as_str()) else {
            continue;
        };
        let id = v["id"].as_str().unwrap();
        let b = expand_hex(&v["hex"]);
        match (want, reg.check("ConversationEvent", &b, false)) {
            ("reject", Err(e)) => {
                refused += 1;
                if Some(e) != v["w0_error"].as_str() {
                    bad.push(format!("{id}: W0 error {e} != {}", v["w0_error"]));
                }
            }
            ("accept", Ok(val)) => {
                accepted += 1;
                if encode(&val) != b {
                    bad.push(format!("{id}: W0 re-encode differs"));
                }
            }
            (w, got) => bad.push(format!(
                "{id}: W0 expected {w}, got {:?}",
                got.map(|_| "accept")
            )),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(
        refused >= 30 && accepted >= 10,
        "refused {refused} accepted {accepted}"
    );
}

#[test]
fn struct_vectors_replay_exactly() {
    let reg = Registry::load_w1_expressive();
    let doc = corpus();
    let structs = doc["structs"].as_array().unwrap();
    assert!(structs.len() >= 50, "only {} struct vectors", structs.len());
    let mut bad = Vec::new();
    for v in structs {
        let id = v["id"].as_str().unwrap();
        let name = v["struct"].as_str().unwrap();
        let b = expand_hex(&v["hex"]);
        let got = reg.check_struct(name, &b, 32_768).and_then(|val| {
            // the union discriminator: InlineCustomEmoji is the Inline variant with type 4 (a standalone struct check only
            // sees the shared inline_type enum, where 3 is a W0 value)
            if name == "InlineCustomEmoji" && uint(val.get(0)) != Some(4) {
                Err("value_range")
            } else {
                Ok(val)
            }
        });
        match (v["expect"].as_str().unwrap(), got) {
            ("reject", Err(e)) => {
                if Some(e) != v["error"].as_str() {
                    bad.push(format!("{id}: error {e} != {}", v["error"]));
                }
            }
            ("accept", Ok(val)) => {
                if encode(&val) != b {
                    bad.push(format!("{id}: re-encode differs"));
                }
            }
            (want, got) => bad.push(format!(
                "{id}: expected {want}, got {:?}",
                got.map(|_| "accept")
            )),
        }
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// The header-only claims validator: every profile and limit, exactly at and one over.
#[test]
fn claims_table_agrees() {
    let rules = Rules::new();
    let doc = corpus();
    let mut bad = Vec::new();
    for r in doc["claims"].as_array().unwrap() {
        let o = |k: &str| r[k].as_u64();
        let got = rules.claims(
            o("kind").unwrap(),
            r["media_profile_id"].as_str().unwrap(),
            r["media_type"].as_str().unwrap(),
            o("plaintext_size").unwrap(),
            o("width"),
            o("height"),
            o("duration_ms"),
        );
        check_vd(r["id"].as_str().unwrap(), got, r, &mut bad);
    }
    assert!(bad.is_empty(), "{bad:#?}");
    assert!(doc["claims"].as_array().unwrap().len() >= 35);
}

/// The container-fact budget (ADR-0251): reject on the first violation, from header facts alone.
#[test]
fn container_budget_table_agrees() {
    let rules = Rules::new();
    let m = &rules.m;
    let doc = corpus();
    let mut bad = Vec::new();
    for r in doc["container"].as_array().unwrap() {
        let f = &r["facts"];
        let res = r["resource_profile"].as_str().unwrap();
        let rp = m["resource_profiles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["id"] == res)
            .unwrap();
        let b = &m["budget"]["profiles"][res];
        let u = |j: &J| j.as_u64().unwrap();
        let (cw, ch) = (u(&f["canvas_w"]), u(&f["canvas_h"]));
        let frames: Vec<[u64; 5]> = f["frames"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                let a = x.as_array().unwrap();
                [u(&a[0]), u(&a[1]), u(&a[2]), u(&a[3]), u(&a[4])]
            })
            .collect();
        let got = (|| -> Verdict {
            if cw == 0 || ch == 0 || frames.is_empty() {
                return vd("invalid", "header_unstated");
            }
            if u(&f["encoded_size"]) > u(&rp["max_encoded_bytes"]) {
                return vd("invalid", "encoded_size");
            }
            match cw
                .checked_mul(ch)
                .and_then(|x| x.checked_mul(frames.len() as u64))
            {
                Some(x) if x <= u(&m["budget"]["multiplication_limit"]) => {}
                _ => return vd("invalid", "overflow"),
            }
            if cw > u(&rp["max_canvas"]) || ch > u(&rp["max_canvas"]) {
                return vd("invalid", "canvas");
            }
            if frames.len() as u64 > u(&b["max_frames"]) {
                return vd("invalid", "frames");
            }
            if frames
                .iter()
                .any(|[x, y, w, h, _]| *w == 0 || *h == 0 || x + w > cw || y + h > ch)
            {
                return vd("invalid", "frame_rect");
            }
            if frames
                .iter()
                .any(|[_, _, w, h, _]| w * h > u(&b["max_pixels_per_frame"]))
            {
                return vd("invalid", "pixels_per_frame");
            }
            if frames.iter().map(|[_, _, w, h, _]| w * h).sum::<u64>() > u(&b["max_total_pixels"]) {
                return vd("invalid", "total_pixels");
            }
            let total: u64 = frames.iter().map(|x| x[4]).sum();
            if total > u(&rp["max_duration_ms"]) {
                return vd("invalid", "duration");
            }
            if frames.len() > 1 && frames.len() as u64 * 1000 > u(&rp["max_fps"]) * total {
                return vd("invalid", "frame_rate");
            }
            if rp["animated"] == "required" && frames.len() < 2 {
                return vd("invalid", "animation_required");
            }
            vd("valid", "-")
        })();
        check_vd(r["id"].as_str().unwrap(), got, r, &mut bad);
    }
    assert!(bad.is_empty(), "{bad:#?}");
    // the budget numbers are the ADR-0251 figures, derived from the design ceilings and never above them
    for (res, frames, per, total) in [
        ("sticker.v1", 90u64, 262_144u64, 23_592_960u64),
        ("custom-emoji.v1", 90, 65_536, 5_898_240),
        ("loop-animation.v1", 300, 1_048_576, 314_572_800),
    ] {
        let b = &m["budget"]["profiles"][res];
        assert_eq!(
            (
                b["max_frames"].as_u64(),
                b["max_pixels_per_frame"].as_u64(),
                b["max_total_pixels"].as_u64()
            ),
            (Some(frames), Some(per), Some(total))
        );
        assert_eq!(frames * per, total);
    }
    assert_eq!(
        m["budget"]["resident_decode_ceiling_bytes"],
        64 * 1024 * 1024
    );
    assert_eq!(m["budget"]["decode_time_limit_ms"], 5000);
}

#[test]
fn lineage_pack_ref_and_emission_gate_agree() {
    let doc = corpus();
    let mut bad = Vec::new();
    let val = |j: &J| from_diag(j);
    for r in doc["lineage"].as_array().unwrap() {
        let id = r["id"].as_str().unwrap();
        let got = if r["kind"] == "release_link" {
            let (p, n) = (val(&r["previous"]), val(&r["release"]));
            if field(&p, 2) != field(&n, 2) {
                vd("invalid", "pack_id_changed")
            } else if uint(field(&n, 3)) != uint(field(&p, 3)).map(|x| x + 1) {
                vd("invalid", "sequence_skip")
            } else if digest_value(field(&n, 4)) != release_id(&p) {
                vd("invalid", "previous_release_mismatch")
            } else {
                vd("valid", "-")
            }
        } else {
            let (s, rel) = (val(&r["state"]), val(&r["release"]));
            if field(&s, 2) != field(&rel, 2) {
                vd("invalid", "pack_id_changed")
            } else if digest_value(field(&s, 7)) != release_id(&rel) {
                vd("invalid", "release_id_mismatch")
            } else if field(&s, 8) != field(&rel, 3) {
                vd("invalid", "release_sequence_mismatch")
            } else {
                vd("valid", "-")
            }
        };
        check_vd(id, got, r, &mut bad);
    }
    for r in doc["pack_ref"].as_array().unwrap() {
        let (a, rel) = (val(&r["asset"]), val(&r["release"]));
        let pr = field(&a, 15).unwrap();
        let got = if field(pr, 0) != field(&rel, 2) {
            vd("unattributed", "pack_id")
        } else if digest_value(field(pr, 1)) != release_id(&rel) {
            vd("unattributed", "release_id")
        } else if let Some(it) = array(field(&rel, 8))
            .iter()
            .find(|i| field(i, 0) == field(pr, 2))
        {
            let ia = field(it, 2).unwrap();
            if [0u64, 1, 2, 3, 4, 7, 8, 9, 11, 12]
                .iter()
                .any(|k| field(ia, *k) != field(&a, *k))
            {
                vd("unattributed", "asset_differs")
            } else {
                vd("attributed", "-")
            }
        } else {
            vd("unattributed", "item_missing")
        };
        check_vd(r["id"].as_str().unwrap(), got, r, &mut bad);
    }
    for r in doc["emit_gate"].as_array().unwrap() {
        let (slot, profile) = (
            r["slot_capability"].as_str().unwrap(),
            r["media_profile_id"].as_str().unwrap(),
        );
        let devices = r["devices"].as_array().unwrap();
        let got = if devices.is_empty() {
            vd("fallback", "no_recipient_devices")
        } else if devices.iter().any(|d| {
            let caps: Vec<&str> = d
                .as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap())
                .collect();
            !caps.contains(&slot) || !caps.contains(&profile)
        }) {
            vd("fallback", "device_lacks_capability")
        } else {
            vd("emit", "-")
        };
        check_vd(r["id"].as_str().unwrap(), got, r, &mut bad);
    }
    assert!(bad.is_empty(), "{bad:#?}");
}

/// No field the freeze introduces carries an AccountID or a delivery capability. The inherited attachment-ref keys 5
/// (locations) and 10 (repair_capability) stay available in a message, where they are ordinary attachment fields, and are
/// refused in a ContentPackRelease asset by the semantic rules above (asset_forbidden_key).
#[test]
fn no_new_field_carries_an_account_or_a_delivery_capability() {
    let f: J = serde_json::from_str(om_codec::schema::FIELDS_JSON).unwrap();
    let structs = f["structs"].as_object().unwrap();
    let new = [
        "ExpressivePackRef",
        "ExpressiveAssetRef",
        "InlineCustomEmoji",
        "ExpressiveAssetV1Extension",
        "ContentPackItem",
        "ContentPackProvenance",
        "ContentPackLicensing",
        "ContentPackRelease",
        "PackController",
        "ContentPackControlState",
    ];
    let forbidden = [
        "account",
        "principal",
        "capability",
        "route",
        "relay",
        "device",
        "mailbox",
        "delivery",
        "endpoint",
    ];
    for name in new {
        for fld in structs[name]["fields"].as_array().unwrap() {
            let (key, fname) = (fld["key"].as_u64().unwrap(), fld["name"].as_str().unwrap());
            let inherited = name == "ExpressiveAssetRef" && key <= 10;
            if inherited {
                continue;
            }
            assert!(
                !forbidden.iter().any(|w| fname.contains(w)),
                "{name}.{fname} looks like an identity or capability field"
            );
            let ty = fld["type"].to_string();
            assert!(
                !ty.contains("PrincipalRef"),
                "{name}.{fname} refers to a principal"
            );
            assert!(
                !ty.contains("DeviceCredential"),
                "{name}.{fname} refers to a device credential"
            );
        }
    }
    // and the CDDL names none of them in the new rules
    let cddl = std::fs::read_to_string(repo("spec/cddl/expressive.cddl")).unwrap();
    let rules: String = cddl
        .lines()
        .filter(|l| !l.trim_start().starts_with(';'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        !rules.contains("account-id") && !rules.contains("device-id"),
        "expressive.cddl names an AccountID or a DeviceID"
    );
    // every accepted ContentPackRelease asset in the corpus is free of the inherited delivery keys
    let reg = Registry::load_w1_expressive();
    let doc = corpus();
    let mut checked = 0;
    for v in doc["vectors"].as_array().unwrap() {
        let is_clean = v["expect"] == "accept"
            && v["object"] == "ContentPackRelease"
            && v["semantic"]["verdict"] == "valid";
        if !is_clean {
            continue;
        }
        let val = reg
            .check("ContentPackRelease", &expand_hex(&v["hex"]), false)
            .unwrap();
        for it in array(val.get(8)) {
            let a = it.get(2).unwrap();
            assert!(
                [5u64, 10, 15, 16].iter().all(|k| a.get(*k).is_none()),
                "{}",
                v["id"]
            );
            checked += 1;
        }
    }
    assert!(checked >= 500, "only {checked} release assets checked");
}

/// The release object bound (2 MiB) is exact on the product codec; one byte over is refused before any field is read.
#[test]
fn release_size_boundary_on_the_product_codec() {
    let reg = Registry::load_w1_expressive();
    let max = 2_097_152usize;
    let base = |pads: &[usize]| {
        let ext: Vec<J> = pads.iter().enumerate().map(|(i, n)| serde_json::json!([format!("org.example.padding-{i:02}"), {"$map": [[0, false], [1, {"$bytes": "00".repeat(*n)}]]}])).collect();
        encode(&from_diag(&serde_json::json!({"$map": [
            [0, 45], [1, 1], [2, {"$bytes": "11".repeat(32)}], [3, 1], [5, "T"], [7, [1]],
            [8, [{"$map": [[0, "a"], [1, 1], [2, {"$map": [
                [0, {"$bytes": "22".repeat(32)}], [1, "image/webp"], [2, 10], [3, {"$map": [[0, 1], [1, {"$bytes": "33".repeat(32)}]]}],
                [4, {"$map": [[0, 1], [1, {"$bytes": "44".repeat(32)}]]}], [7, 8], [8, 8], [11, 1], [12, "om.expressive.webp-v1"], [13, "x"]]}], [5, "a"]]}]],
            [9, {"$map": []}], [23, {"$map": ext}]]})))
    };
    let chunk = 60_000usize;
    let mut pads = vec![chunk; 35];
    // largest final pad that fits, then spend the remaining bytes on a fixed-width pad (its header never changes)
    let (mut lo, mut hi) = (0usize, chunk);
    while lo < hi {
        let mid = (lo + hi).div_ceil(2);
        *pads.last_mut().unwrap() = mid;
        if base(&pads).len() <= max {
            lo = mid
        } else {
            hi = mid - 1
        }
    }
    *pads.last_mut().unwrap() = lo;
    let deficit = max - base(&pads).len();
    pads[0] += deficit;
    let exact = base(&pads);
    let over = {
        let mut p = pads.clone();
        *p.last_mut().unwrap() += 1;
        base(&p)
    };
    assert_eq!(exact.len(), max, "could not size the object exactly");
    assert!(reg.check("ContentPackRelease", &exact, false).is_ok());
    assert_eq!(
        reg.check("ContentPackRelease", &over, false)
            .map(|_: Value| ()),
        Err("size_limit")
    );
}

/// Unassigned and held keys of every new top-level object are `unknown_field`: key 12 (the time field the freeze does not
/// carry) to 22 on a release, 9 to 22 on a control state.
#[test]
fn held_keys_are_refused() {
    let reg = Registry::load_w1_expressive();
    let doc = corpus();
    let min = doc["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["id"] == "ContentPackRelease/minimal")
        .unwrap();
    let val = reg
        .check("ContentPackRelease", &expand_hex(&min["hex"]), false)
        .unwrap();
    for key in 12u64..=22 {
        let Value::Map(mut m) = val.clone() else {
            panic!("map")
        };
        m.push((Value::Uint(key), Value::Uint(1_790_208_000)));
        m.sort_by_key(|a| encode(&a.0));
        assert_eq!(
            reg.check("ContentPackRelease", &encode(&Value::Map(m)), false)
                .map(|_: Value| ()),
            Err("unknown_field"),
            "release key {key}"
        );
    }
}

#[test]
fn scoped_expressive_grammar_keeps_other_inline_sites_at_w0() {
    let doc = corpus();
    let bytes = |section: &str, id: &str| {
        let row = doc[section]
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["id"] == id)
            .unwrap();
        expand_hex(&row["hex"])
    };
    let loose = Registry::load_w1_expressive();
    let scoped = Registry::load_w1_expressive_scoped();
    let create = bytes("vectors", "ConversationEvent/create-inline-custom-emoji");
    let event = scoped.check("ConversationEvent", &create, false).unwrap();
    let rich = event.get(11).unwrap().get(0).unwrap().clone();
    let inline = array(array(rich.get(1))[0].get(1))
        .iter()
        .find(|node| node.get(0) == Some(&Value::Uint(4)))
        .unwrap()
        .clone();

    fn set(value: &mut Value, key: u64, replacement: Value) {
        let Value::Map(fields) = value else {
            panic!("expected map")
        };
        if let Some((_, value)) = fields.iter_mut().find(|(k, _)| *k == Value::Uint(key)) {
            *value = replacement;
        } else {
            fields.push((Value::Uint(key), replacement));
        }
    }

    let w0: J = serde_json::from_reader(GzDecoder::new(
        &include_bytes!("../../../verification/vectors/v005-w0-object-vectors.json.gz")[..],
    ))
    .unwrap();
    let row = w0["vectors"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "MessageRequestPayload/minimal")
        .unwrap();
    let mut greeting = from_diag(&row["body_value"]);
    assert!(
        scoped
            .check_body("MessageRequestPayload", &encode(&greeting))
            .is_ok()
    );
    set(&mut greeting, 6, rich.clone());
    assert_eq!(
        scoped.check_body("MessageRequestPayload", &encode(&greeting)),
        Err("value_range"),
    );

    let entry = |key, value| (Value::Uint(key), value);
    let option = |id, label: &str| {
        Value::Map(vec![
            entry(0, Value::Uint(id)),
            entry(1, Value::Text(label.into())),
        ])
    };
    let poll = Value::Map(vec![
        entry(0, rich.clone()),
        entry(1, Value::Array(vec![option(0, "Yes"), option(1, "No")])),
        entry(2, Value::Uint(1)),
        entry(4, Value::Uint(1)),
    ]);
    assert_eq!(
        scoped.check_struct("PollCreate", &encode(&poll), 65536),
        Err("value_range")
    );
    let mut poll_event = event.clone();
    set(&mut poll_event, 8, Value::Uint(8));
    set(&mut poll_event, 11, poll);
    assert_eq!(
        scoped.check_body("ConversationEvent", &encode(&poll_event)),
        Err("value_range")
    );

    for (shape, value) in [("Inline", inline.clone()), ("RichText", rich.clone())] {
        assert!(loose.check_struct(shape, &encode(&value), 65536).is_ok());
        assert_eq!(
            scoped.check_struct(shape, &encode(&value), 65536),
            Err("value_range")
        );
    }

    let layer = Value::Map(vec![
        entry(0, Value::Uint(2)),
        entry(3, Value::Text("https://example.invalid".into())),
        entry(4, Value::Array(vec![inline])),
    ]);
    let mut nested = rich;
    set(
        &mut nested,
        1,
        Value::Array(vec![Value::Map(vec![
            entry(0, Value::Uint(1)),
            entry(1, Value::Array(vec![layer])),
        ])]),
    );
    let mut edit = event;
    set(&mut edit, 8, Value::Uint(2));
    set(
        &mut edit,
        11,
        Value::Map(vec![entry(0, Value::Bytes(vec![1; 16])), entry(1, nested)]),
    );
    assert!(
        scoped
            .check_body("ConversationEvent", &encode(&edit))
            .is_ok()
    );
}
