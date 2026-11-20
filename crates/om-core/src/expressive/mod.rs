// SPDX-License-Identifier: MPL-2.0
//! W1 expressive media. Wire parsing and media validation are independent of pack availability.

mod asset;
mod pack;
mod webp;

pub use asset::*;
pub use pack::*;
pub use webp::{DecodeFacts, Facts, Frame, validate_facts, walk_webp};

pub const WEBP: &str = "om.expressive.webp-v1";
pub const INLINE: &str = "om.expressive.inline-emoji.v1";
pub const REACTION: &str = "om.expressive.reaction-asset.v1";
pub const EXTENSION: &str = "om.expressive.asset.v1";

pub type Error = &'static str;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[repr(u8)]
pub enum Kind {
    Sticker = 1,
    CustomEmoji = 2,
    ReactionAsset = 3,
    LoopAnimation = 4,
}

impl TryFrom<u64> for Kind {
    type Error = Error;

    fn try_from(value: u64) -> Result<Self, Error> {
        match value {
            1 => Ok(Self::Sticker),
            2 => Ok(Self::CustomEmoji),
            3 => Ok(Self::ReactionAsset),
            4 => Ok(Self::LoopAnimation),
            _ => Err("value_range"),
        }
    }
}

struct Limits {
    canvas: u32,
    bytes: usize,
    duration: u32,
    frames: u32,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sticker => "sticker",
            Self::CustomEmoji => "custom_emoji",
            Self::ReactionAsset => "reaction_asset",
            Self::LoopAnimation => "loop_animation",
        }
    }

    fn limits(self) -> Limits {
        match self {
            Self::Sticker => Limits {
                canvas: 512,
                bytes: 1 << 20,
                duration: 3000,
                frames: 90,
            },
            Self::CustomEmoji | Self::ReactionAsset => Limits {
                canvas: 256,
                bytes: 1 << 19,
                duration: 3000,
                frames: 90,
            },
            Self::LoopAnimation => Limits {
                canvas: 1024,
                bytes: 6 << 20,
                duration: 10000,
                frames: 300,
            },
        }
    }
}

pub(crate) fn registry() -> &'static om_codec::Registry {
    static REGISTRY: std::sync::OnceLock<om_codec::Registry> = std::sync::OnceLock::new();
    REGISTRY.get_or_init(om_codec::Registry::load_w1_expressive_scoped)
}

/// The three capabilities carried in canonical DeviceCredential set order.
pub(crate) fn device_capabilities() -> Vec<String> {
    let mut ids = [WEBP, INLINE, REACTION];
    ids.sort_by_key(|s| (s.len(), *s));
    ids.into_iter().map(str::to_owned).collect()
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Target {
    Attachment,
    Inline,
    Edit { message_id: String },
    Reaction { message_id: String, key: String },
}

impl Target {
    pub(crate) fn slot(&self) -> Option<&'static str> {
        match self {
            Self::Attachment => None,
            Self::Inline | Self::Edit { .. } => Some(INLINE),
            Self::Reaction { .. } => Some(REACTION),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Pending {
    pub target: Target,
    pub presentation: Presentation,
}

/// A local opt-in which records the ADD exposure from the BlobStore policy descriptor.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BlobPolicy {
    #[default]
    Fresh,
    SharedBlob,
}

impl BlobPolicy {
    pub fn metadata_exposure(self) -> Option<&'static str> {
        match self {
            Self::Fresh => None,
            Self::SharedBlob => Some(
                "Shared blob: the BlobStore can link conversations by BlobID. Recipients can link them by BlobID and content key.",
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source<'a> {
    Attachment(usize),
    Inline(usize),
    Reaction { key: &'a str, mine: bool },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(
    tag = "source",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
pub enum SendFrom {
    Prepared {
        blob_id: String,
    },
    Shared {
        conversation_id: String,
        message_id: String,
    },
}
