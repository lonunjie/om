// SPDX-License-Identifier: MPL-2.0
import { execFileSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { asProfileId } from "@om/platform-api";
import { describe, expect, it } from "vitest";
import { createCoreRuntime } from "./runtime";
import { createScopeEngines } from "./scope-engines";
import { createMemoryKv, createMemoryProvider } from "./test-support";
import { instantiateCore } from "./wasm";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const corpus = fileURLToPath(new URL("../../../verification/prototypes/v008-space-wire/", import.meta.url));
const object = "aa".repeat(32);

async function openScope() {
  const core = await instantiateCore(readFileSync(wasm));
  const runtime = await createCoreRuntime({
    core, provider: createMemoryProvider(), storage: () => createMemoryKv(), autoDrain: false,
    home: { tier: "none", observers: ["no_third_party"], migrationAvailable: false, clientOriginChoice: ["default_origin"] },
  });
  return { runtime, engines: createScopeEngines(core) };
}

function frozenVectors() {
  // Replay the same V-008 corpus that the Rust body tests use. Its signatures here are shape-only.
  const python = `
import json, sys
sys.path.insert(0, sys.argv[1])
import python_codec as c
v = json.load(open(sys.argv[1] + '/vectors.json'))
def wire(row):
    body = c.norm(row['value'])
    event = row['object'] == 'PublicationEvent'
    signer = body.get(7, b'\\x01' * 32) if event else b'\\x07' * 16
    return c.enc({1: c.enc(body), 2: [{0: signer, 1: 1, 2: 7 if event else 1, 3: bytes(3373)}]}).hex()
print(json.dumps({row['id']: wire(row) for kind in ('positive', 'negative') for row in v[kind]
                 if row['object'] in ('SpaceControlState', 'PublicationControlState', 'PublicationEvent')}))
`;
  return JSON.parse(execFileSync("python3", ["-IB", "-c", python, corpus], { encoding: "utf8" })) as Record<string, string>;
}

describe.skipIf(!existsSync(wasm))("engineering wasm community engines", () => {
  it("replays frozen V-008 bodies and refuses to promote unsigned authority", async () => {
    const { runtime, engines } = await openScope();
    try {
      const ctx = await runtime.openProfile(asProfileId("vector-reader"));
      const vectors = frozenVectors();
      expect(engines.space.open(ctx, object)).toEqual({ done: true });
      expect(engines.publication.open(ctx, object)).toEqual({ done: true });
      expect(engines.space.acceptState(ctx, object, vectors["space-minimal"]!)).toEqual({ refused: "unknown_signer" });
      expect(engines.space.acceptState(ctx, object, vectors["space-populated"]!)).toEqual({ refused: "unknown_signer" });
      expect(engines.publication.acceptControl(ctx, object, vectors["publication-control"]!)).toEqual({ refused: "unknown_signer" });
      expect(engines.publication.acceptEvent(ctx, object, vectors["publication-post"]!, "00".repeat(1984))).toEqual({ refused: "unverifiable_key" });
      expect(engines.space.acceptState(ctx, object, vectors["space-missing-profile"]!)).toEqual({ refused: "malformed" });
      expect(engines.publication.acceptControl(ctx, object, vectors["publication-empty-authority"]!)).toEqual({ refused: "malformed" });
      expect(engines.publication.acceptEvent(ctx, object, vectors["publication-bad-kind"]!, "")).toEqual({ refused: "malformed" });
      expect(engines.space.view(ctx, object)).toEqual({ head: null, governance: null });
      expect(engines.publication.view(ctx, object)).toEqual({
        head: null, frozen: false, provenance: "checking", conflicts: [], posts: [],
      });
      expect(engines.publication.close(ctx, object)).toEqual({ done: true });
      expect(engines.space.close(ctx, object)).toEqual({ done: true });
      expect(engines.publication.view(ctx, object)).toEqual({ refused: "unknown_handle" });
      expect(engines.space.view(ctx, object)).toEqual({ refused: "unknown_handle" });
    } finally {
      runtime.dispose();
    }
  });

  it("keeps observations, handles and review targets profile-local", async () => {
    const { runtime, engines } = await openScope();
    try {
      const a = await runtime.openProfile(asProfileId("a"));
      const b = await runtime.openProfile(asProfileId("b"));
      expect(engines.publication.open(a, object)).toEqual({ done: true });
      expect(engines.publication.view(b, object)).toEqual({ refused: "unknown_handle" });
      expect(engines.directory.observe(a, "community", object, {
        name: "Garden\u202e\n", description: "x".repeat(161),
      })).toEqual({ done: true });
      const options = { incarnation: "ab".repeat(16), approvalRequired: true, uses: "one_time" } as const;
      const first = engines.directory.observeOffer(a, "community", object, options);
      const second = engines.directory.observeOffer(a, "community", object, { ...options, approvalRequired: false });
      if ("refused" in first || "refused" in second) throw new Error("Expected observed admission offers");
      expect(second.target).not.toEqual(first.target);
      expect(engines.directory.withdrawOffer(a, "community", object, first.target)).toEqual({ refused: "stale_target" });
      expect(engines.directory.list(a)).toEqual({ entries: [{
        kind: "community", id: object, name: "Garden", description: "x".repeat(160), source: "host_observed",
        offer: { target: second.target, approvalRequired: false, uses: "one_time", expiresAtMs: null },
      }] });
      expect(engines.directory.list(b)).toEqual({ entries: [] });
      expect(engines.directory.withdrawOffer(a, "community", object, second.target)).toEqual({ done: true });
      expect(engines.directory.withdraw(a, "community", object)).toEqual({ done: true });
      expect(engines.directory.list(a)).toEqual({ entries: [] });
    } finally {
      runtime.dispose();
    }
  });
});
