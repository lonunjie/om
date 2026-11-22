// SPDX-License-Identifier: MPL-2.0
//! Immutable releases and the local installation transaction. Authority remains scoped to
//! one pack's pinned controller head; message rendering uses its own attachment references.

use super::asset::{digest, text, wire};
use super::{AssetRef, Error, Opened, registry};
use crate::crypto::binding::PROFILE_V1;
use crate::crypto::cbor::{self, V};
use crate::crypto::sig::{self, ROLE_CONTROLLER, ds_hash, ds_sig};
use crate::crypto::{hex, unhex};
use crate::state::SecretHex;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use zeroize::Zeroizing;

mod file;

pub use file::PACK_FILE_MAX;
pub(crate) use file::ReviewedPack;

const RELEASE_HASH: &str = "OM-CONTENT-PACK-RELEASE-HASH-v1";
const CONTROL_HASH: &str = "OM-CONTENT-PACK-CONTROL-HASH-v1";
const CONTROL_SIG: &str = "OM-CONTENT-PACK-CONTROL-SIG-v1";
const CONTROLLER_ID: &str = "OM-CONTENT-PACK-CONTROLLER-KEY-ID-v1";

pub struct Release {
    id: [u8; 32],
    pack_id: [u8; 32],
    sequence: u32,
    previous: Option<[u8; 32]>,
    forked_from: Option<[u8; 32]>,
    title: String,
    items: BTreeMap<String, AssetRef>,
    labels: BTreeMap<String, String>,
}

impl Release {
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        registry().inspect_sensitive("ContentPackRelease", bytes, |_| ())?;
        let body = wire(cbor::decode(bytes))?;
        let id = ds_hash(RELEASE_HASH, bytes);
        let sequence = wire(wire(body.get(3))?.u32())?;
        let previous = body.opt(4).map(digest).transpose()?;
        let forked_from = body.opt(11).map(digest).transpose()?;
        if (sequence == 1) != previous.is_none() {
            return Err("sequence_previous");
        }
        if forked_from.is_some() && sequence != 1 {
            return Err("forked_from_sequence");
        }
        if forked_from == Some(id) {
            return Err("fork_cycle");
        }
        // The codec rejects unknown critical extensions. Other entries stay in the
        // immutable hashed bytes and have no install or rendering semantics.
        let title = text(wire(body.get(5))?, 256, "title_scalars")?.into();
        if let Some(description) = body.opt(6) {
            text(description, 4096, "description_scalars")?;
        }
        if matches!(body.opt(10), Some(V::M(m)) if m.is_empty()) {
            return Err("licensing_empty");
        }
        let mut items = BTreeMap::new();
        let mut labels = BTreeMap::new();
        let mut blobs = BTreeMap::new();
        for item in wire(wire(body.get(8))?.a())? {
            let item_id = text(wire(item.get(0))?, 128, "item_id_scalars")?;
            if items.contains_key(item_id) {
                return Err("item_id_duplicate");
            }
            let value = wire(item.get(2))?;
            if [5, 10, 15, 16].iter().any(|k| value.opt(*k).is_some()) {
                return Err("asset_forbidden_key");
            }
            if item.opt(1) != value.opt(11) {
                return Err("item_kind");
            }
            let label = text(wire(item.get(5))?, 512, "label_scalars")?;
            labels.insert(item_id.into(), label.into());
            for (key, limit, reason) in [(3, 32, "emoji_scalars"), (4, 64, "keyword_scalars")] {
                if let Some(values) = item.opt(key) {
                    for v in wire(values.a())? {
                        text(v, limit, reason)?;
                    }
                }
            }
            let asset = AssetRef::from_value(value)?;
            let blob = asset.attachment.blob_id;
            if let Some(previous) = blobs.insert(blob, value)
                && [0, 1, 2, 3, 4, 7, 8, 9, 11, 12]
                    .iter()
                    .any(|k| previous.opt(*k) != value.opt(*k))
            {
                return Err("blob_identity_conflict");
            }
            items.insert(item_id.into(), asset);
        }
        Ok(Self {
            id,
            pack_id: wire(wire(body.get(2))?.b32())?,
            sequence,
            previous,
            forked_from,
            title,
            items,
            labels,
        })
    }

    pub fn id(&self) -> [u8; 32] {
        self.id
    }
    pub fn pack_id(&self) -> [u8; 32] {
        self.pack_id
    }
    pub fn item(&self, item_id: &str) -> Option<&AssetRef> {
        self.items.get(item_id)
    }

    fn declared_size(&self) -> u128 {
        let mut distinct = BTreeSet::new();
        self.items
            .values()
            .filter(|a| distinct.insert(a.attachment.blob_id))
            .map(|a| u128::from(a.attachment.plaintext_size))
            .sum()
    }

    fn follows(&self, parent_id: [u8; 32], parent_sequence: u32) -> Result<(), Error> {
        if parent_sequence.checked_add(1) != Some(self.sequence) {
            return Err("sequence_skip");
        }
        if self.previous != Some(parent_id) {
            return Err("previous_release_mismatch");
        }
        Ok(())
    }

    pub fn attributes(&self, asset: &AssetRef) -> bool {
        let Some(pack_ref) = &asset.presentation.pack_ref else {
            return false;
        };
        if pack_ref.pack_id != self.pack_id || pack_ref.release_id != self.id {
            return false;
        }
        let Some(named) = self.item(&pack_ref.item_id) else {
            return false;
        };
        let left = named.value();
        let right = asset.value();
        [0, 1, 2, 3, 4, 7, 8, 9, 11, 12]
            .iter()
            .all(|k| left.opt(*k) == right.opt(*k))
    }
}

struct Control {
    hash: [u8; 32],
    pack_id: [u8; 32],
    version: u32,
    previous: Option<[u8; 32]>,
    controllers: BTreeMap<[u8; 16], Vec<u8>>,
    threshold: usize,
    release: [u8; 32],
    sequence: u32,
}

impl Control {
    fn body(bytes: &[u8]) -> Result<Self, Error> {
        registry().check_body("ContentPackControlState", bytes)?;
        let body = wire(cbor::decode(bytes))?;
        let version = wire(wire(body.get(3))?.u32())?;
        let previous = body.opt(4).map(digest).transpose()?;
        if (version == 1) != previous.is_none() {
            return Err("version_genesis");
        }
        let mut controllers = BTreeMap::new();
        let mut keys = BTreeSet::new();
        for c in wire(wire(body.get(5))?.a())? {
            let id: [u8; 16] = wire(wire(c.get(0))?.b())?
                .try_into()
                .map_err(|_| "controller_key_id")?;
            let profile = wire(wire(c.get(1))?.u())?;
            let key = wire(wire(c.get(2))?.b())?;
            if profile != PROFILE_V1 || key.len() != sig::PK_LEN {
                return Err("controller_profile");
            }
            let key_bytes = cbor::encode(&V::A(vec![V::U(profile), V::B(key.to_vec())]));
            if ds_hash(CONTROLLER_ID, &key_bytes)[..16] != id {
                return Err("controller_key_id");
            }
            if !keys.insert(key) || controllers.insert(id, key.to_vec()).is_some() {
                return Err("controllers_duplicate");
            }
        }
        let threshold = wire(wire(body.get(6))?.u())? as usize;
        if threshold > controllers.len() {
            return Err("threshold");
        }
        Ok(Self {
            hash: ds_hash(CONTROL_HASH, bytes),
            pack_id: wire(wire(body.get(2))?.b32())?,
            version,
            previous,
            controllers,
            threshold,
            release: digest(wire(body.get(7))?)?,
            sequence: wire(wire(body.get(8))?.u32())?,
        })
    }

    #[allow(clippy::type_complexity)] // private one-use tuple; an alias would add no clarity
    fn verify(
        envelope: &[u8],
        parent: Option<&Self>,
        release: &Release,
    ) -> Result<(Self, Vec<[u8; 16]>, Vec<u8>), Error> {
        registry().check("ContentPackControlState", envelope, false)?;
        let env = wire(cbor::decode(envelope))?;
        let body = wire(wire(env.get(1))?.b())?;
        let next = Self::body(body)?;
        if next.pack_id != release.pack_id {
            return Err("pack_id_changed");
        }
        if next.release != release.id {
            return Err("release_id_mismatch");
        }
        if next.sequence != release.sequence {
            return Err("release_sequence_mismatch");
        }
        match parent {
            Some(p) => {
                if p.pack_id != next.pack_id {
                    return Err("pack_id_changed");
                }
                if p.version.checked_add(1) != Some(next.version) {
                    return Err("version_step");
                }
                if next.previous != Some(p.hash) {
                    return Err("previous_state_hash");
                }
                let expected_sequence = if next.release == p.release {
                    Some(p.sequence)
                } else {
                    p.sequence.checked_add(1)
                };
                if expected_sequence != Some(next.sequence) {
                    return Err("release_step");
                }
            }
            None if next.version != 1 || next.sequence != 1 => return Err("version_genesis"),
            None => {}
        }
        let auth = parent.unwrap_or(&next);
        let mut signed_by = BTreeSet::new();
        for s in wire(wire(env.get(2))?.a())? {
            let id: [u8; 16] = wire(wire(s.get(0))?.b())?
                .try_into()
                .map_err(|_| "signer_unknown")?;
            let key = auth.controllers.get(&id).ok_or("signer_unknown")?;
            if !signed_by.insert(id) {
                return Err("signer_duplicate");
            }
            if s.opt(1) != Some(&V::U(PROFILE_V1)) || s.opt(2) != Some(&V::U(ROLE_CONTROLLER)) {
                return Err("signature_profile_or_role");
            }
            let msg = ds_sig(CONTROL_SIG, PROFILE_V1, ROLE_CONTROLLER, &id, body);
            sig::verify(key, &msg, wire(wire(s.get(3))?.b())?).map_err(|_| "bad_signature")?;
        }
        if signed_by.len() < auth.threshold {
            return Err("signature_threshold");
        }
        Ok((next, signed_by.into_iter().collect(), body.to_vec()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CachePolicy {
    Full,
    Lazy,
    FavouritesOnly,
}

/// A cache entry supplied from this profile's durable local blob cache before activation.
/// The host keeps these bytes content-addressed by digest. Core revalidates on use.
pub struct CachedBlob<'a> {
    pub blob_id: [u8; 32],
    pub ciphertext: &'a [u8],
}

pub struct Install<'a> {
    pub release: &'a [u8],
    pub expected_release: [u8; 32],
    pub control: Option<&'a [u8]>,
    /// The release ID named by the user's confirmation. It cannot approve a different update.
    pub confirmed_release: [u8; 32],
    pub policy: CachePolicy,
    pub favourites: &'a [String],
    pub cached: &'a [CachedBlob<'a>],
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Stored {
    manifest: Option<SecretHex>,
    release_id: [u8; 32],
    sequence: u32,
    pack_id: [u8; 32],
    control: Option<String>,
    control_envelope_hash: Option<[u8; 32]>,
    signed_by: Vec<[u8; 16]>,
    forked_from: Option<[u8; 32]>,
    policy: CachePolicy,
    favourites: BTreeSet<String>,
    cached: BTreeSet<[u8; 32]>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    recents: BTreeMap<String, u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct Packs(BTreeMap<String, Stored>);

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackView {
    #[serde(serialize_with = "serialize_release_id")]
    pub release_id: [u8; 32],
    pub title: String,
    pub signed_by: Vec<[u8; 16]>,
    pub item_count: usize,
    pub cache_policy: CachePolicy,
    pub favourites: BTreeSet<String>,
    pub cached_blobs: usize,
}

fn serialize_release_id<S: serde::Serializer>(
    id: &[u8; 32],
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&hex(id))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PickerItem {
    item_id: String,
    blob_id: String,
    ciphertext_digest: String,
    kind: &'static str,
    label: String,
    alt_text: String,
    fallback_text: Option<String>,
    reaction_key: Option<String>,
    cached: bool,
    favourite: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPicker {
    pub pack_id: String,
    pub pack: PackView,
    pub items: Vec<PickerItem>,
    pub recents: Vec<String>,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PickerChange {
    Favourite { present: bool },
    Recent,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    pack_id: String,
    pub release_id: String,
    sequence: u32,
    title: String,
    description: Option<String>,
    creator_display: Option<String>,
    source_urls: Vec<String>,
    spdx_id: Option<String>,
    license_url: Option<String>,
    license_text_hash: Option<String>,
    item_count: usize,
    size: String,
    controller_count: usize,
    threshold: usize,
    signed_by: Vec<[u8; 16]>,
    pub control_digest: Option<String>,
}

struct Lineage {
    control: Option<String>,
    control_envelope_hash: Option<[u8; 32]>,
    signed_by: Vec<[u8; 16]>,
    forked_from: Option<[u8; 32]>,
}

impl Packs {
    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub(crate) fn install(&mut self, input: Install<'_>) -> Result<(), Error> {
        let release = Release::parse(input.release)?;
        if input.expected_release != release.id {
            return Err("release_id_mismatch");
        }
        if input.confirmed_release != release.id {
            return Err("confirmation_required");
        }
        let lineage = self.lineage(&release, input.control)?;
        let key = hex(&release.pack_id);
        let old = self.0.get(&key);

        if input.favourites.len() > release.items.len() || input.cached.len() > release.items.len()
        {
            return Err("value_range");
        }
        let mut favourites = BTreeSet::new();
        for id in input.favourites {
            if !release.items.contains_key(id) || !favourites.insert(id.clone()) {
                return Err("item_missing_or_duplicate");
            }
        }
        let eligible: BTreeMap<_, _> = release
            .items
            .iter()
            .filter(|(id, _)| {
                input.policy != CachePolicy::FavouritesOnly || favourites.contains(*id)
            })
            .map(|(_, a)| (a.attachment.blob_id, a))
            .collect();
        let mut cached = BTreeSet::new();
        for blob in input.cached {
            let a = eligible
                .get(&blob.blob_id)
                .ok_or("blob_outside_pack_or_policy")?;
            if !cached.insert(blob.blob_id) {
                return Err("blob_duplicate");
            }
            if !matches!(a.open(blob.ciphertext), Ok(Opened::Renderable { .. })) {
                return Err("invalid_cached_blob");
            }
        }
        if input.policy != CachePolicy::Lazy && cached.len() != eligible.len() {
            return Err("cache_incomplete");
        }
        self.0.insert(
            key,
            Stored {
                manifest: Some(hex(input.release).into()),
                release_id: release.id,
                sequence: release.sequence,
                pack_id: release.pack_id,
                control: lineage.control,
                control_envelope_hash: lineage.control_envelope_hash,
                signed_by: lineage.signed_by,
                forked_from: lineage.forked_from,
                policy: input.policy,
                favourites,
                cached,
                recents: old
                    .filter(|p| p.release_id == release.id)
                    .map(|p| p.recents.clone())
                    .unwrap_or_default(),
            },
        );
        Ok(())
    }

    fn lineage(&self, release: &Release, supplied: Option<&[u8]>) -> Result<Lineage, Error> {
        let key = hex(&release.pack_id);
        let old = self.0.get(&key);
        let old_control = old.and_then(|p| p.control.as_deref());
        if supplied.is_some_and(|bytes| {
            bytes.len() > registry().types["ContentPackControlState"].max_bytes
        }) {
            return Err("size_limit");
        }
        let incoming_hash = supplied.map(|bytes| crate::crypto::sha256(&[bytes]));
        let (control, control_envelope_hash, signed_by) = match (old, supplied, old_control) {
            (None, _, _) if release.sequence != 1 => return Err("sequence_skip"),
            (Some(p), _, control)
                if p.release_id == release.id
                    && (incoming_hash.is_none() || p.control_envelope_hash == incoming_hash) =>
            {
                (
                    control.map(str::to_owned),
                    p.control_envelope_hash,
                    p.signed_by.clone(),
                )
            }
            (Some(_), _, None) | (Some(_), None, _) => return Err("updates_frozen"),
            (previous, Some(signed), old_control) => {
                if let Some(p) = previous
                    && p.release_id != release.id
                {
                    release.follows(p.release_id, p.sequence)?;
                }
                let parent = old_control
                    .map(|s| Control::body(&wire(unhex(s))?))
                    .transpose()?;
                let (_, signers, body) = Control::verify(signed, parent.as_ref(), release)?;
                (Some(hex(&body)), incoming_hash, signers)
            }
            (None, None, _) => (None, None, Vec::new()),
        };
        let forked_from = old.and_then(|p| p.forked_from).or(release.forked_from);
        self.check_fork(release, forked_from)?;

        Ok(Lineage {
            control,
            control_envelope_hash,
            signed_by,
            forked_from,
        })
    }

    pub(crate) fn preview(
        &self,
        bytes: &[u8],
        control: Option<&[u8]>,
    ) -> Result<PackPreview, Error> {
        let release = Release::parse(bytes)?;
        let lineage = self.lineage(&release, control)?;
        let head = lineage
            .control
            .as_deref()
            .map(|s| Control::body(&wire(unhex(s))?))
            .transpose()?;
        let body = wire(cbor::decode(bytes))?;
        let string = |value: Option<&V>| value.and_then(|v| v.t().ok()).map(str::to_owned);
        let provenance = wire(body.get(9))?;
        let licensing = body.opt(10);
        let size = release.declared_size();
        Ok(PackPreview {
            pack_id: hex(&release.pack_id),
            release_id: hex(&release.id),
            sequence: release.sequence,
            title: release.title,
            description: string(body.opt(6)),
            creator_display: string(provenance.opt(0)),
            source_urls: provenance
                .opt(1)
                .and_then(|v| v.a().ok())
                .unwrap_or(&[])
                .iter()
                .filter_map(|v| string(Some(v)))
                .collect(),
            spdx_id: string(licensing.and_then(|v| v.opt(0))),
            license_url: string(licensing.and_then(|v| v.opt(1))),
            license_text_hash: licensing
                .and_then(|v| v.opt(2))
                .map(digest)
                .transpose()?
                .map(|id| hex(&id)),
            item_count: release.items.len(),
            size: size.to_string(),
            controller_count: head.as_ref().map_or(0, |c| c.controllers.len()),
            threshold: head.as_ref().map_or(0, |c| c.threshold),
            signed_by: lineage.signed_by,
            control_digest: lineage.control_envelope_hash.map(|id| hex(&id)),
        })
    }

    fn check_fork(&self, release: &Release, mut target: Option<[u8; 32]>) -> Result<(), Error> {
        let mut visited = BTreeSet::from([release.id]);
        while let Some(id) = target {
            if !visited.insert(id) {
                return Err("fork_cycle");
            }
            target = None;
            if let Some(held) = self.0.values().find(|held| held.release_id == id) {
                if held.pack_id == release.pack_id {
                    return Err("fork_pack_id");
                }
                target = held.forked_from;
            }
        }
        Ok(())
    }

    pub(crate) fn remove(&mut self, id: [u8; 32]) -> Result<(), Error> {
        let p = self
            .0
            .get_mut(&hex(&id))
            .filter(|p| p.manifest.is_some())
            .ok_or("pack_missing")?;
        // Retain the lineage pin so reinstall cannot let a new controller take over the ID.
        p.manifest = None;
        p.favourites.clear();
        p.cached.clear();
        p.recents.clear();
        Ok(())
    }

    pub(crate) fn evict(&mut self, id: [u8; 32], blob: [u8; 32]) -> Result<(), Error> {
        let p = self
            .0
            .get_mut(&hex(&id))
            .filter(|p| p.manifest.is_some())
            .ok_or("pack_missing")?;
        p.cached.remove(&blob);
        Ok(())
    }

    pub(crate) fn release(&self, id: [u8; 32]) -> Option<Release> {
        let p = self.0.get(&hex(&id)).filter(|p| p.manifest.is_some())?;
        let bytes = Zeroizing::new(unhex(p.manifest.as_deref()?).ok()?);
        let release = Release::parse(&bytes).ok()?;
        (release.id == p.release_id && release.pack_id == id).then_some(release)
    }

    pub(crate) fn list(&self) -> Vec<(String, PackView)> {
        self.0
            .values()
            .filter_map(|p| self.view(p.pack_id).map(|view| (hex(&p.pack_id), view)))
            .collect()
    }

    pub(crate) fn picker(&self, id: [u8; 32]) -> Option<PackPicker> {
        let p = self.0.get(&hex(&id))?;
        let release = self.release(id)?;
        let mut recent: Vec<_> = p.recents.iter().collect();
        recent.sort_unstable_by_key(|(_, order)| std::cmp::Reverse(**order));
        Some(PackPicker {
            pack_id: hex(&id),
            pack: self.view(id)?,
            recents: recent.into_iter().map(|(item, _)| item.clone()).collect(),
            items: release
                .items
                .iter()
                .map(|(item_id, asset)| PickerItem {
                    item_id: item_id.clone(),
                    blob_id: hex(&asset.attachment.blob_id),
                    ciphertext_digest: hex(&asset.attachment.ciphertext_digest),
                    kind: asset.presentation.kind.as_str(),
                    label: release.labels[item_id].clone(),
                    alt_text: asset.presentation.alt_text.clone(),
                    fallback_text: asset.presentation.fallback_text.clone(),
                    reaction_key: asset.presentation.reaction_key().map(str::to_owned),
                    cached: p.cached.contains(&asset.attachment.blob_id),
                    favourite: p.favourites.contains(item_id),
                })
                .collect(),
        })
    }

    pub(crate) fn cache_digests(&self) -> BTreeSet<String> {
        self.0
            .values()
            .flat_map(|p| {
                self.release(p.pack_id)
                    .into_iter()
                    .flat_map(|r| r.items.into_values())
                    .filter(|a| p.cached.contains(&a.attachment.blob_id))
                    .map(|a| hex(&a.attachment.ciphertext_digest))
            })
            .collect()
    }

    pub(crate) fn cached_asset(
        &self,
        pack: [u8; 32],
        release: [u8; 32],
        item: &str,
    ) -> Option<AssetRef> {
        let p = self.0.get(&hex(&pack))?;
        if p.release_id != release {
            return None;
        }
        let asset = self.release(pack)?.items.remove(item)?;
        p.cached
            .contains(&asset.attachment.blob_id)
            .then_some(asset)
    }

    pub(crate) fn update_picker(
        &mut self,
        id: [u8; 32],
        release_id: [u8; 32],
        item: &str,
        change: PickerChange,
    ) -> Result<(), Error> {
        let release = self.release(id).ok_or("pack_missing")?;
        if release.id != release_id {
            return Err("release_id_mismatch");
        }
        let asset = release.item(item).ok_or("item_missing_or_duplicate")?;
        let p = self.0.get_mut(&hex(&id)).ok_or("pack_missing")?;
        match change {
            PickerChange::Favourite { present: true } => {
                p.favourites.insert(item.to_owned());
            }
            PickerChange::Favourite { present: false } => {
                p.favourites.remove(item);
                let blob = asset.attachment.blob_id;
                if p.policy == CachePolicy::FavouritesOnly
                    && !p.favourites.iter().any(|id| {
                        release
                            .item(id)
                            .is_some_and(|item| item.attachment.blob_id == blob)
                    })
                {
                    p.cached.remove(&blob);
                }
            }
            PickerChange::Recent => {
                let order = p.recents.values().copied().max().unwrap_or(0);
                let order = order.checked_add(1).ok_or("value_range")?;
                p.recents.insert(item.to_owned(), order);
            }
        }
        Ok(())
    }

    pub(crate) fn view(&self, id: [u8; 32]) -> Option<PackView> {
        let p = self.0.get(&hex(&id))?;
        let r = self.release(id)?;
        Some(PackView {
            release_id: r.id,
            title: r.title,
            signed_by: p.signed_by.clone(),
            item_count: r.items.len(),
            cache_policy: p.policy,
            favourites: p.favourites.clone(),
            cached_blobs: p.cached.len(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::read::GzDecoder;
    use serde_json::Value;

    fn pack(value: &Value) -> Vec<u8> {
        match value {
            Value::String(s) => unhex(s).unwrap(),
            Value::Array(segments) => segments
                .iter()
                .flat_map(|segment| {
                    if segment.is_string() {
                        pack(segment)
                    } else {
                        let b = unhex(segment["fill"].as_str().unwrap()).unwrap()[0];
                        vec![b; segment["len"].as_u64().unwrap() as usize]
                    }
                })
                .collect(),
            _ => panic!("invalid frozen fixture"),
        }
    }

    #[test]
    fn hostile_manifest_corpus_matches_frozen_verdicts() {
        let doc: Value = serde_json::from_reader(GzDecoder::new(
            &include_bytes!("../../../../verification/vectors/expressive-wire-vectors.json.gz")[..],
        ))
        .unwrap();
        for row in doc["vectors"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|row| row["object"] == "ContentPackRelease")
        {
            let result = Release::parse(&pack(&row["hex"]));
            let id = row["id"].as_str().unwrap();
            if row["expect"] == "reject" {
                assert_eq!(result.err(), row["error"].as_str(), "{id}");
            } else if row["semantic"]["verdict"] == "invalid" {
                assert_eq!(result.err(), row["semantic"]["rule"].as_str(), "{id}");
            } else {
                let release = result.unwrap_or_else(|err| panic!("{id}: {err}"));
                let hash = row["derived"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|d| d["kind"] == "hash")
                    .unwrap();
                assert_eq!(hex(&release.id), hash["sha256"].as_str().unwrap(), "{id}");
            }
        }
    }

    #[test]
    fn forks_do_not_extend_other_packs_or_cycle_through_held_lineage() {
        let release = Release {
            id: [1; 32],
            pack_id: [11; 32],
            sequence: 1,
            previous: None,
            forked_from: Some([2; 32]),
            title: "Fork".into(),
            items: BTreeMap::new(),
            labels: BTreeMap::new(),
        };
        let anchor = Stored {
            manifest: None,
            release_id: [2; 32],
            sequence: 2,
            pack_id: [22; 32],
            control: None,
            control_envelope_hash: None,
            signed_by: vec![],
            forked_from: None,
            policy: CachePolicy::Lazy,
            favourites: BTreeSet::new(),
            cached: BTreeSet::new(),
            recents: BTreeMap::new(),
        };
        let mut packs = Packs(BTreeMap::from([("source".into(), anchor)]));
        assert_eq!(packs.check_fork(&release, release.forked_from), Ok(()));
        assert_eq!(release.follows([2; 32], 2), Err("sequence_skip"));
        assert_eq!(
            packs.check_fork(&release, Some(release.id)),
            Err("fork_cycle")
        );
        packs.0.get_mut("source").unwrap().forked_from = Some(release.id);
        assert_eq!(
            packs.check_fork(&release, release.forked_from),
            Err("fork_cycle")
        );
        packs.0.get_mut("source").unwrap().pack_id = release.pack_id;
        assert_eq!(
            packs.check_fork(&release, release.forked_from),
            Err("fork_pack_id")
        );
    }
}
