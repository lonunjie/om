# OM Expressive Media, Content Packs, Provenance and Entitlements

- **Status:** Canonical pre-engineering architecture
- **Checkpoint:** 2026-09-23 07:18 PHT
- **Scope:** stickers, GIF-like looping animation, custom emoji/reactions, portable content packs, provenance/licensing metadata and optional paid-content entitlement proofs

## Decision summary

OM expressive media is **inert bounded media**, never executable content.

The architecture separates four things that products often blur together:

```text
expressive asset     immutable media bytes + accessibility metadata
content pack         portable signed/integrity-addressed collection of assets
provenance           who controls/published this pack/release; not a copyright verdict
entitlement proof    optional evidence accepted by a distributor; never payment settlement
```

A sent sticker/animation/custom reaction must continue to render even if:

- the sender uninstalls the pack;
- the pack catalog disappears;
- the creator's website disappears;
- a marketplace disappears;
- the recipient never bought or installed the source pack;
- the logical pack later updates or is forked.

Therefore **conversation correctness never depends on a live pack/catalog/commerce service**. When an expressive item enters a conversation, the event carries or references the immutable asset through normal OM attachment/blob semantics. Pack identity remains useful for attribution, picker organization and re-installation, but is not a rendering dependency.

## Media kinds are semantic; codec is a profile

OM standardizes semantic kinds:

```text
sticker
custom_emoji
reaction_asset
loop_animation
```

The baseline expressive-media profile is deliberately small:

```text
om.expressive.webp-v1
canonical payload MIME: image/webp
static and animated WebP permitted
alpha/transparency permitted
no audio
no scripts
no HTML/SVG/Lottie/TGS execution model
```

Reasons:

- one broadly supported decoder family across current web/native platforms;
- supports transparency and animation;
- avoids requiring a proprietary/royalty-bearing mandatory codec;
- avoids embedding a JavaScript/vector-animation execution surface into chat;
- keeps conversion/import tools free to accept many creator formats while the interoperable output stays small.

Clients may import PNG, GIF, APNG, AVIF, WebM, Telegram-style assets or other safe local inputs, but for the baseline interoperable expressive profile they normalize to the canonical profile. If an input cannot safely/acceptably normalize, it is sent as an ordinary attachment or through a separately negotiated optional media capability.

A future capability may add AVIF/WebM/vector-animation profiles after cross-platform decoder/security review. Such a profile is optional until promoted through the normal extension process.

## Baseline resource profiles

These are protocol compatibility ceilings, not targets creators should fill.

### `sticker.v1`

- static or animated WebP;
- maximum canvas: 512 x 512;
- maximum encoded size: 1 MiB;
- animated duration: at most 3 seconds;
- animated frame rate: at most 30 fps;
- loop presentation;
- no audio.

### `custom-emoji.v1`

- static or animated WebP;
- maximum canvas: 256 x 256;
- maximum encoded size: 512 KiB;
- animated duration: at most 3 seconds;
- animated frame rate: at most 30 fps;
- no audio.

### `loop-animation.v1`

- animated WebP;
- maximum canvas: 1024 x 1024;
- maximum encoded size: 6 MiB;
- duration: at most 10 seconds;
- frame rate: at most 30 fps;
- no audio.

All profiles additionally enforce a versioned decoded-pixel/frame budget before or during decode so maliciously crafted media cannot force unbounded memory/CPU allocation merely by staying under encoded-byte limits. Decoders are platform/browser/OS-reviewed implementations where practical; OM does not implement a custom WebP codec.

The UI honors reduced-motion/autoplay preferences. Reduced-motion mode may show a representative/static frame until the user explicitly plays the asset.

## ExpressiveAssetRef

Conversation and rich-text references use an immutable asset reference. It is the frozen W0 `attachment-ref` ([`spec/cddl/events.cddl`](../../../spec/cddl/events.cddl)) plus expressive fields, so a recipient has everything it needs to fetch, verify and decrypt the asset (ADR-0129):

```text
ExpressiveAssetRef = attachment-ref + {
  kind                       # sticker | custom_emoji | reaction_asset | loop_animation
  media_profile_id           # e.g. om.expressive.webp-v1
  alt_text                   # bounded accessible fallback
  fallback_text?             # e.g. Unicode emoji/shortcode
  pack_ref?                  # ContentPackID + release/item metadata
}
```

From `attachment-ref` it carries `blob_id`, `media_type`, `plaintext_size`, `ciphertext_digest`, the encryption descriptor (profile and key), optional locations, width, height and `duration_ms`. It is the payload of the W1 reserved slots: `ReactionSet` key 3 (`reaction_asset`) and inline type 4 (`custom_emoji`), sent only under ADR-0128 rule 18 (every recipient device advertises the encoding, otherwise the W0 fallback text is sent).

~~The `blob_id` is the rendering authority.~~ The attachment-ref's digest-bound blob is the rendering authority: a client renders only bytes whose ciphertext matches `ciphertext_digest` and that decrypt under the descriptor. `pack_ref` never changes the bytes of an already-sent event.

**Fresh encryption per send by default.** Each send encrypts the asset under a fresh key and uploads it as a new blob, so one BlobID never links senders or conversations at a BlobStore (the same reason cross-profile content dedup is forbidden in [`MULTI_IDENTITY_AND_PROFILE_ISOLATION.md`](../client/MULTI_IDENTITY_AND_PROFILE_ISOLATION.md)). A client may offer a declared **shared-blob mode** that reuses a pack's already-encrypted blob to save upload. That mode is a named policy choice, off by default, and its linkage (one BlobID and key seen across conversations by the BlobStore and by every recipient) is a line in the BlobStore's MetadataExposureDescriptor that V-015 checks.

If the expressive asset is unavailable, unsupported or motion-disabled, clients render the bounded `alt_text`/`fallback_text` rather than a broken active embed.

## Stickers and animations in messages

A sticker/animation is sent using normal `message.create` plus an expressive attachment/reference. It does **not** need a special transport or separate encryption model: it is an attachment-ref, encrypted per send like any other attachment.

Sending performs the same client-side encryption, blob integrity verification, repair/mirroring and retention behavior as other attachments.

A message may contain text plus expressive media, but clients may present the common "sticker-only" case with larger sticker UX.

## Custom emoji and reactions

Unicode emoji remain the universal zero-dependency reaction baseline.

A custom reaction uses an `ExpressiveAssetRef(kind=reaction_asset|custom_emoji)` plus a textual fallback. The reaction state still converges under the normal `reaction.set` event semantics; only the rendered reaction token is extended.

Rich text may contain a bounded `custom_emoji` inline node whose payload is an `ExpressiveAssetRef` plus textual fallback. It is inert media, not an HTML/image URL node.

A custom emoji/reaction received in a conversation is renderable from the event/blob even when the recipient is not entitled to install the source pack. **Entitlement gates acquisition/install/update of a pack, never viewing conversation content already intentionally delivered to the recipient.**

## ContentPack object model

A content pack is portable data, not code.

```text
ContentPackID                    random stable 256-bit logical identity
ContentPackControlState          optional update/governance lineage
ContentPackRelease               immutable manifest
ContentPackReleaseID             hash of canonical release bytes
ContentPackItem                  typed item + immutable asset reference
```

### Stable identity and immutable releases

`ContentPackID` identifies the logical pack across updates.

Each `ContentPackRelease` is deterministic-CBOR canonical data. `ContentPackReleaseID` is its content hash. A release never mutates in place.

A logical update advances `ContentPackControlState` to a new immutable release. The state is hash-chained and authorized by the pack's declared controllers/threshold, using OM's durable-signature profile.

For very simple sharing, a creator may publish a one-off immutable release without an update channel. It remains installable forever from any mirror holding the bytes.

## Pack manifest

Conceptually:

```text
ContentPackRelease
  pack_id
  release_sequence
  previous_release_id?
  title
  description?
  locale_metadata?
  pack_kind[]                 # sticker/custom-emoji/reaction/mixed
  items[]
  provenance
  licensing?
  forked_from?
  created_hlc?

ContentPackItem
  item_id                     # stable within logical pack where practical
  kind
  expressive_asset_ref
  emoji_associations[]
  keywords[]
  accessibility_label
```

The manifest contains no executable scripts, remote CSS, HTML, shader code, arbitrary SVG, plugin entrypoints or protocol hooks.

## Pack installation and distribution

A pack may be installed from:

- another OM message;
- a portable pack file;
- QR/deep link;
- creator website;
- independent catalog;
- organization/community recommendation;
- local import/conversion tool.

There is **no mandatory OM catalog**.

Catalogs are discovery/index providers. They may rank, moderate, monetize or curate according to their own policy, but they cannot become the authority for `ContentPackID`, release integrity or conversation rendering.

A privacy profile may prohibit automatic fetching from arbitrary third-party URLs. Direct pack sharing should therefore be able to carry the signed manifest and normal OM blob/repair references so recipients can retrieve through their chosen storage paths rather than beaconing a creator/catalog server merely to render a message.

## Offline behavior

Installed-pack UX is local policy:

- full-cache install;
- manifest + lazy assets;
- favorites-only cache;
- eviction under storage pressure.

The protocol invariant is that an already-sent expressive item has its own immutable message/blob reference. A pack's online availability is not needed to replay history.

Clients must clearly distinguish "installed but some picker assets not cached" from "old messages cannot render"; the latter should be rare and handled by normal blob repair.

## Forks and update continuity

A legitimate update keeps the same `ContentPackID` and advances authorized control/release state.

A fork creates a new `ContentPackID` and may declare:

```text
forked_from = old ContentPackReleaseID
```

A fork cannot seize the old pack's update lineage merely by copying its display name/artwork.

Pack-controller rotation follows signed control-state continuity; losing all pack control keys freezes the update channel but does not invalidate already published immutable releases.

## Provenance is not trust or copyright adjudication

A pack/release may contain:

- controller/publisher signature;
- optional binding to an `AccountID`, `OrganizationIdentity` or other verified claim;
- creator display metadata;
- source URL(s);
- `forked_from` / derivation references;
- optional SPDX license identifier, license URL and/or hash of supplied license text.

The UI may say "signed by X" or "publisher controls example.com" only when that evidence exists.

It must **not** translate a valid signature into claims such as:

- "OM verified copyright ownership";
- "legally licensed everywhere";
- "safe/trustworthy";
- "official" merely because the OM Project can parse the pack.

This preserves R-108 and avoids making SeedLife a global copyright registrar.

## Generic entitlement proof: yes, payment rails: no

OM standardizes a small optional **entitlement presentation interface** so independent creators/catalogs/distributors can gate pack acquisition without inventing incompatible client hooks.

It does **not** standardize:

- checkout;
- price discovery;
- currency;
- card/crypto/payment processing;
- tax handling;
- refunds;
- merchant-of-record behavior;
- marketplace ranking;
- a SeedLife token or OM coin.

Conceptually:

```text
EntitlementPresentation
  proof_type
  issuer_ref
  resource_scope             # pack/release/catalog resource
  holder_binding?            # preferably pseudonymous, not necessarily AccountID
  rights[]                   # e.g. fetch_release, fetch_updates
  audience                   # intended verifier/distributor
  nonce                      # anti-replay/challenge binding
  not_before?
  expires_at?
  status_ref?
  proof_bytes
```

Concrete proof schemes live in a registry and may include privacy-preserving credentials. A W3C Verifiable Credential adapter may exist, but OM does not require JSON-LD/VC infrastructure or one issuer ecosystem; OM's canonical core objects remain deterministic-CBOR objects.

Distributors decide which issuers/proof types they accept. The protocol does not equate entitlement with legal ownership or DRM.

## Privacy requirements for entitlement

An entitlement should not become a global purchase-tracking identifier.

Default guidance:

- prefer holder pseudonyms/scoped keys over AccountID;
- bind presentations to verifier/audience and fresh nonce where practical;
- disclose only the requested resource/right;
- do not attach entitlement proofs to every sticker message;
- do not expose purchase history to Room peers;
- pack/catalog access should remain usable through multiple independent distributors where creator policy allows.

## No DRM promise

Once plaintext/media bytes are delivered to an entitled user's device, OM cannot cryptographically guarantee they will never be copied. The architecture therefore avoids pretending entitlement is durable DRM.

Entitlement is a distribution/access convention. Copyright/license obligations remain legal/policy matters outside OM Core.

## Security invariants

1. Content packs are data-only.
2. Pack signatures prove release/control continuity, not safety or copyright ownership.
3. Sent conversation content never depends on a live pack/catalog/entitlement service.
4. Received content never executes code merely because it is a sticker/pack item.
5. Unknown media profiles fail to inert placeholder/fallback semantics.
6. Entitlement proof is optional and scoped to acquisition/update, not chat rendering.
7. Catalog/commerce providers receive no protocol privilege.
8. Resource limits are enforced before expensive decode/allocation where practical.
9. Security UI and accessibility semantics cannot be overridden by content packs.
10. Pack provenance/update continuity must remain independently verifiable after the original catalog/project disappears.
