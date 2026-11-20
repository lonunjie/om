// SPDX-License-Identifier: MPL-2.0
//! The Core state machine: profile namespacing, staged transactions, intents, queries.
//!
//! Sans-IO. Every mutating call stages a transaction and returns a [`Commit`] for the host to make durable;
//! the staged state replaces the visible state only on `finish_commit(id, true)` ("commit before release").
//! One commit may be outstanding at a time; the host serialises calls (the TypeScript driver uses a queue).

use crate::api::*;
use crate::backup::{self, BackupPlan};
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::flows::Tx;
use crate::groups::GroupError;
use crate::groups::carriage::Sealed;
use crate::groups::carry::CarryEnv;
use crate::groups::flow::{
    GroupCtx, GroupIntent, GroupQuery, GroupReply, GroupView, GroupWire, Sender,
};
use crate::groups::mls::GroupCrypto;
use crate::host::*;
use crate::route::{DepositRevocation, MAX_OFFERS, QueueRetirement, RoomRouteOffer, RouteOffer};
use crate::state::*;
use std::collections::BTreeMap;
use zeroize::Zeroize;

/// One relationship-lane event handed to the group flow: the relationship and the flow's answer.
pub type LanePumped = (String, Result<GroupReply, GroupError>);

/// One per-Room mailbox the host polls for the device lane, and the Home (`relay_uri`) that holds it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupMailbox {
    pub mailbox: Vec<u8>,
    pub relay_uri: String,
}

/// What the host needs from Core to carry the device lane (see [`Core::group_carriage_status`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GroupCarriageStatus {
    pub mailboxes: Vec<GroupMailbox>,
    /// Rooms (ConversationID hex, or `invite:<id>`) that owe a route update after a removal.
    pub route_owed: Vec<String>,
    pub outbox: usize,
    pub pending: usize,
    /// Invite, Accept and Decline the 1:1 lane has not taken yet.
    pub lane_out: usize,
    /// Lane items dropped for good (refused for a reason that cannot change, or too old): the person is told.
    pub lane_lost: u32,
}

/// Held group invitations and what could not be kept (see [`Core::group_held_invitations`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HeldInvitations {
    pub invitations: Vec<crate::groups::carriage::HeldInvitation>,
    pub overflow: crate::groups::carriage::LaneOverflow,
}

/// A removal means every remaining device owes the Room a fresh route (ADR-0189 1.3).
fn note_removals(app: &mut AppState, r: &GroupReply, now_s: u64) {
    for e in &r.events {
        if let crate::groups::flow::GroupEvent::MemberRemoved {
            conversation_id, ..
        }
        | crate::groups::flow::GroupEvent::DeviceRemoved { conversation_id } = e
        {
            app.carry_note_removal(conversation_id);
        }
        if let crate::groups::flow::GroupEvent::NoLongerMember { conversation_id } = e {
            app.group_carriage
                .retire_room(&crate::crypto::hex(conversation_id), now_s);
        }
    }
}

struct Slot {
    room_offers: Vec<RoomRouteOffer>,
    pack_upload: zeroize::Zeroizing<Vec<u8>>,
    pack_review: Option<(u64, crate::expressive::ReviewedPack)>,
    state: ProfileState,
    /// Exactly what the host has durably stored for this profile.
    committed: BTreeMap<String, Vec<u8>>,
}

struct Staged {
    id: u64,
    profile: ProfileId,
    next: ProfileState,
    records: BTreeMap<String, Vec<u8>>,
    changed: Vec<OperationId>,
}

enum CallHostEvent<'a> {
    Tick,
    Media(&'a str, crate::calls::MediaState),
    Route(&'a str, crate::calls::CallRoute),
}

pub struct Core<R: RngCore + CryptoRng> {
    rng: R,
    clock: Box<dyn Clock>,
    home: HomeSummary,
    profiles: BTreeMap<ProfileId, Slot>,
    boot: String,
    next_generation: u64,
    next_commit: u64,
    next_pack_review: u64,
    staged: Option<Staged>,
    /// Routes the host's provider adapter allocated for mailboxes Core has not created yet. Memory only: an offer is
    /// worthless without the install secret the adapter holds, so nothing is lost by dropping them at restart.
    offers: Vec<RouteOffer>,
    /// What the BlobStore advertised; memory only (the host supplies it again at start).
    blob_limits: Option<BlobLimits>,
    calls_enabled: bool,
    expressive_enabled: bool,
    recovery_place: Option<crate::recovery_record::RecoveryPlace>,
    /// Direct offers being served, by (profile, BlobID hex). Memory only: they hold the sealed object, which is rebuilt
    /// from the person's file after a restart (`arm_direct_offer`); what must survive is `AppState.direct_offers`.
    direct_senders: BTreeMap<(ProfileId, String), crate::direct::DirectSender>,
    /// Direct fetches in progress, by (profile, message, attachment index). Memory only (resume state is
    /// transport-local, ADR-0158 section 8): after a restart the fetch starts again.
    direct_fetches: BTreeMap<(ProfileId, String, usize), crate::direct::DirectFetch>,
    /// The MLS engine of each open profile's device (ordinary groups, ADR-0177). Memory only and a cache of
    /// `AppState.group_crypto`: it is dropped whenever a transition fails or the host does not make it durable, and
    /// rebuilt from the committed snapshot on next use.
    group_engines: BTreeMap<ProfileId, Box<dyn GroupCrypto>>,
}

/// A new engine for this device, or the one stored in `app` (the loader is chosen by the build: the real OpenMLS engine
/// with `groups-openmls`, the test-only fake in test builds, otherwise groups are unavailable).
fn group_engine_open(
    device_id: &[u8],
    stored: Option<&crate::groups::store::GroupCryptoRec>,
) -> Result<Box<dyn GroupCrypto>, GroupError> {
    #[cfg(feature = "groups-openmls")]
    {
        use crate::groups::openmls::{OpenMlsGroupCrypto, OpenMlsLoader};
        match stored {
            Some(r) => Ok(Box::new(r.open::<OpenMlsLoader>()?)),
            None => Ok(Box::new(
                OpenMlsGroupCrypto::new(device_id).map_err(|e| GroupError(e.0))?,
            )),
        }
    }
    #[cfg(all(not(feature = "groups-openmls"), any(test, feature = "test-loopback")))]
    {
        use crate::groups::fake::{FakeGroupCrypto, FakeLoader};
        match stored {
            Some(r) => Ok(Box::new(r.open::<FakeLoader>()?)),
            None => Ok(Box::new(FakeGroupCrypto::new(device_id))),
        }
    }
    #[cfg(not(any(feature = "groups-openmls", test, feature = "test-loopback")))]
    {
        let _ = (device_id, stored);
        Err(GroupError("groups_unavailable"))
    }
}

impl<R: RngCore + CryptoRng> Core<R> {
    pub fn new(mut rng: R, clock: impl Clock + 'static, home: HomeSummary) -> Self {
        // A per-boot nonce makes every generation unique across process restarts, so a context captured by a
        // previous process can never match a profile opened by this one.
        let mut b = [0u8; 6];
        rng.fill_bytes(&mut b);
        Core {
            rng,
            clock: Box::new(clock),
            home,
            profiles: BTreeMap::new(),
            boot: crate::crypto::hex(&b),
            next_generation: 1,
            next_commit: 1,
            next_pack_review: 1,
            staged: None,
            offers: vec![],
            blob_limits: None,
            calls_enabled: false,
            expressive_enabled: false,
            recovery_place: None,
            direct_senders: BTreeMap::new(),
            direct_fetches: BTreeMap::new(),
            group_engines: BTreeMap::new(),
        }
    }

    /// The host hands over routes for future mailboxes (see [`crate::route`]). Invalid, duplicate and surplus offers
    /// are dropped. Returns how many offers Core holds afterwards; the host tops up whenever this runs low.
    pub fn supply_routes(&mut self, offers: Vec<RouteOffer>) -> usize {
        for o in offers {
            if self.offers.len() >= MAX_OFFERS {
                break;
            }
            if o.is_valid() && !self.offers.iter().any(|x| x.mailbox == o.mailbox) {
                self.offers.push(o);
            }
        }
        self.offers.len()
    }

    /// Add single-use, Home-minted Room offers to this open profile's pool.
    pub fn supply_room_routes(
        &mut self,
        ctx: &ProfileExecutionContext,
        offers: Vec<RoomRouteOffer>,
    ) -> Result<usize, CoreError> {
        self.slot(ctx)?;
        let now = self.clock.now_ms() / 1000;
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        slot.room_offers.retain(|o| o.expires_at > now);
        for offer in offers {
            if slot.room_offers.len() >= MAX_OFFERS {
                break;
            }
            if offer.route.is_valid()
                && offer.id.len() == 16
                && offer.expires_at > now
                && !slot.state.app.group_carriage.owns_grant(&offer.grant())
                && !slot
                    .room_offers
                    .iter()
                    .any(|o| o.route.mailbox == offer.route.mailbox)
            {
                slot.room_offers.push(offer);
            }
        }
        Ok(slot.room_offers.len())
    }

    /// True once the encrypted profile owns the mint or its retirement.
    pub fn room_grant_claimed(
        &self,
        ctx: &ProfileExecutionContext,
        grant: &DepositRevocation,
    ) -> Result<bool, CoreError> {
        Ok(self.slot(ctx)?.state.app.group_carriage.owns_grant(grant))
    }

    /// Retire an unpublished grant at its original Home, even if that Home is currently unavailable.
    pub fn retire_room_grant(
        &mut self,
        ctx: &ProfileExecutionContext,
        grant: DepositRevocation,
        expires_at: u64,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.st
                .app
                .group_carriage
                .retire_unclaimed(grant.clone(), expires_at);
            tx.room_offers.retain(|offer| offer.grant() != grant);
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// The person stopped using the Home `relay_uri`: every relationship with another route publishes a new RelaySet
    /// version without it and keeps polling the dropped route for the overlap (immediately closed when `emergency`).
    /// Returns how many relationships had no other route and so stay on that Home.
    pub fn retire_home(
        &mut self,
        ctx: &ProfileExecutionContext,
        relay_uri: &str,
        emergency: bool,
    ) -> Result<(usize, Step), CoreError> {
        self.slot(ctx)?;
        let (stuck, commit) = self.txn(&ctx.profile_id, |tx| {
            Ok(tx.retire_home(relay_uri, emergency))
        })?;
        Ok((
            stuck,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// Whether this build can run ordinary groups at all (the real engine is linked, or a test build).
    pub fn groups_available() -> bool {
        cfg!(any(
            feature = "groups-openmls",
            test,
            feature = "test-loopback"
        ))
    }

    fn group_engine_take(&mut self, profile: &str) -> Result<Box<dyn GroupCrypto>, GroupError> {
        if let Some(e) = self.group_engines.remove(profile) {
            return Ok(e);
        }
        let st = &self
            .profiles
            .get(profile)
            .ok_or(GroupError("unknown_profile"))?
            .state;
        let dev = st.device.as_ref().ok_or(GroupError("no_identity"))?;
        let mut engine = group_engine_open(&dev.device_id, st.app.group_crypto.as_ref())?;
        // The MLS leaf key is dedicated to MLS and generated inside the engine; the device signs it once, so the
        // credential of every KeyPackage and group names a key an ACS-committed device credential attests.
        if engine.attestation().is_empty() {
            let mut rnd = [0u8; 32];
            self.rng.fill_bytes(&mut rnd);
            let att = crate::groups::endpoint::attest_leaf(
                &dev.signer,
                &dev.device_id,
                &engine.signature_key(),
                rnd,
            );
            engine.set_attestation(att).map_err(|e| GroupError(e.0))?;
        }
        Ok(engine)
    }

    /// Run one ordinary-groups transition (ADR-0177) as a staged transaction: the typed intent of the person, or what
    /// arrived from a relationship or a device. The inner `Err` is a typed refusal (nothing changed, nothing to
    /// commit); the `Step.commit` of a success must be durable before any `Outbound` of the reply is sent.
    pub fn group_do(
        &mut self,
        ctx: &ProfileExecutionContext,
        intent: GroupIntent,
    ) -> Result<(Result<GroupReply, GroupError>, Step), CoreError> {
        if matches!(intent, GroupIntent::CommitAccepted { .. }) {
            // G10 (ADR-0189 section 2): acceptance of an own Commit is Core's judgment, never a host report.
            return Ok((Err(GroupError("acceptance_is_core_owned")), Step::default()));
        }
        self.group_step(ctx, |app, gctx| {
            let r = app.group_do(gctx, intent)?;
            note_removals(app, &r, gctx.now_ms / 1000);
            app.carry_outbox_add(&r, gctx.now_ms / 1000)?;
            Ok(r)
        })
    }

    /// G10: after the host made a step's `Step.commit` durable, accept every own Commit whose three conditions hold
    /// (ADR-0189 section 2) and release its Welcome. The host calls this after each group step and at start-up (it also
    /// finishes a Commit whose step committed just before a crash). Returns one reply per Commit finished.
    pub fn group_settle(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(Result<Vec<GroupReply>, GroupError>, Step), CoreError> {
        let open = self.slot(ctx)?.state.app.group_journal.values().any(|e| {
            e.queued.is_some()
                && matches!(
                    e.state,
                    crate::groups::journal::JournalState::CandidatePrepared
                        | crate::groups::journal::JournalState::PublishedUnconfirmed
                )
        });
        if !open {
            return Ok((Ok(vec![]), Step::default()));
        }
        self.group_step(ctx, |app, gctx| {
            let reps = app.group_settle(gctx)?;
            for r in &reps {
                note_removals(app, r, gctx.now_ms / 1000);
            }
            Ok(reps)
        })
    }

    /// The frames this Core owns until the host deposits them, oldest first (a Commit before any frame of its new
    /// epoch). Seal each with [`Core::group_frame_send`] and report it with [`Core::group_outbox_done`].
    pub fn group_outbox(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Vec<crate::groups::carriage::OutItem>, CoreError> {
        Ok(self.slot(ctx)?.state.app.group_carriage.outbox.clone())
    }

    /// The host deposited these outbox frames (transport progress only; never acceptance).
    pub fn group_outbox_done(
        &mut self,
        ctx: &ProfileExecutionContext,
        ids: &[u64],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let ((), commit) = self.txn(&ctx.profile_id, |tx| {
            tx.st.app.carry_outbox_done(ids);
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Retry a frame after a poll may have supplied its missing account head, Welcome or parent Commit.
    pub fn group_pending_retry(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(Result<Vec<GroupReply>, GroupError>, Step), CoreError> {
        if self.slot(ctx)?.state.app.group_carriage.pending.is_empty() {
            return Ok((Ok(vec![]), Step::default()));
        }
        self.group_step(ctx, |app, gctx| {
            let now_s = gctx.now_ms / 1000;
            let reps = app.carry_pending_retry(gctx, now_s)?;
            for r in &reps {
                note_removals(app, r, gctx.now_ms / 1000);
            }
            Ok(reps)
        })
    }

    /// Ask, through the existing K7 head request, each relationship whose head a held frame is waiting for (rate
    /// limited per relationship). Frames that wait for an account with no relationship cannot be helped (question Q5).
    pub fn group_pending_refresh(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let ((), commit) = self.txn(&ctx.profile_id, |tx| {
            for rel in tx.st.app.carry_pending_wants() {
                tx.ask_head(&rel);
            }
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// After a removal: mint this device's fresh route for the Room and queue the `route_update` frames for the rest of the
    /// roster in Core's outbox (ADR-0189 1.3, ADR-0120 K1), in the same transaction as clearing the owed mark. Returns how
    /// many frames were queued; the host drains them with [`Core::group_outbox`] like any other item, and later revokes
    /// what [`Core::group_routes_expire`] returns. A refusal (`no_route_offer`, `outbox_full`) changes nothing: the Room
    /// stays owed and the call can be repeated.
    pub fn group_route_update(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &[u8],
    ) -> Result<(Result<usize, GroupError>, Step), CoreError> {
        self.slot(ctx)?;
        let mut refused: Option<GroupError> = None;
        let room = crate::crypto::hex(conversation_id);
        let res = self.txn(&ctx.profile_id, |tx| {
            let now_s = tx.now_ms / 1000;
            let st = &mut *tx.st;
            let dev = st
                .device
                .as_ref()
                .ok_or(CoreError::Invalid("no identity"))?;
            let mut env = CarryEnv {
                signer: &dev.signer,
                device_id: dev.device_id,
                account_id: dev.account_id,
                rng: &mut *tx.rng,
                offers: &mut *tx.room_offers,
                now_s,
            };
            match st.app.carry_route_update_queue(&mut env, &room) {
                Ok(n) => Ok(n),
                Err(e) => {
                    refused = Some(e);
                    Err(CoreError::Invalid("group_refused"))
                }
            }
        });
        match res {
            Ok((v, commit)) => Ok((
                Ok(v),
                Step {
                    commit,
                    handle: None,
                },
            )),
            Err(e) => match refused {
                Some(r) => Ok((Err(r), Step::default())),
                None => Err(e),
            },
        }
    }

    /// See [`Core::group_do`]; `from` is what the transport authenticated (a relationship or a member device).
    pub fn group_receive(
        &mut self,
        ctx: &ProfileExecutionContext,
        from: Sender,
        wire: GroupWire,
    ) -> Result<(Result<GroupReply, GroupError>, Step), CoreError> {
        self.group_step(ctx, |app, gctx| {
            let r = app.group_receive(gctx, from, wire)?;
            note_removals(app, &r, gctx.now_ms / 1000);
            app.carry_outbox_add(&r, gctx.now_ms / 1000)?;
            Ok(r)
        })
    }

    fn group_step<T>(
        &mut self,
        ctx: &ProfileExecutionContext,
        f: impl FnOnce(&mut AppState, &mut GroupCtx<'_>) -> Result<T, GroupError>,
    ) -> Result<(Result<T, GroupError>, Step), CoreError> {
        self.slot(ctx)?;
        let profile = ctx.profile_id.clone();
        let mut engine = match self.group_engine_take(&profile) {
            Ok(e) => e,
            Err(e) => return Ok((Err(e), Step::default())),
        };
        let mut refused: Option<GroupError> = None;
        let res = self.txn(&profile, |tx| {
            let now_ms = tx.now_ms;
            let st = &mut *tx.st;
            let dev = st
                .device
                .as_ref()
                .ok_or(CoreError::Invalid("no identity"))?;
            let mut gctx = GroupCtx {
                engine: &mut *engine,
                rng: &mut *tx.rng,
                now_ms,
                signer: &dev.signer,
                my_account: dev.account_id.to_vec(),
            };
            match f(&mut st.app, &mut gctx) {
                Ok(r) => Ok(r),
                Err(e) => {
                    refused = Some(e);
                    Err(CoreError::Invalid("group_refused"))
                }
            }
        });
        match res {
            Ok((reply, commit)) => {
                self.group_engines.insert(profile, engine);
                Ok((
                    Ok(reply),
                    Step {
                        commit,
                        handle: None,
                    },
                ))
            }
            // The engine may have moved past the state that was discarded: it is rebuilt from the committed snapshot.
            Err(e) => match refused {
                Some(r) => Ok((Err(r), Step::default())),
                None => Err(e),
            },
        }
    }

    /// Group carriage (ADR-0189, Package 2a). Send an Invite, Accept or Decline the flow produced (`Outbound` with
    /// `Target::Relationship`) as a kind 15, 16 or 17 event on the relationship. Nothing is queued on a refusal.
    pub fn group_lane_send(
        &mut self,
        ctx: &ProfileExecutionContext,
        relationship_id: &str,
        wire: &GroupWire,
    ) -> Result<(Result<(), GroupError>, Step), CoreError> {
        self.slot(ctx)?;
        let mut refused: Option<GroupError> = None;
        let res = self.txn(&ctx.profile_id, |tx| {
            let now_s = tx.now_ms / 1000;
            let body = {
                let st = &mut *tx.st;
                let dev = st
                    .device
                    .as_ref()
                    .ok_or(CoreError::Invalid("no identity"))?;
                let mut env = CarryEnv {
                    signer: &dev.signer,
                    device_id: dev.device_id,
                    account_id: dev.account_id,
                    rng: &mut *tx.rng,
                    offers: &mut *tx.room_offers,
                    now_s,
                };
                st.app.carry_lane_body(&mut env, wire)
            };
            let body = match body {
                Ok(b) => b,
                Err(e) => {
                    refused = Some(e);
                    return Err(CoreError::Invalid("group_refused"));
                }
            };
            match tx.group_lane_queue(relationship_id, &body)? {
                Ok(()) => {
                    // The item the flow step kept for this lane (if any) is taken in the same transaction.
                    tx.st.app.carry_lane_out_take(relationship_id, wire);
                    Ok(())
                }
                Err(e) => {
                    refused = Some(e);
                    Err(CoreError::Invalid("group_refused"))
                }
            }
        });
        match res {
            Ok(((), commit)) => Ok((
                Ok(()),
                Step {
                    commit,
                    handle: None,
                },
            )),
            Err(e) => match refused {
                Some(r) => Ok((Err(r), Step::default())),
                None => Err(e),
            },
        }
    }

    /// Take the oldest Invite, Accept or Decline the flow kept (`GroupCarriageRec::lane_out`) into the 1:1 queue, in one
    /// transaction with removing it (report D58 closed): a crash repeats nothing and loses nothing. Returns `Ok(true)`
    /// when one was taken (call again), `Ok(false)` when none can move now. A relationship that is not usable yet or a
    /// missing route offer keeps its item (and everything behind it for that relationship); an item older than
    /// `LANE_OUT_TTL_S`, or refused for a reason that cannot change, is dropped and counted (`lane_lost`), never silently.
    pub fn group_lane_flush(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(Result<bool, GroupError>, Step), CoreError> {
        use crate::groups::carriage as cg;
        self.slot(ctx)?;
        let items = self.slot(ctx)?.state.app.group_carriage.lane_out.clone();
        if items.is_empty() {
            return Ok((Ok(false), Step::default()));
        }
        let mut blocked: Vec<String> = vec![];
        let mut last: Option<GroupError> = None;
        for item in items {
            if blocked.contains(&item.relationship) {
                continue;
            }
            let mut refused: Option<GroupError> = None;
            let res = self.txn(&ctx.profile_id, |tx| {
                let now_s = tx.now_ms / 1000;
                let body = {
                    let st = &mut *tx.st;
                    let dev = st
                        .device
                        .as_ref()
                        .ok_or(CoreError::Invalid("no identity"))?;
                    let mut env = CarryEnv {
                        signer: &dev.signer,
                        device_id: dev.device_id,
                        account_id: dev.account_id,
                        rng: &mut *tx.rng,
                        offers: &mut *tx.room_offers,
                        now_s,
                    };
                    st.app.carry_lane_body(&mut env, &item.wire)
                };
                let body = match body {
                    Ok(b) => b,
                    Err(e) => {
                        refused = Some(e);
                        return Err(CoreError::Invalid("group_refused"));
                    }
                };
                match tx.group_lane_queue(&item.relationship, &body)? {
                    Ok(()) => {
                        tx.st
                            .app
                            .group_carriage
                            .lane_out
                            .retain(|l| l.id != item.id);
                        Ok(())
                    }
                    Err(e) => {
                        refused = Some(e);
                        Err(CoreError::Invalid("group_refused"))
                    }
                }
            });
            match res {
                Ok(((), commit)) => {
                    return Ok((
                        Ok(true),
                        Step {
                            commit,
                            handle: None,
                        },
                    ));
                }
                Err(e) => {
                    let Some(why) = refused else { return Err(e) };
                    let now_s = self.clock.now_ms() / 1000;
                    let expired = now_s >= item.since_s.saturating_add(cg::LANE_OUT_TTL_S);
                    let transient = matches!(
                        why.0,
                        "no_route_offer" | "relationship_unavailable" | "no_controller_head"
                    );
                    if transient && !expired {
                        blocked.push(item.relationship.clone());
                        last = Some(why);
                        continue;
                    }
                    // Final: drop it, count it, and tell the host (the person hears of it through `lane_lost`).
                    let ((), commit) = self.txn(&ctx.profile_id, |tx| {
                        let c = &mut tx.st.app.group_carriage;
                        c.lane_out.retain(|l| l.id != item.id);
                        c.lane_out_lost = c.lane_out_lost.saturating_add(1);
                        Ok(())
                    })?;
                    return Ok((
                        Ok(true),
                        Step {
                            commit,
                            handle: None,
                        },
                    ));
                }
            }
        }
        match last {
            Some(e) => Ok((Err(e), Step::default())),
            None => Ok((Ok(false), Step::default())),
        }
    }

    /// Hand every verified relationship-lane event that arrived to the group flow, oldest first, as the host would
    /// call `group_receive(Sender::Relationship(..), wire)`. Returns one result per event.
    pub fn group_lane_pump(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(Vec<LanePumped>, Step), CoreError> {
        self.slot(ctx)?;
        if self.slot(ctx)?.state.app.group_carriage.lane_in.is_empty() {
            return Ok((vec![], Step::default()));
        }
        let (out, step) = self.group_step(ctx, |app, gctx| {
            let mut all = vec![];
            for item in app.carry_lane_take() {
                let r = app
                    .group_receive(
                        gctx,
                        Sender::Relationship(item.relationship_id.clone()),
                        item.wire,
                    )
                    .and_then(|r| app.carry_outbox_add(&r, gctx.now_ms / 1000).map(|()| r));
                all.push((item.relationship_id, r));
            }
            Ok(all)
        })?;
        Ok((out.unwrap_or_default(), step))
    }

    /// Seal a flow `Outbound` that travels on the device lane (Welcome, App) to its destination devices' per-Room
    /// routes. The host deposits each `Sealed.wrapper` at `Sealed.entry`.
    pub fn group_frame_send(
        &mut self,
        ctx: &ProfileExecutionContext,
        out: &crate::groups::flow::Outbound,
    ) -> Result<(Result<Vec<Sealed>, GroupError>, Step), CoreError> {
        self.slot(ctx)?;
        let mut refused: Option<GroupError> = None;
        let res = self.txn(&ctx.profile_id, |tx| {
            let now_s = tx.now_ms / 1000;
            let st = &mut *tx.st;
            let dev = st
                .device
                .as_ref()
                .ok_or(CoreError::Invalid("no identity"))?;
            let mut env = CarryEnv {
                signer: &dev.signer,
                device_id: dev.device_id,
                account_id: dev.account_id,
                rng: &mut *tx.rng,
                offers: &mut *tx.room_offers,
                now_s,
            };
            match st.app.carry_frame_out(&mut env, out) {
                Ok(v) => Ok(v),
                Err(e) => {
                    refused = Some(e);
                    Err(CoreError::Invalid("group_refused"))
                }
            }
        });
        match res {
            Ok((v, commit)) => Ok((
                Ok(v),
                Step {
                    commit,
                    handle: None,
                },
            )),
            Err(e) => match refused {
                Some(r) => Ok((Err(r), Step::default())),
                None => Err(e),
            },
        }
    }

    /// Open a sealed wrapper that arrived at one of this device's per-Room routes and apply it (Welcome, App, Commit
    /// are wired in 2a). A drop leaves the profile exactly as it was.
    pub fn group_frame_receive(
        &mut self,
        ctx: &ProfileExecutionContext,
        wrapper: &[u8],
    ) -> Result<(Result<GroupReply, GroupError>, Step), CoreError> {
        self.group_step(ctx, |app, gctx| {
            let now_s = gctx.now_ms / 1000;
            let r = app.carry_frame_in(gctx, now_s, wrapper)?;
            note_removals(app, &r, gctx.now_ms / 1000);
            app.carry_outbox_add(&r, gctx.now_ms / 1000)?;
            Ok(r)
        })
    }

    /// Replace this device's route for the Room after a removal (ADR-0189 1.3): returns the fresh signed
    /// `GroupRouteRecord`, to send as a `route_update` to the new roster.
    pub fn group_route_replace(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &[u8],
    ) -> Result<(Result<Vec<u8>, GroupError>, Step), CoreError> {
        self.slot(ctx)?;
        let mut refused: Option<GroupError> = None;
        let room = crate::crypto::hex(conversation_id);
        let res = self.txn(&ctx.profile_id, |tx| {
            let now_s = tx.now_ms / 1000;
            let st = &mut *tx.st;
            let dev = st
                .device
                .as_ref()
                .ok_or(CoreError::Invalid("no identity"))?;
            let mut env = CarryEnv {
                signer: &dev.signer,
                device_id: dev.device_id,
                account_id: dev.account_id,
                rng: &mut *tx.rng,
                offers: &mut *tx.room_offers,
                now_s,
            };
            match st.app.carry_route_replace(&mut env, &room) {
                Ok(v) => Ok(v),
                Err(e) => {
                    refused = Some(e);
                    Err(CoreError::Invalid("group_refused"))
                }
            }
        });
        match res {
            Ok((v, commit)) => Ok((
                Ok(v),
                Step {
                    commit,
                    handle: None,
                },
            )),
            Err(e) => match refused {
                Some(r) => Ok((Err(r), Step::default())),
                None => Err(e),
            },
        }
    }

    /// Refresh queued advertisements from independent distribution debt, then drop retired per-Room routes whose
    /// overlap has ended. The returned entries are the deposit capabilities the host revokes (`DELETE /v1/relay/deposit-capabilities/{id}`).
    pub fn group_routes_expire(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(Vec<DepositRevocation>, Step), CoreError> {
        self.slot(ctx)?;
        let (revoke, commit) = self.txn(&ctx.profile_id, |tx| {
            let now_s = tx.now_ms / 1000;
            tx.st.app.maintain_rejoin_invites(tx.now_ms);
            tx.st.app.dangling_mutations.expire(tx.now_ms);
            tx.st.app.carry_outbox_expire(now_s);
            Ok(tx.st.app.carry_routes_expire(now_s))
        })?;
        Ok((
            revoke,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// Dedicated queues whose retire is due at their Home (ADR-0229 section 3): the overlap and every grant revocation on
    /// them are over. Each is returned with its durable request id; the host sends it only to `relay_uri` and reports the
    /// outcome with [`Core::group_queue_retired`]. An unacknowledged one comes back after a backoff, with the same id.
    pub fn group_queue_retires(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(Vec<QueueRetirement>, Step), CoreError> {
        self.slot(ctx)?;
        let (due, commit) = self.txn(&ctx.profile_id, |tx| {
            let now_s = tx.now_ms / 1000;
            Ok(tx.st.app.carry_queue_retires(now_s, &mut *tx.rng))
        })?;
        Ok((
            due,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// The Home acknowledged this retire (or answered not found or revoked): the debt is complete.
    pub fn group_queue_retired(
        &mut self,
        ctx: &ProfileExecutionContext,
        done: &QueueRetirement,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.st.app.group_carriage.queue_retired(done);
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// What the host must watch for the device lane (Package 3): the per-Room mailboxes this device polls (every route
    /// it minted that has not been revoked, including a retired one while it drains) and the Rooms that owe a route
    /// update after a removal (ADR-0189 section 1.3). Read-only; the cursor of a mailbox is the host's transport progress.
    pub fn group_carriage_status(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<GroupCarriageStatus, CoreError> {
        let slot = self.slot(ctx)?;
        let app = &slot.state.app;
        let rec = &app.group_carriage;
        let now = self.clock.now_ms() / 1000;
        let mut mailboxes: Vec<GroupMailbox> = vec![];
        for routes in rec.mine.values() {
            for r in routes {
                if !r.receiving(now) {
                    continue;
                }
                let Some(e) = r.entries.first() else { continue };
                let Ok(mailbox) = crate::crypto::unhex(&r.mailbox) else {
                    continue;
                };
                if !mailboxes.iter().any(|m| m.mailbox == mailbox) {
                    mailboxes.push(GroupMailbox {
                        mailbox,
                        relay_uri: e.relay_uri.clone(),
                    });
                }
            }
        }
        let mut route_owed = rec.replace_owed.clone();
        if let Some(me) = slot.state.device.as_ref() {
            let device = crate::crypto::hex(&me.device_id);
            for (room, roster) in &app.group_devices {
                if roster.contains_key(&device)
                    && !route_owed.contains(room)
                    && !rec.mine.get(room).is_some_and(|routes| {
                        routes.iter().any(|r| {
                            r.retired_at.is_none() && r.grant_id.is_some() && r.expires_at > now
                        })
                    })
                {
                    route_owed.push(room.clone());
                }
            }
        }
        Ok(GroupCarriageStatus {
            mailboxes,
            route_owed,
            outbox: rec.outbox.len(),
            pending: rec.pending.len(),
            lane_out: rec.lane_out.len(),
            lane_lost: rec.lane_out_lost,
        })
    }

    /// Drop revocation debt after the owning Home confirms the idempotent DELETE.
    pub fn group_route_revoked(
        &mut self,
        ctx: &ProfileExecutionContext,
        revoke: &DepositRevocation,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.st.app.group_carriage.revoked(revoke, tx.now_ms / 1000);
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// The group Invitations this device holds that no group engine has seen yet, and any it could not keep (report D70, D71).
    /// Read-only and available on the base module, which has no group engine: the person can be told, and the invitation
    /// stays where it is until the groups module is loaded and the lane is pumped. Never accepts anything.
    pub fn group_held_invitations(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<HeldInvitations, CoreError> {
        let app = &self.slot(ctx)?.state.app;
        Ok(HeldInvitations {
            invitations: app.carry_held_invitations(),
            overflow: app.group_carriage.lane_overflow,
        })
    }

    /// The person has read the overflow notice: clear the counters. Held invitations are untouched.
    pub fn group_lane_overflow_ack(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let ((), commit) = self.txn(&ctx.profile_id, |tx| {
            tx.st.app.group_carriage.lane_overflow = Default::default();
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Answer a typed groups query from the profile's state.
    pub fn group_query(
        &self,
        ctx: &ProfileExecutionContext,
        q: GroupQuery,
    ) -> Result<Result<GroupView, GroupError>, CoreError> {
        let slot = self.slot(ctx)?;
        let me = slot
            .state
            .device
            .as_ref()
            .map(|d| d.account_id.to_vec())
            .unwrap_or_default();
        if matches!(q, GroupQuery::Rejoin) {
            let device = slot.state.device.as_ref().map_or(&[][..], |d| &d.device_id);
            return Ok(Ok(GroupView::Rejoin(slot.state.app.group_rejoin_view(
                &me,
                device,
                self.clock.now_ms(),
            ))));
        }
        Ok(slot.state.app.group_query_as(&me, q))
    }

    /// The BlobStore's advertised limits (from `ProviderCapabilities.limits` of `om.blob.spool.v1`); `None` when the store
    /// offers no blobs. Without them Core refuses attachments (typed `unsupported`) and never sends bytes inline.
    /// Where this Home keeps recovery records (its host, port and `recovery_scope_id`); `None` when it offers none, in
    /// which case an account made now shows the key alone and writes no record (ADR-0200).
    pub fn set_recovery_place(&mut self, place: Option<crate::recovery_record::RecoveryPlace>) {
        self.recovery_place = place;
    }

    pub fn set_blob_limits(&mut self, limits: Option<BlobLimits>) {
        self.blob_limits = limits;
    }

    /// Seal an attachment on this device and persist its identities (BlobID, owner capability, request id, content key)
    /// before any upload. The inner `Err` is the typed refusal ("too big to send right now", store full, no store): it is
    /// decided here, before anything is sent. The `Prepared` bytes may be used only after `Step.commit` is durable.
    pub fn prepare_attachment(
        &mut self,
        ctx: &ProfileExecutionContext,
        plaintext: &[u8],
        meta: crate::attach::Meta,
    ) -> Result<(Result<PreparedAttachment, crate::attach::AttachError>, Step), CoreError> {
        self.slot(ctx)?;
        let (out, commit) = self.txn(&ctx.profile_id, |tx| {
            Ok(tx.prepare_attachment(tx.blob_limits, plaintext, meta))
        })?;
        Ok((
            out,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// Prepare a send from a confirmed installed pack item. The pack bytes are authenticated
    /// before a fresh conversation blob is sealed. Selecting shared mode is explicit per asset.
    pub fn prepare_expressive(
        &mut self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
        item: &str,
        ciphertext: &[u8],
        policy: crate::expressive::BlobPolicy,
    ) -> Result<(Result<PreparedAttachment, crate::attach::AttachError>, Step), CoreError> {
        use crate::attach::{AttachError, Meta};
        use crate::expressive::{BlobPolicy, Opened};
        let slot = self.slot(ctx)?;
        if !self.expressive_enabled {
            return Ok((Err(AttachError::Unsupported), Step::default()));
        }
        let asset = slot
            .state
            .expressive_packs
            .release(pack)
            .and_then(|release| release.item(item).cloned());
        let Some(asset) = asset else {
            return Ok((Err(AttachError::NotFound), Step::default()));
        };
        let (plaintext, facts) = match asset.open(ciphertext) {
            Ok(Opened::Renderable { plaintext, facts }) => {
                (zeroize::Zeroizing::new(plaintext), facts)
            }
            Ok(Opened::Placeholder { .. }) => {
                return Ok((Err(AttachError::Invalid), Step::default()));
            }
            Err(err) => return Ok((Err(err), Step::default())),
        };
        let mut presentation = asset.presentation;
        // A fresh encryption changes the manifest's identity tuple (ADR-0252 rule 7).
        presentation.pack_ref = None;
        presentation.shared_blob = policy == BlobPolicy::SharedBlob;
        let meta = Meta {
            media_type: "image/webp".into(),
            width: Some(facts.width),
            height: Some(facts.height),
            duration_ms: facts.duration_ms.map(u64::from),
            filename: None,
        };
        let (result, commit) = self.txn(&ctx.profile_id, |tx| {
            let result = tx.prepare_attachment(tx.blob_limits, &plaintext, meta);
            if let Ok(prepared) = &result {
                let record = tx
                    .st
                    .app
                    .prepared
                    .get_mut(&prepared.blob_id)
                    .ok_or(CoreError::CorruptState("prepared expressive asset"))?;
                record.expressive = Some(presentation);
            }
            Ok(result)
        })?;
        Ok((
            result,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    fn expressive_asset(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        source: crate::expressive::Source<'_>,
    ) -> Option<crate::expressive::AssetRef> {
        use crate::expressive::{AssetRef, Source};
        let app = &self.slot(ctx).ok()?.state.app;
        let m = app
            .relationships
            .values()
            .flat_map(|r| r.messages.iter())
            .chain(app.relationship_history.values().flatten())
            .find(|m| m.id == message_id && !m.deleted && !m.lane)?;
        let stored = match source {
            Source::Attachment(index) => {
                let attachment = crate::flows::rec_ref(m.attachments.get(index)?)?;
                let ext =
                    crate::crypto::cbor::decode(&crate::crypto::unhex(m.ext.as_deref()?).ok()?)
                        .ok()?;
                return crate::expressive::from_extension(&ext, index, attachment);
            }
            Source::Inline(index) => m.social.inline_assets.get(index)?,
            Source::Reaction { key, mine } => m
                .social
                .reactions
                .iter()
                .find(|r| r.key == key && r.mine == mine && r.present)?
                .asset
                .as_ref()?,
        };
        let bytes = zeroize::Zeroizing::new(crate::crypto::unhex(stored).ok()?);
        AssetRef::parse(&bytes).ok()
    }

    pub fn expressive_presentation(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        source: crate::expressive::Source<'_>,
    ) -> Option<crate::expressive::Presentation> {
        if !self.expressive_enabled {
            return None;
        }
        self.expressive_asset(ctx, message_id, source)
            .map(|asset| asset.presentation)
    }

    /// Attribution is computed from an installed release with a matching immutable item.
    pub fn expressive_attribution(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        source: crate::expressive::Source<'_>,
    ) -> Option<crate::expressive::PackView> {
        if !self.expressive_enabled {
            return None;
        }
        let asset = self.expressive_asset(ctx, message_id, source)?;
        let pack = asset.presentation.pack_ref.as_ref()?.pack_id;
        let packs = &self.slot(ctx).ok()?.state.expressive_packs;
        packs
            .release(pack)?
            .attributes(&asset)
            .then(|| packs.view(pack))
            .flatten()
    }

    pub fn expressive_fetch(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        source: crate::expressive::Source<'_>,
    ) -> Result<NetCall, crate::attach::AttachError> {
        use crate::attach::AttachError;
        if !self.expressive_enabled {
            return Err(AttachError::Unsupported);
        }
        let asset = self
            .expressive_asset(ctx, message_id, source)
            .ok_or(AttachError::NotFound)?;
        if asset.presentation.media_profile_id != crate::expressive::WEBP {
            return Err(AttachError::Unsupported);
        }
        Self::fetch_attachment_ref(asset.attachment)
    }

    pub fn open_expressive(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        source: crate::expressive::Source<'_>,
        ciphertext: &[u8],
    ) -> Result<crate::expressive::Opened, crate::attach::AttachError> {
        use crate::attach::AttachError;
        if !self.expressive_enabled {
            return Err(AttachError::Unsupported);
        }
        self.expressive_asset(ctx, message_id, source)
            .ok_or(AttachError::NotFound)?
            .open(ciphertext)
    }

    /// The ciphertext of a prepared or outgoing attachment again, from its plaintext (sealing is deterministic under the
    /// persisted key), so a host that lost its staged bytes needs only the original file. Refuses plaintext that does not
    /// hash to the committed digest.
    pub fn restage_attachment(
        &self,
        ctx: &ProfileExecutionContext,
        blob_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, crate::attach::AttachError> {
        use crate::attach::AttachError;
        let st = &self.slot(ctx).map_err(|_| AttachError::NotFound)?.state.app;
        let rec = st
            .prepared
            .get(blob_id)
            .map(|p| &p.rec)
            .or_else(|| {
                st.relationships
                    .values()
                    .flat_map(|r| r.messages.iter())
                    .flat_map(|m| m.attachments.iter())
                    .find(|a| a.blob_id == blob_id && (a.owner_cap.is_some() || a.direct))
            })
            .ok_or(AttachError::NotFound)?;
        let r = crate::flows::rec_ref(rec).ok_or(AttachError::NotFound)?;
        let ct = crate::attach::seal(&r.key, &r.blob_id, plaintext);
        if plaintext.len() as u64 != r.plaintext_size
            || crate::attach::ciphertext_digest(&ct) != r.ciphertext_digest
        {
            return Err(AttachError::IntegrityFailure);
        }
        Ok(ct)
    }

    fn attachment_rec(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Result<crate::attach::AttachmentRef, crate::attach::AttachError> {
        use crate::attach::AttachError;
        let st = &self.slot(ctx).map_err(|_| AttachError::NotFound)?.state.app;
        st.relationships
            .values()
            .flat_map(|r| r.messages.iter())
            .chain(st.relationship_history.values().flatten())
            .find(|m| m.id == message_id && !m.deleted)
            .and_then(|m| m.attachments.get(index))
            .and_then(crate::flows::rec_ref)
            .ok_or(AttachError::NotFound)
    }

    /// The fetch for one attachment of a message: where to get the ciphertext and what it must hash to. Not an outbox
    /// item and never run by Core itself; the host decides when (a tap, or its own auto-download policy).
    pub fn attachment_fetch(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Result<NetCall, crate::attach::AttachError> {
        Self::fetch_attachment_ref(self.attachment_rec(ctx, message_id, index)?)
    }

    fn fetch_attachment_ref(
        r: crate::attach::AttachmentRef,
    ) -> Result<NetCall, crate::attach::AttachError> {
        use crate::attach::{AttachError, TAG_LEN};
        if r.plaintext_size > crate::attach::HARD_MAX_PLAINTEXT {
            return Err(AttachError::TooLarge {
                max: crate::attach::HARD_MAX_PLAINTEXT,
            });
        }
        let read_cap = r.read_cap.clone().ok_or(AttachError::NotFound)?;
        if r.locations.is_empty() {
            return Err(AttachError::NotFound);
        }
        Ok(NetCall::GetBlob {
            locations: r.locations,
            read_cap,
            digest: r.ciphertext_digest.to_vec(),
            length: r.plaintext_size + TAG_LEN,
        })
    }

    /// Verify the fetched bytes against the reference's digest and length, then decrypt. Plaintext exists only for bytes
    /// that hash to the digest the (end-to-end authenticated) message committed to.
    pub fn open_attachment(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
        ciphertext: &[u8],
    ) -> Result<OpenedAttachment, crate::attach::AttachError> {
        let r = self.attachment_rec(ctx, message_id, index)?;
        let plaintext = if self.expressive_enabled
            && let Some(asset) = self.expressive_asset(
                ctx,
                message_id,
                crate::expressive::Source::Attachment(index),
            ) {
            match asset.open(ciphertext)? {
                crate::expressive::Opened::Renderable { plaintext, .. } => plaintext,
                crate::expressive::Opened::Placeholder { .. } => {
                    return Err(crate::attach::AttachError::Unsupported);
                }
            }
        } else {
            r.open(ciphertext)?
        };
        Ok(OpenedAttachment {
            plaintext,
            media_type: r.media_type,
            filename: r.filename,
            width: r.width,
            height: r.height,
            duration_ms: r.duration_ms,
        })
    }

    // ------------------------------------------------------------------------------------------ direct transfer
    //
    // Sans-IO like the rest: the host owns the ordered byte channel (and decides when one exists), maps it to
    // `DirectFrame`, and feeds frames in. Core never opens a channel and never learns whether the other person is
    // online. The message is sent at once through the ordinary sealed path; only the file waits (decision G-O1).

    /// How a file of `size` bytes may travel. Pure; nothing is sealed or sent.
    pub fn plan_attachment(&self, size: u64) -> AttachmentPlanView {
        use crate::attach::AttachError;
        match crate::direct::plan_size(self.blob_limits, size) {
            crate::direct::SizePlan::Spool => AttachmentPlanView::Spool,
            crate::direct::SizePlan::OfferDirect { spool_max } => {
                AttachmentPlanView::OfferDirect { limit: spool_max }
            }
            crate::direct::SizePlan::Refused(AttachError::TooLarge { max }) => {
                AttachmentPlanView::TooLarge { limit: max }
            }
            crate::direct::SizePlan::Refused(_) => AttachmentPlanView::Empty,
        }
    }

    /// Seal a file the delivery service will not take and start serving it (only once a message offers it, see
    /// `IntentBody::SendAttachmentsDirect`). The returned `ciphertext` is empty: Core holds the sealed object.
    pub fn prepare_direct_attachment(
        &mut self,
        ctx: &ProfileExecutionContext,
        plaintext: &[u8],
        meta: crate::attach::Meta,
    ) -> Result<(Result<PreparedAttachment, crate::attach::AttachError>, Step), CoreError> {
        self.slot(ctx)?;
        let (out, commit) = self.txn(&ctx.profile_id, |tx| {
            Ok(tx.prepare_direct_attachment(tx.blob_limits, plaintext, meta))
        })?;
        let out = out.map(|(p, sender)| {
            self.direct_senders
                .insert((ctx.profile_id.clone(), p.blob_id.clone()), sender);
            p
        });
        Ok((
            out,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    fn build_direct_sender(
        &mut self,
        ctx: &ProfileExecutionContext,
        blob_id: &str,
        ciphertext: Vec<u8>,
    ) -> Result<(), crate::attach::AttachError> {
        use crate::attach::AttachError;
        let st = &self.slot(ctx).map_err(|_| AttachError::NotFound)?.state.app;
        let rec = st
            .prepared
            .get(blob_id)
            .map(|p| &p.rec)
            .or_else(|| {
                st.relationships
                    .values()
                    .flat_map(|r| r.messages.iter())
                    .flat_map(|m| m.attachments.iter())
                    .find(|a| a.blob_id == blob_id && a.direct)
            })
            .ok_or(AttachError::NotFound)?;
        let r = crate::flows::rec_ref(rec).ok_or(AttachError::NotFound)?;
        let cap = r.read_cap.clone().ok_or(AttachError::NotFound)?;
        let sender = crate::direct::DirectSender::new(
            r.blob_id,
            r.ciphertext_digest,
            cap,
            ciphertext,
            r.key,
        )?;
        self.direct_senders
            .insert((ctx.profile_id.clone(), blob_id.to_string()), sender);
        Ok(())
    }

    /// After a restart: hand the original file back so a still-live offer is served again. Refuses a file that is not
    /// the one that was offered (digest and size), and an offer that was withdrawn, delivered or lapsed.
    pub fn arm_direct_offer(
        &mut self,
        ctx: &ProfileExecutionContext,
        blob_id: &str,
        plaintext: &[u8],
    ) -> Result<(), crate::attach::AttachError> {
        use crate::attach::AttachError;
        let now = self.clock.now_ms();
        let live = self
            .slot(ctx)
            .map_err(|_| AttachError::NotFound)?
            .state
            .app
            .direct_offers
            .get(blob_id)
            .is_some_and(|o| o.status.is_live() && o.expires_ms > now);
        if !live {
            return Err(AttachError::NotFound);
        }
        let ct = self.restage_attachment(ctx, blob_id, plaintext)?;
        self.build_direct_sender(ctx, blob_id, ct)
    }

    /// Drop senders whose offer is no longer live and record the ones that finished. The only durable effect of serving.
    fn direct_reconcile(&mut self, profile: &str) -> Result<Option<Commit>, CoreError> {
        use crate::direct::OfferState;
        let now = self.clock.now_ms();
        let Some(slot) = self.profiles.get(profile) else {
            return Ok(None);
        };
        let offers = &slot.state.app.direct_offers;
        let mut claimed = vec![];
        let mut confirmed = vec![];
        let mut drop_keys = vec![];
        for (k, s) in self.direct_senders.iter().filter(|(k, _)| k.0 == profile) {
            let live = offers
                .get(&k.1)
                .is_some_and(|o| o.status.is_live() && o.expires_ms > now);
            // Only the transitions that change what is stored are written: a claim that is already stored is not
            // rewritten on every frame, and a receipt is the only way to `Confirmed`.
            match (s.state(), offers.get(&k.1).map(|o| o.status)) {
                (OfferState::PeerClaimsReceived, Some(DirectStatus::Waiting)) => {
                    claimed.push(k.1.clone());
                }
                (OfferState::Confirmed, Some(st)) if st.is_live() => confirmed.push(k.1.clone()),
                _ => {}
            }
            if !live || matches!(s.state(), OfferState::Confirmed | OfferState::Withdrawn) {
                drop_keys.push(k.clone());
            }
        }
        for k in drop_keys {
            if let Some(mut s) = self.direct_senders.remove(&k) {
                s.withdraw();
            }
        }
        if claimed.is_empty() && confirmed.is_empty() {
            return Ok(None);
        }
        let ((), commit) = self.txn(profile, |tx| {
            for id in &claimed {
                tx.direct_claimed(id);
            }
            for id in &confirmed {
                tx.direct_confirmed(id);
            }
            Ok(())
        })?;
        Ok(commit)
    }

    /// One frame from the other person's channel. Answers with the frames to send back; a refused request gets the one
    /// uniform `unavailable` (no oracle: unknown, withdrawn, lapsed, not armed and a wrong capability look the same).
    pub fn direct_serve(
        &mut self,
        ctx: &ProfileExecutionContext,
        frame: &crate::direct::DirectFrame,
    ) -> Result<(Vec<crate::direct::DirectFrame>, Step), CoreError> {
        self.direct_serve_on(ctx, 0, frame)
    }

    pub fn direct_serve_on(
        &mut self,
        ctx: &ProfileExecutionContext,
        channel: u64,
        frame: &crate::direct::DirectFrame,
    ) -> Result<(Vec<crate::direct::DirectFrame>, Step), CoreError> {
        use crate::direct::{DirectFrame, ErrorCode};
        self.slot(ctx)?;
        let mut stale = self.direct_reconcile(&ctx.profile_id)?;
        let mut fresh = [0u8; crate::direct::TRANSFER_ID_LEN];
        self.rng.fill_bytes(&mut fresh);
        let profile = ctx.profile_id.clone();
        let out = match frame {
            DirectFrame::FetchBlob {
                request_id,
                blob_id,
                ..
            } => match self
                .direct_senders
                .get_mut(&(profile.clone(), crate::crypto::hex(blob_id)))
            {
                Some(s) => s.handle_on(channel, frame, fresh),
                None => vec![DirectFrame::Error {
                    id: *request_id,
                    code: ErrorCode::Unavailable,
                }],
            },
            DirectFrame::Ack { .. } | DirectFrame::Error { .. } | DirectFrame::Receipt { .. } => {
                self.direct_senders
                    .iter_mut()
                    .filter(|(k, _)| k.0 == profile)
                    .flat_map(|(_, s)| s.handle_on(channel, frame, fresh))
                    .collect()
            }
            _ => vec![DirectFrame::Error {
                id: fresh,
                code: ErrorCode::Protocol,
            }],
        };
        let commit = match self.direct_reconcile(&profile)? {
            Some(c) => Some(c),
            None => stale.take(),
        };
        Ok((
            out,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// The next chunks to send, given how many bytes the channel still buffers (backpressure is the sender's:
    /// 262,144 high, 65,536 low).
    pub fn direct_pump(
        &mut self,
        ctx: &ProfileExecutionContext,
        buffered: usize,
    ) -> Result<Vec<crate::direct::DirectFrame>, CoreError> {
        self.direct_pump_on(ctx, 0, buffered)
    }

    pub fn direct_pump_on(
        &mut self,
        ctx: &ProfileExecutionContext,
        channel: u64,
        buffered: usize,
    ) -> Result<Vec<crate::direct::DirectFrame>, CoreError> {
        self.slot(ctx)?;
        Ok(self
            .direct_senders
            .iter_mut()
            .filter(|(k, sender)| k.0 == ctx.profile_id && sender.owns_channel(channel))
            .flat_map(|(_, s)| s.pump(buffered))
            .collect())
    }

    pub fn direct_disconnected(
        &mut self,
        ctx: &ProfileExecutionContext,
        channel: u64,
    ) -> Result<(), CoreError> {
        self.slot(ctx)?;
        for (_, sender) in self
            .direct_senders
            .iter_mut()
            .filter(|(k, _)| k.0 == ctx.profile_id)
        {
            sender.detach_channel(channel);
        }
        Ok(())
    }

    /// Start (or, after a dropped channel, resume) the direct fetch of one attachment: the request to send. The held
    /// bytes of an earlier attempt are kept in memory and the sender continues from them.
    pub fn direct_fetch_start(
        &mut self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Result<crate::direct::DirectFrame, crate::attach::AttachError> {
        use crate::attach::AttachError;
        let r = self.attachment_rec(ctx, message_id, index)?;
        if !r.locations.is_empty() {
            return Err(AttachError::NotFound); // a stored object: the ordinary fetch
        }
        let mut rid = [0u8; crate::direct::TRANSFER_ID_LEN];
        self.rng.fill_bytes(&mut rid);
        let key = (ctx.profile_id.clone(), message_id.to_string(), index);
        if !self.direct_fetches.contains_key(&key) {
            self.direct_fetches
                .insert(key.clone(), crate::direct::DirectFetch::new(&r, rid)?);
        }
        self.direct_fetches
            .get_mut(&key)
            .and_then(|f| f.request(rid))
            .ok_or(AttachError::NotFound)
    }

    /// One frame from the sender's channel for an attachment; the frames to send back (acks, errors).
    pub fn direct_fetch_handle(
        &mut self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
        frame: &crate::direct::DirectFrame,
    ) -> Result<Vec<crate::direct::DirectFrame>, crate::attach::AttachError> {
        self.direct_fetches
            .get_mut(&(ctx.profile_id.clone(), message_id.to_string(), index))
            .map(|f| f.handle(frame))
            .ok_or(crate::attach::AttachError::NotFound)
    }

    /// The verified ciphertext of a completed direct fetch, for the host to write to its local encrypted blob cache.
    /// `None` until every byte arrived and both digests held. Nothing is confirmed to the sender by reading it.
    pub fn direct_fetch_ciphertext(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Option<Vec<u8>> {
        self.direct_fetches
            .get(&(ctx.profile_id.clone(), message_id.to_string(), index))
            .and_then(|f| f.ciphertext().map(<[u8]>::to_vec))
    }

    /// The host reports the ciphertext is durably in its local encrypted blob cache (ADR-0194 section 2). Answers the
    /// receipt frame to send, if the sender's transfer already ended; a failed or unreported commit sends none, and a
    /// fetch that was opened and forgotten first sends none either (commit before `direct_fetch_open`).
    pub fn direct_fetch_committed(
        &mut self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Result<Vec<crate::direct::DirectFrame>, crate::attach::AttachError> {
        self.direct_fetches
            .get_mut(&(ctx.profile_id.clone(), message_id.to_string(), index))
            .map(|f| f.committed())
            .ok_or(crate::attach::AttachError::NotFound)
    }

    /// Where a direct fetch stands and how many contiguous bytes are held.
    pub fn direct_fetch_progress(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Option<(crate::direct::FetchState, u64)> {
        self.direct_fetches
            .get(&(ctx.profile_id.clone(), message_id.to_string(), index))
            .map(|f| (f.state(), f.held()))
    }

    /// Decrypt a completed direct fetch (digest checked before decryption) and forget it.
    pub fn direct_fetch_open(
        &mut self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        index: usize,
    ) -> Result<OpenedAttachment, crate::attach::AttachError> {
        let r = self.attachment_rec(ctx, message_id, index)?;
        let key = (ctx.profile_id.clone(), message_id.to_string(), index);
        let plaintext = self
            .direct_fetches
            .get(&key)
            .ok_or(crate::attach::AttachError::NotFound)?
            .open()?;
        self.direct_fetches.remove(&key);
        Ok(OpenedAttachment {
            plaintext,
            media_type: r.media_type,
            filename: r.filename,
            width: r.width,
            height: r.height,
            duration_ms: r.duration_ms,
        })
    }

    /// A transfer open right now shows as `Transferring` (memory only; the stored state is `Waiting`).
    fn overlay_transfers(
        &self,
        ctx: &ProfileExecutionContext,
        mut item: TimelineItem,
    ) -> TimelineItem {
        if let TimelineItem::Outgoing { attachments, .. } = &mut item {
            for a in attachments
                .iter_mut()
                .filter(|a| a.state == AttachmentState::WaitingForRecipient)
            {
                if self
                    .direct_senders
                    .get(&(ctx.profile_id.clone(), a.blob_id.clone()))
                    .is_some_and(|s| s.state() == crate::direct::OfferState::Transferring)
                {
                    a.state = AttachmentState::Transferring;
                }
            }
        }
        item
    }

    fn direct_offer_views(
        &self,
        ctx: &ProfileExecutionContext,
        st: &ProfileState,
    ) -> Vec<DirectOfferView> {
        let mut out: Vec<DirectOfferView> = st
            .app
            .direct_offers
            .iter()
            .map(|(id, o)| {
                let armed = self
                    .direct_senders
                    .get(&(ctx.profile_id.clone(), id.clone()));
                let size = st
                    .app
                    .relationships
                    .get(&o.conversation_id)
                    .and_then(|r| r.messages.iter().find(|m| m.id == o.message_id))
                    .and_then(|m| m.attachments.iter().find(|a| &a.blob_id == id))
                    .and_then(crate::flows::rec_ref)
                    .map_or(0, |r| r.plaintext_size);
                let live = o.status.is_live();
                DirectOfferView {
                    conversation_id: o.conversation_id.clone(),
                    message_id: o.message_id.clone(),
                    blob_id: id.clone(),
                    size,
                    state: match (o.status, armed.map(|s| s.state())) {
                        (DirectStatus::Waiting, Some(crate::direct::OfferState::Transferring)) => {
                            AttachmentState::Transferring
                        }
                        (DirectStatus::Waiting, _) => AttachmentState::WaitingForRecipient,
                        (DirectStatus::PeerClaim, _) => AttachmentState::PeerSaysReceived,
                        (DirectStatus::Confirmed, _) => AttachmentState::DeliveredLocally,
                        (DirectStatus::Unconfirmed, _) => AttachmentState::NotConfirmed,
                        _ => AttachmentState::NotOffered,
                    },
                    expires_at: o.expires_ms,
                    needs_file: live && armed.is_none(),
                }
            })
            .collect();
        out.sort_by_key(|v| (v.expires_at, v.blob_id.clone()));
        out
    }

    /// How many unused route offers Core holds.
    pub fn route_offers(&self) -> usize {
        self.offers.len()
    }

    /// The Home each unused route offer is at, so the adapter can top the pool up where it is thinnest.
    pub fn offer_homes(&self) -> Vec<String> {
        self.offers.iter().map(|o| o.relay_uri.clone()).collect()
    }

    // ------------------------------------------------------------------------------------------ profiles

    /// Open (or create, with no records) a profile from the records its encrypted store returned. Each open gets a
    /// fresh generation, so a context captured before a close/reopen is rejected as stale.
    pub fn open_profile(
        &mut self,
        profile_id: &str,
        records: &[Record],
    ) -> Result<ProfileExecutionContext, CoreError> {
        if profile_id.is_empty()
            || profile_id.len() > 128
            || profile_id.chars().any(char::is_control)
        {
            return Err(CoreError::Invalid("profile id"));
        }
        if self.staged.is_some() {
            return Err(CoreError::Busy);
        }
        let generation = format!("gen-{}-{}", self.boot, self.next_generation);
        let mut state = ProfileState::decode(generation.clone(), records)?;
        crate::state::prune_link_offers(&mut state.app, self.clock.now_ms());
        self.next_generation += 1;
        let committed = records
            .iter()
            .map(|r| (r.key.clone(), r.value.clone()))
            .collect();
        self.profiles.insert(
            profile_id.to_string(),
            Slot {
                state,
                committed,
                room_offers: vec![],
                pack_upload: Default::default(),
                pack_review: None,
            },
        );
        Ok(ProfileExecutionContext {
            profile_id: profile_id.to_string(),
            generation,
        })
    }

    /// Drop a profile's in-memory state (the host locks its vault). Pending outbox entries survive in the store.
    pub fn close_profile(&mut self, profile_id: &str) {
        if self
            .staged
            .as_ref()
            .is_some_and(|s| s.profile == profile_id)
        {
            self.staged = None;
        }
        self.profiles.remove(profile_id);
        self.group_engines.remove(profile_id);
    }

    pub fn context_for(&self, profile_id: &str) -> Result<ProfileExecutionContext, CoreError> {
        let slot = self
            .profiles
            .get(profile_id)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        Ok(ProfileExecutionContext {
            profile_id: profile_id.to_string(),
            generation: slot.state.generation.clone(),
        })
    }

    fn slot(&self, ctx: &ProfileExecutionContext) -> Result<&Slot, CoreError> {
        let slot = self
            .profiles
            .get(&ctx.profile_id)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        if slot.state.generation != ctx.generation {
            return Err(CoreError::ProfileAccess("stale profile context"));
        }
        Ok(slot)
    }

    // ------------------------------------------------------------------------------------------ transactions

    fn txn<T>(
        &mut self,
        profile: &str,
        f: impl FnOnce(&mut Tx<'_, R>) -> Result<T, CoreError>,
    ) -> Result<(T, Option<Commit>), CoreError> {
        self.txn_inner(profile, f, false)
    }

    fn txn_inner<T>(
        &mut self,
        profile: &str,
        f: impl FnOnce(&mut Tx<'_, R>) -> Result<T, CoreError>,
        packs_only: bool,
    ) -> Result<(T, Option<Commit>), CoreError> {
        if self.staged.is_some() {
            return Err(CoreError::Busy);
        }
        let open: Vec<String> = self.profiles.keys().cloned().collect();
        let slot = self
            .profiles
            .get_mut(profile)
            .ok_or(CoreError::ProfileAccess("unknown profile"))?;
        let mut next = slot.state.clone();
        let (value, changed) = {
            let mut tx = Tx {
                st: &mut next,
                rng: &mut self.rng,
                now_ms: self.clock.now_ms(),
                calls_enabled: self.calls_enabled,
                expressive_enabled: self.expressive_enabled,
                changed: vec![],
                open_profiles: &open,
                offers: &mut self.offers,
                room_offers: &mut slot.room_offers,
                blob_limits: self.blob_limits,
                recovery_place: self.recovery_place.clone(),
            };
            let v = f(&mut tx)?;
            if !packs_only {
                tx.expire_controller_change();
                tx.maintain_recovery_record();
                tx.maintain_relays();
                tx.maintain_calls();
            }
            (v, tx.changed)
        };
        let records = if packs_only {
            // The reference host applies writes individually. A pack-only transaction
            // changes exactly one record and cannot publish unrelated maintenance.
            let mut records = slot.committed.clone();
            next.encode_pack_record(&mut records)?;
            records
        } else {
            next.encode()?
        };
        let writes = diff(&slot.committed, &records);
        if writes.is_empty() {
            // Nothing durable changed (for example an idempotent retry): visible immediately, nothing to release.
            let slot = self
                .profiles
                .get_mut(profile)
                .ok_or(CoreError::ProfileAccess("unknown profile"))?;
            slot.state = next;
            return Ok((value, None));
        }
        let id = self.next_commit;
        self.next_commit += 1;
        self.staged = Some(Staged {
            id,
            profile: profile.to_string(),
            next,
            records,
            changed,
        });
        Ok((
            value,
            Some(Commit {
                id,
                profile_id: profile.to_string(),
                writes,
            }),
        ))
    }

    /// The host reports whether it made `commit_id` durable. On failure the staged change is discarded, so nothing
    /// derived from it (plaintext, outcomes, network effects) was ever released.
    pub fn finish_commit(&mut self, commit_id: u64, ok: bool) -> Result<Released, CoreError> {
        match self.staged.take() {
            Some(s) if s.id == commit_id => {
                if !ok {
                    // the engine ran ahead of what the host kept: reload it from the committed snapshot
                    self.group_engines.remove(&s.profile);
                    return Ok(Released {
                        profile_id: s.profile,
                        changed_operations: vec![],
                    });
                }
                let slot = self
                    .profiles
                    .get_mut(&s.profile)
                    .ok_or(CoreError::ProfileAccess("profile closed during commit"))?;
                slot.state = s.next;
                slot.committed = s.records;
                Ok(Released {
                    profile_id: s.profile,
                    changed_operations: s.changed,
                })
            }
            other => {
                self.staged = other;
                Err(CoreError::Invalid("unknown commit id"))
            }
        }
    }

    // ------------------------------------------------------------------------------------------ intents

    /// Dispatch resolves to a handle, never to success: success is whatever outcome Core later reports.
    pub fn dispatch(&mut self, intent: &Intent) -> Result<Step, CoreError> {
        let slot = self.slot(&intent.ctx)?;
        if intent.operation_id.is_empty() || intent.operation_id.len() > 128 {
            return Err(CoreError::Invalid("operation id"));
        }
        let handle = OperationHandle {
            operation_id: intent.operation_id.clone(),
            profile_id: intent.ctx.profile_id.clone(),
        };
        if let Some(existing) = slot.state.app.ops.get(&intent.operation_id) {
            if existing.kind != intent.body.kind() {
                return Err(CoreError::OperationIdConflict);
            }
            return Ok(Step {
                commit: None,
                handle: Some(handle),
            }); // exact retry: same logical operation
        }
        let (_, commit) = self.txn(&intent.ctx.profile_id, |tx| tx.apply_intent(intent))?;
        Ok(Step {
            commit,
            handle: Some(handle),
        })
    }

    /// The expressive-media driver opts in per Core instance. Existing credentials keep their
    /// advertised capabilities until the normal account-control path reissues them.
    pub fn enable_expressive_media(&mut self) {
        self.expressive_enabled = true;
    }

    pub fn install_content_pack(
        &mut self,
        ctx: &ProfileExecutionContext,
        input: crate::expressive::Install<'_>,
    ) -> Result<(Result<(), crate::expressive::Error>, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        self.txn_inner(
            &ctx.profile_id,
            |tx| {
                Ok(if tx.expressive_enabled {
                    tx.st.expressive_packs.install(input)
                } else {
                    Err("expressive_disabled")
                })
            },
            true,
        )
    }

    /// Transport chunks only. Completing the upload validates the entire file without a store write.
    pub fn append_pack_file(
        &mut self,
        ctx: &ProfileExecutionContext,
        offset: usize,
        bytes: &[u8],
    ) -> Result<Result<(), crate::expressive::Error>, CoreError> {
        self.slot(ctx)?;
        if !self.expressive_enabled {
            return Ok(Err("expressive_disabled"));
        }
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context");
        if offset == 0 {
            slot.pack_upload = Default::default();
        }
        if slot.pack_upload.len() != offset {
            return Ok(Err("pack_file_shape"));
        }
        if bytes.len() > crate::expressive::PACK_FILE_MAX.saturating_sub(offset) {
            slot.pack_upload = Default::default();
            return Ok(Err("size_limit"));
        }
        slot.pack_upload.extend_from_slice(bytes);
        Ok(Ok(()))
    }

    pub fn review_uploaded_pack_file(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Result<(u64, crate::expressive::PackPreview), crate::expressive::Error>, CoreError>
    {
        self.slot(ctx)?;
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context");
        let bytes = std::mem::take(&mut slot.pack_upload);
        self.review_pack_file(ctx, &bytes)
    }

    pub fn review_pack_file(
        &mut self,
        ctx: &ProfileExecutionContext,
        bytes: &[u8],
    ) -> Result<Result<(u64, crate::expressive::PackPreview), crate::expressive::Error>, CoreError>
    {
        let slot = self.slot(ctx)?;
        if !self.expressive_enabled {
            return Ok(Err("expressive_disabled"));
        }
        let review = match slot.state.expressive_packs.review_file(bytes) {
            Ok(review) => review,
            Err(reason) => return Ok(Err(reason)),
        };
        let preview = review.preview.clone();
        let ticket = self.next_pack_review;
        self.next_pack_review += 1;
        self.profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context")
            .pack_review = Some((ticket, review));
        Ok(Ok((ticket, preview)))
    }

    pub fn begin_pack_install(
        &mut self,
        ctx: &ProfileExecutionContext,
        ticket: u64,
        release_id: &str,
        control_digest: Option<&str>,
    ) -> Result<Result<usize, crate::expressive::Error>, CoreError> {
        self.slot(ctx)?;
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context");
        let Some((id, review)) = &mut slot.pack_review else {
            return Ok(Err("confirmation_required"));
        };
        if *id != ticket {
            return Ok(Err("confirmation_required"));
        }
        Ok(review.begin(&slot.state.expressive_packs, release_id, control_digest))
    }

    /// Issue one validated digest and ciphertext for the host's profile-local cache write.
    pub fn stage_pack_blob(
        &mut self,
        ctx: &ProfileExecutionContext,
        ticket: u64,
    ) -> Result<Result<([u8; 32], Vec<u8>), crate::expressive::Error>, CoreError> {
        self.slot(ctx)?;
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context");
        Ok(match &mut slot.pack_review {
            Some((id, review)) if *id == ticket => review.stage(),
            _ => Err("confirmation_required"),
        })
    }

    /// Report success only after every issued cache write has succeeded. The active
    /// pack inventory changes at the existing one-record durability boundary.
    pub fn commit_pack_install(
        &mut self,
        ctx: &ProfileExecutionContext,
        ticket: u64,
    ) -> Result<(Result<(), crate::expressive::Error>, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context");
        if !slot
            .pack_review
            .as_ref()
            .is_some_and(|(id, _)| *id == ticket)
        {
            return Ok((Err("confirmation_required"), None));
        }
        let (_, review) = slot.pack_review.take().expect("checked ticket");
        self.txn_inner(
            &ctx.profile_id,
            |tx| Ok(review.activate(&mut tx.st.expressive_packs)),
            true,
        )
    }

    pub fn abort_pack_install(
        &mut self,
        ctx: &ProfileExecutionContext,
        ticket: Option<u64>,
    ) -> Result<(), CoreError> {
        self.slot(ctx)?;
        let slot = self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("checked context");
        if ticket.is_none() {
            slot.pack_upload = Default::default();
        }
        if slot
            .pack_review
            .as_ref()
            .is_some_and(|(id, _)| Some(*id) == ticket)
        {
            slot.pack_review = None;
        }
        Ok(())
    }

    pub fn pack_cache_digests(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<std::collections::BTreeSet<String>, CoreError> {
        let slot = self.slot(ctx)?;
        Ok(if self.expressive_enabled {
            slot.state.expressive_packs.cache_digests()
        } else {
            Default::default()
        })
    }

    pub fn pack_inventory_digest(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Option<String>, CoreError> {
        let slot = self.slot(ctx)?;
        Ok(slot.committed.get(crate::state::REC_EXPRESSIVE_PACKS)
            .map(|record| crate::crypto::hex(&crate::crypto::sha256(&[record]))))
    }

    pub fn open_cached_pack_asset(
        &self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
        release: [u8; 32],
        item: &str,
        ciphertext: &[u8],
    ) -> Option<(crate::expressive::Presentation, crate::expressive::Opened)> {
        if !self.expressive_enabled {
            return None;
        }
        let asset = self
            .slot(ctx)
            .ok()?
            .state
            .expressive_packs
            .cached_asset(pack, release, item)?;
        let opened =
            asset
                .open(ciphertext)
                .unwrap_or_else(|_| crate::expressive::Opened::Placeholder {
                    alt_text: asset.presentation.alt_text.clone(),
                });
        Some((asset.presentation, opened))
    }

    pub fn content_pack(
        &self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
    ) -> Result<Option<crate::expressive::PackView>, CoreError> {
        let slot = self.slot(ctx)?;
        Ok(slot.state.expressive_packs.view(pack))
    }

    pub fn expressive_media_available(&self) -> bool {
        self.expressive_enabled
    }

    pub fn preview_content_pack(
        &self,
        ctx: &ProfileExecutionContext,
        release: &[u8],
        control: Option<&[u8]>,
    ) -> Result<Result<crate::expressive::PackPreview, crate::expressive::Error>, CoreError> {
        let slot = self.slot(ctx)?;
        Ok(if self.expressive_enabled {
            slot.state.expressive_packs.preview(release, control)
        } else {
            Err("expressive_disabled")
        })
    }

    pub fn content_packs(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Vec<(String, crate::expressive::PackView)>, CoreError> {
        let slot = self.slot(ctx)?;
        Ok(if self.expressive_enabled {
            slot.state.expressive_packs.list()
        } else {
            Vec::new()
        })
    }

    pub fn expressive_picker(
        &self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
    ) -> Result<Option<crate::expressive::PackPicker>, CoreError> {
        let slot = self.slot(ctx)?;
        Ok(if self.expressive_enabled {
            slot.state.expressive_packs.picker(pack)
        } else {
            None
        })
    }

    pub fn update_expressive_picker(
        &mut self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
        release: [u8; 32],
        item: &str,
        change: crate::expressive::PickerChange,
    ) -> Result<(Result<(), crate::expressive::Error>, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        self.txn_inner(
            &ctx.profile_id,
            |tx| {
                Ok(if tx.expressive_enabled {
                    tx.st
                        .expressive_packs
                        .update_picker(pack, release, item, change)
                } else {
                    Err("expressive_disabled")
                })
            },
            true,
        )
    }

    /// Re-open the same digest-bound source after asynchronous decode. A replaced or
    /// deleted message cannot authorize a frame from the previous visible asset.
    pub fn check_expressive_decode(
        &self,
        ctx: &ProfileExecutionContext,
        message_id: &str,
        source: crate::expressive::Source<'_>,
        ciphertext: &[u8],
        observed: Option<&crate::expressive::DecodeFacts>,
    ) -> bool {
        let Some(observed) = observed else {
            return false;
        };
        let Ok(crate::expressive::Opened::Renderable {
            mut plaintext,
            facts,
        }) = self.open_expressive(ctx, message_id, source, ciphertext)
        else {
            return false;
        };
        use zeroize::Zeroize;
        plaintext.zeroize();
        facts.matches_decode(observed)
    }

    pub fn remove_content_pack(
        &mut self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
    ) -> Result<(Result<(), crate::expressive::Error>, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        self.txn_inner(
            &ctx.profile_id,
            |tx| {
                Ok(if tx.expressive_enabled {
                    tx.st.expressive_packs.remove(pack)
                } else {
                    Err("expressive_disabled")
                })
            },
            true,
        )
    }

    pub fn evict_content_pack_blob(
        &mut self,
        ctx: &ProfileExecutionContext,
        pack: [u8; 32],
        blob: [u8; 32],
    ) -> Result<(Result<(), crate::expressive::Error>, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        self.txn_inner(&ctx.profile_id, |tx| {
            Ok(tx.st.expressive_packs.evict(pack, blob))
        }, true)
    }

    /// The media driver opts in per Core instance. The reference app does not enable it.
    pub fn enable_calls(&mut self) {
        self.calls_enabled = true;
    }

    pub fn call_tick(&mut self, ctx: &ProfileExecutionContext) -> Result<Step, CoreError> {
        self.call_step(ctx, CallHostEvent::Tick)
    }

    pub fn call_media_state(
        &mut self,
        ctx: &ProfileExecutionContext,
        id: &str,
        state: crate::calls::MediaState,
    ) -> Result<Step, CoreError> {
        self.call_step(ctx, CallHostEvent::Media(id, state))
    }

    pub fn call_route_facts(
        &mut self,
        ctx: &ProfileExecutionContext,
        id: &str,
        route: crate::calls::CallRoute,
    ) -> Result<Step, CoreError> {
        self.call_step(ctx, CallHostEvent::Route(id, route))
    }

    fn call_step(
        &mut self,
        ctx: &ProfileExecutionContext,
        event: CallHostEvent<'_>,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.maintain_calls();
            match event {
                CallHostEvent::Tick => {}
                CallHostEvent::Media(id, state) => tx.apply_media_state(id, state),
                CallHostEvent::Route(id, route) => tx.apply_call_route(id, route),
            }
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    pub fn call_description(
        &self,
        ctx: &ProfileExecutionContext,
        id: &str,
        ice: bool,
    ) -> Result<Option<&crate::calls::CallDescription>, CoreError> {
        Ok(self
            .slot(ctx)?
            .state
            .calls
            .current
            .as_ref()
            .filter(|call| {
                call.is_live()
                    && call.view.call_session_id == id
                    && call
                        .view
                        .ring_deadline_ms
                        .is_none_or(|d| d > self.clock.now_ms())
            })
            .and_then(|call| {
                if ice {
                    call.ice.as_ref()
                } else {
                    call.description.as_ref()
                }
            }))
    }

    pub fn take_call_keys(
        &mut self,
        ctx: &ProfileExecutionContext,
        id: &str,
    ) -> Result<Option<crate::calls::InstallMediaKeys>, CoreError> {
        self.slot(ctx)?;
        if self.staged.is_some() {
            return Err(CoreError::Busy);
        }
        let state = &mut self
            .profiles
            .get_mut(&ctx.profile_id)
            .expect("validated profile")
            .state;
        Ok(state
            .calls
            .current
            .as_mut()
            .filter(|call| {
                call.view.state == crate::calls::CallState::Connecting
                    && call.view.call_session_id == id
            })
            .and_then(|call| call.keys.take()))
    }

    /// Durable work is re-listed after a restart. Transient call deposits are released only after
    /// the ratchet commit and live only for the owning boot and their sealed expiry.
    pub fn net_ops(&self, ctx: &ProfileExecutionContext) -> Result<Vec<NetOp>, CoreError> {
        let state = &self.slot(ctx)?.state;
        let now = self.clock.now_ms();
        let mut ops: Vec<_> = state
            .app
            .outbox
            .iter()
            .map(|e| NetOp {
                id: e.id,
                call: e.call.clone(),
            })
            .collect();
        // One signal in flight per leg (ADR-0228 s2): only the oldest deposit of a (call, device) is released.
        let mut busy = std::collections::HashSet::new();
        ops.extend(
            state
                .calls
                .deposits
                .iter()
                .filter(|e| {
                    busy.insert((e.call_id, e.device))
                        && state.calls.last_clock_ms.is_none_or(|last| now >= last)
                        && e.deadline_ms > now
                        && e.not_before_ms <= now
                        && crate::flows::eligible_call_device(state, &e.conversation, e.device)
                })
                .map(|e| NetOp {
                    id: e.id,
                    call: e.call.clone(),
                }),
        );
        Ok(ops)
    }

    pub fn net_result(
        &mut self,
        ctx: &ProfileExecutionContext,
        net_id: u64,
        result: NetResult,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| tx.net_result(net_id, result))?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Mailboxes to poll, with the highest relay sequence already processed and committed.
    pub fn poll_targets(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Vec<PollTarget>, CoreError> {
        let st = &self.slot(ctx)?.state;
        let now = self.clock.now_ms();
        let mut boxes: Vec<(&String, MailboxKind, Option<&String>)> = st
            .app
            .invites
            .values()
            .filter(|i| !i.revoked && !i.used && i.expires_at > now)
            .map(|i| {
                // An invite's requests wait at its introduction service; one made before they existed, in a mailbox.
                let kind = if i.intro_service.is_some() {
                    MailboxKind::Intro
                } else {
                    MailboxKind::Relay
                };
                (&i.mailbox, kind, None)
            })
            .collect();
        // Every route of every relationship that has not been closed: the published ones and those still draining.
        boxes.extend(
            st.app
                .relationships
                .values()
                .flat_map(|r| r.my_boxes.iter())
                .filter(|b| !b.closed)
                .map(|b| {
                    let uri = (!b.route.is_legacy()).then_some(&b.route.relay_uri);
                    (&b.mailbox, MailboxKind::Relay, uri)
                }),
        );
        let mut targets: Vec<PollTarget> = boxes
            .into_iter()
            .filter_map(|(m, kind, uri)| {
                Some(PollTarget {
                    mailbox: crate::crypto::unhex(m).ok()?,
                    after: st.app.cursors.get(m).copied().unwrap_or(0),
                    kind,
                    relay_uri: uri.cloned(),
                    capability: vec![],
                })
            })
            .collect();
        // A device-link ceremony's short-lived queues (D24), while it runs.
        targets.extend(crate::flows::ceremony_poll_targets(st, now));
        Ok(targets)
    }

    /// Relay items for one polled mailbox. Replays (sequence at or below the committed cursor) are ignored.
    pub fn inbound(
        &mut self,
        ctx: &ProfileExecutionContext,
        mailbox: &[u8],
        items: &[Inbound],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let mailbox = crate::crypto::hex(mailbox);
        let items: Vec<(u64, Vec<u8>)> = items.iter().map(|i| (i.seq, i.bytes.clone())).collect();
        let (_, commit) = self.txn(&ctx.profile_id, |tx| tx.inbound(&mailbox, items))?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    // ------------------------------------------------------------------------------------------ recovery (controller side)

    /// Hand Core a `signed-RecoveryRequest` the host observed (transport is the host's: there is no Home endpoint yet).
    /// The first valid observation starts this verifier's own interval; see `flows::recovery_watch`.
    pub fn recovery_observe_request(
        &mut self,
        ctx: &ProfileExecutionContext,
        signed_request: &[u8],
        mono: &MonotonicSample,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.observe_recovery_request(signed_request, mono)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Hand Core a `signed-RecoveryVeto` the host observed.
    pub fn recovery_observe_veto(
        &mut self,
        ctx: &ProfileExecutionContext,
        signed_veto: &[u8],
        mono: &MonotonicSample,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.observe_recovery_veto(signed_veto, mono)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Hand Core the recovery-signed successor for the tracked request `request_id` (its digest, hex).
    pub fn recovery_observe_successor(
        &mut self,
        ctx: &ProfileExecutionContext,
        request_id: &str,
        bundle: &crate::recovery::SuccessorBundle,
        mono: &MonotonicSample,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.observe_recovery_successor(request_id, bundle, mono)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// The recovering device pushes a `RecoveryNotice` for `signed_request` to every active contact over the ordinary
    /// sealed 1:1 deposit (ADR-0190 3.1). Returns how many were queued; the caller repeats it on the resend schedule.
    pub fn recovery_send_notices(
        &mut self,
        ctx: &ProfileExecutionContext,
        signed_request: &[u8],
        successor: Option<&crate::recovery::SuccessorBundle>,
    ) -> Result<(usize, Step), CoreError> {
        self.slot(ctx)?;
        let (n, commit) = self.txn(&ctx.profile_id, |tx| {
            Ok(tx.send_recovery_notices(signed_request, successor))
        })?;
        Ok((
            n,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// Credit the host's durable monotonic reading to every pending request (call it regularly and at start), drop
    /// stale ones and apply a completion whose delay has run.
    pub fn recovery_tick(
        &mut self,
        ctx: &ProfileExecutionContext,
        mono: &MonotonicSample,
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| tx.recovery_tick(mono))?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// Send the device-enrollment notices that are due to the devices that have not acknowledged them (ADR-0193
    /// section 4). The host calls it at start and on its poll cycle; Core keeps no timer and commits only when a
    /// record changed.
    pub fn notice_tick(&mut self, ctx: &ProfileExecutionContext) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.owed_tick();
            tx.maintain_preferences();
            // The sender's repeats of ceremony evidence are due by the clock, not by an inbound batch (ADR-0226 section 5).
            tx.run_evidence_repeats();
            Ok(())
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    pub fn pending_operations(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<Vec<PendingOperationView>, CoreError> {
        Ok(self
            .slot(ctx)?
            .state
            .app
            .ops
            .values()
            .map(|r| PendingOperationView {
                operation_id: r.operation_id.clone(),
                intent_kind: r.kind.clone(),
                outcome: r.outcome.clone(),
            })
            .collect())
    }

    pub fn outcome(
        &self,
        ctx: &ProfileExecutionContext,
        op: &str,
    ) -> Result<Option<OutcomeClass>, CoreError> {
        Ok(self
            .slot(ctx)?
            .state
            .app
            .ops
            .get(op)
            .map(|r| r.outcome.clone()))
    }

    /// TEST ONLY: the profile's app state as JSON, so scenario tests can assert on replay records and public keys.
    /// Compiled only with `test-loopback`, which cannot be combined with `production`.
    #[cfg(feature = "test-loopback")]
    pub fn debug_app_state(
        &self,
        ctx: &ProfileExecutionContext,
    ) -> Result<serde_json::Value, CoreError> {
        serde_json::to_value(&self.slot(ctx)?.state.app).map_err(|_| CoreError::Invalid("state"))
    }

    /// TEST ONLY: SHA-256 (hex) of the profile's device/session state (ratchets, prekeys, ledger). Lets a test prove
    /// that a rejected payload left the cryptographic state untouched. Compiled only with `test-loopback`.
    #[cfg(feature = "test-loopback")]
    pub fn debug_device_digest(&self, ctx: &ProfileExecutionContext) -> Result<String, CoreError> {
        let dev = self
            .slot(ctx)?
            .state
            .device
            .as_ref()
            .ok_or(CoreError::Invalid("no device"))?;
        let bytes = crate::crypto::cbor::encode(&dev.to_cbor());
        Ok(crate::crypto::hex(&crate::crypto::sha256(&[&bytes])))
    }

    /// TEST ONLY: hand a `KIND_ACS_UPDATE` payload (CBOR) to the head-update path of conversation `conversation_id`, as
    /// if a verified session had delivered it (see `Tx::debug_acs_update`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_acs_update(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &str,
        payload: &[u8],
    ) -> Result<(bool, Step), CoreError> {
        self.slot(ctx)?;
        let (ok, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_acs_update(conversation_id, payload)
        })?;
        Ok((
            ok,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// TEST ONLY: see `Tx::debug_link_messaging_device`.
    #[cfg(feature = "test-loopback")]
    pub fn debug_link_messaging_device(
        &mut self,
        ctx: &ProfileExecutionContext,
    ) -> Result<(String, Step), CoreError> {
        self.slot(ctx)?;
        let (id, commit) = self.txn(&ctx.profile_id, |tx| tx.debug_link_messaging_device())?;
        Ok((
            id,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// TEST ONLY: link a credential the test built (see `Tx::debug_link_credential`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_link_credential(
        &mut self,
        ctx: &ProfileExecutionContext,
        credential_body: &[u8],
    ) -> Result<(String, Step), CoreError> {
        self.slot(ctx)?;
        let (id, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_link_credential(credential_body)
        })?;
        Ok((
            id,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// TEST ONLY: link credentials the test built as controllers (see `Tx::debug_link_controllers`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_link_controllers(
        &mut self,
        ctx: &ProfileExecutionContext,
        credential_bodies: &[Vec<u8>],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_link_controllers(credential_bodies)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// TEST ONLY: adopt the introduction of one of our own other devices (see `Tx::debug_adopt_sibling_intro`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_adopt_sibling_intro(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &str,
        intro: &[u8],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_adopt_sibling_intro(conversation_id, intro)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// TEST ONLY: send a payload to one of the account's other devices (see `Tx::debug_send_to_sibling`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_send_to_sibling(
        &mut self,
        ctx: &ProfileExecutionContext,
        device: &str,
        payload: &[u8],
    ) -> Result<(bool, Step), CoreError> {
        self.slot(ctx)?;
        let (ok, commit) = self.txn(&ctx.profile_id, |tx| {
            Ok(tx.debug_send_to_sibling(device, payload))
        })?;
        Ok((
            ok,
            Step {
                commit,
                handle: None,
            },
        ))
    }

    /// TEST ONLY: deposit an arbitrary authenticated inner payload (see `Tx::debug_inject`).
    #[cfg(feature = "test-loopback")]
    pub fn debug_inject_payload(
        &mut self,
        ctx: &ProfileExecutionContext,
        conversation_id: &str,
        payload: &[u8],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| {
            tx.debug_inject(conversation_id, payload)
        })?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    /// TEST ONLY: deposit arbitrary bytes under the authenticated call context.
    #[cfg(feature = "test-loopback")]
    pub fn debug_call_payload(
        &mut self,
        ctx: &ProfileExecutionContext,
        conv: &str,
        bytes: &[u8],
    ) -> Result<Step, CoreError> {
        self.slot(ctx)?;
        let (_, commit) = self.txn(&ctx.profile_id, |tx| tx.debug_call_payload(conv, bytes))?;
        Ok(Step {
            commit,
            handle: None,
        })
    }

    // ------------------------------------------------------------------------------------------ backup file (ADR-0184)

    /// The password-hashing settings and a fresh random salt for the next backup file. The host derives the key from
    /// the person's password with exactly these and hands it to [`Core::export_history`].
    pub fn backup_plan(&mut self) -> BackupPlan {
        let mut salt = [0u8; backup::SALT_LEN];
        self.rng.fill_bytes(&mut salt);
        BackupPlan {
            kdf: backup::default_kdf(),
            salt: salt.to_vec(),
        }
    }

    /// Seal the profile's history (never any key or authority; see `backup::collect`) into one backup file. Reads
    /// committed state only, so it never conflicts with an outstanding commit and changes nothing.
    pub fn export_history(
        &mut self,
        ctx: &ProfileExecutionContext,
        plan: &BackupPlan,
        key: &[u8],
    ) -> Result<Vec<u8>, CoreError> {
        let mut master: [u8; 32] = key
            .try_into()
            .map_err(|_| CoreError::Invalid("backup key"))?;
        let salt: [u8; backup::SALT_LEN] = plan
            .salt
            .as_slice()
            .try_into()
            .map_err(|_| CoreError::Invalid("backup salt"))?;
        let mut nonce = [0u8; backup::NONCE_LEN];
        self.rng.fill_bytes(&mut nonce);
        let out = (|| {
            let archive = backup::collect(&self.slot(ctx)?.state.app, self.clock.now_ms())?;
            let mut body = backup::encode_body(&archive);
            let file = backup::seal(&body, plan.kdf, &salt, &nonce, &master);
            body.zeroize();
            Ok(file?)
        })();
        master.zeroize();
        out.map_err(|e: BackupOrCore| e.0)
    }

    /// What the host needs to derive the key for a backup file. Needs no profile: any file can be looked at first.
    pub fn inspect_backup(file: &[u8]) -> Result<backup::BackupHeader, CoreError> {
        backup::inspect(file).map_err(CoreError::Backup)
    }

    /// Restore a backup file into the profile as read-only history. All or nothing: the whole file is authenticated and
    /// validated before anything is staged, and the staged result is one commit.
    pub fn import_history(
        &mut self,
        ctx: &ProfileExecutionContext,
        file: &[u8],
        key: &[u8],
    ) -> Result<(backup::ImportSummary, Option<Commit>), CoreError> {
        self.slot(ctx)?;
        let mut master: [u8; 32] = key
            .try_into()
            .map_err(|_| CoreError::Invalid("backup key"))?;
        let opened = backup::open(file, &master);
        master.zeroize();
        let mut body = opened.map_err(CoreError::Backup)?;
        let archive = backup::decode_body(&body);
        body.zeroize();
        let archive = archive.map_err(CoreError::Backup)?;
        let now = self.clock.now_ms();
        self.txn(&ctx.profile_id, |tx| {
            backup::apply_import(&mut tx.st.app, archive, now).map_err(CoreError::Backup)
        })
    }

    // ------------------------------------------------------------------------------------------ queries

    /// Typed queries over committed state only. The result is the JSON of the matching platform-API view.
    pub fn query(&self, q: &Query) -> Result<serde_json::Value, CoreError> {
        let to = |v: &dyn erased::Ser| v.to_value();
        match q {
            Query::GetProfileList => {
                let list: Vec<ProfileSummary> = self
                    .profiles
                    .iter()
                    .map(|(id, s)| ProfileSummary {
                        id: id.clone(),
                        label: s.state.app.label.clone(),
                    })
                    .collect();
                Ok(to(&list))
            }
            _ => {
                let ctx = q.ctx().ok_or(CoreError::Invalid("query context"))?;
                let st = &self.slot(ctx)?.state;
                match q {
                    Query::GetConversationList { .. } => Ok(to(&conversation_list(st))),
                    Query::GetRelationshipRestore { .. } => {
                        let now = self.clock.now_ms();
                        let contacts: Vec<_> = st
                            .app
                            .relationship_restore
                            .iter()
                            .flat_map(|r| &r.candidates)
                            .map(|(account, c)| {
                                let invite = st.app.reconnect_invite(c, now);
                                ReconnectContactView {
                                    account_id: account.clone(),
                                    label: c.label.clone(),
                                    invite_operation_id: invite.map(|i| i.operation_id.clone()),
                                    can_retry: invite.is_some_and(|i| i.used),
                                    invite: invite
                                        .filter(|i| {
                                            !i.used
                                                && st.app.ops.get(&i.operation_id).is_some_and(
                                                    |op| op.outcome == OutcomeClass::Committed,
                                                )
                                        })
                                        .map(|i| InviteView {
                                            operation_id: i.operation_id.clone(),
                                            link: i.link.clone(),
                                            expires_at: i.expires_at,
                                        }),
                                    conversation_id: st
                                        .app
                                        .relationships
                                        .values()
                                        .find(|r| {
                                            r.peer_account_id == *account
                                                && r.state == RelState::Active
                                                && !r.blocked
                                        })
                                        .map(|r| r.id.clone()),
                                }
                            })
                            .collect();
                        Ok(to(&contacts))
                    }
                    Query::PlanAttachment { size, .. } => Ok(to(&self.plan_attachment(*size))),
                    Query::GetCallView { .. } => {
                        let now = self.clock.now_ms();
                        Ok(to(&st
                            .calls
                            .current
                            .as_ref()
                            .map(|c| c.view_at(&st.calls.deposits, now))))
                    }
                    Query::GetCallNotices { .. } => Ok(to(&st.app.call_notices)),
                    Query::GetDirectOffers { .. } => Ok(to(&self.direct_offer_views(ctx, st))),
                    Query::GetConversationView {
                        conversation_id, ..
                    } => {
                        if !st.app.relationships.contains_key(conversation_id)
                            && let Some(r) = st.app.restored.get(conversation_id)
                        {
                            return Ok(to(&ConversationView {
                                summary: restored_summary(r),
                                own_account: None,
                                timeline: r
                                    .messages
                                    .iter()
                                    .map(|m| restored_item(m, false))
                                    .collect(),
                            }));
                        }
                        let rel = st
                            .app
                            .relationships
                            .get(conversation_id)
                            .ok_or(CoreError::NotFound("conversation"))?;
                        let me_hex = st
                            .device
                            .as_ref()
                            .map(|d| crate::crypto::hex(&d.account_id))
                            .unwrap_or_default();
                        Ok(to(&ConversationView {
                            summary: summary(rel, &st.app),
                            own_account: st
                                .device
                                .as_ref()
                                .filter(|_| {
                                    rel.state == RelState::Active && !rel.peer_account_id.is_empty()
                                })
                                .map(|d| crate::crypto::hex(&d.account_id)),
                            timeline: st
                                .app
                                .relationship_backup(rel)
                                .into_iter()
                                .flat_map(|r| r.messages.iter().map(|m| restored_item(m, true)))
                                .chain(
                                    st.app
                                        .relationship_history
                                        .get(conversation_id)
                                        .into_iter()
                                        .flat_map(|history| {
                                            history.iter().filter(|m| !m.deleted).map(|m| {
                                                timeline_item(
                                                    m,
                                                    history,
                                                    &st.app.direct_offers,
                                                    true,
                                                    &me_hex,
                                                )
                                            })
                                        }),
                                )
                                .chain(rel.messages.iter().filter(|m| !m.deleted && !m.lane).map(
                                    |m| {
                                        self.overlay_transfers(
                                            ctx,
                                            timeline_item(
                                                m,
                                                &rel.messages,
                                                &st.app.direct_offers,
                                                false,
                                                &me_hex,
                                            ),
                                        )
                                    },
                                ))
                                .collect(),
                        }))
                    }
                    Query::GetIntroductions { .. } => {
                        let now_s = self.clock.now_ms() / 1000;
                        let list: Vec<IntroductionRequest> = st
                            .app
                            .intros
                            .values()
                            // The bootstrap key is the request's deadline: past it the request can no longer be answered.
                            .filter(|i| i.reply_key.expires_at > now_s)
                            .map(|i| IntroductionRequest {
                                id: i.request_id.clone(),
                                from: i.person_id.clone(),
                                display_name: i.display_name.clone(),
                                // A pending request has no binding by construction (ADR-0181 section 7).
                                verification: VerificationCode::Unverified,
                            })
                            .collect();
                        Ok(to(&list))
                    }
                    Query::GetConfirmation { operation_id, .. } => {
                        let now = self.clock.now_ms();
                        let v = st
                            .app
                            .confirmations
                            .iter()
                            .find(|c| &c.operation_id == operation_id && c.expires_at > now)
                            .map(|c| ConfirmationView {
                                nonce: c.nonce.clone(),
                                expires_at: c.expires_at,
                            });
                        Ok(to(&v))
                    }
                    Query::GetIntroductionLoad { .. } => {
                        let l = &st.app.first_contact;
                        let now = self.clock.now_ms().max(l.high_water_ms);
                        let throttled =
                            l.dropped > 0 && l.last_drop_ms + crate::flows::HOUR_MS > now;
                        Ok(to(&IntroductionLoadView {
                            throttled,
                            dropped: if throttled { l.dropped } else { 0 },
                        }))
                    }
                    Query::GetRecoveryKey {
                        key_operation_id, ..
                    } => {
                        let v = st
                            .app
                            .recovery_key
                            .as_ref()
                            .filter(|k| &k.key_operation_id == key_operation_id)
                            .map(|k| RecoveryKeyView {
                                key_operation_id: k.key_operation_id.clone(),
                                groups: k.groups.iter().map(|group| group.to_string()).collect(),
                                sheet: k.sheet.as_deref().map(str::to_owned),
                            });
                        Ok(to(&v))
                    }
                    Query::GetInvite { operation_id, .. } => {
                        let v = st
                            .app
                            .invites
                            .get(operation_id)
                            .filter(|i| !i.revoked)
                            .map(|i| InviteView {
                                operation_id: i.operation_id.clone(),
                                link: i.link.clone(),
                                expires_at: i.expires_at,
                            });
                        Ok(to(&v))
                    }
                    Query::GetHomeSummary { .. } => Ok(to(&self.home)),
                    Query::GetDevices { .. } => Ok(to(&devices_view(st, self.clock.now_ms()))),
                    Query::GetHistoryTransfer { .. } => Ok(crate::flows::history_view(st)
                        .map(|v| to(&v))
                        .unwrap_or(serde_json::Value::Null)),
                    Query::GetLinkRequest { operation_id, .. } => {
                        let now = self.clock.now_ms();
                        Ok(crate::flows::request_view(st, operation_id, now)
                            .map(|v| to(&v))
                            .unwrap_or(serde_json::Value::Null))
                    }
                    Query::GetLinkOffer { operation_id, .. } => {
                        let now = self.clock.now_ms();
                        Ok(crate::flows::offer_view(st, operation_id, now)
                            .map(|v| to(&v))
                            .unwrap_or(serde_json::Value::Null))
                    }
                    Query::GetRecoveryApprovalOffer { operation_id, .. } => {
                        let now = self.clock.now_ms();
                        Ok(crate::flows::approval_offer_view(st, operation_id, now)
                            .map(|v| to(&v))
                            .unwrap_or(serde_json::Value::Null))
                    }
                    Query::GetRecoveryStatus { .. } => {
                        Ok(to(&recovery_status(st, self.clock.now_ms())))
                    }
                    Query::GetPrivacyLevel { .. } => Ok(to(&st
                        .app
                        .privacy_level
                        .unwrap_or(crate::api::PrivacyPolicyLevel::Recommended))),
                    Query::GetProfileList => Err(CoreError::Invalid("unreachable")),
                }
            }
        }
    }
}

/// Lets the export closure use `?` on both `CoreError` and `BackupError`.
struct BackupOrCore(CoreError);
impl From<CoreError> for BackupOrCore {
    fn from(e: CoreError) -> Self {
        BackupOrCore(e)
    }
}
impl From<backup::BackupError> for BackupOrCore {
    fn from(e: backup::BackupError) -> Self {
        BackupOrCore(CoreError::Backup(e))
    }
}

mod erased {
    pub trait Ser {
        fn to_value(&self) -> serde_json::Value;
    }
    impl<T: serde::Serialize> Ser for T {
        fn to_value(&self) -> serde_json::Value {
            serde_json::to_value(self).unwrap_or(serde_json::Value::Null)
        }
    }
}

fn summary(r: &Relationship, app: &AppState) -> ConversationSummary {
    let mut notices = vec![];
    if r.integrity_failures > 0 {
        notices.push(SecurityNotice {
            code: SecurityNoticeCode::IntegrityFailure,
            subject: Some(r.person_id.clone()),
        });
    }
    if r.state == RelState::AcceptedUnbound {
        notices.push(SecurityNotice {
            code: SecurityNoticeCode::UnverifiedContact,
            subject: Some(r.person_id.clone()),
        });
    }
    ConversationSummary {
        id: r.id.clone(),
        person_id: r.person_id.clone(),
        display_name: r.display_name.clone(),
        nickname: r.nickname.clone(),
        peer_account: (!r.peer_account_id.is_empty()).then(|| r.peer_account_id.clone()),
        allow_calls: (r.state == RelState::Active && !r.blocked && !r.peer_account_id.is_empty())
            .then(|| !app.calls_disabled.contains(&r.peer_account_id)),
        last_message_at: r
            .messages
            .iter()
            .rev()
            .find(|m| !m.lane)
            .map(|m| m.sent_at)
            .or_else(|| {
                app.relationship_history
                    .get(&r.id)
                    .and_then(|history| history.iter().rev().find(|m| !m.deleted))
                    .map(|m| m.sent_at)
            })
            .or_else(|| {
                app.relationship_backup(r)
                    .and_then(|backup| backup.messages.last())
                    .map(|m| m.sent_at)
            }),
        notices,
        peer_recovery: peer_recovery_view(r, app),
        relinked_from: r.relinked_from.clone(),
        relationship: match r.state {
            RelState::AwaitingAcceptance => RelationshipStateCode::AwaitingAcceptance,
            RelState::AcceptedUnbound => RelationshipStateCode::AcceptedUnbound,
            RelState::Establishing => RelationshipStateCode::Establishing,
            RelState::Active => RelationshipStateCode::Active,
        },
    }
}

/// The tracked recovery to show for a relationship: the live one, else the latest that ended.
fn peer_recovery_view(r: &Relationship, app: &AppState) -> Option<PeerRecoveryView> {
    let rec = r
        .peer_recovery
        .iter()
        .rev()
        .find(|x| x.is_live())
        .or_else(|| r.peer_recovery.last())?;
    let policy = crate::crypto::unhex(&rec.policy)
        .ok()
        .and_then(|b| crate::recovery::RecoveryPolicy::from_body(&b).ok())?;
    const DAY_MS: u64 = 86_400_000;
    let left_ms = (policy.delay_seconds * 1000).saturating_sub(rec.obs.credited_ms);
    let confirm = !matches!(
        app.privacy_level,
        None | Some(PrivacyPolicyLevel::Recommended)
    );
    Some(PeerRecoveryView {
        request_id: rec.obs.digest.clone(),
        state: rec.state,
        days_left: left_ms.div_ceil(DAY_MS),
        can_decline: rec.is_live(),
        needs_confirm: confirm && rec.state == crate::state::PeerRecoveryState::Ready,
        notify_on_completion: policy.notify_contacts_on_completion,
    })
}

/// The quoted block of a reply: the original is looked up in this conversation only, so a reply never reveals anything
/// from elsewhere, and a missing or deleted original shows as unavailable with no text kept.
fn reply_view(m: &Msg, all: &[Msg]) -> Option<ReplyView> {
    let target = m.social.reply_to.as_deref()?;
    let id = format!("msg-{target}");
    Some(
        match all
            .iter()
            .find(|o| o.id == id && !o.lane && !o.deleted && o.id != m.id)
        {
            Some(o) => ReplyView {
                message_id: id,
                available: true,
                outgoing: o.outgoing,
                preview: o.text.chars().take(REPLY_PREVIEW_CHARS).collect(),
                has_attachment: !o.attachments.is_empty(),
            },
            None => ReplyView {
                message_id: id,
                available: false,
                outgoing: false,
                preview: String::new(),
                has_attachment: false,
            },
        },
    )
}

/// The mentions of a message as shown (ADR-0199); none for a deleted message.
fn mention_views(m: &Msg, me_account: &str) -> Vec<MentionView> {
    if m.deleted {
        return vec![];
    }
    m.social
        .mentions
        .iter()
        .map(|x| MentionView {
            start: x.start,
            end: x.end,
            account_id: x.account.clone(),
            me: x.account == me_account,
        })
        .collect()
}

fn timeline_item(
    m: &Msg,
    all: &[Msg],
    offers: &BTreeMap<Hex, DirectOfferRec>,
    read_only: bool,
    me_account: &str,
) -> TimelineItem {
    let reply = reply_view(m, all);
    let mentions = mention_views(m, me_account);
    let reactions = crate::flows::reaction_views(m);
    let edited = m.social.edited;
    match (&m.operation_id, m.outgoing) {
        (Some(op), true) => TimelineItem::Outgoing {
            message_id: m.id.clone(),
            operation_id: op.clone(),
            text: m.text.clone(),
            sent_at: m.sent_at,
            delivery: m.delivery.clone(),
            attachments: crate::flows::attachment_views(m, offers),
            presentation: crate::flows::presentation(m),
            reply,
            reactions,
            edited,
            mentions,
            read_only,
        },
        // A message our own other device sent (self-sync): ours, with no operation on this device behind it.
        (None, true) => TimelineItem::Outgoing {
            message_id: m.id.clone(),
            operation_id: format!("synced:{}", m.id),
            text: m.text.clone(),
            sent_at: m.sent_at,
            delivery: m.delivery.clone(),
            attachments: crate::flows::attachment_views(m, offers),
            presentation: crate::flows::presentation(m),
            reply,
            reactions,
            edited,
            mentions,
            read_only,
        },
        _ => TimelineItem::Incoming {
            message_id: m.id.clone(),
            text: m.text.clone(),
            sent_at: m.sent_at,
            attachments: crate::flows::attachment_views(m, offers),
            presentation: crate::flows::presentation(m),
            reply,
            reactions,
            edited,
            mentions,
            read_only,
        },
    }
}

fn restored_summary(r: &RestoredChat) -> ConversationSummary {
    ConversationSummary {
        id: r.id.clone(),
        person_id: format!("restored:{}", r.id),
        display_name: r.display_name.clone(),
        nickname: r.nickname.clone(),
        allow_calls: None,
        last_message_at: r.messages.last().map(|m| m.sent_at),
        notices: vec![],
        peer_account: None,
        peer_recovery: None,
        relinked_from: None,
        relationship: RelationshipStateCode::Restored,
    }
}

/// A restored message has no operation behind it. Outgoing ones keep their old state as far as it is still true: an
/// attempt that never left the old device is shown as not sent, since nothing will ever retry it.
fn restored_item(m: &RestoredMsg, in_live_thread: bool) -> TimelineItem {
    let message_id = if in_live_thread {
        format!("restored:{}", m.id)
    } else {
        m.id.clone()
    };
    if m.outgoing {
        TimelineItem::Outgoing {
            message_id,
            operation_id: format!("restored:{}", m.id),
            text: m.text.clone(),
            sent_at: m.sent_at,
            delivery: match m.delivery {
                1 => DeliveryState::Sent,
                2 => DeliveryState::Delivered,
                _ => DeliveryState::Rejected {
                    reason: RejectReason::Unavailable,
                },
            },
            attachments: vec![],
            presentation: None,
            reply: None,
            reactions: vec![],
            edited: false,
            mentions: vec![],
            read_only: true,
        }
    } else {
        TimelineItem::Incoming {
            message_id,
            text: m.text.clone(),
            sent_at: m.sent_at,
            attachments: vec![],
            presentation: None,
            reply: None,
            reactions: vec![],
            edited: false,
            mentions: vec![],
            read_only: true,
        }
    }
}

fn conversation_list(st: &ProfileState) -> Vec<ConversationSummary> {
    // (last activity, summary): live conversations and restored history are listed together, newest first.
    let mut v: Vec<(u64, ConversationSummary)> = st
        .app
        .relationships
        .values()
        .map(|r| {
            let view = summary(r, &st.app);
            (view.last_message_at.unwrap_or(r.created_at), view)
        })
        .collect();
    let attached_backups: std::collections::BTreeSet<_> = st
        .app
        .relationships
        .values()
        .filter_map(|r| st.app.relationship_backup(r).map(|old| &old.id))
        .collect();
    v.extend(
        st.app
            .restored
            .values()
            .filter(|r| {
                !st.app.relationships.contains_key(&r.id) && !attached_backups.contains(&r.id)
            })
            .map(|r| {
                (
                    r.messages.last().map(|m| m.sent_at).unwrap_or(r.created_at),
                    restored_summary(r),
                )
            }),
    );
    v.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id)));
    v.into_iter().map(|(_, s)| s).collect()
}

fn devices_view(st: &ProfileState, now_ms: u64) -> DevicesView {
    let head = (|| {
        let chain: Vec<Vec<u8>> = st
            .app
            .acs_chain
            .iter()
            .map(|c| crate::crypto::unhex(c).ok())
            .collect::<Option<_>>()?;
        let creds: Vec<Vec<u8>> = st
            .app
            .acs_credentials
            .iter()
            .map(|c| crate::crypto::unhex(c).ok())
            .collect::<Option<_>>()?;
        st.app.verify_own_chain(&chain, &creds)
    })();
    let (Some(head), Some(me)) = (head, st.device.as_ref().map(|d| d.device_id)) else {
        return DevicesView {
            control_state: AccountControlStateCode::Normal,
            devices: vec![],
            link_requests: crate::flows::pending_requests(st, now_ms),
            notices: crate::flows::notice_views(st),
            strict_notice: st.app.link_strict_notice,
            controller_change: None,
            can_join_controller_change: false,
            recovery_key_replacement_needed: false,
        };
    };
    use crate::control::{Action, PlanError, plan};
    let forked = !st.app.acs_forks.is_empty();
    let me_active = !forked && head.control.contains(&me) && !head.quarantined.contains(&me);
    let voting = head.control.difference(&head.quarantined).count();
    let frozen = crate::control::safety_frozen(&head);
    let summary = |id: &[u8; 16], role: DeviceRole, state: DeviceStateCode| {
        let hexid = crate::crypto::hex(id);
        let meta = st.app.device_meta.get(&hexid);
        let is_current = *id == me;
        let mut actions = vec![];
        let mut cautions = vec![DeviceCautionCode::HistorySyncUnavailable];
        // ADR-0193 section 10: owed and unacknowledged notices are read from Core, never composed by the UI.
        let owed: Vec<&crate::state::OwedNotice> = st
            .app
            .owed_notices
            .iter()
            .filter(|o| o.sibling_device_id == hexid)
            .collect();
        let never_set_up = st.app.link_offers.values().any(|o| {
            o.approval_undelivered && o.request.as_ref().is_some_and(|r| r.device_id == hexid)
        });
        let notice_state = if never_set_up {
            DeviceNoticeState::NotSetUp
        } else if owed
            .iter()
            .any(|o| o.state != crate::state::OwedState::Held)
        {
            DeviceNoticeState::Owed
        } else if owed.is_empty() {
            DeviceNoticeState::Told
        } else {
            DeviceNoticeState::Held
        };
        let last_attempt_at = owed
            .iter()
            .map(|o| o.last_attempt_at)
            .filter(|t| *t > 0)
            .max();
        if notice_state == DeviceNoticeState::Owed && me_active {
            actions.push(DeviceActionCode::RetryNotice);
        }
        if is_current && role == DeviceRole::Control && voting == 1 {
            cautions.push(DeviceCautionCode::LastVotingController);
        }
        if me_active && !is_current && !frozen && state != DeviceStateCode::Revoked {
            if role == DeviceRole::Control && state == DeviceStateCode::Active {
                match plan(&head, &me, Action::Suspend, id) {
                    Ok(_) => actions.push(DeviceActionCode::Suspend),
                    Err(PlanError::WouldFreeze) => {
                        cautions.push(DeviceCautionCode::SuspendWouldFreeze)
                    }
                    Err(_) => {}
                }
            }
            match plan(&head, &me, Action::Revoke, id) {
                Ok(_) => actions.push(DeviceActionCode::Revoke),
                Err(PlanError::NeedsOtherSigners)
                    if voting == 2
                        && role == DeviceRole::Control
                        && state == DeviceStateCode::Active =>
                {
                    actions.push(DeviceActionCode::RevokeWithApproval);
                }
                _ => {}
            }
            if role == DeviceRole::Messaging && plan(&head, &me, Action::Promote, id).is_ok() {
                actions.push(DeviceActionCode::Promote);
            }
        }
        DeviceSummary {
            id: hexid,
            label: meta
                .map(|m| m.label.clone())
                .unwrap_or_else(|| "Linked device".into()),
            role,
            state,
            is_current,
            enrolled_at: meta.map(|m| m.enrolled_at).unwrap_or(st.app.created_at),
            last_seen: if is_current {
                LastSeenClass::Now
            } else {
                LastSeenClass::Never
            },
            actions,
            cautions,
            notice_state,
            last_attempt_at,
        }
    };
    let mut devices = vec![];
    for id in &head.control {
        let state = if head.quarantined.contains(id) {
            DeviceStateCode::Quarantined
        } else {
            DeviceStateCode::Active
        };
        devices.push(summary(id, DeviceRole::Control, state));
    }
    for id in head.messaging.difference(&head.control) {
        devices.push(summary(id, DeviceRole::Messaging, DeviceStateCode::Active));
    }
    for id in &head.revoked {
        devices.push(summary(id, DeviceRole::Messaging, DeviceStateCode::Revoked));
    }
    DevicesView {
        control_state: if forked {
            AccountControlStateCode::ForkDetected
        } else if frozen {
            AccountControlStateCode::SafetyFreeze
        } else {
            AccountControlStateCode::Normal
        },
        devices,
        link_requests: crate::flows::pending_requests(st, now_ms),
        notices: crate::flows::notice_views(st),
        strict_notice: st.app.link_strict_notice,
        controller_change: crate::flows::controller_change::view(st, &head, now_ms),
        recovery_key_replacement_needed: me_active
            && !frozen
            && st.app.recovery_writer.paused_for_exclusion,
        can_join_controller_change: !forked
            && !frozen
            && head
                .control
                .iter()
                .any(|d| *d != me && !head.quarantined.contains(d))
            && (head.messaging.contains(&me) || me_active),
    }
}

/// `on` only when the recovery key is confirmed stored AND the policy body Core keeps hashes to the commitment in the
/// head AccountControlState (RECOVERY_AND_BACKUP_POLICY.md section 1); anything else, including a profile made before
/// policies existed, is `pending`.
fn recovery_status(st: &ProfileState, now_ms: u64) -> RecoveryStatusView {
    let policy = st
        .app
        .recovery_policy
        .as_deref()
        .and_then(|h| crate::crypto::unhex(h).ok())
        .and_then(|b| crate::recovery::RecoveryPolicy::from_body(&b).ok());
    let head = (|| {
        let unhex_all = |v: &[String]| -> Option<Vec<Vec<u8>>> {
            v.iter().map(|c| crate::crypto::unhex(c).ok()).collect()
        };
        st.app.verify_own_chain(
            &unhex_all(&st.app.acs_chain)?,
            &unhex_all(&st.app.acs_credentials)?,
        )
    })();
    let committed = match (&policy, &head) {
        (Some(p), Some(h)) => {
            h.recovery_commitment == Some(p.hash())
                && p.state == crate::recovery::STATE_ON
                && p.recovery_key().is_some()
        }
        _ => false,
    };
    let on = committed && st.app.recovery_key_saved;
    let scopes = match &policy {
        Some(p) if committed => {
            let mut v = vec![RecoveryScope::Identity];
            if p.recovers.contains(&crate::recovery::TARGET_HISTORY) {
                v.push(RecoveryScope::History);
            }
            if p.recovers
                .contains(&crate::recovery::TARGET_PROVIDER_METADATA)
                || p.recovers.contains(&crate::recovery::TARGET_RELATIONSHIPS)
            {
                v.push(RecoveryScope::Provider);
            }
            v
        }
        _ => vec![RecoveryScope::Identity],
    };
    RecoveryStatusView {
        policy: if on {
            RecoveryPolicyState::On
        } else {
            RecoveryPolicyState::Pending
        },
        custodians: if st.app.recovery_key_saved {
            vec![RecoveryCustodianView {
                code: RecoveryCustodianCode::RecoveryKey,
                distribution_bound: false,
            }]
        } else {
            vec![]
        },
        scopes,
        activation_delay_ms: match &policy {
            Some(p) if committed => p.delay_seconds * 1000,
            _ => crate::recovery::DEFAULT_DELAY_SECONDS * 1000,
        },
        recovery_key_saved: st.app.recovery_key_saved,
        requests: crate::flows::request_views(
            &st.app,
            now_ms,
            match (&head, &policy) {
                (Some(h), Some(p)) if committed => Some((h, p)),
                _ => None,
            },
            st.device.as_ref().map(|d| d.device_id),
        ),
        recovering: st.app.recovery_join.as_ref().map(|j| RecoveringView {
            home: j.home.clone(),
            stage: j.stage,
            restricted: true,
            evidence: j.evidence,
            attempt: j.terminal.or_else(|| j.attempt.as_ref().map(|a| a.state)),
            cancelled_attempt_id: (j.terminal == Some(crate::state::AttemptState::Cancelled))
                .then(|| j.operation_id.clone()),
            floor_cleanup_pending: j.cancelled_floors.is_some(),
            veto_check_incomplete: j.attempt.as_ref().is_some_and(|a| a.veto_page_full),
            retry_from_verified_head: j
                .attempt
                .as_ref()
                .is_some_and(crate::state::OwnerAttemptRec::can_retry_from_verified_head),
            approval: j
                .attempt
                .as_ref()
                .and_then(|a| crate::flows::approval_join_view(a, now_ms)),
            remaining_ms: j
                .attempt
                .as_ref()
                .filter(|a| a.state == crate::state::AttemptState::Waiting)
                .map(|a| {
                    let delay = crate::recovery::RecoveryPolicy::from_body(
                        &crate::crypto::unhex(&a.policy).unwrap_or_default(),
                    )
                    .map_or(crate::recovery::DEFAULT_DELAY_SECONDS, |p| p.delay_seconds)
                        * 1000;
                    delay.saturating_sub(a.obs.credited_ms)
                }),
        }),
        alerts: policy.as_ref().filter(|_| {
            committed
                && st.app.recovery_kit.is_some()
                && head.as_ref().zip(st.device.as_ref()).is_some_and(|(head, device)| {
                    head.control.contains(&device.device_id) && !head.quarantined.contains(&device.device_id)
                })
        }).map(|policy| st.app.recovery_inbox.status(policy.epoch(), now_ms)),
        earlier_code_used: st.app.earlier_recovery_code_used,
        record: match st.app.recovery_record_status(now_ms) {
            crate::state::RecoveryRecordStatus::Off => None,
            crate::state::RecoveryRecordStatus::FinishingSetup => {
                Some(crate::api::RecoveryRecordCode::FinishingSetup)
            }
            crate::state::RecoveryRecordStatus::Ready => {
                Some(crate::api::RecoveryRecordCode::Ready)
            }
            crate::state::RecoveryRecordStatus::NeedsAttention => {
                Some(crate::api::RecoveryRecordCode::NeedsAttention)
            }
            crate::state::RecoveryRecordStatus::NeedsNewKey => {
                Some(crate::api::RecoveryRecordCode::NeedsNewKey)
            }
        },
    }
}
