// SPDX-License-Identifier: MPL-2.0

use super::{EXTENSION, Error, Facts, INLINE, Kind, REACTION, WEBP, registry, walk_webp};
use crate::attach::{AttachError, AttachmentRef};
use crate::crypto::cbor::{self, V};
use crate::crypto::sig;

pub(super) fn wire<T>(v: Result<T, crate::crypto::Error>) -> Result<T, Error> {
    v.map_err(|_| "wrong_type")
}

pub(super) fn digest(v: &V) -> Result<[u8; 32], Error> {
    if v.opt(0) != Some(&V::U(1)) {
        return Err("value_range");
    }
    wire(wire(v.get(1))?.b32())
}

pub(super) fn text(v: &V, max: usize, error: Error) -> Result<&str, Error> {
    let s = wire(v.t())?;
    if s.is_empty() || s.chars().count() > max {
        return Err(error);
    }
    Ok(s)
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PackRef {
    pub pack_id: [u8; 32],
    pub release_id: [u8; 32],
    pub item_id: String,
}

impl PackRef {
    fn parse(v: &V) -> Result<Self, Error> {
        Ok(Self {
            pack_id: wire(wire(v.get(0))?.b32())?,
            release_id: digest(wire(v.get(1))?)?,
            item_id: text(wire(v.get(2))?, 128, "item_id_scalars")?.into(),
        })
    }

    fn value(&self) -> V {
        V::M(vec![
            (0, V::B(self.pack_id.to_vec())),
            (1, sig::digest(&self.release_id)),
            (2, V::T(self.item_id.clone())),
        ])
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Presentation {
    pub kind: Kind,
    pub media_profile_id: String,
    pub alt_text: String,
    pub fallback_text: Option<String>,
    pub pack_ref: Option<PackRef>,
    pub shared_blob: bool,
}

impl Presentation {
    pub fn webp(kind: Kind, alt_text: String, fallback_text: Option<String>) -> Self {
        Self {
            kind,
            media_profile_id: WEBP.into(),
            alt_text,
            fallback_text,
            pack_ref: None,
            shared_blob: false,
        }
    }

    fn parse(v: &V, start: u64) -> Result<Self, Error> {
        let kind = Kind::try_from(wire(wire(v.get(start))?.u())?)?;
        let media_profile_id = wire(wire(v.get(start + 1))?.t())?.into();
        let alt_text = text(wire(v.get(start + 2))?, 512, "alt_text_scalars")?.into();
        let fallback_text = v
            .opt(start + 3)
            .map(|v| text(v, 128, "fallback_text_scalars").map(str::to_owned))
            .transpose()?;
        let pack_ref = v.opt(start + 4).map(PackRef::parse).transpose()?;
        let shared_blob = v.opt(start + 5) == Some(&V::Bool(true));
        Ok(Self {
            kind,
            media_profile_id,
            alt_text,
            fallback_text,
            pack_ref,
            shared_blob,
        })
    }

    fn fields(&self, start: u64) -> Vec<(u64, V)> {
        let mut fields = vec![
            (start, V::U(self.kind as u64)),
            (start + 1, V::T(self.media_profile_id.clone())),
            (start + 2, V::T(self.alt_text.clone())),
        ];
        if let Some(fallback) = &self.fallback_text {
            fields.push((start + 3, V::T(fallback.clone())));
        }
        if let Some(pack_ref) = &self.pack_ref {
            fields.push((start + 4, pack_ref.value()));
        }
        if self.shared_blob {
            fields.push((start + 5, V::Bool(true)));
        }
        fields
    }

    pub fn fallback(&self) -> &str {
        self.fallback_text.as_deref().unwrap_or(&self.alt_text)
    }

    /// ADR-0255: the explicit fallback is the key for newly sent custom reactions.
    pub(crate) fn reaction_key(&self) -> Option<&str> {
        self.fallback_text.as_deref().filter(|key| {
            matches!(self.kind, Kind::CustomEmoji | Kind::ReactionAsset)
                && crate::flows::social::valid_reaction_key(key)
        })
    }

    pub(crate) fn extension(&self, index: u8) -> V {
        let mut fields = self.fields(1);
        fields.push((0, V::U(u64::from(index))));
        V::TM(vec![(
            EXTENSION.into(),
            V::M(vec![(0, V::Bool(false)), (1, V::M(fields))]),
        )])
    }
}

#[derive(Clone, Debug)]
pub struct AssetRef {
    pub attachment: AttachmentRef,
    pub presentation: Presentation,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Opened {
    Renderable { plaintext: Vec<u8>, facts: Facts },
    Placeholder { alt_text: String },
}

impl AssetRef {
    /// Bounded canonical parser for assets carried by a W1 slot or a pack.
    pub fn parse(bytes: &[u8]) -> Result<Self, Error> {
        registry().inspect_sensitive_struct("ExpressiveAssetRef", bytes, 32768, |_| ())?;
        Self::from_value(&wire(cbor::decode(bytes))?)
    }

    pub(crate) fn from_value(v: &V) -> Result<Self, Error> {
        let presentation = Presentation::parse(v, 11)?;
        let attachment = AttachmentRef::from_v(v).ok_or("attachment_ref")?;
        Self::new(attachment, presentation)
    }

    pub(crate) fn new(
        attachment: AttachmentRef,
        presentation: Presentation,
    ) -> Result<Self, Error> {
        let asset = Self {
            attachment,
            presentation,
        };
        asset.claims()?;
        Ok(asset)
    }

    pub fn value(&self) -> V {
        let V::M(mut fields) = self.attachment.to_v() else {
            unreachable!()
        };
        fields.extend(self.presentation.fields(11));
        V::M(fields)
    }

    fn claims(&self) -> Result<(), Error> {
        let a = &self.attachment;
        let p = &self.presentation;
        if p.alt_text.is_empty() || p.alt_text.chars().count() > 512 {
            return Err("alt_text_scalars");
        }
        if let Some(fallback) = &p.fallback_text
            && (fallback.is_empty() || fallback.chars().count() > 128)
        {
            return Err("fallback_text_scalars");
        }
        if let Some(pack) = &p.pack_ref
            && (pack.item_id.is_empty() || pack.item_id.chars().count() > 128)
        {
            return Err("item_id_scalars");
        }
        if p.media_profile_id.is_empty() || p.media_profile_id.len() > 64 {
            return Err("value_range");
        }
        if p.media_profile_id != WEBP {
            return Ok(());
        }
        let limit = p.kind.limits();
        if a.media_type != "image/webp" {
            return Err("media_type");
        }
        if a.plaintext_size == 0 || a.plaintext_size > limit.bytes as u64 {
            return Err("size_over");
        }
        match (a.width, a.height) {
            (Some(1..), Some(1..)) => {}
            _ => return Err("dimensions_missing"),
        }
        if a.width.unwrap() > limit.canvas {
            return Err("width_over");
        }
        if a.height.unwrap() > limit.canvas {
            return Err("height_over");
        }
        match a.duration_ms {
            None if p.kind == Kind::LoopAnimation => Err("duration_required"),
            Some(d) if d == 0 || d > u64::from(limit.duration) => Err("duration_over"),
            _ => Ok(()),
        }
    }

    /// The reference comes from authenticated conversation state or a verified manifest. The
    /// ciphertext digest and AEAD are checked before inspecting any known-profile headers.
    pub fn open(&self, ciphertext: &[u8]) -> Result<Opened, AttachError> {
        if self.presentation.media_profile_id != WEBP || self.claims().is_err() {
            return Ok(self.placeholder());
        }
        let plaintext = self.attachment.open(ciphertext)?;
        match self.validate_plaintext(&plaintext) {
            Ok(facts) => Ok(Opened::Renderable { plaintext, facts }),
            Err(_) => Ok(self.placeholder()),
        }
    }

    pub(crate) fn validate_plaintext(&self, plaintext: &[u8]) -> Result<Facts, Error> {
        if self.presentation.media_profile_id != WEBP {
            return Err("unknown_profile");
        }
        self.claims()?;
        let facts = walk_webp(self.presentation.kind, plaintext)?;
        let a = &self.attachment;
        if facts.duration_ms == Some(0)
            || Some(facts.width) != a.width
            || Some(facts.height) != a.height
            || facts.duration_ms.map(u64::from) != a.duration_ms
            || facts.encoded_size as u64 != a.plaintext_size
        {
            return Err("header_claim_mismatch");
        }
        Ok(facts)
    }

    fn placeholder(&self) -> Opened {
        Opened::Placeholder {
            alt_text: self.presentation.alt_text.clone(),
        }
    }
}

/// Non-critical metadata is usable only when it binds the actual immutable attachment.
pub(crate) fn from_extension(ext: &V, index: usize, attachment: AttachmentRef) -> Option<AssetRef> {
    let V::TM(entries) = ext else { return None };
    let (_, entry) = entries.iter().find(|(key, _)| key == EXTENSION)?;
    if entry.opt(0) != Some(&V::Bool(false)) {
        return None;
    }
    let value = entry.opt(1)?;
    if value.opt(0)?.u().ok()? != index as u64 {
        return None;
    }
    registry()
        .check_struct("ExpressiveAssetV1Extension", &cbor::encode(value), 4096)
        .ok()?;
    let p = Presentation::parse(value, 1).ok()?;
    AssetRef::new(attachment, p).ok()
}

/// Exact capability required by the body. An unknown profile is presentation-only.
pub(crate) fn required_slot(kind: u64, body: &V) -> Result<Option<&'static str>, Error> {
    use crate::events::{K_CREATE, K_EDIT, K_REACTION};
    if kind == K_REACTION
        && let Some(asset) = body.opt(3)
    {
        let asset = AssetRef::from_value(asset)?;
        if body.opt(2) != Some(&V::Bool(true))
            || !matches!(
                asset.presentation.kind,
                Kind::CustomEmoji | Kind::ReactionAsset
            )
        {
            return Err("value_range");
        }
        return Ok(Some(REACTION));
    }
    let rt = match kind {
        K_CREATE => body.opt(0),
        K_EDIT => body.opt(1),
        _ => None,
    };
    let mut has_emoji = false;
    if let Some(rt) = rt {
        visit_inline(rt, |inline| {
            let asset = AssetRef::from_value(wire(inline.get(6))?)?;
            if asset.presentation.kind != Kind::CustomEmoji {
                return Err("value_range");
            }
            text(wire(inline.get(1))?, 128, "fallback_text_scalars")?;
            has_emoji = true;
            Ok(())
        })?;
    }
    Ok(has_emoji.then_some(INLINE))
}

pub(crate) fn visit_inline(
    rt: &V,
    mut visitor: impl FnMut(&V) -> Result<(), Error>,
) -> Result<(), Error> {
    fn inlines(nodes: &V, visitor: &mut impl FnMut(&V) -> Result<(), Error>) -> Result<(), Error> {
        for node in wire(nodes.a())? {
            match node.opt(0) {
                Some(V::U(4)) => visitor(node)?,
                Some(V::U(2 | 3)) => inlines(wire(node.get(4))?, visitor)?,
                _ => {}
            }
        }
        Ok(())
    }
    for block in wire(wire(rt.get(1))?.a())? {
        if matches!(block.opt(0), Some(V::U(1 | 2))) {
            inlines(wire(block.get(1))?, &mut visitor)?;
        }
    }
    Ok(())
}

/// Validate the scoped grammar, then its semantic slot and exact required-capability rule.
pub fn decode_event(bytes: &[u8], enabled: bool) -> Result<crate::events::Event, Error> {
    let validator = if enabled {
        registry()
    } else {
        crate::flows::registry()
    };
    validator.check("ConversationEvent", bytes, false)?;
    let value = wire(cbor::decode(bytes))?;
    let kind = wire(wire(value.get(8))?.u())?;
    let slot = if enabled {
        required_slot(kind, wire(value.get(11))?)?
    } else {
        None
    };
    crate::events::parse_value(&value, bytes, slot).ok_or("value_range")
}

pub(crate) fn inline_assets(rt: Option<&V>) -> Option<Vec<crate::state::SecretHex>> {
    let mut refs = Vec::new();
    if let Some(rt) = rt {
        visit_inline(rt, |node| {
            refs.push(crate::crypto::hex(&cbor::encode(wire(node.get(6))?)).into());
            Ok(())
        })
        .ok()?;
    }
    Some(refs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_key_uses_only_an_explicit_bounded_fallback_verbatim() {
        let max = "👍".repeat(16);
        let over = format!("{max}a");
        for fallback in [" :cheer: ", "e\u{301}", &max] {
            let asset =
                Presentation::webp(Kind::ReactionAsset, "Alt".into(), Some(fallback.into()));
            assert_eq!(asset.reaction_key(), Some(fallback));
        }
        for fallback in [
            None,
            Some(""),
            Some("\u{2003}"),
            Some("a\u{0085}"),
            Some(&over),
        ] {
            let asset =
                Presentation::webp(Kind::CustomEmoji, "Alt".into(), fallback.map(str::to_owned));
            assert_eq!(asset.reaction_key(), None);
        }
        let sticker = Presentation::webp(Kind::Sticker, "Alt".into(), Some(":cheer:".into()));
        assert_eq!(sticker.reaction_key(), None);
    }
}
