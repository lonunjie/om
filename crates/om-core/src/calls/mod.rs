// SPDX-License-Identifier: MPL-2.0
//! One-to-one call state and transient host data (ADR-0218). The profile transaction stages it
//! alongside the ratchet and inbox cursor. The vault encoder only writes AppState and Device.

mod wire;

use crate::api::CallMedia;
use crate::crypto::{
    cbor::{self, V},
    hex,
    kdf::hkdf_sha256,
};
use crate::host::NetCall;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub(crate) use wire::{Body, Signal, valid_sdp};
pub(crate) const SIGNAL_CTX: &[u8] = b"OM-CALL-SIGNAL-CTX-v1";
pub(crate) const OBJECT_TYPE: u64 = 44;
pub(crate) const RING_MS: u64 = 45_000;
pub(crate) const SPAM_MS: u64 = 600_000;
pub(crate) const REPLAY_MS: u64 = SPAM_MS + crate::crypto::delivery::CLOCK_SKEW * 1000;
pub(crate) const SEEN_LIMIT: usize = 512;
pub(crate) const NOTICE_LIMIT: usize = 64;
pub(crate) const DEPOSIT_LIMIT: usize = 128;
pub(crate) const OPERATION_LIMIT: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallState {
    Incoming,
    Starting,
    Ringing,
    Connecting,
    Active,
    Reconnecting,
    Ending,
    Ended,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallReason {
    Busy,
    Declined,
    AnsweredElsewhere,
    Timeout,
    Failed,
    Hangup,
    Canceled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaState {
    Active,
    Interrupted,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CallRoute {
    LanDirect,
    Direct,
    Relay,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallView {
    pub call_session_id: String,
    pub conversation_id: String,
    pub media: CallMedia,
    pub state: CallState,
    pub outgoing: bool,
    pub ring_deadline_ms: Option<u64>,
    pub reason: Option<CallReason>,
    pub local_media_flags: u8,
    pub peer_media_flags: u8,
    pub route: Option<CallRoute>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallNotice {
    pub call_session_id: String,
    pub conversation_id: String,
    pub reason: CallReason,
    pub at_ms: u64,
    pub folded: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DescriptionKind {
    Offer,
    Answer,
    IceUpdate,
}

/// An authenticated peer description for the driver. This is separate from the UI view.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CallDescription {
    pub kind: DescriptionKind,
    pub seq: u32,
    pub sdp: Vec<u8>,
}

/// This host effect is consumed once for the selected device pair. Dropping it wipes both base keys.
#[derive(Clone)]
pub struct InstallMediaKeys {
    bytes: Zeroizing<[u8; 64]>,
}

impl InstallMediaKeys {
    pub const GENERATION: u32 = 0;

    fn new(secret: &[u8; 32], id: &[u8; 16], local: &[u8; 16], peer: &[u8; 16]) -> Self {
        let mut bytes = Zeroizing::new([0; 64]);
        let (send, receive) = bytes.split_at_mut(32);
        derive(secret, id, local, peer, send);
        derive(secret, id, peer, local, receive);
        Self { bytes }
    }

    pub fn send(&self) -> &[u8] {
        &self.bytes[..32]
    }

    pub fn receive(&self) -> &[u8] {
        &self.bytes[32..]
    }
}

fn derive(
    secret: &[u8; 32],
    id: &[u8; 16],
    sender: &[u8; 16],
    recipient: &[u8; 16],
    out: &mut [u8],
) {
    let info = cbor::encode(&V::A(vec![
        V::T("OM-CALL-MEDIA-BASE-KDF-v1".into()),
        V::B(sender.to_vec()),
        V::B(recipient.to_vec()),
        V::U(u64::from(InstallMediaKeys::GENERATION)),
    ]));
    hkdf_sha256(id, secret, &info, out);
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeenOffer {
    pub(crate) id: String,
    pub(crate) account: String,
    pub(crate) at_ms: u64,
    /// Only an offer admitted to ring counts. Acceptance removes it from the unanswered count.
    pub(crate) unanswered: bool,
}

#[derive(Clone, Default)]
pub(crate) struct Calls {
    pub current: Option<Call>,
    pub deposits: Vec<Deposit>,
    pub notices: Vec<CallNotice>,
    pub last_clock_ms: Option<u64>,
}

impl std::fmt::Debug for Calls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Calls(..)")
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DepositKind {
    Offer,
    Answer,
    Other,
}

#[derive(Clone)]
pub(crate) struct Deposit {
    pub id: u64,
    pub call_id: [u8; 16],
    pub conversation: String,
    pub device: [u8; 16],
    pub deadline_ms: u64,
    pub kind: DepositKind,
    pub call: NetCall,
}

#[derive(Clone)]
pub(crate) struct ContactDevice {
    pub id: [u8; 16],
    pub last_seq: Option<u32>,
    pub rejected: Option<CallReason>,
}

#[derive(Clone)]
pub(crate) struct Call {
    pub id: [u8; 16],
    pub view: CallView,
    pub targets: Vec<ContactDevice>,
    pub selected: Option<[u8; 16]>,
    pub next_seq: u32,
    pub secret: Option<Zeroizing<[u8; 32]>>,
    pub keys: Option<InstallMediaKeys>,
    pub description: Option<CallDescription>,
    pub ice: Option<CallDescription>,
}

impl Call {
    pub fn new(
        id: [u8; 16],
        conv: &str,
        media: CallMedia,
        outgoing: bool,
        deadline_ms: u64,
        secret: Zeroizing<[u8; 32]>,
    ) -> Self {
        Self {
            id,
            view: CallView {
                call_session_id: hex(&id),
                conversation_id: conv.into(),
                media,
                state: if outgoing {
                    CallState::Starting
                } else {
                    CallState::Incoming
                },
                outgoing,
                ring_deadline_ms: Some(deadline_ms),
                reason: None,
                local_media_flags: 0,
                peer_media_flags: 0,
                route: None,
            },
            targets: vec![],
            selected: None,
            next_seq: 0,
            secret: Some(secret),
            keys: None,
            description: None,
            ice: None,
        }
    }

    pub fn is_live(&self) -> bool {
        !matches!(self.view.state, CallState::Ended | CallState::Failed)
    }

    pub fn connect(&mut self, local: &[u8; 16], peer: [u8; 16]) {
        if let Some(secret) = self.secret.take() {
            self.keys = Some(InstallMediaKeys::new(&secret, &self.id, local, &peer));
        }
        self.selected = Some(peer);
        self.view.ring_deadline_ms = None;
        self.view.state = CallState::Connecting;
    }

    pub fn finish(&mut self, reason: CallReason) {
        self.secret = None;
        self.keys = None;
        self.description = None;
        self.ice = None;
        self.view.route = None;
        self.view.ring_deadline_ms = None;
        self.view.reason = Some(reason);
        self.view.state = if reason == CallReason::Failed {
            CallState::Failed
        } else {
            CallState::Ended
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adr_0218_media_key_known_answer_and_context_separation() {
        // Independent RFC 5869 HMAC calculation with the deterministic CBOR info from ADR-0218.
        let secret = std::array::from_fn(|i| i as u8);
        let id = std::array::from_fn(|i| i as u8);
        let a = [0x11; 16];
        let b = [0x22; 16];
        let keys = InstallMediaKeys::new(&secret, &id, &a, &b);
        assert!(
            hex(keys.send()) == "c38d3c8229bdb76fdcc6936b61c5be16430dcae2fd5602f00c4ef59c944da9cb"
        );
        let peer = InstallMediaKeys::new(&secret, &id, &b, &a);
        assert!(keys.send() == peer.receive());
        assert!(keys.receive() == peer.send());
        assert!(keys.send() != keys.receive());
        for other in [
            InstallMediaKeys::new(&secret, &[0xff; 16], &a, &b),
            InstallMediaKeys::new(&secret, &id, &[0xff; 16], &b),
            InstallMediaKeys::new(&secret, &id, &a, &[0xff; 16]),
            InstallMediaKeys::new(&[0xff; 32], &id, &a, &b),
        ] {
            assert!(keys.send() != other.send());
            assert!(keys.receive() != other.receive());
        }
    }
}
