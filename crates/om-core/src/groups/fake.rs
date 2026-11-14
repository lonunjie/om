// SPDX-License-Identifier: MPL-2.0
//! TEST-ONLY deterministic in-memory MLS stand-in. It is NOT MLS and provides NO confidentiality, authentication or
//! forward secrecy: "keys" are SHA-256 chains over public bytes and every message is readable JSON. It exists so the
//! parts of Core that surround an MLS engine (the commit journal, the storage adapter, binding, ordering, crash
//! replay) can be built and tested before the production engine's dependency decision is made (groups plan,
//! "Slice 2 dependency gate"). It shares the [`GroupCrypto`] contract and nothing else with a real engine; a test
//! passing against it is evidence about Core's logic, never about cryptography.
//!
//! Compiled only for tests and the `test-loopback` feature, which a `production` build refuses (R-072).

use super::mls::*;
use crate::crypto::{hex, sha256, unhex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const FAKE_TAG: &str = "fake-test-only-v1";
const FORMAT: u32 = 1;
const KP_LIFETIME_MS: u64 = 3 * 24 * 3600 * 1000;

const E: fn(&'static str) -> MlsError = MlsError;

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FKeyPackage {
    device: String,
    nonce: String,
    not_after: u64,
    /// The leaf as `hex(signature key)|hex(attestation)`.
    #[serde(default)]
    sig: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FCommit {
    group_id: String,
    from_epoch: u64,
    committer: String,
    adds: Vec<FKeyPackage>,
    removes: Vec<String>,
    nonce: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FWelcome {
    group_id: String,
    epoch: u64,
    members: Vec<String>,
    secret: String,
    ctx_ext: String,
    /// `SHA-256(kp bytes)` of each added KeyPackage.
    recipients: Vec<String>,
    /// device (hex) -> leaf signature key (hex), every member.
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FPending {
    commit: String,
    welcome: Option<String>,
    resulting_epoch: u64,
    members: Vec<String>,
    secret: String,
    #[serde(default)]
    keys: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FGroup {
    epoch: u64,
    #[serde(default)]
    keys: BTreeMap<String, String>,
    members: Vec<String>,
    secret: String,
    ctx_ext: String,
    pending: Option<FPending>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Snapshot {
    format: u32,
    device: String,
    #[serde(default)]
    sig: String,
    #[serde(default)]
    att: String,
    /// kp reference (hex) -> the KeyPackage bytes (hex), single use.
    key_packages: BTreeMap<String, String>,
    groups: BTreeMap<String, FGroup>,
}

/// The test-only engine. See the module documentation.
#[derive(Clone, Debug)]
pub struct FakeGroupCrypto {
    device: Vec<u8>,
    sig: Vec<u8>,
    att: Vec<u8>,
    key_packages: BTreeMap<String, String>,
    groups: BTreeMap<String, FGroup>,
}

fn next_secret(old: &str, commit: &[u8]) -> String {
    hex(&sha256(&[b"FAKE-EPOCH", old.as_bytes(), commit]))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct FApp {
    group_id: String,
    epoch: u64,
    sender: String,
    nonce: String,
    body: String,
    mac: String,
}

/// Test-only keystream; NOT a cipher.
fn app_xor(secret: &str, epoch: u64, sender: &str, nonce: &str, data: &[u8]) -> Vec<u8> {
    let mut ks = Vec::new();
    let mut i = 0u32;
    while ks.len() < data.len() {
        ks.extend(sha256(&[
            b"FAKE-APP-KS",
            secret.as_bytes(),
            &epoch.to_be_bytes(),
            sender.as_bytes(),
            nonce.as_bytes(),
            &i.to_be_bytes(),
        ]));
        i += 1;
    }
    data.iter().zip(ks).map(|(a, b)| a ^ b).collect()
}

fn app_mac(secret: &str, epoch: u64, sender: &str, nonce: &str, body: &[u8]) -> String {
    hex(&sha256(&[
        b"FAKE-APP-MAC",
        secret.as_bytes(),
        &epoch.to_be_bytes(),
        sender.as_bytes(),
        nonce.as_bytes(),
        body,
    ]))
}

fn frame(kind: &str, body: &impl Serialize) -> Vec<u8> {
    let mut v = format!("FAKE-MLS-{kind}\0").into_bytes();
    v.extend(serde_json::to_vec(body).expect("fake message serializes"));
    v
}

fn unframe<T: for<'de> Deserialize<'de>>(kind: &str, bytes: &[u8]) -> MlsResult<T> {
    let p = format!("FAKE-MLS-{kind}\0").into_bytes();
    let body = bytes.strip_prefix(p.as_slice()).ok_or(E("malformed"))?;
    serde_json::from_slice(body).map_err(|_| E("malformed"))
}

impl FakeGroupCrypto {
    /// A client with a dedicated leaf key derived from its DeviceID (never the device's own key).
    pub fn new(device_id: &[u8]) -> Self {
        FakeGroupCrypto {
            device: device_id.to_vec(),
            sig: sha256(&[b"FAKE-LEAF-SIG", device_id]).to_vec(),
            att: vec![],
            key_packages: BTreeMap::new(),
            groups: BTreeMap::new(),
        }
    }

    /// This client's leaf as stored in a KeyPackage or a group: `hex(key)|hex(attestation)`.
    fn leaf(&self) -> String {
        format!("{}|{}", hex(&self.sig), hex(&self.att))
    }

    fn g(&self, id: &[u8]) -> MlsResult<&FGroup> {
        self.groups.get(&hex(id)).ok_or(E("unknown_group"))
    }

    fn g_mut(&mut self, id: &[u8]) -> MlsResult<&mut FGroup> {
        self.groups.get_mut(&hex(id)).ok_or(E("unknown_group"))
    }

    fn me(&self) -> String {
        hex(&self.device)
    }

    fn prepare(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        adds: Vec<FKeyPackage>,
        add_bytes: &[Vec<u8>],
        removes: Vec<String>,
    ) -> MlsResult<PreparedCommit> {
        let me = self.me();
        let g = self.g(group_id)?.clone();
        if g.pending.is_some() {
            return Err(E("commit_pending"));
        }
        if !g.members.contains(&me) {
            return Err(E("not_a_member"));
        }
        for a in &adds {
            if g.members.contains(&a.device) {
                return Err(E("already_member"));
            }
        }
        for r in &removes {
            if !g.members.contains(r) {
                return Err(E("not_a_member"));
            }
            if *r == self.me() {
                // RFC 9420 section 12.4.3.2: a committer cannot remove itself.
                return Err(E("cannot_remove_self"));
            }
        }
        let c = FCommit {
            group_id: hex(group_id),
            from_epoch: g.epoch,
            committer: me,
            adds,
            removes: removes.clone(),
            nonce: hex(&crate::crypto::rng::fill(env.rng, 16)),
        };
        let commit = frame("COMMIT", &c);
        let mut members = g.members.clone();
        members.retain(|m| !removes.contains(m));
        members.extend(c.adds.iter().map(|a| a.device.clone()));
        members.sort();
        let mut keys = g.keys.clone();
        keys.retain(|d, _| !removes.contains(d));
        keys.extend(c.adds.iter().map(|a| (a.device.clone(), a.sig.clone())));
        let secret = next_secret(&g.secret, &commit);
        let welcome = if c.adds.is_empty() {
            None
        } else {
            Some(frame(
                "WELCOME",
                &FWelcome {
                    group_id: hex(group_id),
                    epoch: g.epoch + 1,
                    members: members.clone(),
                    secret: secret.clone(),
                    ctx_ext: g.ctx_ext.clone(),
                    recipients: add_bytes.iter().map(|b| hex(&sha256(&[b]))).collect(),
                    keys: keys.clone(),
                },
            ))
        };
        let p = FPending {
            commit: hex(&commit),
            welcome: welcome.as_ref().map(|w| hex(w)),
            resulting_epoch: g.epoch + 1,
            members,
            secret,
            keys,
        };
        self.g_mut(group_id)?.pending = Some(p);
        Ok(PreparedCommit {
            commit,
            welcome,
            resulting_epoch: g.epoch + 1,
        })
    }
}

fn to_prepared(p: &FPending) -> PreparedCommit {
    PreparedCommit {
        commit: unhex(&p.commit).expect("own hex"),
        welcome: p.welcome.as_ref().map(|w| unhex(w).expect("own hex")),
        resulting_epoch: p.resulting_epoch,
    }
}

/// Parse `hex(key)|hex(attestation)` (a value without `|` has no attestation).
fn parse_leaf(device: &str, leaf: &str) -> MlsResult<Endpoint> {
    let (k, a) = leaf.split_once('|').unwrap_or((leaf, ""));
    Ok(Endpoint {
        device: unhex(device).map_err(|_| E("malformed"))?,
        signature_key: unhex(k).map_err(|_| E("malformed"))?,
        attestation: unhex(a).map_err(|_| E("malformed"))?,
    })
}

impl GroupCrypto for FakeGroupCrypto {
    fn engine_tag(&self) -> &'static str {
        FAKE_TAG
    }

    fn device_id(&self) -> &[u8] {
        &self.device
    }

    fn signature_key(&self) -> Vec<u8> {
        self.sig.clone()
    }

    fn set_attestation(&mut self, attestation: Vec<u8>) -> MlsResult<()> {
        self.att = attestation;
        Ok(())
    }

    fn attestation(&self) -> Vec<u8> {
        self.att.clone()
    }

    fn key_package_endpoint(&self, key_package: &[u8]) -> MlsResult<Endpoint> {
        let kp: FKeyPackage = unframe("KP", key_package).map_err(|_| E("key_package_invalid"))?;
        parse_leaf(&kp.device, &kp.sig).map_err(|_| E("key_package_invalid"))
    }

    fn welcome_endpoints(&self, welcome: &[u8]) -> MlsResult<Vec<Endpoint>> {
        let w: FWelcome = unframe("WELCOME", welcome).map_err(|_| E("welcome_invalid"))?;
        if !w
            .recipients
            .iter()
            .any(|r| self.key_packages.contains_key(r))
        {
            return Err(E("no_matching_key_package"));
        }
        w.members
            .iter()
            .map(|m| {
                parse_leaf(m, w.keys.get(m).ok_or(E("welcome_invalid"))?)
                    .map_err(|_| E("welcome_invalid"))
            })
            .collect()
    }

    fn key_package(&mut self, env: &mut CryptoEnv<'_>) -> MlsResult<KeyPackage> {
        let kp = FKeyPackage {
            device: self.me(),
            nonce: hex(&crate::crypto::rng::fill(env.rng, 16)),
            not_after: env.now_ms + KP_LIFETIME_MS,
            sig: self.leaf(),
        };
        let bytes = frame("KP", &kp);
        let reference = sha256(&[&bytes]);
        self.key_packages.insert(hex(&reference), hex(&bytes));
        Ok(KeyPackage { bytes, reference })
    }

    fn create_group(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        context_ext: &[u8],
    ) -> MlsResult<()> {
        if self.groups.contains_key(&hex(group_id)) {
            return Err(E("group_exists"));
        }
        self.groups.insert(
            hex(group_id),
            FGroup {
                epoch: 0,
                members: vec![self.me()],
                keys: BTreeMap::from([(self.me(), self.leaf())]),
                secret: hex(&sha256(&[b"FAKE-GENESIS", group_id, context_ext])),
                ctx_ext: hex(context_ext),
                pending: None,
            },
        );
        Ok(())
    }

    fn add_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        key_packages: &[Vec<u8>],
    ) -> MlsResult<PreparedCommit> {
        if key_packages.is_empty() {
            return Err(E("empty_add"));
        }
        let mut adds = Vec::new();
        for b in key_packages {
            let kp: FKeyPackage = unframe("KP", b)?;
            if env.now_ms > kp.not_after {
                return Err(E("key_package_expired"));
            }
            adds.push(kp);
        }
        self.prepare(env, group_id, adds, key_packages, Vec::new())
    }

    fn remove_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        devices: &[Vec<u8>],
    ) -> MlsResult<PreparedCommit> {
        if devices.is_empty() {
            return Err(E("empty_remove"));
        }
        let removes = devices.iter().map(|d| hex(d)).collect();
        self.prepare(env, group_id, Vec::new(), &[], removes)
    }

    fn update_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
    ) -> MlsResult<PreparedCommit> {
        self.prepare(env, group_id, Vec::new(), &[], Vec::new())
    }

    fn pending(&self, group_id: &[u8]) -> MlsResult<Option<PreparedCommit>> {
        Ok(self.g(group_id)?.pending.as_ref().map(to_prepared))
    }

    fn merge_pending(&mut self, group_id: &[u8]) -> MlsResult<EpochInfo> {
        let g = self.g_mut(group_id)?;
        let p = g.pending.take().ok_or(E("no_pending_commit"))?;
        g.epoch = p.resulting_epoch;
        g.members = p.members;
        g.keys = p.keys;
        g.secret = p.secret;
        Ok(EpochInfo {
            epoch: g.epoch,
            still_member: true,
        })
    }

    fn discard_pending(&mut self, group_id: &[u8]) -> MlsResult<()> {
        self.g_mut(group_id)?.pending = None;
        Ok(())
    }

    fn inspect_commit(&self, group_id: &[u8], commit: &[u8]) -> MlsResult<CommitInfo> {
        self.g(group_id)?;
        let c: FCommit = unframe("COMMIT", commit).map_err(|_| E("not_a_commit"))?;
        if c.group_id != hex(group_id) {
            return Err(E("wrong_group"));
        }
        Ok(CommitInfo {
            from_epoch: c.from_epoch,
            resulting_epoch: c.from_epoch + 1,
            committer: unhex(&c.committer).map_err(|_| E("malformed"))?,
            added: c
                .adds
                .iter()
                .map(|a| parse_leaf(&a.device, &a.sig))
                .collect::<MlsResult<_>>()?,
            committer_leaf: None,
        })
    }

    fn process_commit(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        commit: &[u8],
    ) -> MlsResult<EpochInfo> {
        let info = self.inspect_commit(group_id, commit)?;
        let c: FCommit = unframe("COMMIT", commit)?;
        let me = self.me();
        let g = self.g(group_id)?;
        if g.pending.is_some() {
            return Err(E("commit_pending"));
        }
        if info.from_epoch != g.epoch {
            return Err(E("wrong_epoch"));
        }
        if c.committer == me {
            return Err(E("own_commit"));
        }
        if !g.members.contains(&c.committer) {
            return Err(E("not_a_member"));
        }
        let mut members = g.members.clone();
        members.retain(|m| !c.removes.contains(m));
        members.extend(c.adds.iter().map(|a| a.device.clone()));
        members.sort();
        let mut keys = g.keys.clone();
        keys.retain(|d, _| !c.removes.contains(d));
        keys.extend(c.adds.iter().map(|a| (a.device.clone(), a.sig.clone())));
        let secret = next_secret(&g.secret, commit);
        let epoch = g.epoch + 1;
        if c.removes.contains(&me) {
            self.groups.remove(&hex(group_id));
            return Ok(EpochInfo {
                epoch,
                still_member: false,
            });
        }
        let g = self.g_mut(group_id)?;
        g.epoch = epoch;
        g.members = members;
        g.keys = keys;
        g.secret = secret;
        Ok(EpochInfo {
            epoch,
            still_member: true,
        })
    }

    fn process_welcome(
        &mut self,
        _env: &mut CryptoEnv<'_>,
        welcome: &[u8],
    ) -> MlsResult<(Vec<u8>, EpochInfo)> {
        let w: FWelcome = unframe("WELCOME", welcome)?;
        let used = w
            .recipients
            .iter()
            .find(|r| self.key_packages.contains_key(*r))
            .cloned()
            .ok_or(E("no_matching_key_package"))?;
        if self.groups.contains_key(&w.group_id) {
            return Err(E("group_exists"));
        }
        self.key_packages.remove(&used);
        let id = unhex(&w.group_id).map_err(|_| E("malformed"))?;
        self.groups.insert(
            w.group_id,
            FGroup {
                epoch: w.epoch,
                members: w.members,
                keys: w.keys,
                secret: w.secret,
                ctx_ext: w.ctx_ext,
                pending: None,
            },
        );
        Ok((
            id,
            EpochInfo {
                epoch: w.epoch,
                still_member: true,
            },
        ))
    }

    fn encrypt_application(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        plaintext: &[u8],
    ) -> MlsResult<Vec<u8>> {
        let me = self.me();
        let g = self.g(group_id)?;
        if g.pending.is_some() {
            return Err(E("commit_pending"));
        }
        let nonce = hex(&crate::crypto::rng::fill(env.rng, 8));
        let (epoch, secret) = (g.epoch, g.secret.clone());
        let body = app_xor(&secret, epoch, &me, &nonce, plaintext);
        let mac = app_mac(&secret, epoch, &me, &nonce, &body);
        Ok(frame(
            "APP",
            &FApp {
                group_id: hex(group_id),
                epoch,
                sender: me,
                nonce,
                body: hex(&body),
                mac,
            },
        ))
    }

    fn decrypt_application(&mut self, group_id: &[u8], ciphertext: &[u8]) -> MlsResult<Decrypted> {
        let m: FApp = unframe("APP", ciphertext)?;
        let g = self.g(group_id)?;
        if m.group_id != hex(group_id) {
            return Err(E("malformed"));
        }
        if m.epoch != g.epoch {
            return Err(E("wrong_epoch"));
        }
        if !g.members.contains(&m.sender) {
            return Err(E("not_a_member"));
        }
        let body = unhex(&m.body).map_err(|_| E("malformed"))?;
        if m.mac != app_mac(&g.secret, m.epoch, &m.sender, &m.nonce, &body) {
            return Err(E("bad_mac"));
        }
        Ok(Decrypted {
            plaintext: app_xor(&g.secret, m.epoch, &m.sender, &m.nonce, &body),
            sender: unhex(&m.sender).map_err(|_| E("malformed"))?,
            epoch: m.epoch,
        })
    }

    fn epoch(&self, group_id: &[u8]) -> MlsResult<u64> {
        Ok(self.g(group_id)?.epoch)
    }

    fn export_secret(
        &self,
        group_id: &[u8],
        label: &str,
        context: &[u8],
        len: usize,
    ) -> MlsResult<Vec<u8>> {
        let s = &self.g(group_id)?.secret;
        let mut out = Vec::new();
        let mut i = 0u32;
        while out.len() < len {
            out.extend(sha256(&[
                b"FAKE-EXPORT",
                s.as_bytes(),
                label.as_bytes(),
                context,
                &i.to_be_bytes(),
            ]));
            i += 1;
        }
        out.truncate(len);
        Ok(out)
    }

    fn members(&self, group_id: &[u8]) -> MlsResult<Vec<Vec<u8>>> {
        self.g(group_id)?
            .members
            .iter()
            .map(|m| unhex(m).map_err(|_| E("malformed")))
            .collect()
    }

    fn snapshot(&self) -> Vec<u8> {
        serde_json::to_vec(&Snapshot {
            format: FORMAT,
            device: self.me(),
            sig: hex(&self.sig),
            att: hex(&self.att),
            key_packages: self.key_packages.clone(),
            groups: self.groups.clone(),
        })
        .expect("snapshot serializes")
    }
}

/// Loader for [`FakeGroupCrypto`] snapshots.
pub struct FakeLoader;

impl GroupCryptoLoader for FakeLoader {
    type Engine = FakeGroupCrypto;

    fn engine_tag() -> &'static str {
        FAKE_TAG
    }

    fn restore(snapshot: &[u8]) -> MlsResult<FakeGroupCrypto> {
        let s: Snapshot = serde_json::from_slice(snapshot).map_err(|_| E("snapshot_corrupt"))?;
        if s.format != FORMAT {
            return Err(E("snapshot_unsupported_format"));
        }
        Ok(FakeGroupCrypto {
            device: unhex(&s.device).map_err(|_| E("snapshot_corrupt"))?,
            sig: unhex(&s.sig).map_err(|_| E("snapshot_corrupt"))?,
            att: unhex(&s.att).map_err(|_| E("snapshot_corrupt"))?,
            key_packages: s.key_packages,
            groups: s.groups,
        })
    }
}
