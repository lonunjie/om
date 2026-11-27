// SPDX-License-Identifier: MPL-2.0
//! Bounded, device-local snapshots and receipts for the engineering Room reducers (ADR-0277).

use crate::service_control::{Endpoint, ServiceState};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::collections::{BTreeSet, VecDeque};
use std::io::{self, Write};

pub const MAX_SNAPSHOT_BYTES: usize = 512 * 1024;
pub const MAX_ROOM_ROWS: usize = 256;
pub const OPERATION_LIMIT: usize = 64;

pub type OperationId = [u8; 32];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mutation {
    AdmitOffer,
    InstallOffer,
    RemoveMember,
    Remove,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationTarget {
    pub mutation: Mutation,
    pub principal: [u8; 32],
    pub review: Option<[u8; 32]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperationOutcome {
    Applied,
    Removed(usize),
    Refused(String),
}

impl OperationOutcome {
    pub fn refused(reason: &str) -> Self {
        Self::Refused(reason.into())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Receipt {
    id: OperationId,
    target: OperationTarget,
    outcome: OperationOutcome,
}

#[derive(Default, Serialize, Deserialize)]
pub struct RoomOperations(VecDeque<Receipt>);

impl RoomOperations {
    pub fn lookup(&self, id: OperationId, target: OperationTarget) -> Option<OperationOutcome> {
        self.0.iter().find(|entry| entry.id == id).map(|entry| {
            if entry.target == target {
                entry.outcome.clone()
            } else {
                OperationOutcome::refused("operation_conflict")
            }
        })
    }

    pub(crate) fn record(
        &mut self,
        id: OperationId,
        target: OperationTarget,
        outcome: OperationOutcome,
    ) {
        if self.0.len() == OPERATION_LIMIT {
            self.0.pop_front();
        }
        self.0.push_back(Receipt {
            id,
            target,
            outcome,
        });
    }

    fn valid(&self) -> bool {
        let mut seen = BTreeSet::new();
        self.0.len() <= OPERATION_LIMIT
            && self.0.iter().all(|entry| {
                seen.insert(entry.id)
                    && match &entry.outcome {
                        OperationOutcome::Refused(reason) => {
                            !reason.is_empty() && reason.len() <= 64
                        }
                        _ => true,
                    }
            })
    }
}

/// A snapshot carries the same head facts an offer supplies live. Authority is checked against an independent observation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadReference {
    pub id: [u8; 32],
    pub version: u64,
    pub hash: [u8; 32],
    pub endpoints: Vec<(Vec<u8>, u64)>,
}

impl From<&ServiceState> for HeadReference {
    fn from(head: &ServiceState) -> Self {
        Self {
            id: head.service_id,
            version: head.version,
            hash: head.hash,
            endpoints: head
                .endpoints
                .iter()
                .map(|(id, endpoint)| (id.clone(), endpoint.state))
                .collect(),
        }
    }
}

impl HeadReference {
    pub(crate) fn into_state(self) -> ServiceState {
        ServiceState {
            service_id: self.id,
            version: self.version,
            hash: self.hash,
            prev: None,
            controllers: BTreeSet::new(),
            threshold: 0,
            endpoints: self
                .endpoints
                .into_iter()
                .map(|(id, state)| {
                    (
                        id.clone(),
                        Endpoint {
                            id,
                            state,
                            profile: 0,
                            key: Vec::new(),
                            mode: 0,
                            host_ref: None,
                        },
                    )
                })
                .collect(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum SnapshotError {
    Invalid,
    Limit,
}

#[derive(Serialize, Deserialize)]
struct Snapshot<S> {
    version: u8,
    state: S,
    operations: RoomOperations,
}

#[derive(Default)]
struct SnapshotBytes(Vec<u8>);

impl Write for SnapshotBytes {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_SNAPSHOT_BYTES - self.0.len() {
            return Err(io::Error::other("Room snapshot exceeds byte limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub(crate) fn encode<S: Serialize>(
    state: S,
    operations: &RoomOperations,
) -> Result<Vec<u8>, SnapshotError> {
    #[derive(Serialize)]
    struct Borrowed<'a, S> {
        version: u8,
        state: S,
        operations: &'a RoomOperations,
    }
    let mut bytes = SnapshotBytes::default();
    serde_json::to_writer(
        &mut bytes,
        &Borrowed {
            version: 1,
            state,
            operations,
        },
    )
    .map_err(|error| {
        if error.is_io() {
            SnapshotError::Limit
        } else {
            SnapshotError::Invalid
        }
    })?;
    Ok(bytes.0)
}

pub(crate) fn decode<S: Serialize + DeserializeOwned>(
    bytes: &[u8],
) -> Result<(S, RoomOperations), SnapshotError> {
    if bytes.len() > MAX_SNAPSHOT_BYTES {
        return Err(SnapshotError::Limit);
    }
    let snapshot: Snapshot<S> =
        serde_json::from_slice(bytes).map_err(|_| SnapshotError::Invalid)?;
    if snapshot.version != 1
        || !snapshot.operations.valid()
        || encode(&snapshot.state, &snapshot.operations)?.as_slice() != bytes
    {
        return Err(SnapshotError::Invalid);
    }
    Ok((snapshot.state, snapshot.operations))
}
