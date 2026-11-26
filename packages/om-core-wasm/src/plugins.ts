// SPDX-License-Identifier: MPL-2.0
// Typed commands for the engineering-only scope-engines module. The trusted host supplies verified facts and keeps
// one host and grant table per installation. The control inputs are canonical CBOR bodies, in hex.
import type { Done } from "./scope-engines";
import type { WasmCore } from "./wasm";

export type PluginErrorCodes = Readonly<Partial<Record<string, number>>>;
export interface PluginRefusal {
  readonly refused: string;
  readonly code: number;
  /** Core's control or grant refusal variant, when applicable. */
  readonly reason?: string;
}
export interface PluginRegistry {
  readonly errorCodes: PluginErrorCodes;
  readonly capabilities: readonly { readonly id: number; readonly name: string }[];
}
export type PluginCapability = number | string;
export type PluginUint = number | bigint;

export interface PluginHostConfig {
  readonly siteIsolation: boolean;
  readonly budgets: {
    readonly maxMemoryBytes: PluginUint;
    readonly maxFuel: PluginUint;
  };
}

/** Facts checked by the trusted host for this exact outbound action. */
export interface PluginEgressFacts {
  readonly policyAllows: boolean;
  readonly oneToOne: boolean;
  readonly bothConsented: boolean;
  readonly oneShotConfirmed: boolean;
  readonly continuous: boolean;
  readonly exportAuthorized: boolean;
}

export interface PluginUiNode {
  readonly id: string;
  readonly kind: number;
  readonly parent?: string;
  readonly text?: string;
  readonly action?: string;
}

type Answer<T = Done> = T | PluginRefusal;
type Closed = { readonly done: boolean };
type Approval = { readonly outcome: "accepted" | "duplicate" | "forked" };

export interface PluginEngines {
  registry(): PluginRegistry;
  readonly host: {
    open(handle: string, config: PluginHostConfig): Answer;
    close(handle: string): Answer<Closed>;
    instantiate(handle: string, imports: readonly string[], grants: readonly PluginCapability[]): Answer;
    beginInvocation(handle: string): Answer;
    readContent(handle: string, cap: PluginCapability): Answer;
    writePrivate(handle: string): Answer;
    egress(handle: string, cap: PluginCapability, facts: PluginEgressFacts): Answer;
    /** The host showed the exact content and destination and obtained confirmation for this send. */
    sendMessage(handle: string, userConfirmedExactContent: boolean): Answer;
    update(handle: string, surface: string, nodes: readonly PluginUiNode[]): Answer;
    updateCapability(handle: string, cap: PluginCapability, freshReleaseGrant: boolean): Answer;
    account(handle: string, memoryBytes: PluginUint, fuel: PluginUint): Answer;
    reachHostState(handle: string): Answer;
    /** The booleans are the trusted host's checks against the bound origin and channel. */
    miniAppMessage(handle: string, originMatches: boolean, channelMatches: boolean, imitatesHostPrompt: boolean): Answer;
    terminate(handle: string): Answer;
    isTainted(handle: string): Answer<{ readonly tainted: boolean }>;
    isTerminated(handle: string): Answer<{ readonly terminated: boolean }>;
  };
  readonly chain: {
    genesis(handle: string, pluginId: string, anchor: string, body: string, approvers: readonly string[]): Answer<Approval>;
    resume(handle: string, anchor: string, verifiedHead: string): Answer;
    approve(handle: string, body: string, approvers: readonly string[]): Answer<Approval>;
    updateDecision(handle: string, seq: PluginUint, acceptedSeq: PluginUint, pinned?: PluginUint): Answer<{
      readonly ceiling: readonly number[];
      readonly disabledUntilGranted: readonly number[];
    }>;
    matchesRelease(handle: string, body: string): Answer;
    releaseRevoked(handle: string, seq: PluginUint): Answer<{ readonly revoked: boolean }>;
    view(handle: string): Answer<{
      readonly frozen: boolean;
      readonly head: { readonly version: string; readonly hash: string } | null;
    }>;
    close(handle: string): Answer<Closed>;
  };
  readonly grants: {
    open(handle: string): Answer;
    /** Hydrate a fresh table from host-verified persisted incarnation heads. */
    resume(handle: string, verifiedHeads: readonly string[]): Answer;
    grant(handle: string, body: string, issuerAuthorized: boolean): Answer;
    live(handle: string, incarnation: string, now: PluginUint): Answer<{ readonly live: boolean }>;
    capabilityEnabled(handle: string, chain: string, seq: PluginUint, cap: PluginCapability,
      scope: readonly [kind: PluginUint, id: string], now: PluginUint): Answer<{ readonly enabled: boolean }>;
    close(handle: string): Answer<Closed>;
  };
}

/** JSON decimal strings preserve the full u64 range. An unsafe JS number cannot be carried without rounding. */
function uint(n: PluginUint): number | string {
  if (typeof n === "bigint") return n.toString();
  return Number.isSafeInteger(n) ? n : "";
}

export function createPluginEngines(core: Pick<WasmCore, "call">): PluginEngines {
  const q = <T>(request: Record<string, unknown>): T => core.call({ cmd: "plugins_do", request }) as T;
  const host = (handle: string, op: Record<string, unknown>) => q<never>({ engine: "host", handle, op });
  const chain = (handle: string, op: Record<string, unknown>) => q<never>({ engine: "chain", handle, op });
  const grants = (handle: string, op: Record<string, unknown>) => q<never>({ engine: "grants", handle, op });
  return {
    registry: () => q({ engine: "registry" }),
    host: {
      open: (h, { siteIsolation, budgets }) => host(h, { kind: "open", siteIsolation, budgets: {
        maxMemoryBytes: uint(budgets.maxMemoryBytes), maxFuel: uint(budgets.maxFuel),
      } }),
      close: (h) => host(h, { kind: "close" }),
      instantiate: (h, imports, grants) => host(h, { kind: "instantiate", imports, grants }),
      beginInvocation: (h) => host(h, { kind: "begin_invocation" }),
      readContent: (h, cap) => host(h, { kind: "read_content", cap }),
      writePrivate: (h) => host(h, { kind: "write_private" }),
      egress: (h, cap, facts) => host(h, { kind: "egress", cap, facts }),
      sendMessage: (h, userConfirmedExactContent) => host(h, { kind: "send_message", userConfirmedExactContent }),
      update: (h, surface, nodes) => host(h, { kind: "update", surface, nodes }),
      updateCapability: (h, cap, freshReleaseGrant) => host(h, { kind: "update_capability", cap, freshReleaseGrant }),
      account: (h, memoryBytes, fuel) => host(h, { kind: "account", memoryBytes: uint(memoryBytes), fuel: uint(fuel) }),
      reachHostState: (h) => host(h, { kind: "reach_host_state" }),
      miniAppMessage: (h, originMatches, channelMatches, imitatesHostPrompt) => host(h, {
        kind: "mini_app_message", originMatches, channelMatches, imitatesHostPrompt,
      }),
      terminate: (h) => host(h, { kind: "terminate" }),
      isTainted: (h) => host(h, { kind: "is_tainted" }),
      isTerminated: (h) => host(h, { kind: "is_terminated" }),
    },
    chain: {
      genesis: (h, pluginId, anchor, body, approvers) => chain(h, { kind: "genesis", pluginId, anchor, body, approvers }),
      resume: (h, anchor, verifiedHead) => chain(h, { kind: "resume", anchor, verifiedHead }),
      approve: (h, body, approvers) => chain(h, { kind: "approve", body, approvers }),
      updateDecision: (h, seq, acceptedSeq, pinned) => chain(h, {
        kind: "update_decision", seq: uint(seq), acceptedSeq: uint(acceptedSeq), pinned: pinned === undefined ? null : uint(pinned),
      }),
      matchesRelease: (h, body) => chain(h, { kind: "matches_release", body }),
      releaseRevoked: (h, seq) => chain(h, { kind: "release_revoked", seq: uint(seq) }),
      view: (h) => chain(h, { kind: "view" }),
      close: (h) => chain(h, { kind: "close" }),
    },
    grants: {
      open: (h) => grants(h, { kind: "open" }),
      resume: (h, verifiedHeads) => grants(h, { kind: "resume", verifiedHeads }),
      grant: (h, body, issuerAuthorized) => grants(h, { kind: "grant", body, issuerAuthorized }),
      live: (h, incarnation, now) => grants(h, { kind: "live", incarnation, now: uint(now) }),
      capabilityEnabled: (h, chain, seq, cap, [kind, id], now) => grants(h, {
        kind: "capability_enabled", chain, seq: uint(seq), cap, scope: [uint(kind), id], now: uint(now),
      }),
      close: (h) => grants(h, { kind: "close" }),
    },
  };
}
