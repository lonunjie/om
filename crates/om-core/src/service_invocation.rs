// SPDX-License-Identifier: MPL-2.0
//! Invocation-only service path (`services_bots` slice 3; SERVICE_PRINCIPALS_BOTS_AGENTS_AND_LLMS.md "Data-access modes"
//! 1, "ContextDisclosureBundle", "Streaming responses"; ADR-0124 rule 1, ADR-0129). Pure state; the 1:1 session, signing
//! and clocks belong to the caller.
//!
//! * [`Selection`] is what the user picked. A [`Bundle`] can be built only from a selection, so nothing but selected items
//!   leaves; the Room scope and participants are omitted unless the caller names them explicitly.
//! * The bundle goes only to an endpoint the current service control head authorizes ([`ServiceChain::endpoint_usable`]).
//! * Every disclosure writes a content-free [`Receipt`].
//! * Streaming is ephemeral [`Invocation`] state: frames are display data with no conversion to an event, cancel stops
//!   future frames, and the final result reaches the conversation only as a service post (the caller still validates the
//!   authored event) or as a [`Draft`] the user confirms and sends as themselves.

use crate::service_control::ServiceChain;
use std::collections::BTreeMap;

pub const MAX_ITEMS: usize = 32;
pub const MAX_ITEM_BYTES: usize = 16384;
pub const MAX_PARTICIPANTS: usize = 64;
/// Bound on frames kept for one invocation.
pub const MAX_FRAMES: usize = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemKind {
    Message = 1,
    Attachment = 2,
    Draft = 3,
    Topic = 4,
    Text = 5,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub kind: ItemKind,
    pub bytes: Vec<u8>,
}

/// The user's explicit choice of what to disclose.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Selection(Vec<Item>);

#[derive(Debug, PartialEq, Eq)]
pub enum InvokeError {
    EmptySelection,
    TooManyItems,
    ItemTooLarge,
    EmptyItem,
    TooManyParticipants,
    /// The target endpoint is not usable under the current control head.
    EndpointNotUsable,
    Expired,
    BadPurpose,
}

impl Selection {
    pub fn new(items: Vec<Item>) -> Result<Self, InvokeError> {
        if items.is_empty() {
            return Err(InvokeError::EmptySelection);
        }
        if items.len() > MAX_ITEMS {
            return Err(InvokeError::TooManyItems);
        }
        for i in &items {
            if i.bytes.is_empty() {
                return Err(InvokeError::EmptyItem);
            }
            if i.bytes.len() > MAX_ITEM_BYTES {
                return Err(InvokeError::ItemTooLarge);
            }
        }
        Ok(Self(items))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bundle {
    pub invocation: [u8; 16],
    pub service: [u8; 32],
    /// Omitted by default: no stable Room id leaks.
    pub scope_hint: Option<Vec<u8>>,
    pub items: Vec<Item>,
    /// Only the participants the user chose to disclose.
    pub participants: Option<Vec<Vec<u8>>>,
    pub purpose: String,
    pub expires_at: u64,
}

/// Disclosure receipt: what left the boundary, never the content.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub invocation: [u8; 16],
    pub service: [u8; 32],
    pub purpose: String,
    pub item_kinds: Vec<ItemKind>,
    pub total_bytes: usize,
    pub scope_disclosed: bool,
    pub participants_disclosed: usize,
}

#[derive(Debug, Default)]
pub struct Receipts(Vec<Receipt>);

impl Receipts {
    pub fn all(&self) -> &[Receipt] {
        &self.0
    }
}

pub struct Request<'a> {
    pub invocation: [u8; 16],
    pub endpoint: &'a [u8],
    pub selection: Selection,
    pub scope_hint: Option<Vec<u8>>,
    pub participants: Option<Vec<Vec<u8>>>,
    pub purpose: &'a str,
    pub expires_at: u64,
    pub now: u64,
}

/// Builds the bundle for one user-initiated invocation and writes its receipt.
pub fn disclose(
    chain: &ServiceChain,
    service: [u8; 32],
    r: Request<'_>,
    receipts: &mut Receipts,
) -> Result<Bundle, InvokeError> {
    if r.purpose.is_empty() || r.purpose.len() > 128 {
        return Err(InvokeError::BadPurpose);
    }
    if r.expires_at <= r.now {
        return Err(InvokeError::Expired);
    }
    if !chain.endpoint_usable(r.endpoint) {
        return Err(InvokeError::EndpointNotUsable);
    }
    if r.participants
        .as_ref()
        .is_some_and(|p| p.len() > MAX_PARTICIPANTS)
    {
        return Err(InvokeError::TooManyParticipants);
    }
    let items = r.selection.0;
    receipts.0.push(Receipt {
        invocation: r.invocation,
        service,
        purpose: r.purpose.to_string(),
        item_kinds: items.iter().map(|i| i.kind).collect(),
        total_bytes: items.iter().map(|i| i.bytes.len()).sum(),
        scope_disclosed: r.scope_hint.is_some(),
        participants_disclosed: r.participants.as_ref().map_or(0, |p| p.len()),
    });
    Ok(Bundle {
        invocation: r.invocation,
        service,
        scope_hint: r.scope_hint,
        items,
        participants: r.participants,
        purpose: r.purpose.to_string(),
        expires_at: r.expires_at,
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    pub seq: u64,
    pub text: Vec<u8>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Cancelled, finished or expired: no further frames.
    Closed,
    /// Frame from another endpoint than the one the bundle went to.
    WrongEndpoint,
    /// `seq` is not strictly greater than the last accepted one.
    OutOfOrder,
    TooManyFrames,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Open,
    Cancelled,
    Finished,
}

/// How the final candidate may reach the conversation.
#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The service authored a reply: the caller must still validate the `message.create` it sends under the service's
    /// own authorship (`service_authorship`). Streamed frames are not that event.
    ServicePost(Vec<u8>),
    /// A draft the invoking user may edit and send as themselves.
    Draft(Draft),
}

/// A returned draft. It has no way to become an event except through [`Draft::confirm`].
#[derive(Debug, PartialEq, Eq)]
pub struct Draft(Vec<u8>);

/// A message the account sends after confirming a draft: authored by the account, not the service.
#[derive(Debug, PartialEq, Eq)]
pub struct AccountMessage(pub Vec<u8>);

impl Draft {
    pub fn text(&self) -> &[u8] {
        &self.0
    }
    pub fn confirm(self, edited: Vec<u8>) -> AccountMessage {
        AccountMessage(edited)
    }
}

pub struct Invocation {
    id: [u8; 16],
    endpoint: Vec<u8>,
    expires_at: u64,
    phase: Phase,
    last_seq: Option<u64>,
    frames: Vec<Frame>,
}

impl Invocation {
    pub fn start(bundle: &Bundle, endpoint: &[u8]) -> Self {
        Self {
            id: bundle.invocation,
            endpoint: endpoint.to_vec(),
            expires_at: bundle.expires_at,
            phase: Phase::Open,
            last_seq: None,
            frames: Vec::new(),
        }
    }
    pub fn id(&self) -> [u8; 16] {
        self.id
    }
    pub fn phase(&self) -> Phase {
        self.phase
    }

    fn open(&mut self, now: u64) -> Result<(), FrameError> {
        if self.phase == Phase::Open && now >= self.expires_at {
            self.phase = Phase::Cancelled;
        }
        if self.phase == Phase::Open {
            Ok(())
        } else {
            Err(FrameError::Closed)
        }
    }

    pub fn frame(&mut self, from_endpoint: &[u8], f: Frame, now: u64) -> Result<(), FrameError> {
        self.open(now)?;
        if from_endpoint != self.endpoint {
            return Err(FrameError::WrongEndpoint);
        }
        if self.last_seq.is_some_and(|s| f.seq <= s) {
            return Err(FrameError::OutOfOrder);
        }
        if self.frames.len() >= MAX_FRAMES {
            return Err(FrameError::TooManyFrames);
        }
        self.last_seq = Some(f.seq);
        self.frames.push(f);
        Ok(())
    }

    /// Stops future frames. It does not erase what was already disclosed.
    pub fn cancel(&mut self) {
        if self.phase == Phase::Open {
            self.phase = Phase::Cancelled;
        }
    }

    /// Display data only; frames are never conversation events.
    pub fn frames(&self) -> &[Frame] {
        &self.frames
    }

    /// Closes the invocation with a final candidate. A cancelled or expired invocation yields nothing.
    pub fn finish(
        &mut self,
        from_endpoint: &[u8],
        result: Vec<u8>,
        as_draft: bool,
        now: u64,
    ) -> Result<Outcome, FrameError> {
        self.open(now)?;
        if from_endpoint != self.endpoint {
            return Err(FrameError::WrongEndpoint);
        }
        self.phase = Phase::Finished;
        self.frames.clear();
        Ok(if as_draft {
            Outcome::Draft(Draft(result))
        } else {
            Outcome::ServicePost(result)
        })
    }
}

/// Counts receipts per service, for a "who got my data" view.
pub fn receipts_by_service(r: &Receipts) -> BTreeMap<[u8; 32], usize> {
    let mut m = BTreeMap::new();
    for x in r.all() {
        *m.entry(x.service).or_insert(0) += 1;
    }
    m
}
