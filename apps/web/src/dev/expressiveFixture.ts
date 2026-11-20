// SPDX-License-Identifier: MPL-2.0
// The loopback fixture is only reachable from the dev entry point. No pack or image address is fetched.
import type { Harness, SeedExpressivePack } from "@om/core-harness";
import type { ConversationTimelineItem, ProfileId } from "@om/platform-api";
import type { ExpressiveHost } from "@om/ui";

const bytes = Uint8Array.from(atob("UklGRmQAAABXRUJQVlA4TFgAAAAvF8AFABcgEEhs0h9xDYFAYhf0cgsEEozs9rd4/uMPFTBoG8lRtHAO0H2BsCiOxEOo4XkFQkT/J4ClCjQN+LT0VGn55x8Tqr388+95nwbWS/i09JoGoCwF"), (c) => c.charCodeAt(0));
const asset = (kind: "sticker" | "reaction_asset" | "custom_emoji", altText: string): SeedExpressivePack["items"][number]["asset"] => ({
  state: "renderable",
  altText,
  asset: {
    kind, bytes, mediaProfileId: "om.expressive.webp-v1",
    facts: { width: 24, height: 24, frames: 1, durationMs: null, encodedSize: bytes.length, totalPixels: 576 },
  },
});

export const inlineMessage = {
  direction: "incoming",
  text: "Here’s a little 🙂.",
  expressive: [{ source: { kind: "inline", index: 0 }, opened: asset("custom_emoji", "A small smiling face") }],
} as const;

export function expressiveRichText(item: ConversationTimelineItem): unknown {
  if (item.direction !== "incoming" || item.text !== inlineMessage.text) return undefined;
  return { version: 1, blocks: [{ type: "paragraph", children: [
    { type: "text", text: "Here’s a little " },
    { type: "custom_emoji", index: 0, fallbackText: "🙂" },
    { type: "text", text: "." },
  ] }] };
}

export const expressiveFixture: SeedExpressivePack = {
  input: { release: Uint8Array.of(1), control: Uint8Array.of(2) },
  preview: {
    packId: "a1".repeat(32), releaseId: "ab".repeat(32), controlDigest: "ac".repeat(32),
    title: "Little faces", sequence: 1, itemCount: 2, size: String(bytes.length * 2),
    controllerCount: 1, threshold: 1, signedBy: [Array.from({ length: 16 }, (_, i) => i)],
    description: null, creatorDisplay: "Example creator", sourceUrls: [], spdxId: "CC0-1.0",
    licenseUrl: null, licenseTextHash: null,
  },
  items: [
    { itemId: "hello", blobId: "ba".repeat(32), label: "Hello smile", fallbackText: "🙂", asset: asset("sticker", "A smiling sticker") },
    { itemId: "cheer", blobId: "bb".repeat(32), label: "Cheer", fallbackText: "😊", asset: asset("reaction_asset", "A cheering reaction") },
  ],
};

export function devExpressiveHost(harness: Harness, profile: ProfileId): ExpressiveHost {
  const cached = expressiveFixture.items.map(({ blobId }) => ({ blobId, ciphertext: bytes }));
  return {
    review: { input: expressiveFixture.input, cached },
    async describe(ctx, messageId, source) {
      const value = await harness.expressive.query(ctx, { kind: "open", messageId, source });
      if (value === null) return null;
      return { altText: value.altText, local: true, canOpen: value.state === "renderable" };
    },
    readCached: (ctx, blobId) => Promise.resolve(ctx.profileId === profile ? cached.find((b) => b.blobId === blobId)?.ciphertext.slice() ?? null : null),
  };
}
