//! Feasibility prototype for V-005. Not product code. Not for production.
//!
//! Registry-driven W0 object validator. The rules come from
//! spec/registries/om-numeric-fields.json ("rules"), om-object-types.json and
//! om-cbor-profile.json. Walk order: a struct's entries in encoded order
//! (unknown key -> unknown_field, else validate the value), then required keys
//! in ascending order (missing_field). Arrays/sets: count, elements, then set order.

use crate::cbor::{decode, encode, Class, Limits, Value};
use serde_json::Value as J;
use std::collections::HashMap;

pub const FIELDS_JSON: &str = include_str!("../../../../../spec/registries/om-numeric-fields.json");
pub const TYPES_JSON: &str = include_str!("../../../../../spec/registries/om-object-types.json");
pub const PROFILE_JSON: &str = include_str!("../../../../../spec/registries/om-cbor-profile.json");
pub const CRYPTO_JSON: &str = include_str!("../../../../../spec/registries/om-crypto-profiles.json");

#[derive(Debug, Clone)]
pub enum Ty {
    Uint(u64, u64),
    Bool(Option<bool>),
    Bstr(u64, u64),
    Tstr(u64, u64),
    Enum(String),
    Ref(String),
    Array(Box<Ty>, u64, u64, bool),
    Dispatch { on: String, table: HashMap<u64, String> },
    Signed(String),
    Any,
    Ext,
}

#[derive(Debug, Clone)]
pub struct Field {
    pub key: u64,
    pub name: String,
    pub ty: Ty,
    pub req: bool,
}

#[derive(Debug, Clone)]
pub enum Struct {
    Fields { top: bool, fields: Vec<Field> },
    Union { on: u64, variants: HashMap<u64, String> },
}

#[derive(Debug, Clone)]
pub struct ObjType {
    pub id: u64,
    pub name: String,
    pub sig_label: Option<String>,
    pub hash_label: Option<String>,
    pub max_bytes: usize,
}

pub struct Registry {
    pub structs: HashMap<String, Struct>,
    pub enums: HashMap<String, Vec<u64>>,
    pub types: HashMap<String, ObjType>,
    pub known_ext: Vec<String>,
    /// registered key-23 extension -> (critical flag, value struct) (om-numeric-fields "extensions")
    pub ext_schema: HashMap<String, (bool, String)>,
    /// CryptoProfile id -> AsyncKeyBundle field keys that profile requires (required_bundle_fields)
    pub bundle_required: HashMap<u64, Vec<u64>>,
    /// CryptoProfile id -> exact (hpke_enc, request-key public_key) lengths of its pinned MessageRequest HPKE suite
    pub hpke_sizes: HashMap<u64, (usize, usize)>,
    /// CryptoProfile id -> exact (hpke_enc, delivery public_key, ciphertext lengths) for sealed delivery.
    pub sealed_hpke: HashMap<u64, (usize, usize, Vec<usize>)>,
    pub lim: Limits,
}

fn range(j: &J) -> (u64, u64) {
    (j[0].as_u64().unwrap(), j[1].as_u64().unwrap_or(u64::MAX))
}

fn parse_ty(j: &J) -> Ty {
    let o = j.as_object().expect("type object");
    if let Some(r) = o.get("uint") {
        let (a, b) = range(r);
        return Ty::Uint(a, b);
    }
    if let Some(b) = o.get("bool") {
        return Ty::Bool(b.as_bool());
    }
    if let Some(r) = o.get("bstr") {
        let (a, b) = range(r);
        return Ty::Bstr(a, b);
    }
    if let Some(r) = o.get("tstr") {
        let (a, b) = range(r);
        return Ty::Tstr(a, b);
    }
    if let Some(e) = o.get("enum") {
        return Ty::Enum(e.as_str().unwrap().into());
    }
    if let Some(r) = o.get("ref") {
        return Ty::Ref(r.as_str().unwrap().into());
    }
    for (k, set) in [("array", false), ("set", true)] {
        if let Some(inner) = o.get(k) {
            let (a, b) = range(&o["n"]);
            return Ty::Array(Box::new(parse_ty(inner)), a, b, set);
        }
    }
    if let Some(d) = o.get("dispatch") {
        let table = d["table"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), v.as_str().unwrap().to_string())).collect();
        return Ty::Dispatch { on: d["on"].as_str().unwrap().into(), table };
    }
    if let Some(s) = o.get("signed") {
        return Ty::Signed(s.as_str().unwrap().into());
    }
    if o.contains_key("any") {
        return Ty::Any;
    }
    if o.contains_key("ext") {
        return Ty::Ext;
    }
    panic!("unknown registry type {j}")
}

fn parse_fields(a: &J) -> Vec<Field> {
    a.as_array()
        .unwrap()
        .iter()
        .map(|f| Field { key: f["key"].as_u64().unwrap(), name: f["name"].as_str().unwrap().into(), ty: parse_ty(&f["type"]), req: f["req"].as_bool().unwrap() })
        .collect()
}

impl Registry {
    pub fn load() -> Registry {
        let f: J = serde_json::from_str(FIELDS_JSON).unwrap();
        let t: J = serde_json::from_str(TYPES_JSON).unwrap();
        let p: J = serde_json::from_str(PROFILE_JSON).unwrap();
        let common = parse_fields(&f["common_top_level_fields"]);
        let mut structs = HashMap::new();
        for (name, s) in f["structs"].as_object().unwrap() {
            if let Some(u) = s.get("union") {
                let variants = u["variants"].as_object().unwrap().iter().map(|(k, v)| (k.parse().unwrap(), v.as_str().unwrap().to_string())).collect();
                structs.insert(name.clone(), Struct::Union { on: u["on"].as_u64().unwrap(), variants });
            } else {
                let top = s.get("top_level").and_then(|x| x.as_bool()).unwrap_or(false);
                let mut fields = parse_fields(&s["fields"]);
                if top {
                    fields.extend(common.iter().cloned());
                }
                fields.sort_by_key(|x| x.key);
                structs.insert(name.clone(), Struct::Fields { top, fields });
            }
        }
        let enums = f["enums"].as_object().unwrap().iter().map(|(k, v)| (k.clone(), v.as_object().unwrap().keys().map(|x| x.parse().unwrap()).collect())).collect();
        let types = t["types"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| {
                let name = x["name"].as_str().unwrap().to_string();
                (
                    name.clone(),
                    ObjType {
                        id: x["type_id"].as_u64().unwrap(),
                        name,
                        sig_label: x["signed"].get("label").and_then(|l| l.as_str()).map(String::from),
                        hash_label: x["hash"].as_str().map(String::from),
                        max_bytes: x["max_bytes"].as_u64().unwrap() as usize,
                    },
                )
            })
            .collect();
        let l = &p["limits"];
        let lim = Limits {
            max_depth: l["max_depth"].as_u64().unwrap() as usize,
            max_items: l["max_container_items"].as_u64().unwrap(),
            max_string: l["max_string_bytes"].as_u64().unwrap(),
            lenient: false,
        };
        let mut known_ext: Vec<String> = p["extensions"]["known_extensions"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect();
        // registered key-23 extensions (om-numeric-fields "extensions", ADR-0126)
        if let Some(ex) = f.get("extensions").and_then(|x| x.as_array()) {
            known_ext.extend(ex.iter().map(|e| e["name"].as_str().unwrap().to_string()));
        }
        let c: J = serde_json::from_str(CRYPTO_JSON).unwrap();
        let akb = f["structs"]["AsyncKeyBundle"]["fields"].as_array().unwrap();
        let mut bundle_required = HashMap::new();
        for prof in c["profiles"].as_array().unwrap() {
            if let Some(req) = prof.get("required_bundle_fields").and_then(|x| x.as_array()) {
                let keys = req
                    .iter()
                    .map(|n| akb.iter().find(|f| f["name"] == *n).unwrap()["key"].as_u64().unwrap())
                    .collect();
                bundle_required.insert(prof["id"].as_u64().unwrap(), keys);
            }
        }
        let mut hpke_sizes = HashMap::new();
        for prof in c["profiles"].as_array().unwrap() {
            let h = &prof["message_request_hpke"];
            if let (Some(e), Some(k)) = (h["enc_bytes"].as_u64(), h["public_key_bytes"].as_u64()) {
                hpke_sizes.insert(prof["id"].as_u64().unwrap(), (e as usize, k as usize));
            }
        }
        let mut sealed_hpke = HashMap::new();
        for prof in c["profiles"].as_array().unwrap() {
            let h = &prof["sealed_delivery_hpke"];
            if let (Some(e), Some(k), Some(buckets), Some(tag)) =
                (h["enc_bytes"].as_u64(), h["public_key_bytes"].as_u64(), h["plaintext_buckets"].as_array(), h["aead_tag_bytes"].as_u64())
            {
                let ciphertexts = buckets.iter().map(|b| b.as_u64().unwrap() as usize + tag as usize).collect();
                sealed_hpke.insert(prof["id"].as_u64().unwrap(), (e as usize, k as usize, ciphertexts));
            }
        }
        let mut ext_schema = HashMap::new();
        for e in f.get("extensions").and_then(|x| x.as_array()).into_iter().flatten() {
            if let Some(r) = e["value"]["ref"].as_str() {
                ext_schema.insert(e["name"].as_str().unwrap().to_string(), (e["critical"].as_bool().unwrap_or(false), r.to_string()));
            }
        }
        Registry { structs, enums, types, known_ext, ext_schema, bundle_required, hpke_sizes, sealed_hpke, lim }
    }

    /// Decode + validate a top-level object of the declared type (SignedEnvelope for signed types).
    pub fn check(&self, type_name: &str, bytes: &[u8], lenient: bool) -> Result<Value, Class> {
        let t = &self.types[type_name];
        let mut lim = self.lim;
        lim.lenient = lenient;
        let v = decode(bytes, lim, t.max_bytes)?;
        let cx = Cx { r: self, lenient };
        if t.sig_label.is_some() {
            cx.envelope(&v, t)?;
        } else {
            cx.body(&v, t)?;
        }
        Ok(v)
    }
}

struct Cx<'a> {
    r: &'a Registry,
    lenient: bool,
}

impl<'a> Cx<'a> {
    fn envelope(&self, v: &Value, t: &ObjType) -> Result<(), Class> {
        let m = match v {
            Value::Map(m) => m,
            Value::Null => return Err("null_forbidden"),
            _ => return Err("wrong_type"),
        };
        let fields = match &self.r.structs["SignedEnvelope"] {
            Struct::Fields { fields, .. } => fields,
            _ => unreachable!(),
        };
        for (k, x) in m {
            let f = match k {
                Value::Uint(k) => fields.iter().find(|f| f.key == *k),
                _ => None,
            }
            .ok_or("unknown_field")?;
            if f.key == 1 {
                // body: bstr holding one canonical top-level object of type t
                let b = match x {
                    Value::Bytes(b) => b,
                    Value::Null => return Err("null_forbidden"),
                    _ => return Err("wrong_type"),
                };
                let mut lim = self.r.lim;
                lim.lenient = self.lenient;
                let body = decode(b, lim, t.max_bytes)?;
                self.body(&body, t)?;
            } else {
                self.ty(x, &f.ty, m)?;
            }
        }
        for f in fields {
            if f.req && !m.iter().any(|(k, _)| *k == Value::Uint(f.key)) {
                return Err("missing_field");
            }
        }
        Ok(())
    }

    fn body(&self, v: &Value, t: &ObjType) -> Result<(), Class> {
        self.strukt(v, &t.name, Some(t.id))?;
        if t.name == "AsyncKeyBundle" {
            if let Value::Map(m) = v {
                let prof = m.iter().find(|(k, _)| *k == Value::Uint(5)).and_then(|(_, x)| if let Value::Uint(p) = x { Some(*p) } else { None });
                for k in prof.and_then(|p| self.r.bundle_required.get(&p)).into_iter().flatten() {
                    if !m.iter().any(|(kk, _)| *kk == Value::Uint(*k)) {
                        return Err("missing_field");
                    }
                }
            }
        }
        Ok(())
    }

    fn strukt(&self, v: &Value, name: &str, top_id: Option<u64>) -> Result<(), Class> {
        match &self.r.structs[name] {
            Struct::Union { on, variants } => {
                let m = match v {
                    Value::Map(m) => m,
                    Value::Null => return Err("null_forbidden"),
                    _ => return Err("wrong_type"),
                };
                let d = m.iter().find(|(k, _)| *k == Value::Uint(*on)).map(|(_, x)| x).ok_or("missing_field")?;
                let d = match d {
                    Value::Uint(d) => *d,
                    Value::Null => return Err("null_forbidden"),
                    _ => return Err("wrong_type"),
                };
                let variant = variants.get(&d).ok_or("value_range")?;
                self.strukt(v, variant, None)
            }
            Struct::Fields { top, fields } => {
                let m = match v {
                    Value::Map(m) => m,
                    Value::Null => return Err("null_forbidden"),
                    _ => return Err("wrong_type"),
                };
                for (k, x) in m {
                    let k = match k {
                        Value::Uint(k) => *k,
                        _ => return Err("unknown_field"),
                    };
                    let f = fields.iter().find(|f| f.key == k).ok_or("unknown_field")?;
                    if *top && k == 0 {
                        match x {
                            Value::Uint(id) if Some(*id) == top_id => {}
                            Value::Uint(_) => return Err("wrong_object_type"),
                            Value::Null => return Err("null_forbidden"),
                            _ => return Err("wrong_type"),
                        }
                    } else if *top && k == 1 {
                        match x {
                            Value::Uint(1) => {}
                            Value::Uint(_) => return Err("unsupported_version"),
                            Value::Null => return Err("null_forbidden"),
                            _ => return Err("wrong_type"),
                        }
                    } else {
                        self.ty_in(x, &f.ty, m, fields)?;
                    }
                }
                for f in fields {
                    if f.req && !m.iter().any(|(k, _)| *k == Value::Uint(f.key)) {
                        return Err("missing_field");
                    }
                }
                // ADR-0135 §5: a profile's pinned HPKE suite fixes the exact enc / public-key lengths
                let field = |k: u64| m.iter().find(|(kk, _)| *kk == Value::Uint(k)).map(|(_, x)| x);
                let pinned = |pk: u64, bk: u64, pick: fn((usize, usize)) -> usize| -> Result<(), Class> {
                    if let (Some(Value::Uint(p)), Some(Value::Bytes(b))) = (field(pk), field(bk)) {
                        if let Some(s) = self.r.hpke_sizes.get(p) {
                            if b.len() != pick(*s) {
                                return Err("value_range");
                            }
                        }
                    }
                    Ok(())
                };
                match name {
                    "MessageRequest" => pinned(4, 5, |s| s.0)?,
                    "RequestKey" => pinned(0, 1, |s| s.1)?,
                    "DeliveryKeyMaterial" => {
                        if let (Some(Value::Uint(p)), Some(Value::Bytes(b))) = (field(0), field(2)) {
                            if let Some(s) = self.r.sealed_hpke.get(p) {
                                if b.len() != s.1 { return Err("value_range"); }
                            }
                        }
                    },
                    "SealedDeliveryEnvelope" => {
                        if let Some(Value::Uint(p)) = field(2) {
                            if let Some(s) = self.r.sealed_hpke.get(p) {
                                if let Some(Value::Bytes(b)) = field(5) { if b.len() != s.0 { return Err("value_range"); } }
                                if let Some(Value::Bytes(b)) = field(6) { if !s.2.contains(&b.len()) { return Err("value_range"); } }
                            }
                        }
                    },
                    _ => {}
                }
                Ok(())
            }
        }
    }

    fn ty(&self, x: &Value, t: &Ty, parent: &[(Value, Value)]) -> Result<(), Class> {
        self.ty_in(x, t, parent, &[])
    }

    fn ty_in(&self, x: &Value, t: &Ty, parent: &[(Value, Value)], pfields: &[Field]) -> Result<(), Class> {
        if matches!(x, Value::Null) && !matches!(t, Ty::Any) {
            return Err("null_forbidden");
        }
        match t {
            Ty::Uint(a, b) => match x {
                Value::Uint(n) if n >= a && n <= b => Ok(()),
                Value::Uint(_) => Err("value_range"),
                _ => Err("wrong_type"),
            },
            Ty::Bool(c) => match x {
                Value::Bool(v) => match c {
                    Some(c) if c != v => Err("value_range"),
                    _ => Ok(()),
                },
                _ => Err("wrong_type"),
            },
            Ty::Bstr(a, b) => match x {
                Value::Bytes(v) if (v.len() as u64) >= *a && (v.len() as u64) <= *b => Ok(()),
                Value::Bytes(_) => Err("value_range"),
                _ => Err("wrong_type"),
            },
            Ty::Tstr(a, b) => match x {
                Value::Text(v) if (v.len() as u64) >= *a && (v.len() as u64) <= *b => Ok(()),
                Value::Text(_) => Err("value_range"),
                _ => Err("wrong_type"),
            },
            Ty::Enum(e) => match x {
                Value::Uint(n) if self.r.enums[e].contains(n) => Ok(()),
                Value::Uint(_) => Err("value_range"),
                _ => Err("wrong_type"),
            },
            Ty::Ref(s) => self.strukt(x, s, None),
            Ty::Array(inner, a, b, set) => {
                let v = match x {
                    Value::Array(v) => v,
                    _ => return Err("wrong_type"),
                };
                if (v.len() as u64) < *a || (v.len() as u64) > *b {
                    return Err("value_range");
                }
                for e in v {
                    self.ty(e, inner, parent)?;
                }
                if *set && !self.lenient {
                    let enc: Vec<Vec<u8>> = v.iter().map(encode).collect();
                    for w in enc.windows(2) {
                        if w[0] >= w[1] {
                            return Err("set_order");
                        }
                    }
                }
                Ok(())
            }
            Ty::Dispatch { on, table } => {
                let key = pfields.iter().find(|f| &f.name == on).map(|f| f.key).expect("dispatch field");
                let sel = parent.iter().find(|(k, _)| *k == Value::Uint(key)).map(|(_, v)| v);
                match sel.and_then(|s| if let Value::Uint(n) = s { table.get(n) } else { None }) {
                    Some(s) => self.strukt(x, s, None),
                    None => match x {
                        Value::Map(_) => Ok(()), // opaque later-wave body
                        _ => Err("wrong_type"),
                    },
                }
            }
            Ty::Signed(tn) => {
                let b = match x {
                    Value::Bytes(b) => b,
                    _ => return Err("wrong_type"),
                };
                let t = &self.r.types[tn];
                let mut lim = self.r.lim;
                lim.lenient = self.lenient;
                let v = decode(b, lim, t.max_bytes)?;
                self.envelope(&v, t)
            }
            Ty::Any => Ok(()),
            Ty::Ext => {
                let m = match x {
                    Value::Map(m) => m,
                    _ => return Err("wrong_type"),
                };
                if m.is_empty() {
                    return Err("value_range");
                }
                for (k, e) in m {
                    let id = match k {
                        Value::Text(s) => s,
                        _ => return Err("wrong_type"),
                    };
                    if id.is_empty() || id.len() > 128 {
                        return Err("value_range");
                    }
                    self.strukt(e, "ExtEntry", None)?;
                    // ADR-0157: future critical extensions self-identify to the V1 baseline.
                    // Unknown om.critical.* rejects even if the sender lies with critical=false.
                    if !self.r.known_ext.iter().any(|x| x == id)
                        && (e.get(0) == Some(&Value::Bool(true)) || id.starts_with("om.critical."))
                    {
                        return Err("unknown_critical_extension");
                    }
                    // a registered extension carries its registered criticality and a value of its registered struct
                    if let Some((crit, s)) = self.r.ext_schema.get(id.as_str()) {
                        if e.get(0) != Some(&Value::Bool(*crit)) {
                            return Err("value_range");
                        }
                        if *crit {
                            self.strukt(e.get(1).ok_or("missing_field")?, s, None)?;
                        }
                        // ADR-0155: a schema-invalid non-critical registered extension is unusable
                        // presentation metadata; it never decides the containing object's validity.
                    }
                }
                Ok(())
            }
        }
    }
}
