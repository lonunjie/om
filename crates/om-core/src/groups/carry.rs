// SPDX-License-Identifier: MPL-2.0
//! Group carriage, Package 2a: the glue between [`super::carriage`] (wire objects, routes, seals) and the group flow
//! ([`super::flow`]). Every function here either returns a verdict that leaves `AppState` exactly as it was (a drop),
//! or applies the whole step.
//!
//! Wired in 2a: the relationship lane (Invite, Accept, Decline as kinds 15 to 17), the Welcome frame (out and in), the
//! application frame (out and in) and the Commit frame (in). Not wired (Package 2b/3): Commit and LeaveRequest out
//! (they need the signed `GroupControlIntent` envelope, which Core does not produce yet), admit requests, route
//! updates on the wire, and the G10 acceptance rule. Their frames are fully encoded, sealed, opened and strictly
//! decoded by [`super::carriage`]; [`AppState::carry_frame_in`] drops them as `frame_kind_not_wired`.

use super::carriage::{
    self as cg, DeviceEvidence, FrameBody, LaneBody, LaneIn, MyRoute, Release, RouteRecord, Sealed,
};
use super::flow::{
    DeviceRelease, GroupCtx, GroupEvent, GroupReply, GroupWire, InviteDir, InviteState, Outbound,
    Sender, Target,
};
use super::{GroupError, GroupState, R, err};
use crate::crypto::binding::{Acs, Credential};
use crate::crypto::delivery;
use crate::crypto::rng::{CryptoRng, RngCore};
use crate::crypto::sig::DeviceSigner;
use crate::crypto::{hex, unhex};
use crate::route::{RelayEntry, RouteOffer};
use crate::state::{AppState, RelState};

/// What a carriage step needs from the device and the host besides `AppState`.
pub struct CarryEnv<'a, Rg: RngCore + CryptoRng> {
    pub signer: &'a DeviceSigner,
    pub device_id: [u8; 16],
    pub account_id: [u8; 32],
    pub rng: &'a mut Rg,
    /// Single-use route offers: each is a deposit capability the host's provider adapter minted with
    /// `POST /v1/relay/deposit-capabilities` on this device's own queue (ADR-0189 1.3).
    pub offers: &'a mut Vec<RouteOffer>,
    pub now_s: u64,
}

fn un(x: &str) -> R<Vec<u8>> {
    unhex(x).map_err(|_| GroupError("record_corrupt"))
}
fn un16(x: &str) -> R<[u8; 16]> {
    un(x)?.try_into().map_err(|_| GroupError("record_corrupt"))
}
fn invite_room(invite_id: &str) -> String {
    format!("invite:{invite_id}")
}

impl AppState {
    // ------------------------------------------------------------------------------------------ routes

    fn carry_take_offer<Rg: RngCore + CryptoRng>(
        env: &mut CarryEnv<'_, Rg>,
    ) -> R<([u8; 16], RelayEntry)> {
        let at = env
            .offers
            .iter()
            .enumerate()
            .filter(|(_, o)| o.is_valid())
            .min_by_key(|(i, o)| (o.priority, *i))
            .map(|(i, _)| i)
            .ok_or(GroupError("no_route_offer"))?;
        let o = env.offers.remove(at);
        let mailbox =
            <[u8; 16]>::try_from(o.mailbox.as_slice()).map_err(|_| GroupError("no_route_offer"))?;
        let mut e = RelayEntry::new(&o.relay_uri, &o.capability, env.now_s + cg::ROUTE_TTL_S)
            .ok_or(GroupError("no_route_offer"))?;
        e.priority = o.priority;
        Ok((mailbox, e))
    }

    /// Mint this device's route for `room`: a fresh delivery key bound to a fresh deposit capability from the host's
    /// offers, signed by this device (role `device`). Older routes of the room stay until retired. Returns the signed
    /// `GroupRouteRecord` to release.
    pub fn carry_route_mint<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        room: &str,
    ) -> R<Vec<u8>> {
        let (mailbox, entry) = Self::carry_take_offer(env)?;
        let (seed, km) = delivery::new_key(entry.binding(), env.now_s + cg::ROUTE_TTL_S, env.rng);
        let seed = zeroize::Zeroizing::new(seed);
        let rec = RouteRecord {
            device_id: env.device_id,
            delivery: km.clone(),
            relays: vec![entry.clone()],
        };
        let mut rnd = [0u8; 32];
        env.rng.fill_bytes(&mut rnd);
        let signed = rec.sign(env.signer, rnd)?;
        self.group_carriage.add_mine(
            room,
            MyRoute {
                key_id: hex(&km.key_id),
                seed: hex(&*seed).into(),
                mailbox: hex(&mailbox),
                entries: vec![entry],
                record: hex(&signed),
                expires_at: km.expires_at,
                minted_at: env.now_s,
                retired_at: None,
            },
        );
        Ok(signed)
    }

    /// The route to release for `room`: the newest one not retired, or a freshly minted one.
    fn carry_route_for<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        room: &str,
    ) -> R<Vec<u8>> {
        let cur = self
            .group_carriage
            .mine
            .get(room)
            .and_then(|v| {
                v.iter()
                    .rev()
                    .find(|r| r.retired_at.is_none() && r.expires_at > env.now_s)
            })
            .map(|r| r.record.clone());
        match cur {
            Some(r) => un(&r),
            None => self.carry_route_mint(env, room),
        }
    }

    /// Replace every route of `room` after a removal (ADR-0189 1.3): mint a fresh one, retire the older ones (they keep
    /// receiving for [`cg::ROUTE_OVERLAP_S`]) and return the fresh signed record to send as a `route_update` to the new
    /// roster. [`Self::carry_routes_expire`] later returns the capabilities to revoke.
    pub fn carry_route_replace<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        room: &str,
    ) -> R<Vec<u8>> {
        let fresh = self.carry_route_mint(env, room)?;
        self.group_carriage.retire_older(room, env.now_s);
        Ok(fresh)
    }

    /// Retired routes whose overlap is over: dropped here, returned so the host revokes each deposit capability.
    pub fn carry_routes_expire(&mut self, now_s: u64) -> Vec<RelayEntry> {
        self.group_carriage.expire_retired(now_s)
    }

    /// Mark that a removal happened in `conv`: this device owes a fresh route (set by the Core group steps).
    pub fn carry_note_removal(&mut self, conv: &[u8]) {
        let k = hex(conv);
        if !self.group_carriage.replace_owed.contains(&k) {
            self.group_carriage.replace_owed.push(k);
        }
    }

    // ------------------------------------------------------------------------------------------ evidence

    fn carry_own_credential_body(&self, device_id: &[u8; 16]) -> R<Vec<u8>> {
        self.acs_credentials
            .iter()
            .filter_map(|h| unhex(h).ok())
            .find(|b| Credential::from_body(b).is_ok_and(|c| &c.device_id == device_id))
            .ok_or(GroupError("no_own_credential"))
    }

    fn carry_own_credential<Rg: RngCore + CryptoRng>(
        &self,
        env: &mut CarryEnv<'_, Rg>,
    ) -> R<Vec<u8>> {
        let body = self.carry_own_credential_body(&env.device_id)?;
        let mut rnd = [0u8; 32];
        env.rng.fill_bytes(&mut rnd);
        Ok(cg::sign_device_credential(
            env.signer,
            &env.device_id,
            &body,
            rnd,
        ))
    }

    /// A credential is trusted when its body is one the validated head of `account` commits to: for the person on a
    /// relationship, the credentials held for that relationship; for ourselves, our own. A self-signature proves
    /// nothing and is not consulted. Returns the credential.
    fn carry_trusted_credential(&self, signed: &[u8], rel: Option<&str>) -> R<Credential> {
        let body = cg::envelope_body(signed)?;
        let cred = Credential::from_body(&body).map_err(|_| GroupError("malformed"))?;
        let hb = hex(&body);
        let mine = self.acs_credentials.contains(&hb);
        let theirs = rel
            .and_then(|r| self.relationships.get(r))
            .is_some_and(|r| {
                r.peer_credentials.contains(&hb) && hex(&cred.account_id) == r.peer_account_id
            });
        if mine || theirs {
            Ok(cred)
        } else {
            err("credential_not_committed")
        }
    }

    /// This account's current head as a `signed-AccountControlHead`. Only a control device can sign one (role
    /// `controller`): a messaging-only device has no head to give (`no_controller_head`, open question in the report).
    fn carry_own_head<Rg: RngCore + CryptoRng>(&self, env: &mut CarryEnv<'_, Rg>) -> R<Vec<u8>> {
        let state = self
            .acs_chain
            .last()
            .and_then(|h| unhex(h).ok())
            .and_then(|b| crate::crypto::cbor::decode(&b).ok())
            .and_then(|v| Acs::from_body(v.get(1).ok()?.b().ok()?).ok())
            .ok_or(GroupError("no_own_head"))?;
        if !state.control.contains(&env.device_id) {
            return err("no_controller_head");
        }
        let mut rnd = [0u8; 32];
        env.rng.fill_bytes(&mut rnd);
        Ok(cg::sign_account_head(
            env.signer,
            &env.device_id,
            &env.account_id,
            state.generation,
            &state.hash(),
            rnd,
        ))
    }

    // ------------------------------------------------------------------------------------------ lane: out

    /// The typed lane body for an Invite, Accept or Decline the flow produced for the relationship lane. An Accept
    /// releases this device only (releases of the person's other devices need own-device sync: Package 3). The invite
    /// no longer carries a ConversationID (ADR-0189 1.1).
    pub fn carry_lane_body<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        wire: &GroupWire,
    ) -> R<LaneBody> {
        match wire {
            GroupWire::Invite {
                invite_id, name, ..
            } => Ok(LaneBody::Invite {
                invite_id: un16(invite_id)?,
                room_name: name.clone(),
                member_count: None,
            }),
            GroupWire::Decline { invite_id } => Ok(LaneBody::Decline {
                invite_id: un16(invite_id)?,
            }),
            GroupWire::Accept {
                invite_id,
                releases,
            } => {
                let me = hex(&env.device_id);
                let own = releases
                    .iter()
                    .find(|r| r.device == me)
                    .ok_or(GroupError("no_own_release"))?;
                let route = self.carry_route_for(env, &invite_room(invite_id))?;
                let head = self.carry_own_head(env)?;
                let release = Release {
                    device: env.device_id,
                    key_package: un(&own.key_package)?,
                    credential: self.carry_own_credential(env)?,
                    route,
                };
                Ok(LaneBody::Accept {
                    invite_id: un16(invite_id)?,
                    releases: vec![release],
                    head,
                })
            }
            _ => err("wrong_channel"),
        }
    }

    // ------------------------------------------------------------------------------------------ lane: in

    /// A lane event that already passed the 1:1 authority checks (right conversation, authored by the pinned peer
    /// device). Verify it and queue what the flow should see. `Ok(())` always leaves a coherent state; a drop is an
    /// `Err` and changed nothing. Accept: the invite must be ours, outgoing and still offered on this relationship (an
    /// Accept for an unknown `invite_id` is dropped); every release's credential must be committed by the validated
    /// head, its route record must verify under that credential's key with role `device`, name the same device and be
    /// fresh (stale, wrong-device and wrong-role routes are dropped).
    pub fn carry_lane_in(
        &mut self,
        rel: &str,
        event_id: &[u8; 16],
        body: LaneBody,
        now_s: u64,
    ) -> R<()> {
        let eid = hex(event_id);
        if self
            .group_carriage
            .lane_in
            .iter()
            .any(|x| x.event_id == eid)
        {
            return Ok(());
        }
        if self.group_carriage.lane_in.len() >= cg::MAX_LANE_IN {
            return err("lane_full");
        }
        let wire = match &body {
            LaneBody::Invite {
                invite_id,
                room_name,
                ..
            } => GroupWire::Invite {
                invite_id: hex(invite_id),
                conversation_id: String::new(),
                name: room_name.clone(),
            },
            LaneBody::Decline { invite_id } => GroupWire::Decline {
                invite_id: hex(invite_id),
            },
            LaneBody::Accept {
                invite_id,
                releases,
                head,
            } => {
                let id = hex(invite_id);
                let ok = self.group_invites.get(&id).is_some_and(|i| {
                    i.dir == InviteDir::Outgoing
                        && i.state == InviteState::Offered
                        && i.relationship_id == rel
                });
                if !ok {
                    return err("accept_unknown_invite");
                }
                self.carry_check_peer_head(rel, head)?;
                let mut out = vec![];
                let mut learned = vec![];
                for r in releases {
                    let cred = self.carry_trusted_credential(&r.credential, Some(rel))?;
                    if cred.device_id != r.device {
                        return err("route_for_another_device");
                    }
                    let rec = RouteRecord::open(&r.route, &r.device, &cred.signing_pk, now_s)?;
                    out.push(DeviceRelease {
                        device: hex(&r.device),
                        key_package: hex(&r.key_package),
                        pk: hex(&cred.signing_pk),
                    });
                    learned.push((r, rec));
                }
                let room = invite_room(&id);
                for (r, rec) in learned {
                    self.group_carriage.learn_peer(
                        &room,
                        &r.device,
                        &rec,
                        &r.route,
                        &r.credential,
                    )?;
                }
                GroupWire::Accept {
                    invite_id: id,
                    releases: out,
                }
            }
        };
        self.group_carriage.lane_in.push(LaneIn {
            relationship_id: rel.to_string(),
            event_id: eid,
            wire,
        });
        Ok(())
    }

    /// The head an Accept carries must be signed (role `controller`) by a control device of the peer's account, name
    /// that account, and be the head this relationship has pinned (a stale or different head is dropped, never guessed).
    fn carry_check_peer_head(&self, rel: &str, head: &[u8]) -> R<()> {
        let r = self
            .relationships
            .get(rel)
            .ok_or(GroupError("unknown_relationship"))?;
        let pinned = unhex(&r.peer_acs_body)
            .ok()
            .and_then(|b| Acs::from_body(&b).ok())
            .ok_or(GroupError("no_peer_head"))?;
        let creds: Vec<Credential> = r
            .peer_credentials
            .iter()
            .filter_map(|h| unhex(h).ok())
            .filter_map(|b| Credential::from_body(&b).ok())
            .collect();
        let (account, _generation, state_hash) = cg::open_account_head(head, |d| {
            pinned
                .control
                .contains(d)
                .then(|| {
                    creds
                        .iter()
                        .find(|c| &c.device_id == d)
                        .map(|c| c.signing_pk.clone())
                })
                .flatten()
        })?;
        if hex(&account) != r.peer_account_id || state_hash != pinned.hash() {
            return err("head_mismatch");
        }
        Ok(())
    }

    /// Take what the lane has queued, oldest first. The host feeds each to `group_receive` as
    /// `Sender::Relationship(relationship_id)`.
    pub fn carry_lane_take(&mut self) -> Vec<LaneIn> {
        std::mem::take(&mut self.group_carriage.lane_in)
    }

    // ------------------------------------------------------------------------------------------ frames: out

    /// Seal a flow `Outbound` that travels on the device lane. `Welcome` (to the released devices of the invitee) and
    /// `App` (to one device) are wired in 2a; anything else is `frame_kind_not_wired`. A destination with no fresh route
    /// is `route_stale`/`route_missing`: the caller queues and refreshes, never falls back.
    pub fn carry_frame_out<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        out: &Outbound,
    ) -> R<Vec<Sealed>> {
        match (&out.to, &out.wire) {
            (
                Target::Relationship(_),
                GroupWire::Welcome {
                    invite_id,
                    welcome,
                    state,
                    directory,
                },
            ) => self.carry_welcome_out(env, invite_id, welcome, state, directory),
            (
                Target::Device { device, .. },
                GroupWire::App {
                    conversation_id,
                    ciphertext,
                },
            ) => {
                let frame = cg::encode_frame(&FrameBody::App(un(ciphertext)?))?;
                let pr = self
                    .group_carriage
                    .peer(conversation_id, device)
                    .ok_or(GroupError("route_missing"))?
                    .clone();
                let rec = self.carry_open_peer(conversation_id, device, &pr.record, env.now_s)?;
                let (wrapper, entry) = cg::seal_frame(&frame, &rec, env.now_s, env.rng)?;
                Ok(vec![Sealed {
                    device: device.clone(),
                    wrapper,
                    entry,
                }])
            }
            _ => err("frame_kind_not_wired"),
        }
    }

    /// Re-open a stored peer route against the credential it was verified with (freshness is re-checked at `now_s`).
    fn carry_open_peer(
        &self,
        room: &str,
        device: &[u8],
        record: &str,
        now_s: u64,
    ) -> R<RouteRecord> {
        let pr = self
            .group_carriage
            .peer(room, device)
            .ok_or(GroupError("route_missing"))?;
        let cred = Credential::from_body(&cg::envelope_body(&un(&pr.credential)?)?)
            .map_err(|_| GroupError("malformed"))?;
        RouteRecord::open(&un(record)?, device, &cred.signing_pk, now_s)
    }

    fn carry_welcome_out<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        invite_id: &str,
        welcome: &str,
        state: &str,
        directory: &[(String, String, String)],
    ) -> R<Vec<Sealed>> {
        let inv = self
            .group_invites
            .get(invite_id)
            .filter(|i| i.dir == InviteDir::Outgoing)
            .cloned()
            .ok_or(GroupError("unknown_invite"))?;
        let st = GroupState::decode(&un(state)?)?;
        let conv = hex(&st.conversation_id);
        let iroom = invite_room(invite_id);
        // evidence for every directory device: ours, or one the invitee released
        let mut routes = vec![];
        for (dev, _acct, _pk) in directory {
            let d = un16(dev)?;
            if d == env.device_id {
                let route = self.carry_route_for(env, &conv)?;
                routes.push(DeviceEvidence {
                    device: d,
                    credential: self.carry_own_credential(env)?,
                    route,
                });
            } else if let Some(pr) = self.group_carriage.peer(&iroom, &d) {
                routes.push(DeviceEvidence {
                    device: d,
                    credential: un(&pr.credential)?,
                    route: un(&pr.record)?,
                });
            } else {
                // another member's device: its credential chains to a head the invitee cannot check (open question)
                return err("member_evidence_missing");
            }
        }
        let mut rnd = [0u8; 32];
        env.rng.fill_bytes(&mut rnd);
        let signed_state = cg::sign_state(env.signer, &env.device_id, &st.encode(), rnd);
        let frame = cg::encode_frame(&FrameBody::Welcome {
            invite_id: un16(invite_id)?,
            welcome: un(welcome)?,
            state: signed_state,
            routes,
        })?;
        let mut sealed = vec![];
        for r in &inv.releases {
            let d = un(&r.device)?;
            let pr = self
                .group_carriage
                .peer(&iroom, &d)
                .ok_or(GroupError("route_missing"))?
                .clone();
            let rec = self.carry_open_peer(&iroom, &d, &pr.record, env.now_s)?;
            let (wrapper, entry) = cg::seal_frame(&frame, &rec, env.now_s, env.rng)?;
            sealed.push(Sealed {
                device: d,
                wrapper,
                entry,
            });
        }
        // the invitee's devices are Room members now: later frames to them use the same routes
        if let Some(m) = self.group_carriage.peers.get(&iroom).cloned() {
            self.group_carriage.peers.entry(conv).or_default().extend(m);
        }
        Ok(sealed)
    }

    // ------------------------------------------------------------------------------------------ frames: in

    /// Open and handle one sealed wrapper that arrived at one of this device's per-Room routes. Everything is checked
    /// before any group or MLS state is touched; any failure is `Err` (the Core step then discards the whole
    /// transaction, so nothing changed). Welcome: an accepted incoming invite must precede it, it must arrive on the
    /// route minted for that invite, the signed state must be signed by a device of the inviter's account whose
    /// credential the validated head commits to, and every member route must verify.
    pub fn carry_frame_in(
        &mut self,
        gctx: &mut GroupCtx<'_>,
        now_s: u64,
        wrapper: &[u8],
    ) -> R<GroupReply> {
        let key_id = cg::wrapper_key_id(wrapper).ok_or(GroupError("frame_invalid"))?;
        let (room, route) = self
            .group_carriage
            .mine_by_key(&key_id)
            .map(|(r, m)| (r.to_string(), m.clone()))
            .ok_or(GroupError("unknown_route"))?;
        let entry = route.entries.first().ok_or(GroupError("record_corrupt"))?;
        let seed: [u8; 32] = un(&route.seed)?
            .try_into()
            .map_err(|_| GroupError("record_corrupt"))?;
        let seed = zeroize::Zeroizing::new(seed);
        let opened = delivery::open(
            wrapper,
            &seed,
            &key_id,
            &entry.relay_uri,
            &entry.capability,
            now_s,
        )
        .map_err(|_| GroupError("frame_invalid"))?;
        match cg::decode_frame(&opened.inner_envelope)? {
            FrameBody::Welcome {
                invite_id,
                welcome,
                state,
                routes,
            } => self.carry_welcome_in(gctx, now_s, &room, invite_id, welcome, state, routes),
            FrameBody::App(ct) => {
                let wire = GroupWire::App {
                    conversation_id: room,
                    ciphertext: hex(&ct),
                };
                self.group_receive(gctx, Sender::Device(vec![]), wire)
            }
            FrameBody::Commit {
                intent,
                binding,
                commit,
                added,
            } => {
                // The committer and the actor are named by the signatures inside, never by the frame.
                if !added.is_empty() {
                    return err("added_evidence_unverifiable");
                }
                let conv = un(&room)?;
                let committer = cg::envelope_signer(&binding).ok_or(GroupError("malformed"))?;
                let actor = cg::envelope_signer(&intent).ok_or(GroupError("malformed"))?;
                let known = self.group_devices.get(&room);
                if known.is_none_or(|d| {
                    !d.contains_key(&hex(&committer)) || !d.contains_key(&hex(&actor))
                }) {
                    return err("non_roster_sender");
                }
                let wire = GroupWire::Commit {
                    conversation_id: hex(&conv),
                    committer_device: hex(&committer),
                    actor_device: hex(&actor),
                    signed_binding: hex(&binding),
                    commit: hex(&commit),
                    intent: hex(&cg::envelope_body(&intent)?),
                    added: vec![],
                };
                self.group_receive(gctx, Sender::Device(committer.to_vec()), wire)
            }
            _ => err("frame_kind_not_wired"),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn carry_welcome_in(
        &mut self,
        gctx: &mut GroupCtx<'_>,
        now_s: u64,
        room: &str,
        invite_id: [u8; 16],
        welcome: Vec<u8>,
        state: Vec<u8>,
        routes: Vec<DeviceEvidence>,
    ) -> R<GroupReply> {
        let id = hex(&invite_id);
        if room != invite_room(&id) {
            return err("route_for_another_room");
        }
        let inv = self
            .group_invites
            .get(&id)
            .filter(|i| i.dir == InviteDir::Incoming && i.state == InviteState::Accepted)
            .cloned()
            .ok_or(GroupError("welcome_without_accepted_invite"))?;
        let rel = inv.relationship_id.clone();
        let rel_ok = self
            .relationships
            .get(&rel)
            .is_some_and(|r| r.state == RelState::Active && !r.blocked);
        if !rel_ok {
            return err("relationship_not_active");
        }
        let mut verified = vec![];
        let mut dir = vec![];
        for e in &routes {
            let cred = self.carry_trusted_credential(&e.credential, Some(&rel))?;
            if cred.device_id != e.device {
                return err("route_for_another_device");
            }
            let rec = RouteRecord::open(&e.route, &e.device, &cred.signing_pk, now_s)?;
            dir.push((hex(&e.device), hex(&cred.account_id), hex(&cred.signing_pk)));
            verified.push((e.clone(), rec));
        }
        let (signer, body) = cg::open_state(&state, |d| {
            verified
                .iter()
                .find(|(e, _)| &e.device == d)
                .and_then(|(e, _)| {
                    Credential::from_body(&cg::envelope_body(&e.credential).ok()?)
                        .ok()
                        .map(|c| c.signing_pk)
                })
        })?;
        let inviter = un(&inv.peer_account)?;
        let from_inviter = dir
            .iter()
            .any(|(d, a, _)| *d == hex(&signer) && *a == hex(&inviter));
        if !from_inviter {
            return err("state_signer_not_inviter");
        }
        let wire = GroupWire::Welcome {
            invite_id: id.clone(),
            welcome: hex(&welcome),
            state: hex(&body),
            directory: dir,
        };
        let rep = self.group_receive(gctx, Sender::Relationship(rel), wire)?;
        let conv = rep.events.iter().find_map(|e| match e {
            GroupEvent::Joined { conversation_id } => Some(hex(conversation_id)),
            _ => None,
        });
        if let Some(conv) = conv {
            self.group_carriage.rekey(room, &conv);
            for (e, rec) in &verified {
                if e.device[..] != *gctx.engine.device_id() {
                    self.group_carriage.learn_peer(
                        &conv,
                        &e.device,
                        rec,
                        &e.route,
                        &e.credential,
                    )?;
                }
            }
        }
        Ok(rep)
    }
}
