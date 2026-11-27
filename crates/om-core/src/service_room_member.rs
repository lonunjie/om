// SPDX-License-Identifier: MPL-2.0
//! Full-member service path in one Room (`services_bots` slice 4; SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md mode 4,
//! "Revocation" and "Endpoint revocation reaches every Room", ADR-0128 rules 1 and 2, ADR-0129, ADR-0122 rules 1 and 6).
//! Pure state: the MLS Add/Remove and signatures belong to the caller.
//!
//! * Installing a service as a member is a high-risk trust-boundary change: it needs `room.install_service`, and when the
//!   Room configures a quorum, enough HUMAN approvers (service principals and endpoints never count).
//! * The roster entry is always visible and always says the service can read all Room content; a trigger filter limits
//!   automation, never reading ([`RosterEntry::can_read_all_content`] is not settable).
//! * The installation records the control head it was admitted under. The Room keeps the highest validated head and
//!   rejects rollback. A head that revokes or replaces an endpoint stops that endpoint's sending until a Commit removes it.
//! * Removal of a revoked endpoint needs no capability (basis: membership). The coordinator may commit at once; once
//!   `sequencing_timeout` has passed any member may. It is an authority reduction: it applies on any later head. Competing
//!   removals dedupe on `(service, endpoint, head version)`.

use crate::room_snapshot::HeadReference;
use crate::service_authorship::human_quorum;
use crate::service_control::{ACTIVE, ServiceState};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const INSTALL: &str = "room.install_service";

pub type Principal = (u64, Vec<u8>);

#[derive(Debug, PartialEq, Eq)]
pub enum RoomError {
    NotAuthorized,
    /// Fewer human approvers than the Room's configured quorum.
    QuorumNotMet,
    AlreadyInstalled,
    NotInstalled,
    /// A control head older than or equal to the one held with different content.
    Rollback,
    /// The head is for another service.
    WrongService,
    /// The endpoint was not revoked or replaced in the validated head.
    NotRevoked,
    /// Not yet the coordinator's, or before `sequencing_timeout` for another member.
    TooEarly,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TriggerFilter {
    pub event_classes: BTreeSet<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RosterEntry {
    pub service: [u8; 32],
    pub incarnation: Vec<u8>,
    /// Admission head `(version, hash)`.
    pub admitted_under: (u64, [u8; 32]),
    pub endpoints: BTreeSet<Vec<u8>>,
    pub filter: Option<TriggerFilter>,
}

impl RosterEntry {
    /// The UI line the roster must carry, whatever the filter says.
    pub fn can_read_all_content(&self) -> bool {
        true
    }
}

struct Pending {
    /// Head version that revoked or replaced it.
    head_version: u64,
    observed_at: u64,
}

pub struct RoomServices {
    quorum: Option<usize>,
    sequencing_timeout: u64,
    roster: BTreeMap<[u8; 32], RosterEntry>,
    heads: BTreeMap<[u8; 32], HeadReference>,
    pending: BTreeMap<([u8; 32], Vec<u8>), Pending>,
    committed: BTreeSet<([u8; 32], Vec<u8>, u64)>,
}

#[derive(Serialize, Deserialize)]
pub(crate) struct MemberSnapshot {
    pub(crate) roster: RosterEntry,
    pub(crate) head: HeadReference,
    pending: Vec<(Vec<u8>, u64)>,
}

pub(crate) type RemovalKey = ([u8; 32], Vec<u8>, u64);

impl RoomServices {
    /// `quorum`: human approvers needed to install, if the Room configures one.
    pub fn new(quorum: Option<usize>, sequencing_timeout: u64) -> Self {
        Self {
            quorum,
            sequencing_timeout,
            roster: BTreeMap::new(),
            heads: BTreeMap::new(),
            pending: BTreeMap::new(),
            committed: BTreeSet::new(),
        }
    }

    pub(crate) fn policy(&self) -> (Option<usize>, u64) {
        (self.quorum, self.sequencing_timeout)
    }

    pub(crate) fn snapshot_members(&self) -> Vec<MemberSnapshot> {
        self.roster
            .values()
            .map(|roster| MemberSnapshot {
                roster: roster.clone(),
                head: self.heads[&roster.service].clone(),
                pending: self
                    .pending
                    .iter()
                    .filter(|((id, _), _)| *id == roster.service)
                    .map(|((_, endpoint), pending)| (endpoint.clone(), pending.head_version))
                    .collect(),
            })
            .collect()
    }

    pub(crate) fn committed_removals(&self) -> &BTreeSet<RemovalKey> {
        &self.committed
    }

    pub(crate) fn restore_committed_removals(&mut self, removals: BTreeSet<RemovalKey>) {
        self.committed = removals;
    }

    pub(crate) fn restore_member(
        &mut self,
        entry: MemberSnapshot,
        admitted: &HeadReference,
        verify: &impl Fn(&HeadReference) -> bool,
        now: u64,
    ) -> bool {
        let id = entry.roster.service;
        if self.roster.contains_key(&id)
            || entry.head.id != id
            || admitted.id != id
            || entry.roster.admitted_under != (admitted.version, admitted.hash)
            || !verify(admitted)
            || !verify(&entry.head)
            || entry.roster.endpoints.iter().any(|id| {
                !admitted
                    .endpoints
                    .iter()
                    .any(|(ep, state)| ep == id && *state == ACTIVE)
            })
            || entry.pending.iter().any(|(ep, version)| {
                !entry.roster.endpoints.contains(ep) || *version < admitted.version
            })
        {
            return false;
        }
        self.pending
            .extend(entry.pending.into_iter().map(|(ep, head_version)| {
                (
                    (id, ep),
                    Pending {
                        head_version,
                        observed_at: now,
                    },
                )
            }));
        self.heads.insert(id, entry.head);
        self.roster.insert(id, entry.roster);
        true
    }

    /// Installs `head`'s usable endpoints as members. `approvers` are the principals that approved.
    pub fn install(
        &mut self,
        caps: &BTreeSet<String>,
        head: &ServiceState,
        incarnation: Vec<u8>,
        filter: Option<TriggerFilter>,
        approvers: &[Principal],
    ) -> Result<(), RoomError> {
        if !caps.contains(INSTALL) {
            return Err(RoomError::NotAuthorized);
        }
        if let Some(q) = self.quorum {
            let eps: BTreeSet<Vec<u8>> = head.endpoints.keys().cloned().collect();
            let mut humans = human_quorum(approvers, &eps);
            humans.sort();
            humans.dedup();
            if humans.len() < q {
                return Err(RoomError::QuorumNotMet);
            }
        }
        if self.roster.contains_key(&head.service_id) {
            return Err(RoomError::AlreadyInstalled);
        }
        let endpoints = head
            .endpoints
            .values()
            .filter(|e| e.state == ACTIVE)
            .map(|e| e.id.clone())
            .collect();
        self.roster.insert(
            head.service_id,
            RosterEntry {
                service: head.service_id,
                incarnation,
                admitted_under: (head.version, head.hash),
                endpoints,
                filter,
            },
        );
        self.heads.insert(head.service_id, head.into());
        Ok(())
    }

    /// A trigger-filter change is an ordinary installation update: it never changes what the service can read. It still
    /// needs the install capability here (the safe reading while the design is silent on who may edit it).
    pub fn set_filter(
        &mut self,
        caps: &BTreeSet<String>,
        service: &[u8; 32],
        filter: Option<TriggerFilter>,
    ) -> Result<(), RoomError> {
        if !caps.contains(INSTALL) {
            return Err(RoomError::NotAuthorized);
        }
        let e = self
            .roster
            .get_mut(service)
            .ok_or(RoomError::NotInstalled)?;
        e.filter = filter;
        Ok(())
    }

    /// Records a validated control head (the caller checked its chain and signatures). Keeps the highest; an older head
    /// is rejected, the same head is idempotent. Endpoints in the Room that the head revokes or no longer lists become
    /// pending removals.
    pub fn observe_head(&mut self, head: &ServiceState, now: u64) -> Result<(), RoomError> {
        let Some(entry) = self.roster.get(&head.service_id) else {
            return Err(RoomError::NotInstalled);
        };
        let cur = &self.heads[&head.service_id];
        if head.version < cur.version || (head.version == cur.version && head.hash != cur.hash) {
            return Err(RoomError::Rollback);
        }
        if head.version == cur.version {
            return Ok(());
        }
        let gone: Vec<Vec<u8>> = entry
            .endpoints
            .iter()
            .filter(|id| head.endpoints.get(*id).is_none_or(|e| e.state != ACTIVE))
            .cloned()
            .collect();
        self.heads.insert(head.service_id, head.into());
        for id in gone {
            self.pending
                .entry((head.service_id, id))
                .or_insert(Pending {
                    head_version: head.version,
                    observed_at: now,
                });
        }
        Ok(())
    }

    /// Verified endpoint revocation is permanent, including on a fork.
    pub fn observe_revocations(&mut self, head: &ServiceState, now: u64) {
        let Some(entry) = self.roster.get(&head.service_id) else {
            return;
        };
        for id in &entry.endpoints {
            if head
                .endpoints
                .get(id)
                .is_some_and(|ep| ep.state == crate::service_control::REVOKED)
            {
                self.pending
                    .entry((head.service_id, id.clone()))
                    .or_insert(Pending {
                        head_version: head.version,
                        observed_at: now,
                    });
            }
        }
    }

    /// True while the endpoint must send no application messages (a removal is pending).
    pub fn must_stop_sending(&self, service: &[u8; 32], endpoint: &[u8]) -> bool {
        self.pending.contains_key(&(*service, endpoint.to_vec()))
    }

    /// May this member commit the removal of a revoked endpoint now? No capability is needed (basis: membership).
    pub fn may_commit_removal(
        &self,
        service: &[u8; 32],
        endpoint: &[u8],
        is_coordinator: bool,
        now: u64,
    ) -> Result<(), RoomError> {
        let p = self
            .pending
            .get(&(*service, endpoint.to_vec()))
            .ok_or(RoomError::NotRevoked)?;
        if is_coordinator || now >= p.observed_at.saturating_add(self.sequencing_timeout) {
            Ok(())
        } else {
            Err(RoomError::TooEarly)
        }
    }

    /// The Commit that removed the endpoint's leaf was accepted. Returns false for a duplicate commit of the same
    /// `(service, endpoint, head version)`, so competing identical removals apply once.
    pub fn removal_committed(&mut self, service: &[u8; 32], endpoint: &[u8]) -> bool {
        let Some(p) = self.pending.remove(&(*service, endpoint.to_vec())) else {
            return false;
        };
        if !self
            .committed
            .insert((*service, endpoint.to_vec(), p.head_version))
        {
            return false;
        }
        if let Some(e) = self.roster.get_mut(service) {
            e.endpoints.remove(endpoint);
        }
        true
    }

    /// Removes the whole installation through Room control plus MLS removal of every endpoint.
    pub fn remove_installation(
        &mut self,
        caps: &BTreeSet<String>,
        service: &[u8; 32],
    ) -> Result<Vec<Vec<u8>>, RoomError> {
        if !caps.contains(INSTALL) {
            return Err(RoomError::NotAuthorized);
        }
        let e = self.roster.remove(service).ok_or(RoomError::NotInstalled)?;
        self.heads.remove(service);
        self.pending.retain(|(s, _), _| s != service);
        Ok(e.endpoints.into_iter().collect())
    }

    pub fn entry(&self, service: &[u8; 32]) -> Option<&RosterEntry> {
        self.roster.get(service)
    }
    pub fn held_head(&self, service: &[u8; 32]) -> Option<(u64, [u8; 32])> {
        self.heads
            .get(service)
            .map(|head| (head.version, head.hash))
    }
}
