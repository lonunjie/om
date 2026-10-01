// SPDX-License-Identifier: Apache-2.0
// Feasibility prototype for V-019. Not product code. Not for production. Test-provider keys only.
//
// OM-level authenticated migration of an MLS group from the v1 classical suite to a hybrid-PQ suite, built on
// OpenMLS 0.9.0 `MlsGroup::reboot` (OpenMLS does not implement RFC 9420 ReInit). Every result is a boolean,
// count or fixed string so the JSON is identical on every runtime.
//
// `Transition` is a DIAGNOSTIC STAND-IN for the later-wave GroupControlTransition. It is not a wire format.
use openmls::group::StagedWelcome;
use openmls::prelude::tls_codec::{Deserialize as TlsDeserialize, Serialize as TlsSerialize};
use openmls::prelude::*;
use openmls_basic_credential::SignatureKeyPair;
use openmls_rust_crypto::{OpenMlsRustCrypto, RustCrypto};
use openmls_traits::crypto::OpenMlsCrypto;
use openmls_traits::signatures::Signer;
use openmls_traits::types::HashType;
use openmls_traits::OpenMlsProvider;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

pub const CS_V1: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519; // 0x0001
/// Experimental hybrid-PQ suite: X-Wing (ML-KEM-768 + X25519) KEM, AES-128-GCM, SHA-256, Ed25519 signatures.
pub const CS_HYBRID: Ciphersuite = Ciphersuite::MLS_128_MLKEM768X25519_AES128GCM_SHA256_Ed25519; // 0x004F
pub const CS_HYBRID_ALT: Ciphersuite = Ciphersuite::MLS_128_MLKEM768X25519_AES256GCM_SHA384_Ed25519; // 0x004E
pub const CS_V1_CHACHA: Ciphersuite = Ciphersuite::MLS_128_DHKEMX25519_CHACHA20POLY1305_SHA256_Ed25519; // 0x0003

const SIG_LABEL: &[u8] = b"OM-V019-DIAG-GroupControlTransition-v0\0";
const STATE_LABEL: &[u8] = b"OM-V019-DIAG-GroupControlState-v0\0";
const BIND_LABEL: &[u8] = b"OM-V019-DIAG-GroupContextBinding-v0\0";
/// Private-use MLS extension type carrying the OM binding inside the new group's GroupContext.
const EXT_OM_BINDING: u16 = 0xFF51;
const PAST_EPOCHS: usize = 2;

type E = Box<dyn std::error::Error>;
type R<T> = Result<T, E>;

// ------------------------------------------------------------------------------------------------
// Profile registry stand-in: ciphersuite id -> security_generation (ADR-0165: the only ordering axis).
// ------------------------------------------------------------------------------------------------
pub fn registry_generation(cs: u16) -> Option<u64> {
    match cs {
        0x0001 | 0x0003 => Some(1), // classical v1 profiles
        0x004E | 0x004F => Some(2), // experimental hybrid-PQ profiles
        _ => None,
    }
}

// ------------------------------------------------------------------------------------------------
// Diagnostic stand-ins: roster / GroupControlState, authority, floor, transition.
// ------------------------------------------------------------------------------------------------
fn lp(out: &mut Vec<u8>, b: &[u8]) {
    out.extend_from_slice(&(b.len() as u32).to_be_bytes());
    out.extend_from_slice(b);
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

mod hexser {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(b: &Vec<u8>, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&super::hex(b))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(serde::de::Error::custom))
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct RosterEntry {
    pub device_id: String,
    pub sig_pub: Vec<u8>,
    pub admin: bool,
}

/// Stand-in for GroupControlState: conversation id + roster (device, signature key, role).
#[derive(Clone, Debug)]
pub struct Roster {
    pub conversation_id: Vec<u8>,
    pub entries: Vec<RosterEntry>,
}

impl Roster {
    pub fn state_hash(&self) -> Vec<u8> {
        let mut e: Vec<&RosterEntry> = self.entries.iter().collect();
        e.sort_by(|a, b| a.device_id.cmp(&b.device_id));
        let mut buf = STATE_LABEL.to_vec();
        lp(&mut buf, &self.conversation_id);
        for x in e {
            lp(&mut buf, x.device_id.as_bytes());
            lp(&mut buf, &x.sig_pub);
            buf.push(x.admin as u8);
        }
        RustCrypto::default().hash(HashType::Sha2_256, &buf).unwrap()
    }
    pub fn find(&self, device_id: &str) -> Option<&RosterEntry> {
        self.entries.iter().find(|e| e.device_id == device_id)
    }
    pub fn authority(&self) -> Authority {
        Authority { migrate: self.entries.iter().filter(|e| e.admin).map(|e| e.device_id.clone()).collect() }
    }
}

/// Devices allowed to author a profile migration (here: roster admins).
#[derive(Clone, Debug)]
pub struct Authority {
    pub migrate: BTreeSet<String>,
}

/// Monotone security floor plus the identity of the current MLS group (what a client persists).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Floor {
    pub conversation_id: Vec<u8>,
    pub mls_group_id: Vec<u8>,
    pub generation: u64,
    pub ciphersuite: u16,
    pub state_hash: Vec<u8>,
}

impl Floor {
    /// Applies an already-verified transition. Idempotent: applying it again leaves the floor unchanged.
    pub fn apply(&mut self, t: &Transition) {
        self.mls_group_id = t.new_mls_group_id.clone();
        self.generation = t.new_security_generation;
        self.ciphersuite = t.new_ciphersuite;
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Transition {
    #[serde(with = "hexser")]
    pub conversation_id: Vec<u8>,
    #[serde(with = "hexser")]
    pub old_mls_group_id: Vec<u8>,
    #[serde(with = "hexser")]
    pub new_mls_group_id: Vec<u8>,
    pub old_security_generation: u64,
    pub new_security_generation: u64,
    pub old_ciphersuite: u16,
    pub new_ciphersuite: u16,
    #[serde(with = "hexser")]
    pub group_control_state_hash: Vec<u8>,
    pub author_device_id: String,
    #[serde(with = "hexser")]
    pub signature: Vec<u8>,
}

impl Transition {
    /// Canonical bytes: domain-separation label, then every field in fixed order, length-prefixed.
    pub fn signed_bytes(&self, with_label: bool) -> Vec<u8> {
        let mut b = if with_label { SIG_LABEL.to_vec() } else { Vec::new() };
        lp(&mut b, &self.conversation_id);
        lp(&mut b, &self.old_mls_group_id);
        lp(&mut b, &self.new_mls_group_id);
        lp(&mut b, &self.old_security_generation.to_be_bytes());
        lp(&mut b, &self.new_security_generation.to_be_bytes());
        lp(&mut b, &self.old_ciphersuite.to_be_bytes());
        lp(&mut b, &self.new_ciphersuite.to_be_bytes());
        lp(&mut b, &self.group_control_state_hash);
        lp(&mut b, self.author_device_id.as_bytes());
        b
    }
    pub fn sign(mut self, signer: &impl Signer) -> R<Self> {
        self.signature = sign_bytes(signer, &self.signed_bytes(true))?;
        Ok(self)
    }
    pub fn binding_bytes(&self) -> Vec<u8> {
        binding_bytes(&self.conversation_id, &self.group_control_state_hash, self.new_security_generation)
    }
}

fn sign_bytes(signer: &impl Signer, data: &[u8]) -> R<Vec<u8>> {
    signer.sign(data).map_err(|e| format!("sign: {e:?}").into())
}

fn binding_bytes(conv: &[u8], state_hash: &[u8], gen: u64) -> Vec<u8> {
    let mut b = BIND_LABEL.to_vec();
    lp(&mut b, conv);
    lp(&mut b, state_hash);
    lp(&mut b, &gen.to_be_bytes());
    b
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reject {
    ConversationMismatch,
    UnknownAuthor,
    BadSignature,
    Unauthorized,
    StateHashMismatch,
    UnregisteredProfile,
    ProfileGenerationMismatch,
    Downgrade,
    NotAboveFloor,
    AlreadyApplied,
    StaleParent,
    GroupIdReuse,
    // MLS-level / welcome-level checks
    KeyPackageNotInRoster,
    UnexpectedMember,
    MissingMember,
    WelcomeGroupIdMismatch,
    WelcomeCiphersuiteMismatch,
    WelcomeBindingMismatch,
    MlsError(String),
}

impl Reject {
    pub fn name(&self) -> String {
        match self {
            Reject::MlsError(_) => "MlsError".into(),
            other => format!("{other:?}"),
        }
    }
}

impl<T: std::fmt::Display> From<T> for Reject {
    fn from(e: T) -> Self {
        Reject::MlsError(e.to_string())
    }
}

/// verify_transition(transition, roster, floor, authority): pure OM-level check, no MLS state.
/// Check order is part of the contract: identity, signature, authority, state, profile/floor, parent.
pub fn verify_transition(t: &Transition, roster: &Roster, floor: &Floor, authority: &Authority) -> Result<(), Reject> {
    if t.conversation_id != floor.conversation_id || t.conversation_id != roster.conversation_id {
        return Err(Reject::ConversationMismatch);
    }
    let author = roster.find(&t.author_device_id).ok_or(Reject::UnknownAuthor)?;
    RustCrypto::default()
        .verify_signature(SignatureScheme::ED25519, &t.signed_bytes(true), &author.sig_pub, &t.signature)
        .map_err(|_| Reject::BadSignature)?;
    if !authority.migrate.contains(&t.author_device_id) {
        return Err(Reject::Unauthorized);
    }
    if t.group_control_state_hash != floor.state_hash || t.group_control_state_hash != roster.state_hash() {
        return Err(Reject::StateHashMismatch);
    }
    match registry_generation(t.new_ciphersuite) {
        None => return Err(Reject::UnregisteredProfile),
        Some(g) if g != t.new_security_generation => return Err(Reject::ProfileGenerationMismatch),
        _ => {}
    }
    if t.new_security_generation < floor.generation {
        return Err(Reject::Downgrade);
    }
    if t.new_security_generation == floor.generation {
        // Same generation: the already-applied transition is an idempotent no-op; anything else is not above the floor.
        return Err(if t.new_mls_group_id == floor.mls_group_id && t.new_ciphersuite == floor.ciphersuite {
            Reject::AlreadyApplied
        } else {
            Reject::NotAboveFloor
        });
    }
    if t.old_mls_group_id != floor.mls_group_id
        || t.old_security_generation != floor.generation
        || t.old_ciphersuite != floor.ciphersuite
    {
        return Err(Reject::StaleParent);
    }
    if t.new_mls_group_id == t.old_mls_group_id {
        return Err(Reject::GroupIdReuse);
    }
    Ok(())
}

// ------------------------------------------------------------------------------------------------
// Devices and MLS plumbing
// ------------------------------------------------------------------------------------------------
pub struct Dev {
    pub name: &'static str,
    pub provider: OpenMlsRustCrypto,
    /// OM device key (Ed25519). Roster-bound; signs transitions; leaf key of v1 and Ed25519-based hybrid suites.
    pub signer: SignatureKeyPair,
    pub cred: CredentialWithKey,
    /// Separate leaf signer/credential for a new suite whose signature scheme differs from the device key's
    /// (None when the device key is reused, as with the Ed25519 hybrid suites).
    pub new_leaf: Option<(SignatureKeyPair, CredentialWithKey)>,
    pub old: Option<MlsGroup>,
    pub new: Option<MlsGroup>,
}

fn cred_for(name: &str, signer: &SignatureKeyPair) -> CredentialWithKey {
    CredentialWithKey {
        credential: BasicCredential::new(name.as_bytes().to_vec()).into(),
        signature_key: signer.to_public_vec().into(),
    }
}

fn mk_dev(name: &'static str) -> R<Dev> {
    let provider = OpenMlsRustCrypto::default();
    let signer = SignatureKeyPair::new(CS_V1.signature_algorithm())?;
    signer.store(provider.storage())?;
    let cred = cred_for(name, &signer);
    Ok(Dev { name, provider, signer, cred, new_leaf: None, old: None, new: None })
}

impl Dev {
    fn prep_new_leaf(&mut self, cs: Ciphersuite) -> R<()> {
        self.new_leaf = if cs.signature_algorithm() == self.signer.signature_scheme() {
            None
        } else {
            let s = SignatureKeyPair::new(cs.signature_algorithm())?;
            s.store(self.provider.storage())?;
            let c = cred_for(self.name, &s);
            Some((s, c))
        };
        Ok(())
    }
    fn leaf(&self) -> (&SignatureKeyPair, CredentialWithKey) {
        match &self.new_leaf {
            Some((s, c)) => (s, c.clone()),
            None => (&self.signer, self.cred.clone()),
        }
    }
    fn new_key_package(&mut self, cs: Ciphersuite) -> R<KeyPackage> {
        self.prep_new_leaf(cs)?;
        let (s, c) = self.leaf();
        let caps = Capabilities::builder().extensions(vec![ExtensionType::Unknown(EXT_OM_BINDING)]).build();
        Ok(KeyPackage::builder()
            .leaf_node_capabilities(caps)
            .build(cs, &self.provider, s, c)?
            .key_package()
            .clone())
    }
    fn send(&mut self, new: bool, text: &[u8]) -> R<Vec<u8>> {
        let Dev { provider, signer, new_leaf, old, new: newg, .. } = self;
        let s: &SignatureKeyPair = if new { new_leaf.as_ref().map(|x| &x.0).unwrap_or(signer) } else { signer };
        let g = if new { newg.as_mut() } else { old.as_mut() }.ok_or("no group")?;
        Ok(g.create_message(provider, s, text)?.tls_serialize_detached()?)
    }
    fn process(&mut self, new: bool, bytes: &[u8]) -> R<ProcessedMessageContent> {
        let pm = MlsMessageIn::tls_deserialize_exact(bytes)?.try_into_protocol_message()?;
        let provider = &self.provider;
        let g = if new { self.new.as_mut() } else { self.old.as_mut() }.ok_or("no group")?;
        Ok(g.process_message(provider, pm)?.into_content())
    }
    fn recv_app(&mut self, new: bool, bytes: &[u8]) -> R<Vec<u8>> {
        match self.process(new, bytes)? {
            ProcessedMessageContent::ApplicationMessage(m) => Ok(m.into_bytes()),
            _ => Err("not an application message".into()),
        }
    }
    fn accept_commit(&mut self, new: bool, bytes: &[u8]) -> R<()> {
        match self.process(new, bytes)? {
            ProcessedMessageContent::StagedCommitMessage(sc) => {
                let provider = &self.provider;
                let g = if new { self.new.as_mut() } else { self.old.as_mut() }.ok_or("no group")?;
                g.merge_staged_commit(provider, *sc)?;
                Ok(())
            }
            _ => Err("not a commit".into()),
        }
    }
    /// Self-update commit in the OLD group, merged by the committer; returns the commit bytes for the others.
    fn old_self_update(&mut self) -> R<Vec<u8>> {
        let g = self.old.as_mut().ok_or("no group")?;
        let b = g
            .commit_builder()
            .force_self_update(true)
            .load_psks(self.provider.storage())?
            .build(self.provider.rand(), self.provider.crypto(), &self.signer, |_| true)?
            .stage_commit(&self.provider)?;
        let bytes = b.commit().tls_serialize_detached()?;
        g.merge_pending_commit(&self.provider)?;
        Ok(bytes)
    }
    fn exporter(&self, new: bool) -> Option<Vec<u8>> {
        let g = if new { self.new.as_ref() } else { self.old.as_ref() }?;
        g.export_secret(self.provider.crypto(), "om-v019", &[], 32).ok()
    }
    fn epoch(&self, new: bool) -> Option<u64> {
        let g = if new { self.new.as_ref() } else { self.old.as_ref() }?;
        Some(g.epoch().as_u64())
    }
}

fn create_config(cs: Ciphersuite) -> MlsGroupCreateConfig {
    MlsGroupCreateConfig::builder()
        .ciphersuite(cs)
        .use_ratchet_tree_extension(true)
        .max_past_epochs(PAST_EPOCHS)
        .build()
}

pub const OLD_GROUP_ID: &[u8] = b"om-v019-old-mls-group";
pub const NEW_GROUP_ID: &[u8] = b"om-v019-new-mls-group";
pub const CONVERSATION_ID: &[u8] = b"om-conversation-0123456789abcdef";

pub struct World {
    pub devs: Vec<Dev>, // [alice, bob, carol]
    pub roster: Roster,
    pub authority: Authority,
    pub floor: Floor,
}

/// v1 group of alice (creator), bob, carol. `admins` are the devices allowed to migrate.
pub fn world(admins: &[&str]) -> R<World> {
    let mut devs: Vec<Dev> = ["alice", "bob", "carol"].iter().map(|n| mk_dev(n)).collect::<R<_>>()?;
    let cfg = create_config(CS_V1);
    let mut g = MlsGroup::new_with_group_id(
        &devs[0].provider,
        &devs[0].signer,
        &cfg,
        GroupId::from_slice(OLD_GROUP_ID),
        devs[0].cred.clone(),
    )?;
    let kps: Vec<KeyPackage> = devs[1..]
        .iter()
        .map(|d| {
            Ok(KeyPackage::builder()
                .build(CS_V1, &d.provider, &d.signer, d.cred.clone())?
                .key_package()
                .clone())
        })
        .collect::<R<_>>()?;
    let (_c, welcome, _gi) = g.add_members(&devs[0].provider, &devs[0].signer, &kps)?;
    g.merge_pending_commit(&devs[0].provider)?;
    let w = welcome.tls_serialize_detached()?;
    devs[0].old = Some(g);
    for d in devs[1..].iter_mut() {
        let wm = match MlsMessageIn::tls_deserialize_exact(&w[..])?.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err("not a welcome".into()),
        };
        let sw = StagedWelcome::new_from_welcome(&d.provider, cfg.join_config(), wm, None)?;
        d.old = Some(sw.into_group(&d.provider)?);
    }
    let roster = Roster {
        conversation_id: CONVERSATION_ID.to_vec(),
        entries: devs
            .iter()
            .map(|d| RosterEntry { device_id: d.name.into(), sig_pub: d.signer.to_public_vec(), admin: admins.contains(&d.name) })
            .collect(),
    };
    let authority = roster.authority();
    let floor = Floor {
        conversation_id: CONVERSATION_ID.to_vec(),
        mls_group_id: OLD_GROUP_ID.to_vec(),
        generation: 1,
        ciphersuite: u16::from(CS_V1),
        state_hash: roster.state_hash(),
    };
    Ok(World { devs, roster, authority, floor })
}

/// Unsigned transition template from the current floor to (new group id, new suite, new generation).
pub fn template(w: &World, author: &str, new_gid: &[u8], new_cs: Ciphersuite, new_gen: u64) -> Transition {
    Transition {
        conversation_id: w.floor.conversation_id.clone(),
        old_mls_group_id: w.floor.mls_group_id.clone(),
        new_mls_group_id: new_gid.to_vec(),
        old_security_generation: w.floor.generation,
        new_security_generation: new_gen,
        old_ciphersuite: w.floor.ciphersuite,
        new_ciphersuite: u16::from(new_cs),
        group_control_state_hash: w.floor.state_hash.clone(),
        author_device_id: author.into(),
        signature: vec![],
    }
}

pub struct Migration {
    pub transition: Transition,
    pub welcome: Vec<u8>,
}

fn om_extensions(t: &Transition) -> R<Extensions<GroupContext>> {
    Ok(Extensions::single(Extension::Unknown(EXT_OM_BINDING, UnknownExtension(t.binding_bytes())))?)
}

fn new_group_caps() -> Capabilities {
    Capabilities::builder().extensions(vec![ExtensionType::Unknown(EXT_OM_BINDING)]).build()
}

/// Raw reboot with caller-supplied key packages: no roster filtering. Used by `author_migrate` after its
/// roster filter, and directly by the rogue-author vector.
fn raw_reboot(author: &mut Dev, t: &Transition, kps: Vec<KeyPackage>) -> R<(MlsGroup, Vec<u8>)> {
    author.prep_new_leaf(Ciphersuite::try_from(t.new_ciphersuite)?)?;
    let ext = om_extensions(t)?;
    let (s, c) = author.leaf();
    let old = author.old.as_ref().ok_or("no old group")?;
    let (mut new_g, bundle) = old
        .reboot(GroupId::from_slice(&t.new_mls_group_id))
        .refine_group_builder(|b| b.ciphersuite(Ciphersuite::try_from(t.new_ciphersuite).unwrap()).with_capabilities(new_group_caps()))
        .finish(ext, kps, |b| b, &author.provider, s, c)?;
    new_g.merge_pending_commit(&author.provider)?;
    let (_commit, welcome, _gi) = bundle.into_messages();
    let welcome = welcome.ok_or("reboot produced no welcome")?.tls_serialize_detached()?;
    Ok((new_g, welcome))
}

fn kp_in_roster(kp: &KeyPackage, roster: &Roster) -> Result<(), Reject> {
    let bc = BasicCredential::try_from(kp.leaf_node().credential().clone())?;
    let id = String::from_utf8_lossy(bc.identity()).to_string();
    let entry = roster.find(&id).ok_or(Reject::KeyPackageNotInRoster)?;
    // Identity binding by roster key only holds when the new suite keeps the roster's signature scheme.
    if kp.leaf_node().signature_key().as_slice() != entry.sig_pub.as_slice() {
        return Err(Reject::KeyPackageNotInRoster);
    }
    Ok(())
}

/// Author side: build, sign and self-verify the transition, accept only roster-bound key packages, reboot.
pub fn author_migrate(
    w: &mut World,
    author_idx: usize,
    new_gid: &[u8],
    new_cs: Ciphersuite,
    kps: Vec<KeyPackage>,
) -> Result<Migration, Reject> {
    let new_gen = registry_generation(u16::from(new_cs)).ok_or(Reject::UnregisteredProfile)?;
    let name = w.devs[author_idx].name;
    let t = template(w, name, new_gid, new_cs, new_gen).sign(&w.devs[author_idx].signer)?;
    verify_transition(&t, &w.roster, &w.floor, &w.authority)?;
    for kp in &kps {
        kp_in_roster(kp, &w.roster)?;
    }
    let (g, welcome) = raw_reboot(&mut w.devs[author_idx], &t, kps)?;
    w.devs[author_idx].new = Some(g);
    Ok(Migration { transition: t, welcome })
}

/// Joiner-side checks on a staged welcome before it becomes a group.
pub fn check_staged(staged: &StagedWelcome, t: &Transition, roster: &Roster) -> Result<(), Reject> {
    let gc = staged.group_context();
    if gc.group_id().as_slice() != t.new_mls_group_id.as_slice() {
        return Err(Reject::WelcomeGroupIdMismatch);
    }
    if u16::from(gc.ciphersuite()) != t.new_ciphersuite {
        return Err(Reject::WelcomeCiphersuiteMismatch);
    }
    match gc.extensions().unknown(EXT_OM_BINDING) {
        Some(u) if u.0 == t.binding_bytes() => {}
        _ => return Err(Reject::WelcomeBindingMismatch),
    }
    let mut seen = BTreeSet::new();
    for m in staged.members() {
        let bc = BasicCredential::try_from(m.credential.clone())?;
        let id = String::from_utf8_lossy(bc.identity()).to_string();
        match roster.find(&id) {
            Some(e) if e.sig_pub == m.signature_key => {
                seen.insert(id);
            }
            _ => return Err(Reject::UnexpectedMember),
        }
    }
    if seen.len() != roster.entries.len() {
        return Err(Reject::MissingMember);
    }
    Ok(())
}

/// Joiner side: verify the transition against the local floor, stage the welcome, run the roster/binding checks.
pub fn join_new(dev: &mut Dev, mig: &Migration, roster: &Roster, floor: &Floor, authority: &Authority) -> Result<(), Reject> {
    verify_transition(&mig.transition, roster, floor, authority)?;
    let wm = match MlsMessageIn::tls_deserialize_exact(&mig.welcome[..])?.extract() {
        MlsMessageBodyIn::Welcome(w) => w,
        _ => return Err(Reject::MlsError("not a welcome".into())),
    };
    let cfg = dev.old.as_ref().ok_or("no old group")?.configuration().clone();
    let staged = StagedWelcome::new_from_welcome(&dev.provider, &cfg, wm, None)?;
    check_staged(&staged, &mig.transition, roster)?;
    dev.new = Some(staged.into_group(&dev.provider)?);
    Ok(())
}

/// Full happy path: alice migrates, bob and carol verify and join, everyone applies the floor.
pub fn migrate_all(w: &mut World, new_cs: Ciphersuite) -> R<Migration> {
    let kps = vec![w.devs[1].new_key_package(new_cs)?, w.devs[2].new_key_package(new_cs)?];
    let mig = author_migrate(w, 0, NEW_GROUP_ID, new_cs, kps).map_err(|e| format!("author_migrate: {e:?}"))?;
    let (roster, floor, authority) = (w.roster.clone(), w.floor.clone(), w.authority.clone());
    for i in 1..3 {
        join_new(&mut w.devs[i], &mig, &roster, &floor, &authority).map_err(|e| format!("join {i}: {e:?}"))?;
    }
    w.floor.apply(&mig.transition);
    Ok(mig)
}

// ------------------------------------------------------------------------------------------------
// Vectors
// ------------------------------------------------------------------------------------------------
struct Rec {
    vectors: Vec<Value>,
    failed: usize,
}

impl Rec {
    fn vector(&mut self, name: &str, expectation: &str, f: impl FnOnce() -> R<(bool, Value)>) {
        let (ok, observed) = match f() {
            Ok(x) => x,
            Err(e) => (false, json!({"error": e.to_string()})),
        };
        if !ok {
            self.failed += 1;
        }
        self.vectors.push(json!({"name": name, "expectation": expectation, "result": if ok {"pass"} else {"fail"}, "observed": observed}));
    }
}

fn rej<T>(r: Result<T, Reject>) -> String {
    match r {
        Ok(_) => "Accepted".into(),
        Err(e) => e.name(),
    }
}

fn exporters_equal(w: &World, new: bool) -> bool {
    let e0 = w.devs[0].exporter(new);
    e0.is_some() && w.devs.iter().all(|d| d.exporter(new) == e0)
}

/// Both directions of messaging between every ordered pair in the given group generation.
fn full_mesh_messages(w: &mut World, new: bool) -> R<bool> {
    let n = w.devs.len();
    for s in 0..n {
        let msg = format!("hello-from-{}-{}", w.devs[s].name, new);
        let ct = w.devs[s].send(new, msg.as_bytes())?;
        for r in 0..n {
            if r != s && w.devs[r].recv_app(new, &ct)? != msg.as_bytes() {
                return Ok(false);
            }
        }
    }
    Ok(true)
}

/// Tail message sent by bob in the last pre-migration epoch, delivered after migration and `lag` old-group commits.
fn tail_after_lag(lag: usize) -> R<Value> {
    let mut w = world(&["alice"])?;
    let tail = w.devs[1].send(false, b"tail-message-last-old-epoch")?;
    let e_before = w.devs[0].epoch(false).unwrap();
    migrate_all(&mut w, CS_HYBRID)?;
    let e_after_migration = w.devs[0].epoch(false).unwrap();
    for _ in 0..lag {
        let c = w.devs[0].old_self_update()?;
        w.devs[1].accept_commit(false, &c)?;
        w.devs[2].accept_commit(false, &c)?;
    }
    let carol = w.devs[2].recv_app(false, &tail).map(|m| m == b"tail-message-last-old-epoch").unwrap_or(false);
    let alice = w.devs[0].recv_app(false, &tail).map(|m| m == b"tail-message-last-old-epoch").unwrap_or(false);
    Ok(json!({"lag": lag, "carol_decrypts": carol, "alice_decrypts": alice,
              "old_epoch_unchanged_by_migration": e_before == e_after_migration}))
}

pub fn run_all() -> R<Value> {
    let mut rec = Rec { vectors: vec![], failed: 0 };

    rec.vector("v1_group_three_members_exchange_messages", "classical v1 group with alice, bob, carol: every member reads every other member's message", || {
        let mut w = world(&["alice"])?;
        let ok = full_mesh_messages(&mut w, false)?;
        let g = w.devs[0].old.as_ref().unwrap();
        Ok((ok && g.members().count() == 3 && g.ciphersuite() == CS_V1 && g.group_id().as_slice() == OLD_GROUP_ID,
            json!({"members": g.members().count(), "ciphersuite": u16::from(g.ciphersuite()), "epoch": g.epoch().as_u64()})))
    });

    rec.vector("migrate_to_hybrid_all_members_join_exporters_match", "reboot into the hybrid suite: new MLS group id, all 3 members join, equal exporter secret, mesh messaging works, exporter differs from the old group", || {
        let mut w = world(&["alice"])?;
        let old_export = w.devs[0].exporter(false);
        let m = migrate_all(&mut w, CS_HYBRID)?;
        let mesh = full_mesh_messages(&mut w, true)?;
        let eq = exporters_equal(&w, true);
        let g = w.devs[0].new.as_ref().unwrap();
        let ok = mesh && eq && g.members().count() == 3 && g.ciphersuite() == CS_HYBRID
            && m.transition.new_mls_group_id != m.transition.old_mls_group_id
            && w.devs[0].exporter(true) != old_export
            && w.devs.iter().all(|d| d.new.as_ref().map(|g| g.ciphersuite()) == Some(CS_HYBRID));
        Ok((ok, json!({"new_ciphersuite": u16::from(g.ciphersuite()), "new_epoch": g.epoch().as_u64(), "members": g.members().count(),
                       "mesh_messages": mesh, "exporters_equal": eq, "group_id_changed": m.transition.new_mls_group_id != m.transition.old_mls_group_id})))
    });

    rec.vector("conversation_id_and_state_hash_unchanged_across_migration", "ConversationID and the GroupControlState hash are identical before and after; the new MLS group carries them in its GroupContext binding", || {
        let mut w = world(&["alice"])?;
        let (c0, h0) = (w.floor.conversation_id.clone(), w.floor.state_hash.clone());
        let m = migrate_all(&mut w, CS_HYBRID)?;
        let h1 = w.roster.state_hash();
        let bound = w.devs.iter().all(|d| {
            d.new.as_ref().and_then(|g| g.extensions().unknown(EXT_OM_BINDING).map(|u| u.0 == m.transition.binding_bytes())).unwrap_or(false)
        });
        let ok = c0 == w.floor.conversation_id && h0 == w.floor.state_hash && h0 == h1 && m.transition.conversation_id == c0
            && m.transition.group_control_state_hash == h0 && bound;
        Ok((ok, json!({"conversation_id_unchanged": c0 == w.floor.conversation_id, "state_hash_unchanged": h0 == h1, "binding_in_new_group_context_all_members": bound,
                       "floor_generation_after": w.floor.generation})))
    });

    rec.vector("valid_transition_accepted_and_serialises", "a correctly signed authorised upgrade passes verify_transition on every member; the diagnostic JSON round-trips", || {
        let w = world(&["alice"])?;
        let t = template(&w, "alice", NEW_GROUP_ID, CS_HYBRID, 2).sign(&w.devs[0].signer)?;
        let accepted = verify_transition(&t, &w.roster, &w.floor, &w.authority).is_ok();
        let j = serde_json::to_string(&t)?;
        let back: Transition = serde_json::from_str(&j)?;
        let label_domain_separated = SIG_LABEL.last() == Some(&0u8) && t.signed_bytes(true).starts_with(SIG_LABEL);
        Ok((accepted && back == t && label_domain_separated, json!({"accepted": accepted, "roundtrip": back == t, "signed_bytes_start_with_label": label_domain_separated})))
    });

    rec.vector("downgrade_hybrid_to_classical_rejected", "after migrating to hybrid (generation 2), an authorised, correctly signed transition back to the classical suite is rejected: Downgrade at its true generation, ProfileGenerationMismatch if it claims a higher generation", || {
        let mut w = world(&["alice"])?;
        migrate_all(&mut w, CS_HYBRID)?;
        let down = template(&w, "alice", b"om-v019-downgrade-group", CS_V1, 1).sign(&w.devs[0].signer)?;
        let lie = template(&w, "alice", b"om-v019-downgrade-group", CS_V1, 3).sign(&w.devs[0].signer)?;
        let r1 = rej(verify_transition(&down, &w.roster, &w.floor, &w.authority));
        let r2 = rej(verify_transition(&lie, &w.roster, &w.floor, &w.authority));
        let floor_unchanged = w.floor.generation == 2 && w.floor.ciphersuite == u16::from(CS_HYBRID);
        Ok((r1 == "Downgrade" && r2 == "ProfileGenerationMismatch" && floor_unchanged, json!({"classical_at_gen1": r1, "classical_claiming_gen3": r2, "floor_unchanged": floor_unchanged})))
    });

    rec.vector("equal_generation_lower_profile_rejected", "at generation 2 on 0x004F, a transition to another generation-2 profile (0x004E, smaller profile id) is rejected as NotAboveFloor; the unregistered ciphersuite 0x0007 is rejected as UnregisteredProfile", || {
        let mut w = world(&["alice"])?;
        migrate_all(&mut w, CS_HYBRID)?;
        let eq = template(&w, "alice", b"om-v019-equal-gen-group", CS_HYBRID_ALT, 2).sign(&w.devs[0].signer)?;
        let unreg = template(&w, "alice", b"om-v019-unreg-group", Ciphersuite::MLS_256_DHKEMP384_AES256GCM_SHA384_P384, 9).sign(&w.devs[0].signer)?;
        let r1 = rej(verify_transition(&eq, &w.roster, &w.floor, &w.authority));
        let r2 = rej(verify_transition(&unreg, &w.roster, &w.floor, &w.authority));
        Ok((r1 == "NotAboveFloor" && r2 == "UnregisteredProfile", json!({"equal_generation_other_profile": r1, "unregistered_profile": r2})))
    });

    rec.vector("unauthorized_author_rejected", "a roster member that is not in the migration authority set (bob, plain member) is rejected as Unauthorized even with a valid signature; a signer outside the roster is UnknownAuthor; the author-side API refuses before building anything", || {
        let mut w = world(&["alice"])?;
        let by_bob = template(&w, "bob", NEW_GROUP_ID, CS_HYBRID, 2).sign(&w.devs[1].signer)?;
        let mallory = mk_dev("mallory")?;
        let by_mallory = template(&w, "mallory", NEW_GROUP_ID, CS_HYBRID, 2).sign(&mallory.signer)?;
        let r1 = rej(verify_transition(&by_bob, &w.roster, &w.floor, &w.authority));
        let r2 = rej(verify_transition(&by_mallory, &w.roster, &w.floor, &w.authority));
        let kps = vec![w.devs[0].new_key_package(CS_HYBRID)?, w.devs[2].new_key_package(CS_HYBRID)?];
        let r3 = rej(author_migrate(&mut w, 1, NEW_GROUP_ID, CS_HYBRID, kps));
        let no_new_group = w.devs[1].new.is_none();
        Ok((r1 == "Unauthorized" && r2 == "UnknownAuthor" && r3 == "Unauthorized" && no_new_group,
            json!({"member_author": r1, "non_roster_author": r2, "author_side_api_refuses": r3, "no_group_created": no_new_group})))
    });

    rec.vector("tampered_fields_fail_signature", "changing new group id, ciphersuite, generation, state hash, old group id or author after signing yields BadSignature; a signature by another roster key is BadSignature", || {
        let w = world(&["alice", "bob"])?;
        let good = template(&w, "alice", NEW_GROUP_ID, CS_HYBRID, 2).sign(&w.devs[0].signer)?;
        let check = |f: &dyn Fn(&mut Transition)| {
            let mut t = good.clone();
            f(&mut t);
            rej(verify_transition(&t, &w.roster, &w.floor, &w.authority))
        };
        let results = json!({
            "new_group_id": check(&|t| t.new_mls_group_id = b"om-v019-evil".to_vec()),
            "new_ciphersuite": check(&|t| t.new_ciphersuite = u16::from(CS_HYBRID_ALT)),
            "new_generation": check(&|t| t.new_security_generation = 3),
            "state_hash": check(&|t| t.group_control_state_hash[0] ^= 1),
            "old_group_id": check(&|t| t.old_mls_group_id = b"om-v019-other-old".to_vec()),
            "author_swapped_to_bob": check(&|t| t.author_device_id = "bob".into()),
        });
        let mut forged = good.clone();
        forged.signature = sign_bytes(&w.devs[1].signer, &forged.signed_bytes(true))?; // bob signs, claims alice
        let wrong_key = rej(verify_transition(&forged, &w.roster, &w.floor, &w.authority));
        let mut nolabel = good.clone();
        nolabel.signature = sign_bytes(&w.devs[0].signer, &nolabel.signed_bytes(false))?; // right key, no domain label
        let no_label = rej(verify_transition(&nolabel, &w.roster, &w.floor, &w.authority));
        let all_bad = results.as_object().unwrap().values().all(|v| v == "BadSignature");
        let baseline = rej(verify_transition(&good, &w.roster, &w.floor, &w.authority));
        Ok((all_bad && wrong_key == "BadSignature" && no_label == "BadSignature" && baseline == "Accepted",
            json!({"baseline_untampered": baseline, "tampered": results, "signature_by_other_roster_key": wrong_key, "signature_without_domain_label": no_label})))
    });

    rec.vector("authorised_author_changing_state_hash_rejected", "an admin that correctly signs a transition carrying a different GroupControlState hash is rejected as StateHashMismatch (the migration may not change group authority)", || {
        let w = world(&["alice"])?;
        let mut t = template(&w, "alice", NEW_GROUP_ID, CS_HYBRID, 2);
        t.group_control_state_hash[0] ^= 1;
        let t = t.sign(&w.devs[0].signer)?;
        let r = rej(verify_transition(&t, &w.roster, &w.floor, &w.authority));
        Ok((r == "StateHashMismatch", json!({"result": r})))
    });

    rec.vector("non_roster_member_cannot_join_new_group", "(a) a device outside the welcome cannot process it; (b) the author-side filter refuses a non-roster key package; (c) a rogue authorised author that includes a non-roster key package is caught by every honest joiner's staged-welcome roster check and nobody joins", || {
        // (a)
        let mut w = world(&["alice"])?;
        let mut mallory = mk_dev("mallory")?;
        let kps = vec![w.devs[1].new_key_package(CS_HYBRID)?, w.devs[2].new_key_package(CS_HYBRID)?];
        let m = author_migrate(&mut w, 0, NEW_GROUP_ID, CS_HYBRID, kps).map_err(|e| format!("{e:?}"))?;
        let _kp_unused = mallory.new_key_package(CS_HYBRID)?;
        let wm = match MlsMessageIn::tls_deserialize_exact(&m.welcome[..])?.extract() {
            MlsMessageBodyIn::Welcome(w) => w,
            _ => return Err("not a welcome".into()),
        };
        let a = StagedWelcome::new_from_welcome(&mallory.provider, create_config(CS_HYBRID).join_config(), wm, None).is_err();
        // (b)
        let mut w2 = world(&["alice"])?;
        let kps = vec![w2.devs[1].new_key_package(CS_HYBRID)?, mallory.new_key_package(CS_HYBRID)?];
        let b = rej(author_migrate(&mut w2, 0, NEW_GROUP_ID, CS_HYBRID, kps));
        let b_no_group = w2.devs[0].new.is_none();
        // (c)
        let mut w3 = world(&["alice"])?;
        let t = template(&w3, "alice", NEW_GROUP_ID, CS_HYBRID, 2).sign(&w3.devs[0].signer)?;
        let kps = vec![w3.devs[1].new_key_package(CS_HYBRID)?, w3.devs[2].new_key_package(CS_HYBRID)?, mallory.new_key_package(CS_HYBRID)?];
        let (_g, welcome) = raw_reboot(&mut w3.devs[0], &t, kps)?;
        let rogue = Migration { transition: t, welcome };
        let (roster, floor, authority) = (w3.roster.clone(), w3.floor.clone(), w3.authority.clone());
        let c1 = rej(join_new(&mut w3.devs[1], &rogue, &roster, &floor, &authority));
        let c2 = rej(join_new(&mut w3.devs[2], &rogue, &roster, &floor, &authority));
        let c_none_joined = w3.devs[1].new.is_none() && w3.devs[2].new.is_none();
        Ok((a && b == "KeyPackageNotInRoster" && b_no_group && c1 == "UnexpectedMember" && c2 == "UnexpectedMember" && c_none_joined,
            json!({"a_outsider_cannot_process_welcome": a, "b_author_filter": b, "b_no_group_created": b_no_group, "c_rogue_author_bob": c1, "c_rogue_author_carol": c2, "c_nobody_joined": c_none_joined})))
    });

    rec.vector("joiner_rejects_welcome_not_matching_transition", "a joiner refuses a welcome whose MLS group id or ciphersuite differs from the signed transition it was paired with; the matching pairing is accepted; a joiner whose staged welcome was rejected cannot retry with the same key package", || {
        // One fresh world per case: a rejected staged welcome consumes the joiner's key package (observed below).
        let rig = |w: &mut World| -> R<(Transition, Vec<u8>)> {
            let t = template(w, "alice", NEW_GROUP_ID, CS_HYBRID, 2).sign(&w.devs[0].signer)?;
            let kps = vec![w.devs[1].new_key_package(CS_HYBRID)?, w.devs[2].new_key_package(CS_HYBRID)?];
            let (_g, welcome) = raw_reboot(&mut w.devs[0], &t, kps)?;
            Ok((t, welcome))
        };
        let mut w1 = world(&["alice"])?;
        let (_t, welcome1) = rig(&mut w1)?;
        let t_id = template(&w1, "alice", b"om-v019-other-new-group", CS_HYBRID, 2).sign(&w1.devs[0].signer)?;
        let (roster, floor, authority) = (w1.roster.clone(), w1.floor.clone(), w1.authority.clone());
        let r_id = rej(join_new(&mut w1.devs[1], &Migration { transition: t_id, welcome: welcome1.clone() }, &roster, &floor, &authority));
        let t_ok = template(&w1, "alice", NEW_GROUP_ID, CS_HYBRID, 2).sign(&w1.devs[0].signer)?;
        let retry_res = join_new(&mut w1.devs[1], &Migration { transition: t_ok, welcome: welcome1 }, &roster, &floor, &authority);
        let retry_detail = match &retry_res { Err(Reject::MlsError(m)) => m.clone(), _ => String::new() };
        let retry_same_kp = rej(retry_res);

        let mut w2 = world(&["alice"])?;
        let (_t, welcome2) = rig(&mut w2)?;
        let t_cs = template(&w2, "alice", NEW_GROUP_ID, CS_HYBRID_ALT, 2).sign(&w2.devs[0].signer)?;
        let (roster, floor, authority) = (w2.roster.clone(), w2.floor.clone(), w2.authority.clone());
        let r_cs = rej(join_new(&mut w2.devs[1], &Migration { transition: t_cs, welcome: welcome2 }, &roster, &floor, &authority));

        let mut w3 = world(&["alice"])?;
        let (t3, welcome3) = rig(&mut w3)?;
        let (roster, floor, authority) = (w3.roster.clone(), w3.floor.clone(), w3.authority.clone());
        let ok_join = rej(join_new(&mut w3.devs[1], &Migration { transition: t3, welcome: welcome3 }, &roster, &floor, &authority));
        Ok((r_id == "WelcomeGroupIdMismatch" && r_cs == "WelcomeCiphersuiteMismatch" && ok_join == "Accepted" && retry_same_kp == "MlsError",
            json!({"group_id_differs": r_id, "ciphersuite_differs": r_cs, "matching_transition": ok_join, "retry_after_rejected_staging_with_same_key_package": retry_same_kp, "retry_error": retry_detail})))
    });

    for lag in 0..=4usize {
        let name = format!("old_group_tail_message_after_{lag}_old_epoch_advances");
        let expect_ok = lag <= PAST_EPOCHS;
        let expectation = if expect_ok {
            format!("tail message from the last pre-migration epoch, delivered after migration and {lag} old-group commit(s), is decryptable (past_epoch_window = {PAST_EPOCHS})")
        } else {
            format!("the same tail message after {lag} old-group commits (window {PAST_EPOCHS} exceeded) is NOT decryptable")
        };
        rec.vector(&name, &expectation, || {
            let o = tail_after_lag(lag)?;
            let c = o["carol_decrypts"].as_bool().unwrap();
            let a = o["alice_decrypts"].as_bool().unwrap();
            Ok((c == expect_ok && a == expect_ok && o["old_epoch_unchanged_by_migration"].as_bool().unwrap(), o))
        });
    }

    rec.vector("old_group_stays_live_in_mls_after_migration", "INFORMATIONAL: migration does not retire the old MLS group; after migration the old group still sends, receives and commits (an OM-level seal is required, not provided by reboot)", || {
        let mut w = world(&["alice"])?;
        migrate_all(&mut w, CS_HYBRID)?;
        let ct = w.devs[1].send(false, b"sent-in-old-group-after-migration")?;
        let read = w.devs[2].recv_app(false, &ct)? == b"sent-in-old-group-after-migration";
        let e0 = w.devs[0].epoch(false).unwrap();
        let c = w.devs[0].old_self_update()?;
        w.devs[1].accept_commit(false, &c)?;
        w.devs[2].accept_commit(false, &c)?;
        let advanced = w.devs[0].epoch(false).unwrap() == e0 + 1 && w.devs[2].epoch(false) == w.devs[0].epoch(false);
        Ok((read && advanced, json!({"old_group_message_read_after_migration": read, "old_group_commit_accepted_after_migration": advanced})))
    });

    rec.vector("replay_of_applied_transition_rejected_idempotent", "after applying the migration, the same transition is rejected as AlreadyApplied and applying the floor twice changes nothing", || {
        let mut w = world(&["alice"])?;
        let m = migrate_all(&mut w, CS_HYBRID)?;
        let floor_after = w.floor.clone();
        let replay = rej(verify_transition(&m.transition, &w.roster, &w.floor, &w.authority));
        w.floor.apply(&m.transition);
        let idempotent = w.floor == floor_after;
        // Also: a fresh-keyed re-signature of the same content (new signature bytes, same fields) is still AlreadyApplied.
        let resigned = m.transition.clone().sign(&w.devs[0].signer)?;
        let replay2 = rej(verify_transition(&resigned, &w.roster, &w.floor, &w.authority));
        Ok((replay == "AlreadyApplied" && replay2 == "AlreadyApplied" && idempotent, json!({"replay": replay, "resigned_replay": replay2, "floor_idempotent": idempotent})))
    });

    rec.vector("stale_parent_rejected", "a validly signed upgrade whose old group/generation is not the current floor (made against a stale view) is rejected as StaleParent", || {
        let mut w = world(&["alice"])?;
        let stale = template(&w, "alice", b"om-v019-stale-group", CS_HYBRID, 2);
        // Move the floor to a hypothetical later generation-1 group (same generation, other group id) so the old parent no longer matches.
        w.floor.mls_group_id = b"om-v019-some-other-v1-group".to_vec();
        let stale = stale.sign(&w.devs[0].signer)?;
        let r = rej(verify_transition(&stale, &w.roster, &w.floor, &w.authority));
        Ok((r == "StaleParent", json!({"result": r})))
    });

    rec.vector("concurrent_competing_migrations_are_order_dependent", "INFORMATIONAL: two admins sign different valid migrations from the same parent; each verifier accepts the one it sees first and rejects the other, so two verifiers with opposite arrival order diverge (verify_transition alone does not converge; ADR-0173 canonical-successor ordering is required)", || {
        let w = world(&["alice", "bob"])?;
        let ta = template(&w, "alice", b"om-v019-group-by-alice", CS_HYBRID, 2).sign(&w.devs[0].signer)?;
        let tb = template(&w, "bob", b"om-v019-group-by-bob", CS_HYBRID, 2).sign(&w.devs[1].signer)?;
        let run = |first: &Transition, second: &Transition| {
            let mut f = w.floor.clone();
            let r1 = rej(verify_transition(first, &w.roster, &f, &w.authority));
            if r1 == "Accepted" {
                f.apply(first);
            }
            let r2 = rej(verify_transition(second, &w.roster, &f, &w.authority));
            (r1, r2, f.mls_group_id)
        };
        let v1 = run(&ta, &tb);
        let v2 = run(&tb, &ta);
        let diverge = v1.2 != v2.2;
        let each_accepts_exactly_one = v1.0 == "Accepted" && v1.1 != "Accepted" && v2.0 == "Accepted" && v2.1 != "Accepted";
        Ok((diverge && each_accepts_exactly_one, json!({"verifier_1_order_alice_then_bob": [v1.0, v1.1], "verifier_2_order_bob_then_alice": [v2.0, v2.1], "verifiers_end_on_different_groups": diverge})))
    });

    let suites = suite_matrix()?;

    let provider = OpenMlsRustCrypto::default();
    let supported: Vec<String> = provider.crypto().supported_ciphersuites().iter().map(|c| format!("0x{:04x}", u16::from(*c))).collect();
    let failed = rec.failed;
    Ok(json!({
        "library": "openmls 0.9.0 (rust_crypto 0.6.0, basic_credential 0.6.0, memory storage, features fork-resolution + draft-ietf-mls-pq-ciphersuites)",
        "ciphersuites": {
            "v1": {"id": "0x0001", "name": "MLS_128_DHKEMX25519_AES128GCM_SHA256_Ed25519", "security_generation": 1},
            "hybrid_primary": {"id": "0x004f", "name": "MLS_128_MLKEM768X25519_AES128GCM_SHA256_Ed25519", "security_generation": 2,
                               "note": "X-Wing (ML-KEM-768 + X25519) hybrid KEM; signatures are classical Ed25519"},
            "hybrid_alt_equal_generation": {"id": "0x004e", "name": "MLS_128_MLKEM768X25519_AES256GCM_SHA384_Ed25519", "security_generation": 2},
        },
        "provider_supported_ciphersuites": supported,
        "past_epoch_window": PAST_EPOCHS,
        "vectors": rec.vectors,
        "vectors_failed": failed,
        "suite_matrix": suites,
    }))
}

/// Informational: reboot-based migration (no OM transition) from v1 into every ciphersuite the provider advertises.
fn suite_matrix() -> R<Value> {
    let provider = OpenMlsRustCrypto::default();
    let mut rows = vec![];
    for cs in provider.crypto().supported_ciphersuites() {
        let id = u16::from(cs);
        let mut row = json!({"id": format!("0x{id:04x}"), "name": format!("{cs:?}"), "signature_scheme": format!("{:?}", cs.signature_algorithm())});
        let res = (|| -> R<(bool, bool)> {
            let mut w = world(&["alice"])?;
            let kps = vec![w.devs[1].new_key_package(cs)?, w.devs[2].new_key_package(cs)?];
            let t = Transition { new_ciphersuite: id, new_mls_group_id: NEW_GROUP_ID.to_vec(), ..template(&w, "alice", NEW_GROUP_ID, cs, 0) };
            let (g, welcome) = raw_reboot(&mut w.devs[0], &t, kps)?;
            w.devs[0].new = Some(g);
            let m = Migration { transition: t.clone(), welcome };
            for i in 1..3 {
                let wm = match MlsMessageIn::tls_deserialize_exact(&m.welcome[..])?.extract() {
                    MlsMessageBodyIn::Welcome(w) => w,
                    _ => return Err("not a welcome".into()),
                };
                let cfg = w.devs[i].old.as_ref().ok_or("no old")?.configuration().clone();
                let staged = StagedWelcome::new_from_welcome(&w.devs[i].provider, &cfg, wm, None)?;
                w.devs[i].new = Some(staged.into_group(&w.devs[i].provider)?);
            }
            let eq = exporters_equal(&w, true);
            let mesh = full_mesh_messages(&mut w, true)?;
            // Is the roster's Ed25519 device key still the leaf key in the new group?
            let roster_key_kept = w.devs.iter().all(|d| d.leaf().0.to_public_vec() == d.signer.to_public_vec());
            Ok((eq && mesh, roster_key_kept))
        })();
        match res {
            Ok((ok, kept)) => {
                row["migrates_and_messages"] = json!(ok);
                row["roster_ed25519_key_is_new_leaf_key"] = json!(kept);
            }
            Err(e) => {
                row["migrates_and_messages"] = json!(false);
                row["error"] = json!(e.to_string());
            }
        }
        rows.push(row);
    }
    Ok(Value::Array(rows))
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use wasm_bindgen::prelude::*;
    #[wasm_bindgen]
    pub fn run_all_json() -> Result<String, JsError> {
        let v = super::run_all().map_err(|e| JsError::new(&e.to_string()))?;
        Ok(serde_json::to_string_pretty(&v).unwrap())
    }
}
