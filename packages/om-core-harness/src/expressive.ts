// SPDX-License-Identifier: MPL-2.0
// DEV/TEST ONLY. Fixtures stand in for Core-validated manifests and opened media.
import type {
  ExpressiveApi, ExpressivePackInput, ExpressivePackItem, ExpressivePackPreview, ExpressivePackQuery,
  ExpressivePicker, ExpressiveQueryResult, ExpressiveSource, MessageId, OpenedExpressive,
  ProfileExecutionContext,
} from "@om/platform-api";

export interface SeedExpressivePack {
  readonly input: ExpressivePackInput;
  readonly preview: ExpressivePackPreview;
  readonly items: readonly {
    readonly itemId: string;
    readonly blobId: string;
    readonly label: string;
    readonly asset: Extract<OpenedExpressive, { readonly state: "renderable" }>;
    readonly fallbackText?: string;
  }[];
}

type Renderable = SeedExpressivePack["items"][number]["asset"];

function equalBytes(a: Uint8Array | undefined, b: Uint8Array | undefined): boolean {
  return a === undefined ? b === undefined : b !== undefined && a.length === b.length && a.every((byte, i) => byte === b[i]);
}

export function createExpressiveSim(options: {
  enabled: boolean;
  settle: <T>(fn: () => T) => Promise<T>;
  stateFor: (ctx: ProfileExecutionContext) => object;
  newBlobId: () => string;
  opened: (ctx: ProfileExecutionContext, messageId: MessageId, source: ExpressiveSource) => OpenedExpressive | null;
}) {
  interface HeldPack {
    readonly fixture: SeedExpressivePack;
    readonly policy: ExpressivePicker["pack"]["cachePolicy"];
    readonly favourites: Set<string>;
    readonly recents: Set<string>;
    readonly cached: Set<string>;
  }
  interface ProfileState {
    fixtures: readonly SeedExpressivePack[];
    packs: Map<string, HeldPack>;
    prepared: Map<string, SeedExpressivePack["items"][number]>;
  }
  const states = new WeakMap<object, ProfileState>();
  const state = (ctx: ProfileExecutionContext): ProfileState => {
    const owner = options.stateFor(ctx);
    const held: ProfileState = states.get(owner) ?? { fixtures: [], packs: new Map(), prepared: new Map() };
    states.set(owner, held);
    return held;
  };
  const view = (p: HeldPack): ExpressivePicker["pack"] => ({
    releaseId: p.fixture.preview.releaseId,
    title: p.fixture.preview.title,
    itemCount: p.fixture.items.length,
    cachePolicy: p.policy,
    favourites: [...p.favourites].sort(),
    cachedBlobs: p.cached.size,
    signedBy: p.fixture.preview.signedBy,
  });
  const fixtureFor = (ctx: ProfileExecutionContext, input: ExpressivePackInput) =>
    state(ctx).fixtures.find((f) => equalBytes(f.input.release, input.release) && equalBytes(f.input.control, input.control));

  const api: ExpressiveApi = {
    available: () => options.enabled,
    do: (ctx, intent) => options.settle(() => {
      const held = state(ctx);
      if (!options.enabled) return { refused: "expressive_disabled" };
      switch (intent.kind) {
        case "install_pack": {
          const fixture = fixtureFor(ctx, intent.input);
          if (fixture === undefined) return { refused: "invalid" };
          if (fixture.preview.releaseId !== intent.confirmation.releaseId || fixture.preview.controlDigest !== intent.confirmation.controlDigest) {
            return { refused: "confirmation_required" };
          }
          const favourites = new Set(intent.favourites);
          if (favourites.size !== intent.favourites.length || intent.favourites.some((id) => !fixture.items.some((item) => item.itemId === id))) {
            return { refused: "item_missing_or_duplicate" };
          }
          const eligible = fixture.items.filter((item) => intent.policy !== "FavouritesOnly" || favourites.has(item.itemId));
          const cached = new Set<string>();
          for (const blob of intent.cached) {
            const item = eligible.find((x) => x.blobId === blob.blobId);
            if (item === undefined) return { refused: "blob_outside_pack_or_policy" };
            if (cached.has(blob.blobId)) return { refused: "blob_duplicate" };
            if (!equalBytes(item.asset.asset.bytes, blob.ciphertext)) return { refused: "invalid_cached_blob" };
            cached.add(blob.blobId);
          }
          if (intent.policy !== "Lazy" && eligible.some((item) => !cached.has(item.blobId))) return { refused: "cache_incomplete" };
          const current = held.packs.get(fixture.preview.packId);
          const recents = current?.fixture.preview.releaseId === fixture.preview.releaseId ? current.recents : new Set<string>();
          held.packs.set(fixture.preview.packId, { fixture, policy: intent.policy, cached, favourites, recents });
          break;
        }
        case "remove_pack":
          if (!held.packs.delete(intent.packId)) return { refused: "pack_missing" };
          break;
        case "update_picker": {
          const pack = held.packs.get(intent.packId);
          if (pack === undefined) return { refused: "pack_missing" };
          if (pack.fixture.preview.releaseId !== intent.releaseId) return { refused: "release_id_mismatch" };
          const item = pack.fixture.items.find((x) => x.itemId === intent.itemId);
          if (item === undefined) return { refused: "item_missing_or_duplicate" };
          if (intent.change.kind === "recent") {
            pack.recents.delete(item.itemId);
            pack.recents.add(item.itemId);
          } else if (intent.change.present) {
            pack.favourites.add(item.itemId);
          } else {
            pack.favourites.delete(item.itemId);
            if (pack.policy === "FavouritesOnly" && !pack.fixture.items.some((x) => x.blobId === item.blobId && pack.favourites.has(x.itemId))) {
              pack.cached.delete(item.blobId);
            }
          }
          break;
        }
      }
      return { accepted: true };
    }),
    query<Q extends ExpressivePackQuery>(ctx: ProfileExecutionContext, q: Q): Promise<ExpressiveQueryResult[Q["kind"]]> {
      return options.settle(() => {
        const held = state(ctx);
        let result: ExpressiveQueryResult[ExpressivePackQuery["kind"]];
        switch (q.kind) {
          case "packs":
            result = (options.enabled ? [...held.packs].map(([id, p]) => [id, view(p)] as const) : []);
            break;
          case "picker": {
            const pack = options.enabled ? held.packs.get(q.packId) : undefined;
            result = (pack === undefined ? null : {
              packId: q.packId,
              pack: view(pack),
              recents: [...pack.recents].reverse(),
              items: pack.fixture.items.map((item) => ({
                itemId: item.itemId, blobId: item.blobId, label: item.label, kind: item.asset.asset.kind,
                altText: item.asset.altText, fallbackText: item.fallbackText ?? null,
                cached: pack.cached.has(item.blobId), favourite: pack.favourites.has(item.itemId),
              })),
            });
            break;
          }
          case "preview_pack": {
            const fixture = options.enabled ? fixtureFor(ctx, q.input) : undefined;
            result = (fixture === undefined ? { refused: options.enabled ? "invalid" : "expressive_disabled" } : { preview: fixture.preview });
            break;
          }
          case "open": {
            const opened = options.enabled ? options.opened(ctx, q.messageId, q.source) : null;
            result = (opened?.state === "renderable"
              ? { ...opened, asset: { ...opened.asset, bytes: opened.asset.bytes.slice() } }
              : opened);
            break;
          }
        }
        return result as ExpressiveQueryResult[Q["kind"]];
      });
    },
    prepare: (ctx, selection: ExpressivePackItem, ciphertext) => options.settle(() => {
      const held = state(ctx);
      if (!options.enabled) return { ok: false, refusal: { error: "unsupported" } };
      const pack = held.packs.get(selection.packId);
      const item = pack?.fixture.preview.releaseId === selection.releaseId ? pack.fixture.items.find((x) => x.itemId === selection.itemId) : undefined;
      if (item === undefined) return { ok: false, refusal: { error: "not_found" } };
      if (!equalBytes(item.asset.asset.bytes, ciphertext)) return { ok: false, refusal: { error: "integrity_failure" } };
      const blobId = options.newBlobId();
      held.prepared.set(blobId, item);
      return { ok: true, blobId };
    }),
    async decode(ctx, messageId, source, decoder, decodeOptions) {
      const opened = options.enabled ? options.opened(ctx, messageId, source) : null;
      if (opened?.state !== "renderable") return opened;
      const { width, height, frames } = opened.asset.facts;
      const placeholder = { state: "placeholder" as const, altText: opened.altText };
      let decoded;
      try { decoded = await decoder.decode({ ...opened.asset, bytes: opened.asset.bytes.slice() }, decodeOptions); }
      catch { return placeholder; }
      if (decoded.state === "fallback") return placeholder;
      const current = options.opened(ctx, messageId, source);
      if (current !== opened || decoded.facts.width !== width || decoded.facts.height !== height ||
          (decoded.facts.frameCount !== null && decoded.facts.frameCount !== frames)) {
        decoded.playback?.close();
        return placeholder;
      }
      return { ...decoded, altText: opened.altText };
    },
  };
  return {
    api,
    seed(ctx: ProfileExecutionContext, fixtures: readonly SeedExpressivePack[]) { state(ctx).fixtures = fixtures; },
    prepared(ctx: ProfileExecutionContext, blobId: string): { asset: Renderable; fallbackText?: string } | undefined {
      return state(ctx).prepared.get(blobId);
    },
    consume(ctx: ProfileExecutionContext, blobId: string) { state(ctx).prepared.delete(blobId); },
  };
}

export function sourceKey(source: ExpressiveSource): string {
  if (source.kind === "reaction") return JSON.stringify([source.kind, source.mine, source.key]);
  return JSON.stringify([source.kind, source.index]);
}
