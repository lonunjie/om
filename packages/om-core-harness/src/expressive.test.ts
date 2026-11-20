// SPDX-License-Identifier: MPL-2.0
import { expect, it } from "vitest";
import { asConversationId, asOperationId, asPersonId, asProfileId } from "@om/platform-api";
import type { ExpressiveMediaApi, ExpressivePackIntent } from "@om/platform-api";
import { createHarness, type SeedExpressivePack } from "./harness";

const packId = "a1".repeat(32);
const releaseId = "ab".repeat(32);
const itemId = "smile";
const bytes = Uint8Array.of(0xaa);
const fixture: SeedExpressivePack = {
  input: { release: Uint8Array.of(1) },
  preview: {
    packId, releaseId, title: "A pack", sequence: 1, itemCount: 1, size: "1", controllerCount: 0, threshold: 0,
    controlDigest: null, description: null, creatorDisplay: null, sourceUrls: [], signedBy: [],
    spdxId: null, licenseUrl: null, licenseTextHash: null,
  },
  items: [{
    itemId, blobId: "b1".repeat(32), label: "Smile", fallbackText: "🙂",
    asset: { state: "renderable", altText: "A smiling face", asset: {
      kind: "sticker", mediaProfileId: "om.expressive.webp-v1", bytes,
      facts: { width: 1, height: 1, frames: 1, durationMs: null, encodedSize: 1, totalPixels: 1 },
    } },
  }],
};

it("loops a prepared sticker through the timeline and keeps picker state local to the profile", async () => {
  const h = createHarness({ expressiveMedia: true });
  const conversationId = asConversationId("one");
  const ctx = h.seedProfile({
    profileId: asProfileId("a"), label: "A", expressivePacks: [fixture],
    conversations: [{ id: conversationId, personId: asPersonId("b"), displayName: "B" }],
  });
  const other = h.seedProfile({ profileId: asProfileId("other"), label: "Other" });
  const preview = await h.expressive.query(ctx, { kind: "preview_pack", input: fixture.input });
  expect(preview).toEqual({ preview: fixture.preview });
  if (!("preview" in preview)) throw new Error("pack preview refused");
  const install: ExpressivePackIntent = { kind: "install_pack", input: fixture.input, confirmation: preview.preview, policy: "Lazy", cached: [], favourites: [] };
  expect(await h.expressive.do(ctx, { ...install, confirmation: { releaseId, controlDigest: "wrong" } })).toEqual({ refused: "confirmation_required" });
  expect(await h.expressive.do(ctx, install)).toEqual({ accepted: true });
  const selected = { packId, releaseId, itemId };
  await h.expressive.do(ctx, { ...selected, kind: "update_picker", change: { kind: "favourite", present: true } });
  await h.expressive.do(ctx, { ...selected, kind: "update_picker", change: { kind: "recent" } });
  expect(await h.expressive.query(ctx, { kind: "picker", packId })).toMatchObject({ recents: [itemId], items: [{ itemId, favourite: true }] });
  expect(await h.expressive.query(other, { kind: "packs" })).toEqual([]);

  const prepared = await h.expressive.prepare(ctx, selected, bytes);
  if (!prepared.ok) throw new Error("sticker prepare refused");
  await h.dispatch({ kind: "SendExpressive", ctx, operationId: asOperationId("s"), conversationId, target: "Attachment", asset: { source: "prepared", blobId: prepared.blobId } });
  const message = (await h.query({ kind: "GetConversationView", ctx, conversationId })).timeline[0];
  if (message === undefined) throw new Error("sticker missing");
  expect(message.text).toBe("🙂");
  await h.expressive.do(ctx, { kind: "remove_pack", packId });
  const source = { kind: "attachment", index: 0 } as const;
  expect(await h.expressive.query(ctx, { kind: "open", messageId: message.messageId, source })).toEqual(fixture.items[0]?.asset);
  expect(await h.expressive.query(other, { kind: "open", messageId: message.messageId, source })).toBeNull();
  const decoder: ExpressiveMediaApi = {
    decode: () => Promise.resolve({ state: "decoded", facts: { width: 2, height: 1, frameCount: null }, frame: { rgba: new Uint8ClampedArray(8), durationUs: null } }),
  };
  expect(await h.expressive.decode(ctx, message.messageId, source, decoder, { reducedMotion: true })).toEqual({ state: "placeholder", altText: "A smiling face" });
});
