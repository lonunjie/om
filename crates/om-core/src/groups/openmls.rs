// SPDX-License-Identifier: MPL-2.0
//! The real MLS engine behind [`GroupCrypto`]: stock OpenMLS 0.9.0 with the RustCrypto provider, unmodified.
//!
//! **DEPENDS ON ADR-0187, which is a proposed delegated draft and is not accepted.** This whole module, the
//! `groups-openmls` feature, the two plumbing shims under `third_party/openmls-wasm-shims/`, the `[patch.crates-io]`
//! block in the workspace manifest and the `groups-openmls` feature of `om-wasm` exist only because of it. To revert:
//! delete this file, its `mod` line in `groups.rs`, the feature and optional dependencies in the two manifests, the
//! patch block, the shim directory and the engine-specific test files. Nothing else in Core refers to this module.
//!
//! What it is, in OM terms:
//!
//! * One value is one MLS client = one DeviceIdentity. The credential is OpenMLS' `BasicCredential` whose identity
//!   is the DeviceID plus the device-signed attestation of the leaf's dedicated signature key (so `members` and
//!   `committer` are DeviceIDs, as the trait says). The leaf key is generated here and is not the device's key. The suite is the v1 suite
//!   `MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519`; there is no other and no choice (CRYPTOGRAPHY_AND_AGILITY).
//! * **Randomness and time (ADR-0187 decision 3).** The stock library cannot be handed a per-call generator, so
//!   [`CryptoEnv::rng`] and [`CryptoEnv::now_ms`] are *not consumed* by this engine. The library draws entropy from
//!   `getrandom`, which in a wasm build is routed to the host import `om_host.random_fill` (the same source and
//!   route as Core's own `OsRng`), and reads the clock through the `web-time` shim, routed to `om_host.now_ms`.
//!   No new generator and no new cryptography is written here. The module comment on [`super::mls`] says the same.
//! * **Storage (D-g10).** The library's in-memory store is copied out by [`GroupCrypto::snapshot`] into an OM-owned,
//!   versioned, length-prefixed record (not the library's own persistence format, not `serde_json`), wrapped by
//!   [`super::store::GroupCryptoRec`] with a hash and the engine tag. The record holds MLS private key material, as the
//!   fake's did not; it is part of `AppState`, so the host's encrypted-at-rest storage protects it like the device keys.
//! * **Pending Commit (ADR-0163).** `add_members`/`remove_members` leave the library's own pending-commit state
//!   (stored with the group), and the exact Commit and Welcome bytes are kept beside it so [`GroupCrypto::pending`]
//!   returns identical bytes after a restore. `merge_pending` and `discard_pending` close it.
//! * The Room's encrypted context (`context_ext`, which carries the ConversationID) is an MLS GroupContext extension
//!   of the private-use type [`OM_CONTEXT_EXT`]; every KeyPackage this engine makes declares support for it. The code
//!   point is an engineering placeholder: it needs a registry entry before any group ships (report, "What remains").

use super::mls::{
    CommitInfo, CryptoEnv, Decrypted, Endpoint, EpochInfo, GroupCrypto, GroupCryptoLoader,
    KeyPackage as OmKeyPackage, MlsError, MlsResult, PreparedCommit,
};
use crate::crypto::sha256;
use ::openmls::prelude::tls_codec::{Deserialize as TlsDe, Serialize as TlsSer};
use ::openmls::prelude::*;
use ::openmls_basic_credential::SignatureKeyPair;
use ::openmls_rust_crypto::OpenMlsRustCrypto;
use ::openmls_traits::OpenMlsProvider;
use std::collections::BTreeMap;

/// Stable engine tag stored beside the snapshot.
pub const OPENMLS_TAG: &str = "openmls-0.9";
/// The one cipher suite of v1 groups.
const CS: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519;
/// Private-use MLS extension type carrying Core's encrypted Room context (ConversationID). Placeholder, see module docs.
pub const OM_CONTEXT_EXT: u16 = 0xF04D;
/// Snapshot format written by this build.
const FORMAT: u32 = 2;
const MAGIC: &[u8; 8] = b"OMMLSv01";

const E: fn(&'static str) -> MlsError = MlsError;

/// The BasicCredential identity: one length byte, the DeviceID, then the device-signed attestation of the leaf key
/// (the rest). The attestation is empty only for an engine-level test client.
fn identity(device: &[u8], attestation: &[u8]) -> Vec<u8> {
    let mut v = vec![device.len() as u8];
    v.extend_from_slice(device);
    v.extend_from_slice(attestation);
    v
}

/// Split a credential's identity into (DeviceID, attestation); both empty if it is not in the form above.
fn split_identity(c: &Credential) -> (Vec<u8>, Vec<u8>) {
    let Ok(b) = BasicCredential::try_from(c.clone()) else {
        return (vec![], vec![]);
    };
    let id = b.identity();
    match id.split_first() {
        Some((n, rest)) if rest.len() >= *n as usize => {
            let (d, a) = rest.split_at(*n as usize);
            (d.to_vec(), a.to_vec())
        }
        _ => (vec![], vec![]),
    }
}

/// Pending Commit bytes kept beside the library's pending-commit state.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Held {
    commit: Vec<u8>,
    welcome: Option<Vec<u8>>,
    resulting_epoch: u64,
}

/// The OpenMLS-backed engine. See the module documentation.
pub struct OpenMlsGroupCrypto {
    device: Vec<u8>,
    provider: OpenMlsRustCrypto,
    signer: SignatureKeyPair,
    /// The device's signature over (DeviceID, leaf key), carried in the credential; empty until Core sets it.
    attestation: Vec<u8>,
    pending: BTreeMap<Vec<u8>, Held>,
}

fn caps() -> Capabilities {
    Capabilities::new(
        None,
        Some(&[CS]),
        Some(&[ExtensionType::Unknown(OM_CONTEXT_EXT)]),
        None,
        None,
    )
}

impl OpenMlsGroupCrypto {
    /// A new client for `device_id` with a fresh signature key dedicated to MLS (entropy from the host, see the
    /// module docs). The device's own signing key never enters the engine: Core has the device sign this leaf key and
    /// stores the result with [`GroupCrypto::set_attestation`].
    pub fn new(device_id: &[u8]) -> MlsResult<Self> {
        let provider = OpenMlsRustCrypto::default();
        let signer =
            SignatureKeyPair::new(CS.signature_algorithm()).map_err(|_| E("keygen_failed"))?;
        signer
            .store(provider.storage())
            .map_err(|_| E("keygen_failed"))?;
        Ok(OpenMlsGroupCrypto {
            device: device_id.to_vec(),
            provider,
            signer,
            attestation: vec![],
            pending: BTreeMap::new(),
        })
    }

    fn cred(&self) -> CredentialWithKey {
        CredentialWithKey {
            credential: BasicCredential::new(identity(&self.device, &self.attestation)).into(),
            signature_key: self.signer.to_public_vec().into(),
        }
    }

    fn load(&self, gid: &[u8]) -> MlsResult<MlsGroup> {
        MlsGroup::load(self.provider.storage(), &GroupId::from_slice(gid))
            .map_err(|_| E("storage_failed"))?
            .ok_or(E("unknown_group"))
    }

    fn create_config(ctx_ext: Option<&[u8]>) -> MlsResult<MlsGroupCreateConfig> {
        let mut b = MlsGroupCreateConfig::builder()
            .ciphersuite(CS)
            .use_ratchet_tree_extension(true)
            .max_past_epochs(0)
            .capabilities(caps());
        if let Some(c) = ctx_ext {
            let ext = Extension::Unknown(OM_CONTEXT_EXT, UnknownExtension(c.to_vec()));
            let exts = Extensions::single(ext).map_err(|_| E("context_extension_invalid"))?;
            b = b.with_group_context_extensions(exts);
        }
        Ok(b.build())
    }

    fn parse_kp(&self, bytes: &[u8]) -> MlsResult<KeyPackage> {
        let kp =
            KeyPackageIn::tls_deserialize_exact(bytes).map_err(|_| E("key_package_invalid"))?;
        kp.validate(self.provider.crypto(), ProtocolVersion::Mls10)
            .map_err(|e| match e {
                KeyPackageVerifyError::LifetimeError(_) => E("key_package_expired"),
                _ => E("key_package_invalid"),
            })
    }

    /// The DeviceID a credential names (empty when the credential is not one of ours).
    fn ident(c: &Credential) -> Vec<u8> {
        split_identity(c).0
    }

    /// The leaf as Core reads it: the DeviceID and attestation of the credential, and the leaf's signature key.
    fn endpoint(c: &Credential, key: &[u8]) -> Endpoint {
        let (device, attestation) = split_identity(c);
        Endpoint {
            device,
            signature_key: key.to_vec(),
            attestation,
        }
    }

    fn prepared(
        &self,
        gid: &[u8],
        commit: &MlsMessageOut,
        welcome: Option<&MlsMessageOut>,
        g: &MlsGroup,
    ) -> MlsResult<PreparedCommit> {
        let ser = |m: &MlsMessageOut| {
            m.tls_serialize_detached()
                .map_err(|_| E("serialize_failed"))
        };
        let p = PreparedCommit {
            commit: ser(commit)?,
            welcome: welcome.map(ser).transpose()?,
            resulting_epoch: g.epoch().as_u64() + 1,
        };
        let _ = gid;
        Ok(p)
    }

    fn hold(&mut self, gid: &[u8], p: &PreparedCommit) {
        self.pending.insert(
            gid.to_vec(),
            Held {
                commit: p.commit.clone(),
                welcome: p.welcome.clone(),
                resulting_epoch: p.resulting_epoch,
            },
        );
    }

    /// Parse a received handshake or application message, check it is for `g`'s current epoch.
    fn protocol_message(g: &MlsGroup, bytes: &[u8]) -> MlsResult<ProtocolMessage> {
        let m = MlsMessageIn::tls_deserialize_exact(bytes).map_err(|_| E("message_invalid"))?;
        let pm = m
            .try_into_protocol_message()
            .map_err(|_| E("message_invalid"))?;
        if pm.epoch() != g.epoch() {
            return Err(E("wrong_epoch"));
        }
        Ok(pm)
    }

    /// A throwaway provider holding a copy of this client's store, so a Commit can be read without applying it.
    fn scratch(&self) -> OpenMlsRustCrypto {
        let p = OpenMlsRustCrypto::default();
        let src = self.provider.storage().values.read().expect("store lock");
        *p.storage().values.write().expect("store lock") = src.clone();
        p
    }
}

impl GroupCrypto for OpenMlsGroupCrypto {
    fn engine_tag(&self) -> &'static str {
        OPENMLS_TAG
    }

    fn device_id(&self) -> &[u8] {
        &self.device
    }

    fn signature_key(&self) -> Vec<u8> {
        self.signer.to_public_vec()
    }

    fn set_attestation(&mut self, attestation: Vec<u8>) -> MlsResult<()> {
        if self.device.len() > 255 || attestation.len() > 8192 {
            return Err(E("attestation_invalid"));
        }
        self.attestation = attestation;
        Ok(())
    }

    fn attestation(&self) -> Vec<u8> {
        self.attestation.clone()
    }

    fn key_package_endpoint(&self, key_package: &[u8]) -> MlsResult<Endpoint> {
        let kp = self.parse_kp(key_package)?;
        Ok(Self::endpoint(
            kp.leaf_node().credential(),
            kp.leaf_node().signature_key().as_slice(),
        ))
    }

    fn welcome_endpoints(&self, welcome: &[u8]) -> MlsResult<Vec<Endpoint>> {
        let m = MlsMessageIn::tls_deserialize_exact(welcome).map_err(|_| E("welcome_invalid"))?;
        let MlsMessageBodyIn::Welcome(w) = m.extract() else {
            return Err(E("welcome_invalid"));
        };
        let cfg = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .max_past_epochs(0)
            .build();
        // On a scratch copy of the store, so no KeyPackage of ours is consumed by reading.
        let scratch = self.scratch();
        let staged =
            StagedWelcome::new_from_welcome(&scratch, &cfg, w, None).map_err(|e| match e {
                WelcomeError::NoMatchingKeyPackage => E("no_matching_key_package"),
                _ => E("welcome_rejected"),
            })?;
        Ok(staged
            .members()
            .map(|m| Self::endpoint(&m.credential, &m.signature_key))
            .collect())
    }

    fn key_package(&mut self, _env: &mut CryptoEnv<'_>) -> MlsResult<OmKeyPackage> {
        let bundle = KeyPackage::builder()
            .leaf_node_capabilities(caps())
            .build(CS, &self.provider, &self.signer, self.cred())
            .map_err(|_| E("key_package_failed"))?;
        let bytes = bundle
            .key_package()
            .tls_serialize_detached()
            .map_err(|_| E("serialize_failed"))?;
        let reference = sha256(&[&bytes]);
        Ok(OmKeyPackage { bytes, reference })
    }

    fn create_group(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        context_ext: &[u8],
    ) -> MlsResult<()> {
        if MlsGroup::load(self.provider.storage(), &GroupId::from_slice(group_id))
            .map_err(|_| E("storage_failed"))?
            .is_some()
        {
            return Err(E("group_exists"));
        }
        let cfg = Self::create_config(Some(context_ext))?;
        MlsGroup::new_with_group_id(
            &self.provider,
            &self.signer,
            &cfg,
            GroupId::from_slice(group_id),
            self.cred(),
        )
        .map_err(|_| E("create_failed"))?;
        Ok(())
    }

    fn add_commit(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        key_packages: &[Vec<u8>],
    ) -> MlsResult<PreparedCommit> {
        let mut g = self.load(group_id)?;
        if self.pending.contains_key(group_id) || g.pending_commit().is_some() {
            return Err(E("commit_pending"));
        }
        let kps = key_packages
            .iter()
            .map(|b| self.parse_kp(b))
            .collect::<MlsResult<Vec<_>>>()?;
        let (commit, welcome, _gi) = g
            .add_members(&self.provider, &self.signer, &kps)
            .map_err(|_| E("add_failed"))?;
        let p = self.prepared(group_id, &commit, Some(&welcome), &g)?;
        self.hold(group_id, &p);
        Ok(p)
    }

    fn remove_commit(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        devices: &[Vec<u8>],
    ) -> MlsResult<PreparedCommit> {
        let mut g = self.load(group_id)?;
        if self.pending.contains_key(group_id) || g.pending_commit().is_some() {
            return Err(E("commit_pending"));
        }
        let own = g.own_leaf_index();
        let mut leaves = Vec::new();
        for d in devices {
            let idx = g
                .members()
                .find(|m| Self::ident(&m.credential) == *d)
                .map(|m| m.index)
                .ok_or(E("not_a_member"))?;
            if idx == own {
                return Err(E("cannot_remove_self"));
            }
            leaves.push(idx);
        }
        let (commit, welcome, _gi) = g
            .remove_members(&self.provider, &self.signer, &leaves)
            .map_err(|_| E("remove_failed"))?;
        let p = self.prepared(group_id, &commit, welcome.as_ref(), &g)?;
        self.hold(group_id, &p);
        Ok(p)
    }

    fn update_commit(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
    ) -> MlsResult<PreparedCommit> {
        let mut g = self.load(group_id)?;
        if self.pending.contains_key(group_id) || g.pending_commit().is_some() {
            return Err(E("commit_pending"));
        }
        let bundle = g
            .self_update(&self.provider, &self.signer, LeafNodeParameters::default())
            .map_err(|_| E("update_failed"))?;
        let commit = bundle.into_commit();
        let p = self.prepared(group_id, &commit, None, &g)?;
        self.hold(group_id, &p);
        Ok(p)
    }

    fn pending(&self, group_id: &[u8]) -> MlsResult<Option<PreparedCommit>> {
        self.load(group_id)?;
        Ok(self.pending.get(group_id).map(|h| PreparedCommit {
            commit: h.commit.clone(),
            welcome: h.welcome.clone(),
            resulting_epoch: h.resulting_epoch,
        }))
    }

    fn merge_pending(&mut self, group_id: &[u8]) -> MlsResult<EpochInfo> {
        let mut g = self.load(group_id)?;
        if !self.pending.contains_key(group_id) {
            return Err(E("no_pending_commit"));
        }
        g.merge_pending_commit(&self.provider)
            .map_err(|_| E("merge_failed"))?;
        self.pending.remove(group_id);
        Ok(EpochInfo {
            epoch: g.epoch().as_u64(),
            still_member: g.is_active(),
        })
    }

    fn discard_pending(&mut self, group_id: &[u8]) -> MlsResult<()> {
        let mut g = self.load(group_id)?;
        g.clear_pending_commit(self.provider.storage())
            .map_err(|_| E("storage_failed"))?;
        self.pending.remove(group_id);
        Ok(())
    }

    fn inspect_commit(&self, group_id: &[u8], commit: &[u8]) -> MlsResult<CommitInfo> {
        // Our own pending Commit cannot be opened by its author (MLS forbids it); Core already knows what it made.
        if let Some(h) = self.pending.get(group_id).filter(|h| h.commit == commit) {
            let epoch = self.load(group_id)?.epoch().as_u64();
            return Ok(CommitInfo {
                from_epoch: epoch,
                resulting_epoch: h.resulting_epoch,
                committer: self.device.clone(),
                added: vec![],
                committer_leaf: None,
            });
        }
        let scratch = self.scratch();
        let mut g = MlsGroup::load(scratch.storage(), &GroupId::from_slice(group_id))
            .map_err(|_| E("storage_failed"))?
            .ok_or(E("unknown_group"))?;
        let from_epoch = g.epoch().as_u64();
        // A competing candidate of ours does not stop us reading someone else's Commit (on the scratch copy only).
        g.clear_pending_commit(scratch.storage())
            .map_err(|_| E("storage_failed"))?;
        let pm = Self::protocol_message(&g, commit)
            // a Commit for another epoch is reported as such; anything unreadable is "not a commit"
            .map_err(|e| {
                if e.0 == "wrong_epoch" {
                    e
                } else {
                    E("not_a_commit")
                }
            })?;
        let processed = g
            .process_message(&scratch, pm)
            .map_err(|_| E("not_a_commit"))?;
        let committer = Self::ident(processed.credential());
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(sc) => Ok(CommitInfo {
                from_epoch,
                resulting_epoch: from_epoch + 1,
                committer,
                added: sc
                    .add_proposals()
                    .map(|a| {
                        let leaf = a.add_proposal().key_package().leaf_node();
                        Self::endpoint(leaf.credential(), leaf.signature_key().as_slice())
                    })
                    .collect(),
                committer_leaf: sc
                    .update_path_leaf_node()
                    .map(|leaf| Self::endpoint(leaf.credential(), leaf.signature_key().as_slice())),
            }),
            _ => Err(E("not_a_commit")),
        }
    }

    fn process_commit(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        commit: &[u8],
    ) -> MlsResult<EpochInfo> {
        let mut g = self.load(group_id)?;
        if self.pending.contains_key(group_id) || g.pending_commit().is_some() {
            return Err(E("commit_pending"));
        }
        let pm = Self::protocol_message(&g, commit)?;
        let processed = g
            .process_message(&self.provider, pm)
            .map_err(|_| E("commit_rejected"))?;
        match processed.into_content() {
            ProcessedMessageContent::StagedCommitMessage(sc) => {
                g.merge_staged_commit(&self.provider, *sc)
                    .map_err(|_| E("merge_failed"))?;
                Ok(EpochInfo {
                    epoch: g.epoch().as_u64(),
                    still_member: g.is_active(),
                })
            }
            _ => Err(E("not_a_commit")),
        }
    }

    fn process_welcome(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        welcome: &[u8],
    ) -> MlsResult<(Vec<u8>, EpochInfo)> {
        let m = MlsMessageIn::tls_deserialize_exact(welcome).map_err(|_| E("welcome_invalid"))?;
        let MlsMessageBodyIn::Welcome(w) = m.extract() else {
            return Err(E("welcome_invalid"));
        };
        let cfg = MlsGroupJoinConfig::builder()
            .use_ratchet_tree_extension(true)
            .max_past_epochs(0)
            .build();
        let staged = StagedWelcome::new_from_welcome(&self.provider, &cfg, w, None).map_err(
            |e| match e {
                WelcomeError::NoMatchingKeyPackage => E("no_matching_key_package"),
                _ => E("welcome_rejected"),
            },
        )?;
        let g = staged
            .into_group(&self.provider)
            .map_err(|_| E("welcome_rejected"))?;
        Ok((
            g.group_id().as_slice().to_vec(),
            EpochInfo {
                epoch: g.epoch().as_u64(),
                still_member: g.is_active(),
            },
        ))
    }

    fn encrypt_application(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        plaintext: &[u8],
    ) -> MlsResult<Vec<u8>> {
        let mut g = self.load(group_id)?;
        if self.pending.contains_key(group_id) || g.pending_commit().is_some() {
            return Err(E("commit_pending"));
        }
        if !g.is_active() {
            return Err(E("unknown_group"));
        }
        let m = g
            .create_message(&self.provider, &self.signer, plaintext)
            .map_err(|_| E("encrypt_failed"))?;
        m.tls_serialize_detached()
            .map_err(|_| E("serialize_failed"))
    }

    fn decrypt_application(&mut self, group_id: &[u8], ciphertext: &[u8]) -> MlsResult<Decrypted> {
        let mut g = self.load(group_id)?;
        if !g.is_active() {
            return Err(E("unknown_group"));
        }
        let epoch = g.epoch().as_u64();
        let pm = Self::protocol_message(&g, ciphertext)?;
        let processed = g
            .process_message(&self.provider, pm)
            .map_err(|_| E("decrypt_failed"))?;
        let sender = Self::ident(processed.credential());
        match processed.into_content() {
            ProcessedMessageContent::ApplicationMessage(a) => Ok(Decrypted {
                plaintext: a.into_bytes(),
                sender,
                epoch,
            }),
            _ => Err(E("not_application")),
        }
    }

    fn epoch(&self, group_id: &[u8]) -> MlsResult<u64> {
        Ok(self.load(group_id)?.epoch().as_u64())
    }

    fn export_secret(
        &self,
        group_id: &[u8],
        label: &str,
        context: &[u8],
        len: usize,
    ) -> MlsResult<Vec<u8>> {
        let g = self.load(group_id)?;
        g.export_secret(self.provider.crypto(), label, context, len)
            .map_err(|_| E("export_failed"))
    }

    fn members(&self, group_id: &[u8]) -> MlsResult<Vec<Vec<u8>>> {
        let g = self.load(group_id)?;
        let mut v: Vec<Vec<u8>> = g.members().map(|m| Self::ident(&m.credential)).collect();
        v.sort();
        Ok(v)
    }

    fn snapshot(&self) -> Vec<u8> {
        let mut w = Vec::new();
        w.extend_from_slice(MAGIC);
        put_u32(&mut w, FORMAT);
        put_bytes(&mut w, &self.device);
        put_bytes(&mut w, &self.signer.to_public_vec());
        put_bytes(&mut w, &self.attestation);
        // the library store, copied key by key in sorted order so equal states give equal bytes
        let sorted: BTreeMap<Vec<u8>, Vec<u8>> = self
            .provider
            .storage()
            .values
            .read()
            .expect("store lock")
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        put_u32(&mut w, sorted.len() as u32);
        for (k, v) in &sorted {
            put_bytes(&mut w, k);
            put_bytes(&mut w, v);
        }
        put_u32(&mut w, self.pending.len() as u32);
        for (gid, h) in &self.pending {
            put_bytes(&mut w, gid);
            put_bytes(&mut w, &h.commit);
            match &h.welcome {
                Some(x) => {
                    w.push(1);
                    put_bytes(&mut w, x);
                }
                None => w.push(0),
            }
            w.extend_from_slice(&h.resulting_epoch.to_be_bytes());
        }
        w
    }
}

fn put_u32(w: &mut Vec<u8>, n: u32) {
    w.extend_from_slice(&n.to_be_bytes());
}
fn put_bytes(w: &mut Vec<u8>, b: &[u8]) {
    put_u32(w, b.len() as u32);
    w.extend_from_slice(b);
}

struct Rd<'a>(&'a [u8]);
impl<'a> Rd<'a> {
    fn take(&mut self, n: usize) -> MlsResult<&'a [u8]> {
        if self.0.len() < n {
            return Err(E("snapshot_corrupt"));
        }
        let (a, b) = self.0.split_at(n);
        self.0 = b;
        Ok(a)
    }
    fn u32(&mut self) -> MlsResult<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().expect("4 bytes"),
        ))
    }
    fn bytes(&mut self) -> MlsResult<Vec<u8>> {
        let n = self.u32()? as usize;
        Ok(self.take(n)?.to_vec())
    }
}

/// Rebuilds [`OpenMlsGroupCrypto`] from a snapshot.
pub struct OpenMlsLoader;

impl GroupCryptoLoader for OpenMlsLoader {
    type Engine = OpenMlsGroupCrypto;

    fn engine_tag() -> &'static str {
        OPENMLS_TAG
    }

    fn restore(snapshot: &[u8]) -> MlsResult<OpenMlsGroupCrypto> {
        let mut r = Rd(snapshot);
        if r.take(8)? != MAGIC {
            return Err(E("snapshot_corrupt"));
        }
        let format = r.u32()?;
        if format > FORMAT {
            return Err(E("snapshot_too_new"));
        }
        let device = r.bytes()?;
        let sig_pk = r.bytes()?;
        // format 1 had no attestation: such a client never binds until Core attests it again
        let attestation = if format >= 2 { r.bytes()? } else { vec![] };
        let provider = OpenMlsRustCrypto::default();
        let n = r.u32()?;
        {
            let mut m = provider.storage().values.write().expect("store lock");
            for _ in 0..n {
                let k = r.bytes()?;
                let v = r.bytes()?;
                m.insert(k, v);
            }
        }
        let mut pending = BTreeMap::new();
        for _ in 0..r.u32()? {
            let gid = r.bytes()?;
            let commit = r.bytes()?;
            let welcome = match r.take(1)?[0] {
                0 => None,
                1 => Some(r.bytes()?),
                _ => return Err(E("snapshot_corrupt")),
            };
            let resulting_epoch = u64::from_be_bytes(r.take(8)?.try_into().expect("8 bytes"));
            pending.insert(
                gid,
                Held {
                    commit,
                    welcome,
                    resulting_epoch,
                },
            );
        }
        let signer = SignatureKeyPair::read(provider.storage(), &sig_pk, CS.signature_algorithm())
            .ok_or(E("snapshot_corrupt"))?;
        Ok(OpenMlsGroupCrypto {
            device,
            provider,
            signer,
            attestation,
            pending,
        })
    }
}
