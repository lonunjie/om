// SPDX-License-Identifier: MPL-2.0
//! Registry-driven W0 object validator. The rules come from
//! spec/registries/om-numeric-fields.json ("rules"), om-object-types.json and
//! om-cbor-profile.json. Walk order: a struct's entries in encoded order
//! (unknown key -> unknown_field, else validate the value), then required keys
//! in ascending order (missing_field). Arrays/sets: count, elements, then set order.

use crate::cbor::{Class, Limits, Value, decode, encode};
use serde_json::Value as J;
use std::collections::HashMap;

pub const FIELDS_JSON: &str = include_str!("../../../spec/registries/om-numeric-fields.json");
pub const TYPES_JSON: &str = include_str!("../../../spec/registries/om-object-types.json");
pub const PROFILE_JSON: &str = include_str!("../../../spec/registries/om-cbor-profile.json");
pub const W1_EVENT_BODIES_JSON: &str =
    include_str!("../../../spec/registries/om-w1-event-bodies.json");
pub const W1_EXPRESSIVE_SLOTS_JSON: &str =
    include_str!("../../../spec/registries/om-w1-expressive-slots.json");
pub const CRYPTO_JSON: &str = include_str!("../../../spec/registries/om-crypto-profiles.json");

#[derive(Debug, Clone)]
pub enum Ty {
    Uint(u64, u64),
    Bool(Option<bool>),
    Bstr(u64, u64),
    Tstr(u64, u64),
    Enum(String),
    Ref(String),
    Array(Box<Ty>, u64, u64, bool),
    Dispatch {
        on: String,
        table: HashMap<u64, String>,
    },
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
    Fields {
        top: bool,
        fields: Vec<Field>,
    },
    Union {
        on: u64,
        variants: HashMap<u64, String>,
    },
}

#[derive(Debug, Clone)]
pub struct ObjType {
    pub id: u64,
    pub name: String,
    pub sig_label: Option<String>,
    pub hash_label: Option<String>,
    pub max_bytes: usize,
    /// Names of the signing roles this type's envelope may carry (`signed.roles` of om-object-types.json).
    pub roles: Vec<String>,
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
    /// `sig_role` value -> name, for the roles that are valid only on object types whose `signed.roles` list them
    /// (enum_value_waves marks them later-wave; a decoder of that wave lifts the marker, see [`Registry::load_w3`]).
    pub scoped_roles: HashMap<u64, String>,
    /// struct name -> fields marked with a later `"wave"` in the registry; held back from `structs` until a decoder of
    /// that wave enables them ([`Registry::load_w4`]).
    pub deferred_fields: HashMap<String, Vec<Field>>,
    /// enum name -> values marked `"W4"` in `enum_value_waves`; W0 to W3 decoders reject them as unknown and only
    /// [`Registry::load_w4`] enables them (the plugin capability, error and state ids of ADR-0266).
    pub deferred_enums: HashMap<String, Vec<u64>>,
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
        let table = d["table"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), v.as_str().unwrap().to_string()))
            .collect();
        return Ty::Dispatch {
            on: d["on"].as_str().unwrap().into(),
            table,
        };
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

/// A field entry carrying a `"wave"` marker belongs to a later wave: a W0 decoder treats its key as unknown (the key is
/// rejected exactly as if unregistered) and only [`Registry::load_w4`] enables it.
fn parse_fields(a: &J) -> Vec<Field> {
    parse_fields_where(a, false)
}

fn parse_fields_where(a: &J, later_wave: bool) -> Vec<Field> {
    a.as_array()
        .unwrap()
        .iter()
        .filter(|f| f.get("wave").is_some() == later_wave)
        .map(|f| Field {
            key: f["key"].as_u64().unwrap(),
            name: f["name"].as_str().unwrap().into(),
            ty: parse_ty(&f["type"]),
            req: f["req"].as_bool().unwrap(),
        })
        .collect()
}

impl Registry {
    pub fn load() -> Registry {
        Self::from_json(
            serde_json::from_str(FIELDS_JSON).unwrap(),
            serde_json::from_str(TYPES_JSON).unwrap(),
            serde_json::from_str(PROFILE_JSON).unwrap(),
        )
    }

    /// The registry a later-wave `groups` decoder uses: the W0 numeric-fields registry with the additive W1
    /// event-body overlay (om-w1-event-bodies.json) merged in, as `verification/prototypes/v004-group-wire/overlay.py`
    /// does for the prototype codecs. A kind-to-struct pair that names an existing kind with a different struct is an error.
    /// The registry a `communities_publications` (W3) decoder uses: the W1 registry with `sig_role` 7
    /// (`publication_signing_key`) enabled. ADR-0168 F-16: that role is valid only on object types whose `signed.roles`
    /// list it (PublicationEvent), which `check` enforces for every scoped role, so enabling it adds nothing elsewhere.
    pub fn load_w3() -> Registry {
        let mut r = Self::load_w1();
        let roles: Vec<u64> = r.scoped_roles.keys().copied().collect();
        r.enums.get_mut("sig_role").unwrap().extend(roles);
        r
    }

    /// The registry a `services_bots` (W4) decoder uses (ADR-0262): the W3 registry with the later-wave struct fields
    /// enabled, today `ConversationEvent` key 13 (`representation`). W0 to W3 decoders keep rejecting key 13.
    pub fn load_w4() -> Registry {
        let mut r = Self::load_w3();
        for (name, values) in std::mem::take(&mut r.deferred_enums) {
            if let Some(set) = r.enums.get_mut(&name) {
                set.extend(values);
            }
        }
        for (name, fields) in std::mem::take(&mut r.deferred_fields) {
            if let Some(Struct::Fields { fields: have, .. }) = r.structs.get_mut(&name) {
                have.extend(fields);
                have.sort_by_key(|x| x.key);
            }
        }
        r
    }

    pub fn load_w1() -> Registry {
        let mut f: J = serde_json::from_str(FIELDS_JSON).unwrap();
        Self::apply_event_bodies(&mut f);
        Self::from_json(
            f,
            serde_json::from_str(TYPES_JSON).unwrap(),
            serde_json::from_str(PROFILE_JSON).unwrap(),
        )
    }

    fn apply_event_bodies(f: &mut J) {
        let ov: J = serde_json::from_str(W1_EVENT_BODIES_JSON).unwrap();
        for (target, table) in ov["dispatch"].as_object().unwrap() {
            let (st, field) = target.split_once('.').unwrap();
            let fields = f["structs"][st]["fields"].as_array_mut().unwrap();
            let fld = fields.iter_mut().find(|x| x["name"] == field).unwrap();
            let t = fld["type"]["dispatch"]["table"].as_object_mut().unwrap();
            for (kind, name) in table.as_object().unwrap() {
                if let Some(old) = t.get(kind) {
                    assert_eq!(old, name, "overlay conflicts with {target}[{kind}]");
                }
                t.insert(kind.clone(), name.clone());
            }
        }
        // enum values the overlay enables for a W1 decoder: lift their later-wave marker (ADR-0168 F-16 marks them W1)
        if let Some(ev) = ov["enum_values"].as_object() {
            for (e, vals) in ev {
                let waves = f["enum_value_waves"][e].as_object_mut().unwrap();
                for v in vals.as_array().unwrap() {
                    let v = v.as_str().unwrap();
                    assert_eq!(
                        waves.get(v).and_then(|w| w.as_str()),
                        Some("W1"),
                        "{e}[{v}] is not marked W1"
                    );
                    waves.remove(v);
                }
            }
        }
    }

    /// The registry an `expressive_media` decoder uses (ADR-0252): the W0 numeric-fields registry with the additive slot
    /// overlay (om-w1-expressive-slots.json) merged in, as `verification/prototypes/expressive-wire/overlay.py` does for
    /// the prototype codecs. It fills ReactionSet key 3 and RichText inline type 4. Verification evidence only: no
    /// production decode path uses it (the feature is behind a capability flag until the V-012 release evidence exists).
    pub fn load_w1_expressive() -> Registry {
        let mut f: J = serde_json::from_str(FIELDS_JSON).unwrap();
        Self::apply_expressive_slots(&mut f);
        Self::from_json(
            f,
            serde_json::from_str(TYPES_JSON).unwrap(),
            serde_json::from_str(PROFILE_JSON).unwrap(),
        )
    }

    /// Product W1 grammar. The frozen overlay supplies the slot schemas; expressive.cddl supplies
    /// their scope. Keep the baseline RichText graph for polls, greetings and standalone Inline.
    pub fn load_w1_expressive_scoped() -> Registry {
        let mut f: J = serde_json::from_str(FIELDS_JSON).unwrap();
        Self::apply_event_bodies(&mut f);
        let baseline_inline = f["structs"]["Inline"].clone();
        Self::apply_expressive_slots(&mut f);
        let structs = f["structs"].as_object_mut().unwrap();
        for (name, field, replacement) in [
            ("RichText", "blocks", "BlockX"),
            ("BlockInlines", "inlines", "InlineX"),
            ("InlineLink", "label", "InlineX"),
            ("InlineMention", "label", "InlineX"),
        ] {
            let mut shape = structs[name].clone();
            let fields = shape["fields"].as_array_mut().unwrap();
            let field = fields.iter_mut().find(|f| f["name"] == field).unwrap();
            field["type"]["array"]["ref"] = replacement.into();
            structs.insert(format!("{name}X"), shape);
        }
        let mut block = structs["Block"].clone();
        block["union"]["variants"]["1"] = "BlockInlinesX".into();
        block["union"]["variants"]["2"] = "BlockInlinesX".into();
        structs.insert("BlockX".into(), block);
        let mut inline = structs.insert("Inline".into(), baseline_inline).unwrap();
        inline["union"]["variants"]["2"] = "InlineLinkX".into();
        inline["union"]["variants"]["3"] = "InlineMentionX".into();
        structs.insert("InlineX".into(), inline);
        for (name, key) in [("MessageCreate", 0), ("MessageEdit", 1)] {
            let fields = structs.get_mut(name).unwrap()["fields"]
                .as_array_mut()
                .unwrap();
            let content = fields.iter_mut().find(|f| f["key"] == key).unwrap();
            content["type"]["ref"] = "RichTextX".into();
        }
        Self::from_json(
            f,
            serde_json::from_str(TYPES_JSON).unwrap(),
            serde_json::from_str(PROFILE_JSON).unwrap(),
        )
    }

    fn apply_expressive_slots(f: &mut J) {
        let ov: J = serde_json::from_str(W1_EXPRESSIVE_SLOTS_JSON).unwrap();
        for (st, add) in ov["struct_fields"].as_object().unwrap() {
            let fields = f["structs"][st]["fields"].as_array_mut().unwrap();
            for nf in add.as_array().unwrap() {
                assert!(
                    !fields.iter().any(|x| x["key"] == nf["key"]),
                    "overlay field {st}.{} is already assigned",
                    nf["key"]
                );
                fields.push(nf.clone());
            }
        }
        for (un, add) in ov["union_variants"].as_object().unwrap() {
            let t = f["structs"][un]["union"]["variants"]
                .as_object_mut()
                .unwrap();
            for (value, name) in add.as_object().unwrap() {
                if let Some(old) = t.get(value) {
                    assert_eq!(old, name, "overlay conflicts with {un}[{value}]");
                }
                t.insert(value.clone(), name.clone());
            }
        }
        for (e, vals) in ov["enum_values"].as_object().unwrap() {
            let waves = f["enum_value_waves"][e].as_object_mut().unwrap();
            for v in vals.as_array().unwrap() {
                let v = v.as_str().unwrap();
                assert_eq!(
                    waves.get(v).and_then(|w| w.as_str()),
                    Some("W1"),
                    "{e}[{v}] is not marked W1"
                );
                waves.remove(v);
            }
        }
    }

    fn from_json(f: J, t: J, p: J) -> Registry {
        let common = parse_fields(&f["common_top_level_fields"]);
        let mut structs = HashMap::new();
        let mut deferred: HashMap<String, Vec<Field>> = HashMap::new();
        for (name, s) in f["structs"].as_object().unwrap() {
            if let Some(u) = s.get("union") {
                let variants = u["variants"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(k, v)| (k.parse().unwrap(), v.as_str().unwrap().to_string()))
                    .collect();
                structs.insert(
                    name.clone(),
                    Struct::Union {
                        on: u["on"].as_u64().unwrap(),
                        variants,
                    },
                );
            } else {
                let top = s
                    .get("top_level")
                    .and_then(|x| x.as_bool())
                    .unwrap_or(false);
                let mut fields = parse_fields(&s["fields"]);
                let later = parse_fields_where(&s["fields"], true);
                if !later.is_empty() {
                    deferred.insert(name.clone(), later);
                }
                if top {
                    fields.extend(common.iter().cloned());
                }
                fields.sort_by_key(|x| x.key);
                structs.insert(name.clone(), Struct::Fields { top, fields });
            }
        }
        // ADR-0168 F-16: this is a W0 decoder, so enum values introduced in a later wave (enum_value_waves) are not W0-valid.
        let later_wave = |e: &str, v: &str| {
            f["enum_value_waves"][e][v]
                .as_str()
                .is_some_and(|w| w != "W0")
        };
        let mut deferred_enums: HashMap<String, Vec<u64>> = HashMap::new();
        for (e, vals) in f["enums"].as_object().unwrap() {
            for v in vals.as_object().unwrap().keys() {
                if f["enum_value_waves"][e][v].as_str() == Some("W4") {
                    deferred_enums
                        .entry(e.clone())
                        .or_default()
                        .push(v.parse().unwrap());
                }
            }
        }
        let enums = f["enums"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    v.as_object()
                        .unwrap()
                        .keys()
                        .filter(|x| !later_wave(k, x))
                        .map(|x| x.parse().unwrap())
                        .collect(),
                )
            })
            .collect();
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
                        sig_label: x["signed"]
                            .get("label")
                            .and_then(|l| l.as_str())
                            .map(String::from),
                        hash_label: x["hash"].as_str().map(String::from),
                        max_bytes: x["max_bytes"].as_u64().unwrap() as usize,
                        roles: x["signed"]["roles"]
                            .as_array()
                            .map(|r| {
                                r.iter()
                                    .filter_map(|n| n.as_str().map(String::from))
                                    .collect()
                            })
                            .unwrap_or_default(),
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
        let mut known_ext: Vec<String> = p["extensions"]["known_extensions"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap().to_string())
            .collect();
        // registered key-23 extensions (om-numeric-fields "extensions", ADR-0126)
        if let Some(ex) = f.get("extensions").and_then(|x| x.as_array()) {
            known_ext.extend(ex.iter().map(|e| e["name"].as_str().unwrap().to_string()));
        }
        let c: J = serde_json::from_str(CRYPTO_JSON).unwrap();
        let akb = f["structs"]["AsyncKeyBundle"]["fields"].as_array().unwrap();
        let mut bundle_required = HashMap::new();
        for prof in c["profiles"].as_array().unwrap() {
            if let Some(req) = prof
                .get("required_bundle_fields")
                .and_then(|x| x.as_array())
            {
                let keys = req
                    .iter()
                    .map(|n| {
                        akb.iter().find(|f| f["name"] == *n).unwrap()["key"]
                            .as_u64()
                            .unwrap()
                    })
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
            if let (Some(e), Some(k), Some(buckets), Some(tag)) = (
                h["enc_bytes"].as_u64(),
                h["public_key_bytes"].as_u64(),
                h["plaintext_buckets"].as_array(),
                h["aead_tag_bytes"].as_u64(),
            ) {
                let ciphertexts = buckets
                    .iter()
                    .map(|b| b.as_u64().unwrap() as usize + tag as usize)
                    .collect();
                sealed_hpke.insert(
                    prof["id"].as_u64().unwrap(),
                    (e as usize, k as usize, ciphertexts),
                );
            }
        }
        let mut ext_schema = HashMap::new();
        for e in f
            .get("extensions")
            .and_then(|x| x.as_array())
            .into_iter()
            .flatten()
        {
            if let Some(r) = e["value"]["ref"].as_str() {
                ext_schema.insert(
                    e["name"].as_str().unwrap().to_string(),
                    (e["critical"].as_bool().unwrap_or(false), r.to_string()),
                );
            }
        }
        let scoped_roles = f["enums"]["sig_role"]
            .as_object()
            .unwrap()
            .iter()
            .filter(|(v, _)| later_wave("sig_role", v))
            .map(|(v, n)| (v.parse().unwrap(), n.as_str().unwrap().to_string()))
            .collect();
        Registry {
            structs,
            enums,
            types,
            known_ext,
            ext_schema,
            bundle_required,
            hpke_sizes,
            sealed_hpke,
            lim,
            scoped_roles,
            deferred_fields: deferred,
            deferred_enums,
        }
    }

    /// The decoder limits for one object type. A string cannot be longer than the object that holds it, so the
    /// profile's global string limit never binds below an object's own `max_bytes`: the types whose registered size
    /// exceeds it (MessageRequest, SealedDeliveryEnvelope, the group objects) carry ciphertexts and states that the
    /// frozen CDDL bounds at up to `max_bytes` (for example `bstr .size (17..131088)`), and the per-field schema
    /// bounds still apply. Without this, a 64 KiB sealed-delivery bucket could never be decoded.
    fn limits_for(&self, t: &ObjType) -> Limits {
        let mut l = self.lim;
        l.max_string = l.max_string.max(t.max_bytes as u64);
        l
    }

    /// Decode + validate a top-level object of the declared type (SignedEnvelope for signed types).
    pub fn check(&self, type_name: &str, bytes: &[u8], lenient: bool) -> Result<Value, Class> {
        let t = self.types.get(type_name).ok_or("unknown_type")?;
        let mut lim = self.limits_for(t);
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

impl Registry {
    /// Validate and inspect a registered unsigned object that can contain a secret. Parser-owned byte
    /// strings and text are wiped on parse/schema errors and after the borrowed inspection returns.
    pub fn inspect_sensitive<T>(
        &self,
        type_name: &str,
        bytes: &[u8],
        inspect: impl FnOnce(&Value) -> T,
    ) -> Result<T, Class> {
        let t = self.types.get(type_name).ok_or("unknown_type")?;
        if t.sig_label.is_some() {
            return Err("unknown_type");
        }
        let v = crate::cbor::decode_guarded(bytes, self.limits_for(t), t.max_bytes, true)?;
        Cx {
            r: self,
            lenient: false,
        }
        .body(v.value(), t)?;
        Ok(inspect(v.value()))
    }

    /// Inspect a canonical struct whose byte strings may contain attachment keys.
    pub fn inspect_sensitive_struct<T>(
        &self,
        name: &str,
        bytes: &[u8],
        max_bytes: usize,
        inspect: impl FnOnce(&Value) -> T,
    ) -> Result<T, Class> {
        if !self.structs.contains_key(name) {
            return Err("unknown_type");
        }
        let mut lim = self.lim;
        lim.max_string = lim.max_string.max(max_bytes as u64);
        let v = crate::cbor::decode_guarded(bytes, lim, max_bytes, true)?;
        Cx {
            r: self,
            lenient: false,
        }
        .strukt(v.value(), name, None)?;
        Ok(inspect(v.value()))
    }

    /// Decode + validate the *inner body* of a signed type (the bytes carried in SignedEnvelope key 1), without
    /// looking at signatures. Used to check bodies Core builds before it signs them.
    /// Checks an already decoded value against a named (not necessarily top-level) struct, as a nested payload.
    pub fn check_struct_value(&self, name: &str, v: &Value) -> Result<(), Class> {
        if !self.structs.contains_key(name) {
            return Err("unknown_type");
        }
        Cx {
            r: self,
            lenient: false,
        }
        .strukt(v, name, None)
    }

    pub fn check_body(&self, type_name: &str, body: &[u8]) -> Result<Value, Class> {
        let t = self.types.get(type_name).ok_or("unknown_type")?;
        let v = decode(body, self.limits_for(t), t.max_bytes)?;
        Cx {
            r: self,
            lenient: false,
        }
        .body(&v, t)?;
        Ok(v)
    }
}

impl Registry {
    /// Decode + validate bytes as one registered non-top-level struct (for example the `FrameBody*` structs, whose
    /// bytes travel inside a `GroupTransportFrame` bstr, ADR-0189 and the Q1 ruling of 2026-11-14). No grammar change:
    /// the struct's own field table is applied exactly as it is when the struct is nested in an object. `max_bytes`
    /// bounds the whole input.
    pub fn check_struct(&self, name: &str, bytes: &[u8], max_bytes: usize) -> Result<Value, Class> {
        if !self.structs.contains_key(name) {
            return Err("unknown_type");
        }
        let mut lim = self.lim;
        lim.max_string = lim.max_string.max(max_bytes as u64);
        let v = decode(bytes, lim, max_bytes)?;
        Cx {
            r: self,
            lenient: false,
        }
        .strukt(&v, name, None)?;
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
        // A scoped (later-wave) signing role is valid only on a type whose `signed.roles` names it.
        if let Some((_, Value::Array(sigs))) = m.iter().find(|(k, _)| *k == Value::Uint(2)) {
            for sig in sigs {
                let Value::Map(sm) = sig else { continue };
                if let Some((_, Value::Uint(role))) = sm.iter().find(|(k, _)| *k == Value::Uint(2))
                    && let Some(name) = self.r.scoped_roles.get(role)
                    && !t.roles.contains(name)
                {
                    return Err("value_range");
                }
            }
        }
        Ok(())
    }

    fn body(&self, v: &Value, t: &ObjType) -> Result<(), Class> {
        self.strukt(v, &t.name, Some(t.id))?;
        service_rules(v, &t.name)?;
        bridge_rules(v, &t.name)?;
        plugin_rules(self.r, v, &t.name)?;
        if t.name == "AsyncKeyBundle"
            && let Value::Map(m) = v
        {
            let prof = m
                .iter()
                .find(|(k, _)| *k == Value::Uint(5))
                .and_then(|(_, x)| {
                    if let Value::Uint(p) = x {
                        Some(*p)
                    } else {
                        None
                    }
                });
            for k in prof
                .and_then(|p| self.r.bundle_required.get(&p))
                .into_iter()
                .flatten()
            {
                if !m.iter().any(|(kk, _)| *kk == Value::Uint(*k)) {
                    return Err("missing_field");
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
                let d = m
                    .iter()
                    .find(|(k, _)| *k == Value::Uint(*on))
                    .map(|(_, x)| x)
                    .ok_or("missing_field")?;
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
                let field = |k: u64| {
                    m.iter()
                        .find(|(kk, _)| *kk == Value::Uint(k))
                        .map(|(_, x)| x)
                };
                let pinned =
                    |pk: u64, bk: u64, pick: fn((usize, usize)) -> usize| -> Result<(), Class> {
                        if let (Some(Value::Uint(p)), Some(Value::Bytes(b))) =
                            (field(pk), field(bk))
                            && let Some(s) = self.r.hpke_sizes.get(p)
                            && b.len() != pick(*s)
                        {
                            return Err("value_range");
                        }
                        Ok(())
                    };
                match name {
                    "MessageRequest" => pinned(4, 5, |s| s.0)?,
                    "RequestKey" => pinned(0, 1, |s| s.1)?,
                    "DeliveryKeyMaterial" => {
                        if let (Some(Value::Uint(p)), Some(Value::Bytes(b))) = (field(0), field(2))
                            && let Some(s) = self.r.sealed_hpke.get(p)
                            && b.len() != s.1
                        {
                            return Err("value_range");
                        }
                    }
                    "SealedDeliveryEnvelope" => {
                        if let Some(Value::Uint(p)) = field(2)
                            && let Some(s) = self.r.sealed_hpke.get(p)
                        {
                            if let Some(Value::Bytes(b)) = field(5)
                                && b.len() != s.0
                            {
                                return Err("value_range");
                            }
                            if let Some(Value::Bytes(b)) = field(6)
                                && !s.2.contains(&b.len())
                            {
                                return Err("value_range");
                            }
                        }
                    }
                    _ => {}
                }
                Ok(())
            }
        }
    }

    fn ty(&self, x: &Value, t: &Ty, parent: &[(Value, Value)]) -> Result<(), Class> {
        self.ty_in(x, t, parent, &[])
    }

    fn ty_in(
        &self,
        x: &Value,
        t: &Ty,
        parent: &[(Value, Value)],
        pfields: &[Field],
    ) -> Result<(), Class> {
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
                let key = pfields
                    .iter()
                    .find(|f| &f.name == on)
                    .map(|f| f.key)
                    .expect("dispatch field");
                let sel = parent
                    .iter()
                    .find(|(k, _)| *k == Value::Uint(key))
                    .map(|(_, v)| v);
                match sel.and_then(|s| {
                    if let Value::Uint(n) = s {
                        table.get(n)
                    } else {
                        None
                    }
                }) {
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

fn mget(m: &[(Value, Value)], k: u64) -> Option<&Value> {
    m.iter()
        .find(|(kk, _)| *kk == Value::Uint(k))
        .map(|(_, x)| x)
}

fn uint(v: Option<&Value>) -> Option<u64> {
    if let Some(Value::Uint(x)) = v {
        Some(*x)
    } else {
        None
    }
}

/// Cross-field rules of the W4 service objects (ADR-0262, `spec/cddl/service.cddl`) that one object's own bytes decide;
/// the per-field registry cannot express them. Chain rules that need a prior head (stale, forked, rollback) belong to the
/// service control engine, not here. The Python prototype `verification/prototypes/services-wire/python_codec.py` states
/// the same rules, and the shared corpus checks both.
fn service_rules(v: &Value, name: &str) -> Result<(), Class> {
    let Value::Map(m) = v else { return Ok(()) };
    let chain = match name {
        "ServiceControlState" => Some((3, 4)),
        "ServiceInstallation" | "RepresentationGrant" => Some((14, 15)),
        _ => None,
    };
    if let Some((ver, prev)) = chain {
        let version = uint(mget(m, ver)).ok_or("missing_field")?;
        match (version, mget(m, prev).is_some()) {
            (1, true) => return Err("forked_chain"),
            (v, false) if v > 1 => return Err("missing_field"),
            _ => {}
        }
    }
    let strings = |k: u64| -> Vec<&str> {
        match mget(m, k) {
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|x| {
                    if let Value::Text(t) = x {
                        Some(t.as_str())
                    } else {
                        None
                    }
                })
                .collect(),
            _ => vec![],
        }
    };
    match name {
        "ServiceControlState" => {
            let controllers = match mget(m, 5) {
                Some(Value::Array(a)) => a.len() as u64,
                _ => 0,
            };
            if uint(mget(m, 6)).ok_or("missing_field")? > controllers {
                return Err("value_range");
            }
        }
        "ServiceInstallation" => {
            let scope = match mget(m, 4) {
                Some(Value::Map(s)) => uint(mget(s, 0)),
                _ => None,
            }
            .ok_or("missing_field")?;
            let mode = uint(mget(m, 6)).ok_or("missing_field")?;
            if mode == 3 && scope != 2 || mode == 2 && scope != 1 {
                return Err("value_range");
            }
            if mode == 3 && mget(m, 13).is_none() {
                return Err("missing_field");
            }
            if mode != 3 && mget(m, 12).is_some() {
                return Err("value_range");
            }
        }
        "RepresentationGrant" => {
            if let (Some(from), Some(to)) = (uint(mget(m, 10)), uint(mget(m, 11)))
                && from >= to
            {
                return Err("value_range");
            }
            if strings(6).contains(&"message.read_other_participant") && mget(m, 13).is_none() {
                return Err("missing_field");
            }
        }
        "ConversationEvent" => {
            let service_author =
                matches!(mget(m, 5), Some(Value::Map(p)) if uint(mget(p, 0)) == Some(2));
            if mget(m, 13).is_some() && !service_author {
                return Err("value_range");
            }
            if service_author && uint(mget(m, 8)) == Some(1) {
                let empty = match mget(m, 11) {
                    Some(Value::Map(b)) => {
                        mget(b, 0).is_none()
                            && match mget(b, 1) {
                                Some(Value::Array(a)) => a.is_empty(),
                                None => true,
                                _ => false,
                            }
                    }
                    _ => false,
                };
                if empty {
                    return Err("value_range");
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn arr(v: Option<&Value>) -> &[Value] {
    match v {
        Some(Value::Array(a)) => a,
        _ => &[],
    }
}

fn map_of(v: Option<&Value>) -> Option<&[(Value, Value)]> {
    match v {
        Some(Value::Map(m)) => Some(m),
        _ => None,
    }
}

fn field(x: &Value, k: u64) -> Option<&Value> {
    match x {
        Value::Map(e) => mget(e, k),
        _ => None,
    }
}

fn distinct(items: &[&Value]) -> bool {
    items
        .iter()
        .enumerate()
        .all(|(i, a)| items[i + 1..].iter().all(|b| a != b))
}

/// Interop trace body (`interop-trace-body`): hop count equals the path, within the limit, no repeated bridge or
/// transformation.
fn trace_rules(t: &[(Value, Value)]) -> Result<(), Class> {
    let path = arr(mget(t, 2));
    let hops = uint(mget(t, 3)).ok_or("missing_field")?;
    let limit = uint(mget(t, 4)).ok_or("missing_field")?;
    if path.len() as u64 != hops || hops > limit {
        return Err("value_range");
    }
    if !distinct(&path.iter().collect::<Vec<_>>()) {
        return Err("loop_detected");
    }
    if !distinct(&arr(mget(t, 5)).iter().collect::<Vec<_>>()) {
        return Err("value_range");
    }
    Ok(())
}

/// Foreign provenance body (`foreign-provenance-body`), standalone or as key 14 of an event.
fn provenance_rules(p: &[(Value, Value)]) -> Result<(), Class> {
    let flags = arr(mget(p, 7));
    if flags.contains(&Value::Uint(1)) && flags.len() != 1 {
        return Err("value_range");
    }
    if mget(p, 3).is_none() && !flags.contains(&Value::Uint(8)) {
        return Err("missing_field");
    }
    if let Some(t) = map_of(mget(p, 9)) {
        trace_rules(t)?;
        let fp = mget(p, 8).ok_or("missing_field")?;
        if Some(fp) != mget(t, 1) {
            return Err("provenance_mismatch");
        }
    }
    Ok(())
}

/// Cross-field rules of the W4 bridge objects (ADR-0264, `spec/cddl/bridge.cddl`) that one object's own bytes decide.
/// Rules that need a prior head, a live control state or a Room mapping belong to the bridge engine, not here. The
/// Python prototype `verification/prototypes/bridges-wire/python_codec.py` states the same rules.
fn bridge_rules(v: &Value, name: &str) -> Result<(), Class> {
    let Value::Map(m) = v else { return Ok(()) };
    let chain = match name {
        "BridgeControlState" => Some((3, 4)),
        "BridgeMapping" | "ForeignIdentityLink" => Some((10, 11)),
        _ => None,
    };
    if let Some((ver, prev)) = chain {
        let version = uint(mget(m, ver)).ok_or("missing_field")?;
        match (version, mget(m, prev).is_some()) {
            (1, true) => return Err("forked_chain"),
            (v, false) if v > 1 => return Err("missing_field"),
            _ => {}
        }
    }
    match name {
        "InteropTrace" => trace_rules(map_of(mget(m, 2)).ok_or("missing_field")?)?,
        "ForeignProvenance" => provenance_rules(map_of(mget(m, 2)).ok_or("missing_field")?)?,
        "BridgeControlState" => {
            if uint(mget(m, 6)).ok_or("missing_field")? > arr(mget(m, 5)).len() as u64 {
                return Err("value_range");
            }
            let ends: Vec<_> = arr(mget(m, 7)).iter().filter_map(|e| field(e, 0)).collect();
            let adapters = arr(mget(m, 8));
            let ids: Vec<_> = adapters.iter().filter_map(|a| field(a, 0)).collect();
            let spaces: Vec<_> = adapters.iter().filter_map(|a| field(a, 1)).collect();
            if !distinct(&ends) || !distinct(&ids) || !distinct(&spaces) {
                return Err("set_order");
            }
            for a in adapters {
                let Some(cap) = map_of(field(a, 2)) else {
                    continue;
                };
                let limits = arr(mget(cap, 5));
                let sems: Vec<_> = limits.iter().filter_map(|l| field(l, 0)).collect();
                if !distinct(&sems) {
                    return Err("set_order");
                }
                if sems.iter().any(|x| !arr(mget(cap, 0)).contains(x)) {
                    return Err("value_range");
                }
            }
        }
        "BridgeMapping" => {
            let pol = map_of(mget(m, 9)).ok_or("missing_field")?;
            let (relay, meta) = (
                mget(pol, 0) == Some(&Value::Bool(true)),
                mget(pol, 2) == Some(&Value::Bool(true)),
            );
            let dir = uint(mget(m, 7));
            if relay && !meta || !meta && matches!(dir, Some(1) | Some(3)) {
                return Err("wrong_role");
            }
        }
        "ForeignIdentityLink" => {
            let bridge_proof = uint(mget(m, 5)) == Some(5);
            if bridge_proof != mget(m, 12).is_some() {
                return Err(if bridge_proof {
                    "missing_field"
                } else {
                    "value_range"
                });
            }
            if let (Some(nb), Some(exp)) = (uint(mget(m, 13)), uint(mget(m, 8)))
                && nb >= exp
            {
                return Err("value_range");
            }
        }
        "ConversationEvent" => {
            let author = map_of(mget(m, 5));
            let kind = author.and_then(|p| uint(mget(p, 0)));
            let bridge_author = kind == Some(3);
            if let Some(f) = mget(m, 14) {
                if mget(m, 13).is_some() {
                    return Err("wrong_role");
                }
                let Some(a) = author.filter(|_| bridge_author) else {
                    return Err("wrong_role");
                };
                let f = map_of(Some(f)).ok_or("value_range")?;
                provenance_rules(f)?;
                let (Some(_), Some(t)) = (mget(f, 8), map_of(mget(f, 9))) else {
                    return Err("missing_field");
                };
                if arr(mget(t, 2)).last() != mget(a, 1) {
                    return Err("provenance_mismatch");
                }
                if uint(mget(t, 3)) >= uint(mget(t, 4)) {
                    return Err("loop_detected");
                }
            } else if bridge_author {
                return Err("missing_field");
            }
            if bridge_author && uint(mget(m, 8)) == Some(1) {
                let empty = match map_of(mget(m, 11)) {
                    Some(b) => mget(b, 0).is_none() && arr(mget(b, 1)).is_empty(),
                    None => false,
                };
                if empty {
                    return Err("value_range");
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn flag(v: Option<&Value>) -> bool {
    v == Some(&Value::Bool(true))
}

/// The set-valued digest key of a digest map `{0: 1, 1: bytes}`.
fn digest_bytes(v: Option<&Value>) -> Option<&Value> {
    match v {
        Some(Value::Map(m)) => mget(m, 1),
        _ => None,
    }
}

fn uint_set(v: Option<&Value>) -> Vec<u64> {
    arr(v).iter().filter_map(|x| uint(Some(x))).collect()
}

/// Cross-field rules of the W4 plugin objects (ADR-0266, `spec/cddl/plugin.cddl`) that one object's own bytes decide,
/// including the nested payload of the plugin/host envelopes. Rules that need a prior release, a live grant, a clock,
/// a verified channel or host policy are the plugin host's, not here. The Python prototype
/// `verification/prototypes/plugins-wire/python_codec.py` states the same rules.
fn plugin_rules(r: &Registry, v: &Value, name: &str) -> Result<(), Class> {
    let Value::Map(m) = v else { return Ok(()) };
    match name {
        "PluginPackageManifest" => {
            let version = uint(mget(m, 3)).ok_or("missing_field")?;
            match (version, mget(m, 5).is_some()) {
                (1, true) => return Err("forked_chain"),
                (v, false) if v > 1 => return Err("missing_field"),
                _ => {}
            }
            let name_of = |x: &Value| field(x, 1).cloned();
            let comps: Vec<Value> = arr(mget(m, 6)).iter().filter_map(name_of).collect();
            let entry: Vec<Value> = arr(mget(m, 8))
                .iter()
                .filter_map(|e| {
                    Some(Value::Array(vec![
                        field(e, 0)?.clone(),
                        field(e, 1)?.clone(),
                    ]))
                })
                .collect();
            let limits: Vec<Value> = arr(mget(m, 11))
                .iter()
                .filter_map(|l| field(l, 0).cloned())
                .collect();
            if !distinct(&comps.iter().collect::<Vec<_>>())
                || !distinct(&entry.iter().collect::<Vec<_>>())
                || !distinct(&limits.iter().collect::<Vec<_>>())
            {
                return Err("set_order");
            }
            if !arr(mget(m, 10)).is_empty() && !uint_set(mget(m, 9)).contains(&14) {
                return Err("value_range");
            }
        }
        "PluginControlState" => {
            let keys = arr(mget(m, 5));
            let active = keys.iter().filter(|k| uint(field(k, 3)) == Some(1)).count() as u64;
            if uint(mget(m, 6)).ok_or("missing_field")? > active {
                return Err("value_range");
            }
            let key_ids: Vec<&Value> = keys.iter().filter_map(|k| field(k, 0)).collect();
            if !distinct(&key_ids) {
                return Err("set_order");
            }
            let rel = arr(mget(m, 7));
            let seqs: Vec<u64> = rel.iter().filter_map(|x| uint(field(x, 0))).collect();
            let mut sorted = seqs.clone();
            sorted.sort_unstable();
            if sorted.windows(2).any(|w| w[0] == w[1]) {
                return Err("set_order");
            }
            if sorted.iter().enumerate().any(|(i, s)| *s != i as u64 + 1) {
                return Err("stale_chain");
            }
            let ids: Vec<&Value> = rel
                .iter()
                .filter_map(|x| digest_bytes(field(x, 1)))
                .collect();
            if !distinct(&ids) {
                return Err("set_order");
            }
            for x in rel {
                let (Some(id), Some(mh)) = (field(x, 1), field(x, 2)) else {
                    return Err("missing_field");
                };
                if id != mh {
                    return Err("value_range");
                }
                let (req, ceil) = (uint_set(field(x, 3)), uint_set(field(x, 4)));
                if !ceil.iter().all(|c| req.contains(c)) {
                    return Err("capability_escalation");
                }
                match (uint(field(x, 0)), field(x, 7).is_some()) {
                    (Some(1), true) => return Err("forked_chain"),
                    (Some(n), false) if n > 1 => return Err("missing_field"),
                    _ => {}
                }
            }
            let by_seq = |n: u64| rel.iter().find(|x| uint(field(x, 0)) == Some(n));
            for n in 2..=sorted.len() as u64 {
                let (new, old) = (
                    by_seq(n).ok_or("stale_chain")?,
                    by_seq(n - 1).ok_or("stale_chain")?,
                );
                if field(new, 7) != field(old, 1) {
                    return Err("forked_chain");
                }
                let (nc, oc) = (uint_set(field(new, 4)), uint_set(field(old, 4)));
                if !nc.iter().all(|c| oc.contains(c)) {
                    return Err("capability_escalation");
                }
            }
        }
        "PluginCapabilityGrant" => {
            if uint(mget(m, 7)).ok_or("missing_field")?
                <= uint(mget(m, 6)).ok_or("missing_field")?
            {
                return Err("value_range");
            }
            scope_rules(mget(m, 5))?;
            if let Some(sc) = map_of(mget(m, 5))
                && uint(mget(sc, 0)) == Some(1)
                && mget(sc, 1) != mget(m, 2)
            {
                return Err("value_range");
            }
        }
        "PluginCapabilityCall" => {
            scope_rules(mget(m, 7))?;
            let cap = uint(mget(m, 5)).ok_or("missing_field")?;
            let eg = map_of(mget(m, 9));
            if let Some(e) = eg {
                if !matches!(cap, 11 | 14 | 17) {
                    return Err("value_range");
                }
                let kind = uint(mget(e, 0));
                if kind == Some(3) && mget(e, 3).is_none() {
                    return Err("missing_field");
                }
                if matches!(kind, Some(2) | Some(4)) && mget(e, 2).is_none() {
                    return Err("missing_field");
                }
            }
            if matches!(cap, 14 | 17) {
                match eg {
                    None => return Err("missing_field"),
                    Some(e) if uint(mget(e, 0)) == Some(1) => return Err("value_range"),
                    _ => {}
                }
            }
            if cap == 11 {
                let sc = map_of(mget(m, 7)).and_then(|s| uint(mget(s, 0)));
                if sc != Some(2) {
                    return Err("value_range");
                }
                match eg {
                    None => return Err("missing_field"),
                    Some(e) if uint(mget(e, 0)) != Some(1) => return Err("value_range"),
                    _ => {}
                }
            }
        }
        "PluginUiTreeUpdate" => {
            let nodes = arr(mget(m, 6));
            let ids: Vec<&Value> = nodes.iter().filter_map(|n| field(n, 0)).collect();
            if !distinct(&ids) {
                return Err("value_range");
            }
            let mut roots = 0;
            for n in nodes {
                let (mut cur, mut depth) = (n, 0);
                if field(cur, 2).is_none() {
                    roots += 1;
                }
                while let Some(parent) = field(cur, 2) {
                    depth += 1;
                    if depth > 8 {
                        return Err("value_range");
                    }
                    let Some(next) = nodes.iter().find(|x| field(x, 0) == Some(parent)) else {
                        return Err("value_range");
                    };
                    if depth > nodes.len() {
                        return Err("value_range");
                    }
                    cur = next;
                }
            }
            if roots != 1 {
                return Err("value_range");
            }
        }
        "PluginHostRequest" | "PluginHostResponse" => {
            let request = name == "PluginHostRequest";
            let inner = match (request, uint(mget(m, 7))) {
                (true, Some(1)) => "PluginCapabilityCall",
                (true, Some(2)) => "PluginUiTreeUpdate",
                (true, Some(3)) => "PluginEvent",
                (false, Some(1)) => "PluginCallResult",
                (false, Some(2)) => "PluginHostPolicyDecision",
                (false, Some(3)) => "PluginError",
                _ => return Err("value_range"),
            };
            let Some(Value::Bytes(payload)) = mget(m, 8) else {
                return Err("missing_field");
            };
            let top = r.types.get(inner);
            let Some(x) = top
                .map(|t| t.max_bytes)
                .or(Some(65536))
                .and_then(|max| decode(payload, Limits::OM, max).ok())
            else {
                return Err("non_canonical");
            };
            if let Some(t) = top {
                if uint(x.get(0)) != Some(t.id) {
                    return Err("value_range");
                }
                r.check_body(inner, payload)?;
                let same = if inner == "PluginError" {
                    x.get(2) == mget(m, 5)
                } else {
                    x.get(2) == mget(m, 2) && x.get(3) == mget(m, 3) && x.get(4) == mget(m, 5)
                };
                if !same {
                    return Err("value_range");
                }
            } else {
                r.check_struct_value(inner, &x)?;
                if x.get(0) != mget(m, 5) {
                    return Err("value_range");
                }
                if inner == "PluginHostPolicyDecision" {
                    decision_rules(&x)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

fn scope_rules(sc: Option<&Value>) -> Result<(), Class> {
    let Some(sc) = map_of(sc) else { return Ok(()) };
    let want = match uint(mget(sc, 0)) {
        Some(1) | Some(3) => 16,
        Some(2) => 32,
        _ => return Err("value_range"),
    };
    match mget(sc, 1) {
        Some(Value::Bytes(b)) if b.len() == want => Ok(()),
        _ => Err("value_range"),
    }
}

/// Host policy decision (`PluginHostPolicyDecision`): the single-object rules about confirmation and evidence.
fn decision_rules(x: &Value) -> Result<(), Class> {
    let Value::Map(d) = x else { return Ok(()) };
    let (tainted, allowed, confirmed) = (flag(mget(d, 2)), flag(mget(d, 3)), flag(mget(d, 4)));
    let egress = uint(mget(d, 6));
    if confirmed && mget(d, 8).is_none() {
        return Err("missing_field");
    }
    let outbound = allowed && tainted && matches!(egress, Some(2) | Some(4));
    if outbound && mget(d, 9).is_none() {
        return Err("missing_field");
    }
    if outbound && uint(mget(d, 7)) == Some(1) && !flag(mget(d, 5)) {
        return Err("bilateral_consent_required");
    }
    if allowed && uint(mget(d, 1)) == Some(11) && !confirmed {
        return Err("user_confirmation_required");
    }
    if allowed && tainted && egress == Some(3) && !confirmed {
        return Err("user_confirmation_required");
    }
    Ok(())
}
