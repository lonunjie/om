// SPDX-License-Identifier: MPL-2.0
//! Group carriage, Package 2a: the glue between [`super::carriage`] (wire objects, routes, seals) and the group flow
//! ([`super::flow`]). Every function here either returns a verdict that leaves `AppState` exactly as it was (a drop),
//! or applies the whole step.
//!
//! Wired: the relationship lane (Invite, Accept, Decline as kinds 15 to 17), and every device-lane frame except the
//! admit request: Welcome, application, Commit and LeaveRequest (signed `GroupControlIntent`, type 29), and route
//! updates, in and out. Package 2b added the Core-owned outbox and the G10 acceptance rule (`group_settle`), the
//! pending-Welcome/Commit store (D37, bounded and expiring), and messaging-only devices (D36, by the account's
//! validated head). `admit_request` is not wired: it needs an anchor for the head of an account the device has no
//! relationship with (report 2026-11-14-groups-carriage-status.md, question Q5); [`AppState::carry_frame_in`] drops it
//! as `frame_kind_not_wired`.

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

    /// The tip of this account's own validated chain.
    fn carry_own_tip(&self) -> R<Acs> {
        self.acs_chain
            .last()
            .and_then(|h| unhex(h).ok())
            .and_then(|b| crate::crypto::cbor::decode(&b).ok())
            .and_then(|v| Acs::from_body(v.get(1).ok()?.b().ok()?).ok())
            .ok_or(GroupError("no_own_head"))
    }

    /// Check a `signed-AccountControlHead` against this account's own validated chain tip: role `controller`, signed by
    /// a control device of the tip whose credential the tip commits to, naming this account, the tip's generation and
    /// hash. Nothing else is accepted, so a stale or foreign head is never cached or presented.
    fn carry_own_head_ok(&self, head: &[u8]) -> bool {
        let Ok(tip) = self.carry_own_tip() else {
            return false;
        };
        let creds: Vec<Credential> = self
            .acs_credentials
            .iter()
            .filter_map(|h| unhex(h).ok())
            .filter_map(|b| Credential::from_body(&b).ok())
            .collect();
        let Ok((account, generation, state_hash)) = cg::open_account_head(head, |d| {
            if !tip.control.contains(d) || tip.quarantined.contains(d) {
                return None;
            }
            creds
                .iter()
                .find(|c| {
                    &c.device_id == d
                        && tip
                            .credentials
                            .iter()
                            .any(|(x, h)| x == d && *h == c.hash())
                })
                .map(|c| c.signing_pk.clone())
        }) else {
            return false;
        };
        account == tip.account_id && generation == tip.generation && state_hash == tip.hash()
    }

    /// A control device signs the head of its chain tip (role `controller`) to hand to the account's messaging devices
    /// with the enrollment notice. `None` for a device that is not in the tip's active control set.
    pub fn carry_sign_head_for_peers<Rg: RngCore + CryptoRng>(
        &self,
        signer: &DeviceSigner,
        device_id: &[u8; 16],
        tip: &Acs,
        rng: &mut Rg,
    ) -> Option<Vec<u8>> {
        if !tip.control.contains(device_id) || tip.quarantined.contains(device_id) {
            return None;
        }
        let mut rnd = [0u8; 32];
        rng.fill_bytes(&mut rnd);
        Some(cg::sign_account_head(
            signer,
            device_id,
            &tip.account_id,
            tip.generation,
            &tip.hash(),
            rnd,
        ))
    }

    /// A messaging device keeps the newest head a control device gave it (D36). Only a head that passes
    /// [`Self::carry_own_head_ok`] against the current own tip is kept; anything else is ignored.
    pub fn carry_head_adopt(&mut self, head: &[u8]) -> bool {
        if !self.carry_own_head_ok(head) {
            return false;
        }
        self.group_carriage.head_cache = Some(hex(head));
        true
    }

    /// This account's current head as a `signed-AccountControlHead`. A control device signs one (role `controller`). A
    /// messaging-only device has no controller key: it presents the head a control device gave it, but only while that
    /// head is still the tip of its own chain, and otherwise fails closed (`no_controller_head`; it joins once the next
    /// enrollment notice or chain advance brings a fresh head). No new authority: the signer is always a control
    /// device of the account (object type 2 allows only the role `controller`).
    fn carry_own_head<Rg: RngCore + CryptoRng>(&self, env: &mut CarryEnv<'_, Rg>) -> R<Vec<u8>> {
        let state = self.carry_own_tip()?;
        if !state.control.contains(&env.device_id) {
            return match self
                .group_carriage
                .head_cache
                .as_deref()
                .and_then(|h| unhex(h).ok())
            {
                Some(h) if self.carry_own_head_ok(&h) => Ok(h),
                _ => err("no_controller_head"),
            };
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

    /// Seal a flow `Outbound` that travels on the device lane: Welcome (to the released devices of the invitee), App,
    /// Commit and LeaveRequest (each to one member device). A destination with no fresh route is
    /// `route_stale`/`route_missing`: the caller queues and refreshes, never falls back. `admit_request` is not wired
    /// (see the report, question Q5).
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
                self.carry_seal_to(env, conversation_id, device, &frame)
            }
            (
                Target::Device { device, .. },
                GroupWire::Commit {
                    conversation_id,
                    signed_binding,
                    commit,
                    added,
                    signed_intent,
                    ..
                },
            ) => {
                let intent = un(signed_intent
                    .as_deref()
                    .ok_or(GroupError("intent_unsigned"))?)?;
                let mut evidence = vec![];
                for (d, _, _) in added {
                    let d = un(d)?;
                    let pr = self
                        .carry_invite_peer(&d)
                        .ok_or(GroupError("member_evidence_missing"))?;
                    evidence.push(DeviceEvidence {
                        device: d
                            .as_slice()
                            .try_into()
                            .map_err(|_| GroupError("record_corrupt"))?,
                        credential: un(&pr.credential)?,
                        route: un(&pr.record)?,
                    });
                }
                let frame = cg::encode_frame(&FrameBody::Commit {
                    intent,
                    binding: un(signed_binding)?,
                    commit: un(commit)?,
                    added: evidence,
                })?;
                self.carry_seal_to(env, conversation_id, device, &frame)
            }
            (
                Target::Device { device, .. },
                GroupWire::LeaveRequest {
                    conversation_id,
                    signed,
                    ..
                },
            ) => {
                let intent = un(signed.as_deref().ok_or(GroupError("intent_unsigned"))?)?;
                let frame = cg::encode_frame(&FrameBody::LeaveRequest { intent })?;
                self.carry_seal_to(env, conversation_id, device, &frame)
            }
            (
                Target::Device { device, .. },
                GroupWire::RouteUpdate {
                    conversation_id,
                    routes,
                },
            ) => {
                let routes = routes.iter().map(|r| un(r)).collect::<R<Vec<_>>>()?;
                let frame = cg::encode_frame(&FrameBody::RouteUpdate { routes })?;
                self.carry_seal_to(env, conversation_id, device, &frame)
            }
            _ => err("frame_kind_not_wired"),
        }
    }

    /// Seal `frame` to the stored per-Room route of `device`.
    fn carry_seal_to<Rg: RngCore + CryptoRng>(
        &self,
        env: &mut CarryEnv<'_, Rg>,
        room: &str,
        device: &[u8],
        frame: &[u8],
    ) -> R<Vec<Sealed>> {
        let pr = self
            .group_carriage
            .peer(room, device)
            .ok_or(GroupError("route_missing"))?
            .clone();
        let rec = self.carry_open_peer(room, device, &pr.record, env.now_s)?;
        let (wrapper, entry) = cg::seal_frame(frame, &rec, env.now_s, env.rng)?;
        Ok(vec![Sealed {
            device: device.to_vec(),
            wrapper,
            entry,
        }])
    }

    /// The route the invitee released for a device, kept under the invite's room until the Welcome gives the Room.
    fn carry_invite_peer(&self, device: &[u8]) -> Option<&cg::PeerRoute> {
        self.group_carriage
            .peers
            .iter()
            .filter(|(room, _)| room.starts_with("invite:"))
            .find_map(|(room, _)| self.group_carriage.peer(room, device))
    }

    /// Mint a fresh route for `conv` and return one `route_update` outbound per other roster device (ADR-0189 1.3:
    /// after a removal, each remaining device mints a fresh route and tells the new roster). Seal each with
    /// [`Self::carry_frame_out`].
    pub fn carry_route_update_out<Rg: RngCore + CryptoRng>(
        &mut self,
        env: &mut CarryEnv<'_, Rg>,
        conv: &str,
    ) -> R<Vec<Outbound>> {
        let fresh = self.carry_route_replace(env, conv)?;
        let me = hex(&env.device_id);
        Ok(self
            .group_devices
            .get(conv)
            .map(|d| {
                d.iter()
                    .filter(|(dev, _)| **dev != me)
                    .filter_map(|(dev, r)| {
                        Some(Outbound {
                            to: Target::Device {
                                account: unhex(&r.account).ok()?,
                                device: unhex(dev).ok()?,
                            },
                            wire: GroupWire::RouteUpdate {
                                conversation_id: conv.to_string(),
                                routes: vec![hex(&fresh)],
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default())
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
        // evidence for every directory device: ours, one the invitee released, or one a member released earlier
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
            } else if let Some(pr) = self
                .group_carriage
                .peer(&iroom, &d)
                .or_else(|| self.group_carriage.peer(&conv, &d))
            {
                routes.push(DeviceEvidence {
                    device: d,
                    credential: un(&pr.credential)?,
                    route: un(&pr.record)?,
                });
            } else {
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

    // ------------------------------------------------------------------------------------------ outbox and G10

    /// Put the device-lane frames of a step's reply into the Core outbox, in order, in the same transaction as the
    /// step. When the reply prepared a Commit, its journal entry records how many frames were queued
    /// (`queued`, G10 condition 1). The relationship lane (Invite, Accept, Decline) is not here: it rides the 1:1
    /// queue, which is already durable. A full outbox refuses the step (fail closed).
    pub fn carry_outbox_add(&mut self, rep: &GroupReply) -> R<()> {
        let intent = rep.events.iter().find_map(|e| match e {
            GroupEvent::CommitPrepared { intent_id } => Some(intent_id.clone()),
            _ => None,
        });
        let mut n = 0u32;
        for o in &rep.out {
            let is_commit = matches!(o.wire, GroupWire::Commit { .. });
            if !matches!(
                o.wire,
                GroupWire::Commit { .. }
                    | GroupWire::LeaveRequest { .. }
                    | GroupWire::App { .. }
                    | GroupWire::Welcome { .. }
                    | GroupWire::RouteUpdate { .. }
            ) {
                continue;
            }
            if self.group_carriage.outbox.len() >= cg::MAX_OUTBOX {
                return err("outbox_full");
            }
            let to = match &o.to {
                Target::Relationship(r) => cg::OutTarget::Relationship(r.clone()),
                Target::Device { account, device } => cg::OutTarget::Device {
                    account: hex(account),
                    device: hex(device),
                },
            };
            let id = self.group_carriage.next_out;
            self.group_carriage.next_out += 1;
            self.group_carriage.outbox.push(cg::OutItem {
                id,
                intent_id: if is_commit { intent.clone() } else { None },
                to,
                wire: o.wire.clone(),
            });
            if is_commit {
                n += 1;
            }
        }
        if let Some(id) = intent
            && let Some(e) = self.group_journal.get_mut(&id)
        {
            e.queued = Some(n);
        }
        Ok(())
    }

    /// The host deposited these outbox frames: drop them (transport progress, never acceptance).
    pub fn carry_outbox_done(&mut self, ids: &[u64]) {
        self.group_carriage.outbox.retain(|o| !ids.contains(&o.id));
    }

    /// G10 (ADR-0189 section 2): accept this device's own Commit at once when (1) the exact Commit bytes, binding and
    /// intent are in the journal and every destination's frames are queued in the outbox, in order (`queued`, set by
    /// the step that prepared it, which the host has made durable before calling this); (2) the parent is still
    /// canonical (checked by the accept itself); (3) no valid Commit with the same parent that sorts before it was
    /// received (such a Commit already superseded this entry, so it is no longer open). No timer, no relay or
    /// recipient acknowledgement, no relay order. A candidate whose parent moved is `SUPERSEDED`
    /// ([`GroupEvent::CommitSuperseded`]); its tentative state is never merged. Returns one reply per Commit finished.
    pub fn group_settle(&mut self, gctx: &mut GroupCtx<'_>) -> R<Vec<GroupReply>> {
        let open: Vec<String> = self
            .group_journal
            .values()
            .filter(|e| {
                e.queued.is_some()
                    && e.candidate.is_some()
                    && matches!(
                        e.state,
                        super::journal::JournalState::CandidatePrepared
                            | super::journal::JournalState::PublishedUnconfirmed
                    )
            })
            .map(|e| e.intent_id.clone())
            .collect();
        let mut all = vec![];
        for id in open {
            let rep = match self.group_do(
                gctx,
                super::flow::GroupIntent::CommitAccepted {
                    intent_id: id.clone(),
                },
            ) {
                Ok(r) => r,
                Err(GroupError("stale_parent")) => {
                    self.group_journal_supersede(&id, &mut *gctx.engine, "parent_moved")?;
                    GroupReply {
                        events: vec![GroupEvent::CommitSuperseded { intent_id: id }],
                        out: vec![],
                    }
                }
                Err(e) => return Err(e),
            };
            self.carry_outbox_add(&rep)?;
            all.push(rep);
        }
        Ok(all)
    }

    // ------------------------------------------------------------------------------------------ trust and pending

    /// What the validated heads this device holds say about a signed `DeviceCredential`. A credential is trusted only
    /// when its body is one a validated head commits to: this account's own chain, or the pinned head of a relationship
    /// with that account. A self-signature proves nothing and is never consulted. `Hold` is not a refusal: the head
    /// that would commit it may be stale (a relationship exists, so the existing K7 refresh can supply it) or unknown
    /// (no relationship: nothing in the design says where to anchor it, report question Q5).
    fn carry_trust(&self, signed: &[u8]) -> R<Trust> {
        let body = cg::envelope_body(signed)?;
        let cred = Credential::from_body(&body).map_err(|_| GroupError("malformed"))?;
        let hb = hex(&body);
        if self.acs_credentials.contains(&hb) {
            return Ok(Trust::Ok(cred));
        }
        let own = self
            .acs_credentials
            .iter()
            .filter_map(|h| unhex(h).ok())
            .find_map(|b| Credential::from_body(&b).ok())
            .map(|c| c.account_id);
        if own == Some(cred.account_id) {
            return err("credential_not_committed");
        }
        let acct = hex(&cred.account_id);
        let rels: Vec<&crate::state::Relationship> = self
            .relationships
            .values()
            .filter(|r| r.peer_account_id == acct && r.state == RelState::Active && !r.blocked)
            .collect();
        if rels.iter().any(|r| r.peer_credentials.contains(&hb)) {
            return Ok(Trust::Ok(cred));
        }
        if rels.is_empty() {
            Ok(Trust::Hold {
                refresh: vec![],
                unanchored: vec![acct],
            })
        } else {
            Ok(Trust::Hold {
                refresh: rels.iter().map(|r| r.id.clone()).collect(),
                unanchored: vec![],
            })
        }
    }

    /// Keep `wrapper` until a head arrives. Bounded ([`cg::MAX_PENDING`]) and expiring ([`cg::PENDING_TTL_S`]); the
    /// same wrapper again is one entry. `since` keeps the original age across a retry that holds again.
    fn carry_hold(
        &mut self,
        wrapper: &[u8],
        now_s: u64,
        since: Option<u64>,
        refresh: Vec<String>,
        unanchored: Vec<String>,
    ) -> R<GroupReply> {
        let w = hex(wrapper);
        let list = &mut self.group_carriage.pending;
        match list.iter_mut().find(|p| p.wrapper == w) {
            Some(p) => {
                p.refresh = refresh;
                p.unanchored = unanchored;
            }
            None => {
                if list.len() >= cg::MAX_PENDING {
                    return err("pending_full");
                }
                list.push(cg::PendingFrame {
                    wrapper: w,
                    since_s: since.unwrap_or(now_s),
                    refresh,
                    unanchored,
                });
            }
        }
        Ok(GroupReply {
            events: vec![GroupEvent::Held("account_head_missing")],
            out: vec![],
        })
    }

    /// The relationships whose head refresh would help a held frame: the host sends the existing K7 head request for
    /// each (`Core::group_pending_refresh`).
    pub fn carry_pending_wants(&self) -> Vec<String> {
        let mut v: Vec<String> = self
            .group_carriage
            .pending
            .iter()
            .flat_map(|p| p.refresh.iter().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    }

    /// Retry every held frame (a head may have arrived) after dropping the expired ones. A frame that verifies is
    /// applied; one that still lacks a head stays held with its original age; one that now fails is dropped.
    pub fn carry_pending_retry(&mut self, gctx: &mut GroupCtx<'_>, now_s: u64) -> Vec<GroupReply> {
        self.group_carriage
            .pending
            .retain(|p| p.since_s.saturating_add(cg::PENDING_TTL_S) > now_s);
        let held = std::mem::take(&mut self.group_carriage.pending);
        let mut out = vec![];
        for p in held {
            let Ok(w) = unhex(&p.wrapper) else { continue };
            // a failed attempt must not leave half a change: apply on a copy, keep it only on success
            let mut trial = self.clone();
            // an Err is a frame that is now invalid (for example the invitation is gone): dropped
            if let Ok(rep) = trial.carry_frame_in_at(gctx, now_s, &w, Some(p.since_s)) {
                *self = trial;
                out.push(rep);
            }
        }
        out
    }

    // ------------------------------------------------------------------------------------------ frames: in

    /// Open and handle one sealed wrapper that arrived at one of this device's per-Room routes. Everything is checked
    /// before any group or MLS state is touched; any failure is `Err` (the Core step then discards the whole
    /// transaction, so nothing changed). A frame that names a device of an account whose head is missing or stale is
    /// held ([`GroupEvent::Held`]), never accepted unverified.
    pub fn carry_frame_in(
        &mut self,
        gctx: &mut GroupCtx<'_>,
        now_s: u64,
        wrapper: &[u8],
    ) -> R<GroupReply> {
        self.carry_frame_in_at(gctx, now_s, wrapper, None)
    }

    fn carry_frame_in_at(
        &mut self,
        gctx: &mut GroupCtx<'_>,
        now_s: u64,
        wrapper: &[u8],
        since: Option<u64>,
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
        let held = Held {
            wrapper,
            since,
            now_s,
        };
        match cg::decode_frame(&opened.inner_envelope)? {
            FrameBody::Welcome {
                invite_id,
                welcome,
                state,
                routes,
            } => self.carry_welcome_in(gctx, &held, &room, invite_id, welcome, state, routes),
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
            } => self.carry_commit_in(gctx, &held, &room, intent, binding, commit, added),
            FrameBody::LeaveRequest { intent } => {
                let dir = self.group_devices.get(&room).cloned().unwrap_or_default();
                let (signer, body) = cg::open_intent(&intent, |d| {
                    dir.get(&hex(d)).and_then(|r| unhex(&r.pk).ok())
                })
                .map_err(|_| GroupError("non_roster_sender"))?;
                let wire = GroupWire::LeaveRequest {
                    conversation_id: room,
                    intent: hex(&body),
                    signed: Some(hex(&intent)),
                };
                self.group_receive(gctx, Sender::Device(signer.to_vec()), wire)
            }
            FrameBody::RouteUpdate { routes } => self.carry_route_update_in(now_s, &room, routes),
            _ => err("frame_kind_not_wired"),
        }
    }

    /// `route_update`: each record must be signed (role `device`) by a device of this Room's roster, name that device
    /// and be fresh and newer than the stored one. Nothing applies unless every record verifies.
    fn carry_route_update_in(
        &mut self,
        now_s: u64,
        room: &str,
        routes: Vec<Vec<u8>>,
    ) -> R<GroupReply> {
        let dir = self.group_devices.get(room).cloned().unwrap_or_default();
        let mut learned = vec![];
        for r in &routes {
            let dev = cg::envelope_signer(r).ok_or(GroupError("malformed"))?;
            let pk = dir
                .get(&hex(&dev))
                .and_then(|d| unhex(&d.pk).ok())
                .ok_or(GroupError("non_roster_sender"))?;
            let cred = self
                .group_carriage
                .peer(room, &dev)
                .map(|p| p.credential.clone())
                .ok_or(GroupError("route_missing"))?;
            let rec = RouteRecord::open(r, &dev, &pk, now_s)?;
            learned.push((dev, rec, r.clone(), un(&cred)?));
        }
        for (dev, rec, r, cred) in learned {
            self.group_carriage
                .learn_peer(room, &dev, &rec, &r, &cred)?;
        }
        Ok(GroupReply::default())
    }

    #[allow(clippy::too_many_arguments)]
    fn carry_commit_in(
        &mut self,
        gctx: &mut GroupCtx<'_>,
        held: &Held<'_>,
        room: &str,
        intent: Vec<u8>,
        binding: Vec<u8>,
        commit: Vec<u8>,
        added: Vec<DeviceEvidence>,
    ) -> R<GroupReply> {
        // The committer and the actor are named by the signatures inside, never by the frame: the intent must verify
        // under the key our own directory holds for a roster device.
        let conv = un(room)?;
        let dir = self.group_devices.get(room).cloned().unwrap_or_default();
        let committer = cg::envelope_signer(&binding).ok_or(GroupError("malformed"))?;
        let (actor, body) = cg::open_intent(&intent, |d| {
            dir.get(&hex(d)).and_then(|r| unhex(&r.pk).ok())
        })
        .map_err(|_| GroupError("non_roster_sender"))?;
        if !dir.contains_key(&hex(&committer)) {
            return err("non_roster_sender");
        }
        // Evidence for added devices: every credential chains to a validated head, every route verifies.
        let (mut ok, mut refresh, mut unanchored) = (vec![], vec![], vec![]);
        for e in &added {
            match self.carry_trust(&e.credential)? {
                Trust::Ok(cred) => {
                    if cred.device_id != e.device {
                        return err("route_for_another_device");
                    }
                    let rec = RouteRecord::open(&e.route, &e.device, &cred.signing_pk, held.now_s)?;
                    ok.push((e.clone(), cred, rec));
                }
                Trust::Hold {
                    refresh: r,
                    unanchored: u,
                } => {
                    refresh.extend(r);
                    unanchored.extend(u);
                }
            }
        }
        if !refresh.is_empty() || !unanchored.is_empty() {
            refresh.sort();
            refresh.dedup();
            unanchored.sort();
            unanchored.dedup();
            return self.carry_hold(held.wrapper, held.now_s, held.since, refresh, unanchored);
        }
        let wire = GroupWire::Commit {
            conversation_id: hex(&conv),
            committer_device: hex(&committer),
            actor_device: hex(&actor),
            signed_binding: hex(&binding),
            commit: hex(&commit),
            intent: hex(&body),
            added: ok
                .iter()
                .map(|(e, c, _)| (hex(&e.device), hex(&c.account_id), hex(&c.signing_pk)))
                .collect(),
            signed_intent: Some(hex(&intent)),
        };
        let rep = self.group_receive(gctx, Sender::Device(committer.to_vec()), wire)?;
        // routes of the devices the accepted Commit added (only if it registered them)
        for (e, _c, rec) in &ok {
            if self
                .group_devices
                .get(room)
                .is_some_and(|d| d.contains_key(&hex(&e.device)))
            {
                self.group_carriage
                    .learn_peer(room, &e.device, rec, &e.route, &e.credential)?;
            }
        }
        Ok(rep)
    }

    #[allow(clippy::too_many_arguments)]
    fn carry_welcome_in(
        &mut self,
        gctx: &mut GroupCtx<'_>,
        held: &Held<'_>,
        room: &str,
        invite_id: [u8; 16],
        welcome: Vec<u8>,
        state: Vec<u8>,
        routes: Vec<DeviceEvidence>,
    ) -> R<GroupReply> {
        let now_s = held.now_s;
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
        let (mut verified, mut dir) = (vec![], vec![]);
        let (mut refresh, mut unanchored) = (vec![], vec![]);
        for e in &routes {
            match self.carry_trust(&e.credential)? {
                Trust::Ok(cred) => {
                    if cred.device_id != e.device {
                        return err("route_for_another_device");
                    }
                    let rec = RouteRecord::open(&e.route, &e.device, &cred.signing_pk, now_s)?;
                    dir.push((hex(&e.device), hex(&cred.account_id), hex(&cred.signing_pk)));
                    verified.push((e.clone(), rec));
                }
                Trust::Hold {
                    refresh: r,
                    unanchored: u,
                } => {
                    refresh.extend(r);
                    unanchored.extend(u);
                }
            }
        }
        if !refresh.is_empty() || !unanchored.is_empty() {
            refresh.sort();
            refresh.dedup();
            unanchored.sort();
            unanchored.dedup();
            return self.carry_hold(held.wrapper, now_s, held.since, refresh, unanchored);
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

/// The verdict of [`AppState::carry_trust`].
enum Trust {
    Ok(Credential),
    Hold {
        refresh: Vec<String>,
        unanchored: Vec<String>,
    },
}

/// What a step needs to hold the wrapper it is handling.
struct Held<'a> {
    wrapper: &'a [u8],
    since: Option<u64>,
    now_s: u64,
}
