// SPDX-License-Identifier: MPL-2.0
// Replay the ADR-0266 control corpus and the policy cases in crates/om-core/tests/plugin_adversarial.rs.
// Build the engineering module with pnpm --filter @om/core-wasm build:wasm:scope before running this suite.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { createPluginEngines } from "./scope-engines";
import type { PluginEgressFacts, PluginEngines, PluginRegistry, PluginUiNode } from "./scope-engines";
import { instantiateCore } from "./wasm";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const repo = new URL("../../../", import.meta.url);
const registry = readJson<{ enums: Record<"plugin_capability_id" | "plugin_error_code", Record<string, string>> }>(
  "spec/registries/om-numeric-fields.json",
).enums;

function readJson<T>(path: string): T {
  return JSON.parse(readFileSync(new URL(path, repo), "utf8")) as T;
}

type FixtureValue = number | boolean | string | FixtureMap | FixtureValue[];
interface FixtureMap { [key: string]: FixtureValue }
interface Vector {
  id: string;
  object: string;
  error_class?: string;
  value: FixtureMap;
  wire_hex: string;
  prior_value?: FixtureMap;
  control_value?: FixtureMap;
  now?: number;
}
const corpus = readJson<{ positive: Vector[]; negative: Vector[] }>("verification/prototypes/plugins-wire/vectors.json");

function vector(id: string): Vector {
  const found = [...corpus.positive, ...corpus.negative].find((v) => v.id.startsWith(id + "-"));
  if (!found) throw new Error(`Missing plugin vector ${id}`);
  return found;
}

/** Extract the exact body from key 1 of the corpus's deterministic two-entry SignedEnvelope. */
function body(v: Vector): string {
  const bytes = Buffer.from(v.wire_hex, "hex");
  const header = bytes.readUInt8(2);
  if (bytes.readUInt16BE(0) !== 0xa201 || header < 0x40 || header > 0x5a) {
    throw new Error(`Expected a SignedEnvelope in ${v.id}`);
  }
  const size = header < 0x58 ? 0 : 1 << (header - 0x58);
  const length = size === 0 ? header - 0x40 : bytes.readUIntBE(3, size);
  return bytes.subarray(3 + size, 3 + size + length).toString("hex");
}

function cborHeader(major: number, n: number): Buffer {
  if (n < 24) return Buffer.from([(major << 5) | n]);
  const size = n <= 0xff ? 1 : n <= 0xffff ? 2 : 4;
  const out = Buffer.alloc(1 + size);
  out[0] = (major << 5) | (size === 1 ? 24 : size === 2 ? 25 : 26);
  out.writeUIntBE(n, 1, size);
  return out;
}

function cborBytes(major: number, bytes: Buffer): Buffer {
  return Buffer.concat([cborHeader(major, bytes.length), bytes]);
}

/** Encode the corpus's decoded prior bodies, as the Rust replay does. */
function fixtureCbor(value: FixtureValue): Buffer {
  if (typeof value === "boolean") return Buffer.from([value ? 0xf5 : 0xf4]);
  if (typeof value === "number") return cborHeader(0, value);
  if (typeof value === "string") return cborBytes(3, Buffer.from(value));
  if (Array.isArray(value)) return Buffer.concat([cborHeader(4, value.length), ...value.map(fixtureCbor)]);
  if (typeof value.$bytes === "string") return cborBytes(2, Buffer.from(value.$bytes, "hex"));
  const entries = Object.entries(value).sort(([a], [b]) => Number(a) - Number(b));
  return Buffer.concat([cborHeader(5, entries.length), ...entries.flatMap(([key, v]) => [
    cborHeader(0, Number(key)), fixtureCbor(v),
  ])]);
}

function controlHash(bodyHex: string): string {
  const input = Buffer.concat([
    cborHeader(4, 2),
    fixtureCbor("OM-PLUGIN-CONTROL-HASH-v1"),
    cborBytes(2, Buffer.from(bodyHex, "hex")),
  ]);
  return createHash("sha256").update(input).digest("hex");
}

const bytesField = (v: FixtureValue | undefined) => (v as { $bytes: string }).$bytes;
const approvers = (s: FixtureMap) => (s["5"] as FixtureMap[]).map((key) => bytesField(key["0"]));

const done = { done: true };
const accepted = { outcome: "accepted" };
const budgets = { maxMemoryBytes: 16 << 20, maxFuel: 10_000_000 };
const u64Max = (1n << 64n) - 1n;
const noEgress: PluginEgressFacts = {
  policyAllows: false, oneToOne: false, bothConsented: false,
  oneShotConfirmed: false, continuous: false, exportAuthorized: false,
};

const content = "content.read_current_conversation";
const net = "net.fetch_origin_scoped";
const send = "conversation.send_message";
const node = (id: number, kind: number, parent?: number): PluginUiNode => ({
  id: id.toString(16).padStart(32, "0"), kind, text: "Label",
  ...(parent === undefined ? {} : { parent: parent.toString(16).padStart(32, "0") }),
});
const tree = [node(1, 4), node(2, 2, 1)];

type Step = (host: PluginEngines["host"], handle: string) => unknown;
const instantiate = (imports: readonly string[], grants: readonly string[] = imports): Step =>
  (host, h) => host.instantiate(h, imports, grants);
const start = instantiate([content, net]);
const read: Step = (host, h) => host.readContent(h, content);
const egress = (cap = net, facts = noEgress): Step => (host, h) => host.egress(h, cap, facts);
const sendStep = (confirmed: boolean): Step => (host, h) => host.sendMessage(h, confirmed);
const updateCap = (fresh: boolean): Step => (host, h) => host.updateCapability(h, net, fresh);
const account = (memory: number | bigint, fuel: number | bigint): Step => (host, h) => host.account(h, memory, fuel);
const update = (surface: string, nodes: readonly PluginUiNode[] = tree): Step => (host, h) => host.update(h, surface, nodes);
const miniApp = (origin: boolean, channel: boolean, prompt: boolean): Step =>
  (host, h) => host.miniAppMessage(h, origin, channel, prompt);

const cases: readonly { id: string; steps: readonly Step[]; refused?: string }[] = [
  { id: "C1-granted-imports-instantiate", steps: [start] },
  { id: "C2-untainted-network-use", steps: [start, egress()] },
  { id: "C3-confirmed-send", steps: [instantiate([send]), sendStep(true)] },
  { id: "C4-small-resource-use", steps: [account(1 << 20, 1000)] },
  { id: "C5-declarative-ui-tree", steps: [update("panel")] },
  { id: "C6-update-within-ceiling-or-freshly-granted", steps: [updateCap(true)] },
  { id: "H1-ambient-wasi-import", steps: [instantiate(["wasi:filesystem/types"], [])], refused: "unknown_capability" },
  { id: "H2-import-without-grant", steps: [instantiate([send], ["ui.panel"])], refused: "capability_not_granted" },
  { id: "H3-host-memory-and-key-read", steps: [(host, h) => host.reachHostState(h)], refused: "host_policy_denied" },
  { id: "H4-egress-after-plaintext-read", steps: [start, read, egress()], refused: "tainted_egress_denied" },
  { id: "H5-egress-via-service-invoke-after-read", steps: [start, read, egress("service.invoke")], refused: "tainted_egress_denied" },
  {
    id: "H6-launder-through-plugin-private-storage",
    steps: [start, read, (host, h) => host.writePrivate(h), (host, h) => host.beginInvocation(h), egress()],
    refused: "tainted_egress_denied",
  },
  {
    id: "H7-one-to-one-needs-both-participants",
    steps: [start, read, egress(net, { ...noEgress, policyAllows: true, oneToOne: true })],
    refused: "bilateral_consent_required",
  },
  {
    id: "H8-continuous-egress-is-export",
    steps: [start, read, egress(net, { ...noEgress, policyAllows: true, continuous: true })],
    refused: "tainted_egress_denied",
  },
  { id: "H9-send-without-confirmation", steps: [instantiate([send]), sendStep(false)], refused: "user_confirmation_required" },
  { id: "H10-capability-escalation-on-update", steps: [updateCap(false)], refused: "release_not_authorized" },
  { id: "H11-memory-exhaustion", steps: [account(u64Max, 1000)], refused: "resource_budget_exceeded" },
  { id: "H12-fuel-exhaustion-loop", steps: [account(1 << 20, u64Max)], refused: "resource_budget_exceeded" },
  { id: "H13-ui-covers-security-surface", steps: [update("om.security.verification")], refused: "invalid_ui_tree" },
  {
    id: "H14-ui-spoofs-host-prompt", steps: [update("panel", [node(1, 4), { ...node(2, 2, 1), action: "om.permission.allow" }])],
    refused: "invalid_ui_tree",
  },
  { id: "H15-raw-dom-ownership", steps: [update("panel", [node(1, 4), node(2, 7, 1)])], refused: "invalid_ui_tree" },
  { id: "H16-mini-app-wrong-origin", steps: [miniApp(false, true, false)], refused: "origin_channel_mismatch" },
  { id: "H17-mini-app-wrong-channel", steps: [miniApp(true, false, false)], refused: "origin_channel_mismatch" },
  { id: "H18-mini-app-imitates-host-prompt", steps: [miniApp(true, true, true)], refused: "invalid_ui_tree" },
];

const genesis = vector("P02");
const genesisBody = body(genesis);
const pluginId = bytesField(genesis.value["2"]);
const anchor = controlHash(genesisBody);
const publishers = approvers(genesis.value);
const updateBody = body(vector("P15"));
const revocation = body(vector("P19"));
const activeGrant = vector("P03");
const grantRevocation = vector("P18");
const incarnation = bytesField(activeGrant.value["11"]);

const controlRefusals: Partial<Record<string, string>> = {
  N48: "MissingPrev",
  N50: "UnknownPrev",
  N51: "BadVersion",
  N52: "KeyRemoved",
  N53: "UnknownPrev",
  N84: "UnknownPrev",
  N103: "ReleaseResurrected",
};
const grantRefusals: Partial<Record<string, string>> = {
  N45: "Resurrection",
  N46: "Fork",
  N47: "Resurrection",
  N102: "Resurrection",
};

describe.skipIf(!existsSync(wasm))("plugin host and control through scope-engines", () => {
  let plugins: PluginEngines;
  let pluginRegistry: PluginRegistry;

  beforeAll(async () => {
    plugins = createPluginEngines(await instantiateCore(readFileSync(wasm)));
    pluginRegistry = plugins.registry();
  });

  function refusal(refused: string, reason?: string) {
    return {
      refused, code: pluginRegistry.errorCodes[refused],
      ...(reason === undefined ? {} : { reason }),
    };
  }

  it("matches both registry tables against the frozen registrations", () => {
    const errorCodes = Object.fromEntries(Object.entries(registry.plugin_error_code).map(([id, name]) => [name, Number(id)]));
    const capabilities = Object.entries(registry.plugin_capability_id).map(([id, name]) => ({ id: Number(id), name }));
    expect(pluginRegistry).toEqual({ errorCodes, capabilities });
  });

  it.each(cases)("$id", ({ id, steps, refused }) => {
    expect(plugins.host.open(id, { siteIsolation: true, budgets })).toEqual(done);
    for (const [index, step] of steps.entries()) {
      expect(step(plugins.host, id)).toEqual(index === steps.length - 1 && refused ? refusal(refused) : done);
    }
    if (refused === "resource_budget_exceeded") {
      expect(plugins.host.isTerminated(id)).toEqual({ terminated: true });
      expect(plugins.host.beginInvocation(id)).toEqual(refusal("host_policy_denied"));
    }
    expect(plugins.host.close(id)).toEqual(done);
  });

  it("keeps termination final and isolates host handles", () => {
    const { host } = plugins;
    expect(host.open("stop", { siteIsolation: true, budgets })).toEqual(done);
    expect(host.open("other", { siteIsolation: true, budgets })).toEqual(done);
    expect(host.instantiate("stop", [content, send], [content, send])).toEqual(done);
    expect(host.readContent("stop", content)).toEqual(done);
    expect(host.isTainted("stop")).toEqual({ tainted: true });
    expect(host.isTainted("other")).toEqual({ tainted: false });
    expect(host.terminate("stop")).toEqual(done);
    expect(host.isTerminated("stop")).toEqual({ terminated: true });
    expect(host.open("stop", { siteIsolation: true, budgets })).toEqual(refusal("host_policy_denied"));
    expect(host.instantiate("stop", [send], [send])).toEqual(refusal("host_policy_denied"));
    expect(host.sendMessage("stop", true)).toEqual(refusal("host_policy_denied"));
    expect(host.update("stop", "panel", tree)).toEqual(refusal("host_policy_denied"));
    expect(host.isTerminated("other")).toEqual({ terminated: false });
  });

  it("requires caller budgets and refuses content imports without site isolation", () => {
    const { host } = plugins;
    expect(host.open("bad-budget", { siteIsolation: true, budgets: { ...budgets, maxFuel: u64Max + 1n } }))
      .toEqual(refusal("malformed_request"));
    expect(host.open("no-isolation", { siteIsolation: false, budgets })).toEqual(done);
    expect(host.instantiate("no-isolation", [content], [content])).toEqual(refusal("host_policy_denied"));
    const ui = pluginRegistry.capabilities.find(({ name }) => name === "ui.panel");
    if (!ui) throw new Error("Missing ui.panel capability");
    expect(host.instantiate("no-isolation", [ui.name], [ui.id])).toEqual(done);
  });

  it("validates UI depth, node count, UTF-8 limits and tree structure", () => {
    const { host } = plugins;
    expect(host.open("ui", { siteIsolation: true, budgets })).toEqual(done);
    const depth = (n: number) => Array.from({ length: n }, (_, i) => node(i, 4, i ? i - 1 : undefined));
    const width = (n: number) => Array.from({ length: n }, (_, i) => node(i, 4, i ? 0 : undefined));
    expect(host.update("ui", "panel", depth(8))).toEqual(done);
    expect(host.update("ui", "panel", width(128))).toEqual(done);
    expect(host.update("ui", "panel", [{ ...node(1, 4), text: "é".repeat(512), action: "a".repeat(128) }])).toEqual(done);
    for (const nodes of [
      [], depth(9), width(129), [node(1, 4), node(1, 2, 1)], [node(1, 4), node(2, 2)],
      [node(1, 4), node(2, 2, 3)], [node(1, 4), node(2, 2, 3), node(3, 2, 2)],
      [{ ...node(1, 4), text: "é".repeat(513) }], [{ ...node(1, 4), action: "a".repeat(129) }],
    ]) {
      expect(host.update("ui", "panel", nodes)).toEqual(refusal("invalid_ui_tree"));
    }
  });

  it("replays the frozen genesis, update, manifest and release revocation", () => {
    const { chain } = plugins;
    expect(chain.genesis("chain", pluginId, "00".repeat(32), genesisBody, publishers))
      .toEqual(refusal("release_not_authorized", "MissingPrev"));
    expect(chain.genesis("chain", pluginId, anchor, genesisBody, publishers)).toEqual(accepted);
    expect(chain.approve("chain", genesisBody, publishers)).toEqual({ outcome: "duplicate" });
    expect(chain.matchesRelease("chain", body(vector("P14")))).toEqual(refusal("release_not_authorized"));
    expect(chain.approve("chain", updateBody, publishers)).toEqual(accepted);
    expect(chain.matchesRelease("chain", body(vector("P01")))).toEqual(done);
    expect(chain.matchesRelease("chain", body(vector("P14")))).toEqual(done);
    expect(chain.updateDecision("chain", 2, 1)).toEqual({
      ceiling: [1, 3, 5, 11, 14, 17], disabledUntilGranted: [2],
    });
    expect(chain.updateDecision("chain", 1, 2)).toEqual(refusal("release_not_authorized", "Rollback"));
    expect(chain.updateDecision("chain", 1, 2, 1)).toMatchObject({ disabledUntilGranted: [] });
    expect(chain.updateDecision("chain", 9, 2)).toEqual(refusal("release_not_authorized", "UnknownRelease"));
    expect(chain.approve("chain", revocation, publishers)).toEqual(accepted);
    expect(chain.approve("chain", updateBody, publishers)).toEqual(refusal("release_not_authorized", "Rollback"));
    expect(chain.updateDecision("chain", 1, 2, 1)).toEqual(refusal("release_not_authorized", "Revoked"));
    expect(chain.releaseRevoked("chain", 1)).toEqual({ revoked: true });
    expect(chain.releaseRevoked("chain", 2)).toEqual({ revoked: false });
    expect(chain.view("chain")).toEqual({ frozen: false, head: { version: "3", hash: controlHash(revocation) } });
  });

  it.each(corpus.negative.filter(
    (v) => v.object === "PluginControlState" && ["stale_chain", "forked_chain", "capability_escalation"].includes(v.error_class ?? ""),
  ))("refuses $id", (v) => {
    const { chain } = plugins;
    const candidate = body(v);
    let result;
    if (v.prior_value) {
      const prior = fixtureCbor(v.prior_value).toString("hex");
      expect(chain.resume(v.id, controlHash(prior), prior)).toEqual(done);
      result = chain.approve(v.id, candidate, approvers(v.prior_value));
      expect(chain.close(v.id)).toEqual(done);
    } else {
      result = chain.genesis(v.id, bytesField(v.value["2"]), controlHash(candidate), candidate, approvers(v.value));
    }
    const reason = controlRefusals[v.id.split("-")[0]!];
    expect(result).toEqual(reason ? refusal("release_not_authorized", reason) : refusal("malformed_request"));
  });

  it.each(corpus.negative.filter((v) => v.object === "PluginPackageManifest" && v.control_value))(
    "refuses $id", (v) => {
      const prior = fixtureCbor(v.control_value!).toString("hex");
      expect(plugins.chain.resume(v.id, controlHash(prior), prior)).toEqual(done);
      expect(plugins.chain.matchesRelease(v.id, body(v))).toEqual(refusal("release_not_authorized"));
      expect(plugins.chain.close(v.id)).toEqual(done);
    },
  );

  it("carries only live grants and requires an exact-release grant beyond the ceiling", () => {
    const { chain, grants } = plugins;
    expect(chain.resume("grant-release", anchor, updateBody)).toEqual(done);
    expect(grants.open("grant-table")).toEqual(done);
    const active = body(activeGrant);
    expect(grants.grant("grant-table", active, false)).toEqual(refusal("host_policy_denied", "IssuerNotAuthorized"));
    expect(grants.grant("grant-table", active, true)).toEqual(done);
    const scope = [1, bytesField(activeGrant.value["2"])] as const;
    const enabled = (cap: number, now = 50) => grants.capabilityEnabled("grant-table", "grant-release", 2, cap, scope, now);
    expect(enabled(1)).toEqual({ enabled: true });
    expect(enabled(2)).toEqual({ enabled: false });
    expect(grants.grant("grant-table", body(vector("P16")), true)).toEqual(done);
    expect(enabled(2)).toEqual({ enabled: true });
    expect(enabled(9)).toEqual({ enabled: false });
    expect(grants.live("grant-table", incarnation, 5)).toEqual({ live: false });
    expect(grants.live("grant-table", incarnation, 50)).toEqual({ live: true });
    expect(grants.live("grant-table", incarnation, 100_000)).toEqual({ live: false });
    expect(grants.grant("grant-table", body(grantRevocation), true)).toEqual(done);
    expect(enabled(1)).toEqual({ enabled: false });
    expect(grants.live("grant-table", incarnation, 50)).toEqual({ live: false });
    expect(grants.grant("grant-table", active, true)).toEqual(refusal("capability_not_granted", "Resurrection"));
    expect(grants.resume("grant-table", [active])).toEqual(refusal("host_policy_denied"));
  });

  it.each(corpus.negative.filter(
    (v) => v.object === "PluginCapabilityGrant" && ["stale_chain", "forked_chain", "grant_expired"].includes(v.error_class ?? ""),
  ))("replays $id", (v) => {
    const { grants } = plugins;
    if (v.prior_value) {
      expect(grants.resume(v.id, [fixtureCbor(v.prior_value).toString("hex")])).toEqual(done);
      expect(grants.grant(v.id, body(v), true)).toEqual(refusal("capability_not_granted", grantRefusals[v.id.split("-")[0]!]));
    } else {
      expect(grants.open(v.id)).toEqual(done);
      expect(grants.grant(v.id, body(v), true)).toEqual(done);
      expect(grants.live(v.id, bytesField(v.value["11"]), v.now!)).toEqual({ live: false });
    }
    expect(grants.close(v.id)).toEqual(done);
  });
});
