// SPDX-License-Identifier: MPL-2.0
//! The frozen CallSignal (type 44) and per-kind rules from call.cddl.

use super::{CallReason, OBJECT_TYPE};
use crate::api::CallMedia;
use crate::crypto::cbor::{self, V};
use om_codec::cbor::Value;
use zeroize::{Zeroize, Zeroizing};

pub(crate) enum Body {
    Offer(CallMedia, Vec<u8>, Zeroizing<[u8; 32]>),
    Answer(Vec<u8>, u8),
    Reject(CallReason),
    End(CallReason),
    Ice(Vec<u8>),
    Media(u8),
}

pub(crate) struct Signal {
    pub id: [u8; 16],
    pub seq: u32,
    pub body: Body,
}

impl CallReason {
    fn from_wire(n: u64) -> Option<Self> {
        use CallReason::*;
        Some(match n {
            1 => Declined,
            2 => Busy,
            3 => AnsweredElsewhere,
            4 => Timeout,
            5 => Failed,
            6 => Hangup,
            7 => Canceled,
            _ => return None,
        })
    }

    fn wire(self) -> u64 {
        use CallReason::*;
        match self {
            Declined => 1,
            Busy => 2,
            AnsweredElsewhere => 3,
            Timeout => 4,
            Failed => 5,
            Hangup => 6,
            Canceled => 7,
        }
    }
}

pub(crate) fn valid_sdp(sdp: &[u8]) -> bool {
    !sdp.is_empty() && sdp.len() <= 16_384
}

impl Signal {
    pub fn encode(&self) -> Zeroizing<Vec<u8>> {
        let (kind, mut fields) = match &self.body {
            Body::Offer(media, sdp, secret) => (
                1,
                vec![
                    (5, V::U(if *media == CallMedia::Audio { 1 } else { 2 })),
                    (6, V::B(sdp.clone())),
                    (7, V::M(vec![(0, V::B(secret.to_vec())), (1, V::U(1))])),
                ],
            ),
            Body::Answer(sdp, flags) => {
                (2, vec![(6, V::B(sdp.clone())), (9, V::U((*flags).into()))])
            }
            Body::Reject(reason) => (3, vec![(8, V::U(reason.wire()))]),
            Body::End(reason) => (4, vec![(8, V::U(reason.wire()))]),
            Body::Ice(sdp) => (5, vec![(6, V::B(sdp.clone()))]),
            Body::Media(flags) => (6, vec![(9, V::U((*flags).into()))]),
        };
        fields.extend([
            (0, V::U(OBJECT_TYPE)),
            (1, V::U(1)),
            (2, V::U(kind)),
            (3, V::B(self.id.to_vec())),
            (4, V::U(self.seq.into())),
        ]);
        let mut map = V::M(fields);
        let capacity = 128
            + match &self.body {
                Body::Offer(_, sdp, _) | Body::Answer(sdp, _) | Body::Ice(sdp) => sdp.len(),
                _ => 0,
            };
        let mut bytes = Zeroizing::new(Vec::with_capacity(capacity));
        cbor::enc(&map, &mut bytes);
        if let V::M(fields) = &mut map
            && let Some((_, V::M(package))) = fields.iter_mut().find(|(k, _)| *k == 7)
            && let Some((0, V::B(secret))) = package.first_mut()
        {
            secret.zeroize();
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        crate::flows::registry()
            .inspect_sensitive("CallSignal", bytes, Self::from_value)
            .ok()
            .flatten()
    }

    fn from_value(v: &Value) -> Option<Self> {
        let uint = |v: Option<&Value>| match v? {
            Value::Uint(n) => Some(*n),
            _ => None,
        };
        fn bstr(v: &Value) -> Option<&[u8]> {
            match v {
                Value::Bytes(b) => Some(b),
                _ => None,
            }
        }
        let id = bstr(v.get(3)?)?.try_into().ok()?;
        let seq = uint(v.get(4))?.try_into().ok()?;
        let mask = (5..=9).fold(0, |mask, key| {
            mask | (u8::from(v.get(key).is_some()) << (key - 5))
        });
        let sdp = || bstr(v.get(6)?).map(<[u8]>::to_vec);
        let flags = || u8::try_from(uint(v.get(9))?).ok();
        let reason = || CallReason::from_wire(uint(v.get(8))?);
        let body = match (uint(v.get(2))?, mask) {
            (1, 0b00111) => {
                let media = match uint(v.get(5))? {
                    1 => CallMedia::Audio,
                    2 => CallMedia::Video,
                    _ => return None,
                };
                let mut secret = Zeroizing::new([0; 32]);
                secret.copy_from_slice(bstr(v.get(7)?.get(0)?)?);
                Body::Offer(media, sdp()?, secret)
            }
            (2, 0b10010) => Body::Answer(sdp()?, flags()?),
            (3, 0b01000) => match reason()? {
                r @ (CallReason::Busy | CallReason::Declined) => Body::Reject(r),
                _ => return None,
            },
            (4, 0b01000) => match reason()? {
                CallReason::Busy | CallReason::Declined => return None,
                r => Body::End(r),
            },
            (5, 0b00010) => Body::Ice(sdp()?),
            (6, 0b10000) => Body::Media(flags()?),
            _ => return None,
        };
        Some(Self { id, seq, body })
    }
}
