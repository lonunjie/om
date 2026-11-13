// SPDX-License-Identifier: MPL-2.0
//! The OM storage adapter for group crypto state, behind Core's own storage boundary.
//!
//! A library's serialization of its MLS state is never the canonical format (CRYPTOGRAPHY_AND_AGILITY): what Core
//! persists is a [`GroupCryptoRec`], an OM-owned record that carries the engine's opaque snapshot as a tagged blob.
//! The canonical group state is the `GroupControlState` head in `AppState.groups` and the journal; the blob is
//! engine-private and replaceable by a loader for the same tag.
//!
//! Versioning and compatibility:
//!
//! * `version` is the OM record version (this file: 1). A reader refuses a record newer than it understands
//!   (`store_too_new`) rather than guess. New fields are additive with serde defaults, so an old record still loads.
//! * `engine` is the tag of the engine that wrote the snapshot; the loader must be for the same tag
//!   (`engine_mismatch`). The snapshot carries its own format version, which the engine's loader checks.
//! * `snapshotHash` is checked on every open, so a corrupted blob is noticed and refused (`record_corrupt`), never
//!   handed to an engine.
//! * `AppState.groupCrypto` is absent in records written before this slice: it loads as `None`.

use super::mls::{GroupCrypto, GroupCryptoLoader};
use super::{GroupError, R, err};
use crate::crypto::{hex, sha256, unhex};
use serde::{Deserialize, Serialize};

/// The record version this build writes and reads.
pub const STORE_VERSION: u32 = 1;

const LBL: &[u8] = b"OM-GROUP-CRYPTO-SNAPSHOT-v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupCryptoRec {
    pub version: u32,
    /// Tag of the engine that wrote `snapshot`.
    pub engine: String,
    /// The engine's opaque snapshot (hex). Never interpreted by Core.
    pub snapshot: String,
    /// `SHA-256(label || engine || snapshot bytes)` (hex).
    pub snapshot_hash: String,
}

fn hash_of(engine: &str, snapshot: &[u8]) -> String {
    hex(&sha256(&[LBL, engine.as_bytes(), snapshot]))
}

impl GroupCryptoRec {
    pub fn of(engine: &dyn GroupCrypto) -> Self {
        let snap = engine.snapshot();
        GroupCryptoRec {
            version: STORE_VERSION,
            engine: engine.engine_tag().to_string(),
            snapshot_hash: hash_of(engine.engine_tag(), &snap),
            snapshot: hex(&snap),
        }
    }

    /// Rebuild the engine with loader `L`. Fails closed on a newer record, another engine or a corrupted blob.
    pub fn open<L: GroupCryptoLoader>(&self) -> R<L::Engine> {
        if self.version > STORE_VERSION {
            return err("store_too_new");
        }
        if self.engine != L::engine_tag() {
            return err("engine_mismatch");
        }
        let snap = unhex(&self.snapshot).map_err(|_| GroupError("record_corrupt"))?;
        if hash_of(&self.engine, &snap) != self.snapshot_hash {
            return err("record_corrupt");
        }
        L::restore(&snap).map_err(|e| GroupError(e.0))
    }
}

impl crate::state::AppState {
    /// Persist the engine's current state (replaces the previous record).
    pub fn group_crypto_save(&mut self, engine: &dyn GroupCrypto) {
        self.group_crypto = Some(GroupCryptoRec::of(engine));
    }

    /// Reopen the stored engine, or `None` when this device has not created or joined a group.
    pub fn group_crypto_open<L: GroupCryptoLoader>(&self) -> R<Option<L::Engine>> {
        self.group_crypto
            .as_ref()
            .map(|r| r.open::<L>())
            .transpose()
    }
}
