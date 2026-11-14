// SPDX-License-Identifier: MPL-2.0
//! `GroupCrypto`: exactly what Core needs from an MLS (RFC 9420) engine, and nothing of any library's own types.
//!
//! One engine value is one MLS client, i.e. one DeviceIdentity (GROUP_CHAT_ARCHITECTURE: one MLS client per
//! authorized device). Core owns the surrounding policy: the OM `GroupControlState`, the `GroupCommitBinding`, the
//! ADR-0163 journal and the storage format. The engine owns only MLS key schedule state.
//!
//! Boundary rules (CRYPTOGRAPHY_AND_AGILITY, ADR-0118, G2 in the groups plan):
//!
//! * Randomness and time: [`CryptoEnv`] carries Core's RNG and clock on every call, and the test-only fake draws
//!   from it, so its tests are reproducible. The real engine (stock OpenMLS) cannot be handed a per-call generator, so
//!   it draws entropy and reads the clock from the host by the same route Core's own `OsRng` uses (on wasm: the
//!   `om_host.random_fill` and `om_host.now_ms` imports) and ignores `CryptoEnv`. It never invents a generator. This
//!   rewording of D-g9 is proposed by ADR-0187 (a delegated draft, not accepted).
//! * Everything crossing the trait is opaque bytes (KeyPackages, Commits, Welcomes) or plain integers. A library's
//!   serialization of its own state leaves only through [`GroupCrypto::snapshot`] and is stored by
//!   [`super::store`] as an opaque, versioned, engine-tagged blob; it is never the canonical format.
//! * A Commit is created *pending*: the engine keeps the tentative post-commit state beside the live epoch until Core
//!   calls [`GroupCrypto::merge_pending`] (canonical acceptance, ADR-0163 rule 5) or
//!   [`GroupCrypto::discard_pending`] (superseded or abandoned). [`GroupCrypto::pending`] returns the exact bytes
//!   again after a restore, which is what makes a crash retry replay identical bytes (ADR-0163 rule 4).
//! * No application message is produced under a pending epoch; that is Core policy, enforced by the journal.
//!
//! Implementations: [`super::fake::FakeGroupCrypto`] (test-only) and, with the `groups-openmls` feature,
//! [`super::openmls::OpenMlsGroupCrypto`] (OpenMLS 0.9.0, ADR-0177; depends on the proposed ADR-0187).

use crate::crypto::rng::RngCore;

/// A failure inside the engine. The code is a stable, provider-neutral string; no library detail crosses.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MlsError(pub &'static str);

impl core::fmt::Display for MlsError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for MlsError {}

pub type MlsResult<T> = Result<T, MlsError>;

/// Everything non-deterministic an engine may use, supplied by Core on each call.
pub struct CryptoEnv<'a> {
    /// Core's RNG (the host's entropy). The fake draws from here; the real engine uses the host directly (see above).
    pub rng: &'a mut dyn RngCore,
    /// Core's clock, milliseconds since the Unix epoch, as the host reports it (the fake's KeyPackage lifetimes).
    pub now_ms: u64,
}

/// A freshly generated KeyPackage: the opaque public message and the identifier Core tracks it by.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyPackage {
    /// The serialized KeyPackage message, as released to an inviter.
    pub bytes: Vec<u8>,
    /// Engine-independent reference: `SHA-256(bytes)` (Core can compute it without the engine).
    pub reference: [u8; 32],
}

/// A Commit that is created and held pending, with the Welcome for any added members.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedCommit {
    /// The exact MLS Commit message bytes; `mls_commit_hash` of the binding hashes these (OM-GROUP-MLS-COMMIT-HASH-v1).
    pub commit: Vec<u8>,
    /// The Welcome for the added members, if the Commit adds any.
    pub welcome: Option<Vec<u8>>,
    /// The epoch the group has after the Commit merges: the current epoch plus one.
    pub resulting_epoch: u64,
}

/// What a processed Commit or Welcome left the group at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpochInfo {
    pub epoch: u64,
    /// Whether this client is still a member (false after a Commit that removed it).
    pub still_member: bool,
}

/// What an engine can say about a Commit without applying it: enough for Core to bind it before it merges.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitInfo {
    pub from_epoch: u64,
    pub resulting_epoch: u64,
    /// DeviceIDs of the committing leaf.
    pub committer: Vec<u8>,
}

/// An application message opened by [`GroupCrypto::decrypt_application`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decrypted {
    pub plaintext: Vec<u8>,
    /// DeviceID of the sending leaf.
    pub sender: Vec<u8>,
    /// The epoch the message was sent in (its authorization epoch, ADR-0121).
    pub epoch: u64,
}

/// One MLS client. See the module documentation for the contract.
pub trait GroupCrypto {
    /// Stable engine tag stored beside the snapshot (for example `"openmls-0.9"`); selects the loader on restore.
    fn engine_tag(&self) -> &'static str;

    /// The DeviceID this client acts as (its credential identity).
    fn device_id(&self) -> &[u8];

    /// Generate one single-use KeyPackage. Its private half stays inside the engine until a Welcome consumes it.
    fn key_package(&mut self, env: &mut CryptoEnv<'_>) -> MlsResult<KeyPackage>;

    /// Create group `group_id` at epoch 0 with only this client as a member. `context_ext` is the opaque bytes of the
    /// encrypted GroupContext extension (which carries the ConversationID; never visible outside the group).
    fn create_group(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        context_ext: &[u8],
    ) -> MlsResult<()>;

    /// Create a pending Commit that adds the clients behind `key_packages`. Refused while another Commit is pending.
    fn add_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        key_packages: &[Vec<u8>],
    ) -> MlsResult<PreparedCommit>;

    /// Create a pending Commit that removes the leaves of `devices`. Refused while another Commit is pending.
    fn remove_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        devices: &[Vec<u8>],
    ) -> MlsResult<PreparedCommit>;

    /// Create a pending Commit that changes no membership: a self-update of this client's own leaf. It carries the
    /// transitions that change Room policy or roles without adding or removing anyone (ADR-0189 section 3). Refused
    /// while another Commit is pending.
    fn update_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
    ) -> MlsResult<PreparedCommit>;

    /// The pending Commit, byte for byte, if one exists (also after a snapshot restore).
    fn pending(&self, group_id: &[u8]) -> MlsResult<Option<PreparedCommit>>;

    /// Merge the pending Commit: it was accepted as canonical. The epoch advances.
    fn merge_pending(&mut self, group_id: &[u8]) -> MlsResult<EpochInfo>;

    /// Drop the pending Commit and its tentative state: it was superseded or abandoned. The epoch is unchanged.
    fn discard_pending(&mut self, group_id: &[u8]) -> MlsResult<()>;

    /// Read a received Commit without applying it. Fails `not_a_commit` / `unknown_group`.
    fn inspect_commit(&self, group_id: &[u8], commit: &[u8]) -> MlsResult<CommitInfo>;

    /// Apply a received Commit (never one this client made). Refused `wrong_epoch` unless it is for the current
    /// epoch, and refused while a Commit of this client is pending (Core discards or merges that first).
    fn process_commit(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        commit: &[u8],
    ) -> MlsResult<EpochInfo>;

    /// Join from a Welcome that adds one of this client's KeyPackages. Returns the group id and the epoch joined.
    /// Whether an accepted invitation precedes it is Core's decision, made before this is called.
    fn process_welcome(
        &mut self,
        env: &mut CryptoEnv<'_>,
        welcome: &[u8],
    ) -> MlsResult<(Vec<u8>, EpochInfo)>;

    /// Encrypt one application message under the current merged epoch (one MLS ciphertext; Core wraps it per
    /// recipient device). Refused `commit_pending` while a Commit of this client is pending (ADR-0163 rule 6).
    fn encrypt_application(
        &mut self,
        env: &mut CryptoEnv<'_>,
        group_id: &[u8],
        plaintext: &[u8],
    ) -> MlsResult<Vec<u8>>;

    /// Open an application message. Refused `wrong_epoch` unless it is for the current merged epoch (this engine
    /// keeps no past-epoch window yet, slice 4), `unknown_group` when this client is not (or no longer) a member.
    fn decrypt_application(&mut self, group_id: &[u8], ciphertext: &[u8]) -> MlsResult<Decrypted>;

    /// The merged epoch of the group.
    fn epoch(&self, group_id: &[u8]) -> MlsResult<u64>;

    /// MLS exporter secret for the current merged epoch (RFC 9420 section 8.5).
    fn export_secret(
        &self,
        group_id: &[u8],
        label: &str,
        context: &[u8],
        len: usize,
    ) -> MlsResult<Vec<u8>>;

    /// DeviceIDs of the current merged members, sorted.
    fn members(&self, group_id: &[u8]) -> MlsResult<Vec<Vec<u8>>>;

    /// The engine's complete state as opaque bytes, including any pending Commit. Tagged by [`Self::engine_tag`].
    fn snapshot(&self) -> Vec<u8>;
}

/// Rebuilds an engine from a snapshot, selected by the stored tag and version (see [`super::store`]).
pub trait GroupCryptoLoader {
    type Engine: GroupCrypto;
    /// The tag this loader reads.
    fn engine_tag() -> &'static str;
    /// Restore from `snapshot` (an engine-defined blob, which carries its own format version).
    fn restore(snapshot: &[u8]) -> MlsResult<Self::Engine>;
}
