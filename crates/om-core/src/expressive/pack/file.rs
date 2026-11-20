// SPDX-License-Identifier: MPL-2.0
//! ADR-0254 local interchange. The contained W1 objects retain their own authority.

use super::*;
use om_codec::{Limits, Value};

pub const PACK_FILE_MAX: usize = 32 * 1024 * 1024;
const OVERHEAD: u128 = 64 * 1024;
const FORMAT: &str = "om.content-pack-file.v1";

struct File {
    release: Vec<u8>,
    controls: Vec<Vec<u8>>,
    predecessors: Option<Vec<Vec<u8>>>,
    blobs: BTreeMap<[u8; 32], Vec<u8>>,
}

fn bytes(value: Value) -> Result<Vec<u8>, Error> {
    match value {
        Value::Bytes(bytes) => Ok(bytes),
        _ => Err("pack_file_shape"),
    }
}

fn array(value: Value) -> Result<Vec<Value>, Error> {
    match value {
        Value::Array(values) => Ok(values),
        _ => Err("pack_file_shape"),
    }
}

fn chain(value: Value) -> Result<Vec<Vec<u8>>, Error> {
    let values = array(value)?;
    if values.len() > 64 {
        return Err("pack_chain_limit");
    }
    values.into_iter().map(bytes).collect()
}

impl File {
    fn decode(input: &[u8]) -> Result<(Self, Release), Error> {
        let limits = Limits {
            max_depth: 5,
            max_items: 512,
            max_string: (6 << 20) + crate::attach::TAG_LEN,
            ..Limits::OM
        };
        let Value::Map(fields) = om_codec::decode(input, limits, PACK_FILE_MAX)? else {
            return Err("pack_file_shape");
        };
        let mut fields = fields.into_iter();
        let mut next = |key| match fields.next() {
            Some((Value::Uint(k), v)) if k == key => Ok(v),
            _ => Err("pack_file_shape"),
        };
        if next(1)? != Value::Text(FORMAT.into()) {
            return Err("pack_file_format");
        }
        let manifest = bytes(next(2)?)?;
        let target = Release::parse(&manifest)?;
        let controls = chain(next(3)?)?;
        let entries = array(next(4)?)?;
        let predecessors = match fields.next() {
            Some((Value::Uint(5), v)) if target.sequence > 1 => Some(chain(v)?),
            None => None,
            _ => return Err("pack_file_shape"),
        };
        if fields.next().is_some() {
            return Err("pack_file_shape");
        }
        if entries.len() > target.items.len() {
            return Err("pack_blob_count");
        }

        let mut expected: BTreeMap<_, Vec<&AssetRef>> = BTreeMap::new();
        let mut distinct = BTreeSet::new();
        for asset in target.items.values() {
            let attachment = &asset.attachment;
            if distinct.insert(attachment.blob_id) {
                expected
                    .entry(attachment.ciphertext_digest)
                    .or_default()
                    .push(asset);
            }
        }
        if (input.len() as u128) > target.declared_size() + OVERHEAD {
            return Err("size_limit");
        }
        let mut blobs = BTreeMap::new();
        for entry in entries {
            let Value::Map(fields) = entry else {
                return Err("pack_file_shape");
            };
            let [
                (Value::Text(bytes_key), body),
                (Value::Text(digest_key), Value::Map(hash)),
            ] = <[_; 2]>::try_from(fields).map_err(|_| "pack_file_shape")?
            else {
                return Err("pack_file_shape");
            };
            let [
                (Value::Uint(0), Value::Uint(1)),
                (Value::Uint(1), Value::Bytes(id)),
            ] = <[_; 2]>::try_from(hash).map_err(|_| "pack_file_shape")?
            else {
                return Err("pack_file_shape");
            };
            if bytes_key != "bytes" || digest_key != "digest" {
                return Err("pack_file_shape");
            }
            let id: [u8; 32] = id.try_into().map_err(|_| "pack_file_shape")?;
            let body = bytes(body)?;
            let assets = expected.get(&id).ok_or("pack_blob_unlisted")?;
            if blobs.contains_key(&id) {
                return Err("blob_duplicate");
            }
            for asset in assets {
                let Ok(Opened::Renderable { plaintext, .. }) = asset.open(&body) else {
                    return Err("invalid_cached_blob");
                };
                drop(Zeroizing::new(plaintext));
            }
            blobs.insert(id, body);
        }
        if blobs.len() != expected.len() {
            return Err("cache_incomplete");
        }
        Ok((
            Self {
                release: manifest,
                controls,
                predecessors,
                blobs,
            },
            target,
        ))
    }
}

pub(crate) struct ReviewedPack {
    pub(crate) preview: PackPreview,
    baseline: Packs,
    next: Packs,
    blobs: std::collections::btree_map::IntoIter<[u8; 32], Vec<u8>>,
    begun: bool,
}

impl ReviewedPack {
    pub(crate) fn begin(
        &mut self,
        current: &Packs,
        release_id: &str,
        control_digest: Option<&str>,
    ) -> Result<usize, Error> {
        if self.begun
            || self.preview.release_id != release_id
            || self.preview.control_digest.as_deref() != control_digest
            || current != &self.baseline
        {
            return Err("confirmation_required");
        }
        self.begun = true;
        Ok(self.blobs.len())
    }

    pub(crate) fn stage(&mut self) -> Result<([u8; 32], Vec<u8>), Error> {
        if !self.begun {
            return Err("confirmation_required");
        }
        self.blobs.next().ok_or("pack_stage_incomplete")
    }

    pub(crate) fn activate(self, current: &mut Packs) -> Result<(), Error> {
        if !self.begun || self.blobs.len() != 0 {
            return Err("pack_stage_incomplete");
        }
        if *current != self.baseline {
            return Err("confirmation_required");
        }
        *current = self.next;
        Ok(())
    }
}

impl Packs {
    pub(crate) fn review_file(&self, input: &[u8]) -> Result<ReviewedPack, Error> {
        let (file, target) = File::decode(input)?;
        let key = hex(&target.pack_id);
        let held = self.0.get(&key);
        let mut next = self.clone();

        let mut manifests = Vec::new();
        let mut replay_from_genesis = false;
        if let Some(predecessors) = &file.predecessors {
            let mut parent = None;
            for bytes in predecessors {
                let release = Release::parse(bytes)?;
                if release.pack_id != target.pack_id {
                    return Err("pack_id_changed");
                }
                if let Some((id, sequence)) = parent {
                    release.follows(id, sequence)?;
                }
                parent = Some((release.id, release.sequence));
                manifests.push((release, bytes.as_slice()));
            }
            let (id, sequence) = parent.ok_or("pack_predecessors")?;
            target.follows(id, sequence)?;
            let first = &manifests[0].0;
            if first.sequence == 1 {
                self.check_fork(first, first.forked_from)?;
                replay_from_genesis = true;
                next.0.remove(&key);
            } else if !held.is_some_and(|p| {
                first.id == p.release_id || first.follows(p.release_id, p.sequence).is_ok()
            }) {
                return Err("pack_needs_earlier_version");
            }
        } else if target.sequence > 1
            && !held.is_some_and(|p| {
                target.follows(p.release_id, p.sequence).is_ok() || target.id == p.release_id
            })
        {
            return Err("pack_needs_earlier_version");
        }
        manifests.push((target, &file.release));

        let mut index = 0;
        let mut matched = false;
        let mut anchored = !replay_from_genesis || held.is_none();
        for envelope in &file.controls {
            // Inspect only to select the manifest. The unchanged lineage verifier checks
            // the canonical envelope, the state link, and the controlling threshold.
            registry().check("ContentPackControlState", envelope, false)?;
            let env = wire(cbor::decode(envelope))?;
            let body = wire(cbor::decode(wire(wire(env.get(1))?.b())?))?;
            let committed = digest(wire(body.get(7))?)?;
            if matched && manifests[index].0.id != committed {
                index += 1;
            }
            let (release, bytes) = manifests.get(index).ok_or("release_id_mismatch")?;
            if release.id != committed {
                return Err("release_id_mismatch");
            }
            next.install(Install {
                release: bytes,
                expected_release: release.id,
                confirmed_release: release.id,
                control: Some(envelope),
                policy: CachePolicy::Lazy,
                favourites: &[],
                cached: &[],
            })?;
            matched = true;
            let pin = &next.0[&key];
            anchored |=
                held.is_some_and(|p| p.release_id == pin.release_id && p.control == pin.control);
        }
        let (release, bytes) = &manifests[manifests.len() - 1];
        if index + 1 != manifests.len() || !anchored {
            return Err("pack_predecessors");
        }
        if !matched {
            if release.sequence > 1 {
                return Err("pack_predecessors");
            }
            next.install(Install {
                release: bytes,
                expected_release: release.id,
                confirmed_release: release.id,
                control: None,
                policy: CachePolicy::Lazy,
                favourites: &[],
                cached: &[],
            })?;
        }
        let preview = next.preview(bytes, None)?;
        let installed = next.0.get_mut(&key).ok_or("pack_predecessors")?;
        installed.cached = release
            .items
            .values()
            .map(|a| a.attachment.blob_id)
            .collect();
        installed.policy = CachePolicy::Full;
        if let Some(p) = held.filter(|p| p.release_id == release.id) {
            installed.recents.clone_from(&p.recents);
            installed.favourites.clone_from(&p.favourites);
        }
        Ok(ReviewedPack {
            preview,
            baseline: self.clone(),
            next,
            blobs: file.blobs.into_iter(),
            begun: false,
        })
    }
}

#[cfg(test)]
mod tests;
