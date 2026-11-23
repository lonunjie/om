// SPDX-License-Identifier: MPL-2.0
//! Offline first contact over the LAN channel (`local_delivery` slice 5; LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE.md,
//! ADR-0158, V-024 offline-first-contact model, R-350). Sans-IO state machine over evidence the caller already verified.
//!
//! The only route to a relationship is the ordinary IntroGate flow run locally: invite capability, an intro-identity-signed
//! ticket and request key, an ordinary MessageRequest whose reply route is the ephemeral LAN RelayEntry, then reciprocal
//! IntroIdentityBinding on both sides. LAN reachability alone never creates a relationship or device eligibility, and
//! asynchronous key fetch stays denied until the relationship is Active.

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum State {
    /// Nothing known about the peer.
    Unknown,
    /// A LAN tag from our invite matched: reachability only.
    Reachable,
    /// A verified intro-identity-signed ticket and request key.
    Ticketed,
    /// A valid ordinary MessageRequest was exchanged.
    Requested,
    /// One side's IntroIdentityBinding verified.
    HalfBound,
    /// Both bindings verified: an ordinary relationship exists.
    Active,
}

#[derive(Debug, PartialEq, Eq)]
pub enum StepError {
    /// The evidence arrived before the stage it depends on.
    OutOfOrder,
    /// The evidence did not verify.
    NotVerified,
}

#[derive(Debug, Default)]
pub struct FirstContact {
    invite_match: bool,
    ticket: bool,
    request: bool,
    own_binding: bool,
    peer_binding: bool,
}

impl FirstContact {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> State {
        if self.own_binding && self.peer_binding {
            State::Active
        } else if self.own_binding || self.peer_binding {
            State::HalfBound
        } else if self.request {
            State::Requested
        } else if self.ticket {
            State::Ticketed
        } else if self.invite_match {
            State::Reachable
        } else {
            State::Unknown
        }
    }

    fn need(ok: bool) -> Result<(), StepError> {
        if ok {
            Ok(())
        } else {
            Err(StepError::OutOfOrder)
        }
    }
    fn verified(v: bool) -> Result<(), StepError> {
        if v {
            Ok(())
        } else {
            Err(StepError::NotVerified)
        }
    }

    /// A rendezvous tag derived from our invite secret matched.
    pub fn reach(&mut self) {
        self.invite_match = true;
    }

    /// The local IntroGate adapter returned a ticket and request key; `signature_verified` is the intro-identity check.
    pub fn ticket(&mut self, signature_verified: bool) -> Result<(), StepError> {
        Self::need(self.invite_match)?;
        Self::verified(signature_verified)?;
        self.ticket = true;
        Ok(())
    }

    /// An ordinary MessageRequest, valid and addressed to the ticketed request key.
    pub fn request(&mut self, valid: bool) -> Result<(), StepError> {
        Self::need(self.ticket)?;
        Self::verified(valid)?;
        self.request = true;
        Ok(())
    }

    /// Our own reciprocal IntroIdentityBinding verified.
    pub fn own_binding(&mut self, verified: bool) -> Result<(), StepError> {
        Self::need(self.request)?;
        Self::verified(verified)?;
        self.own_binding = true;
        Ok(())
    }

    /// The peer's reciprocal IntroIdentityBinding verified.
    pub fn peer_binding(&mut self, verified: bool) -> Result<(), StepError> {
        Self::need(self.request)?;
        Self::verified(verified)?;
        self.peer_binding = true;
        Ok(())
    }

    /// Reachability was lost (tag retired, link gone). Before Active the attempt starts over; an Active relationship is an
    /// ordinary one and does not depend on the LAN.
    pub fn reachability_lost(&mut self) {
        if self.state() != State::Active {
            *self = Self::default();
        }
    }

    /// Relationship-scoped signed AsyncKeyBundle fetch: only once the relationship is Active.
    pub fn async_key_fetch_allowed(&self) -> bool {
        self.state() == State::Active
    }

    /// Device eligibility follows the ordinary relationship, never LAN reachability.
    pub fn device_eligible(&self) -> bool {
        self.state() == State::Active
    }
}
