// SPDX-License-Identifier: MPL-2.0
import type { ProfileExecutionContext } from "@om/platform-api";
import type { Done, Refusal } from "./scope-engines";
import type { WasmCore } from "./wasm";

export type DirectoryKind = "community" | "room" | "publication";
export type AdmissionPlace = Exclude<DirectoryKind, "publication">;
export type AdmissionUses = "one_time" | "limited" | "reusable";

export interface AdmissionTerms {
  readonly approvalRequired: boolean;
  readonly uses?: AdmissionUses;
  readonly expiresAtMs?: number;
}
export interface DirectoryEntry {
  readonly kind: DirectoryKind;
  readonly id: string;
  readonly name: string;
  readonly description: string | null;
  readonly source: "host_observed";
  readonly offer: {
    readonly target: string;
    readonly approvalRequired: boolean;
    readonly uses: AdmissionUses | null;
    readonly expiresAtMs: number | null;
  } | null;
}

export interface ScopedControlHead {
  readonly hash: string;
  /** Decimal u64. */
  readonly version: string;
}
export interface SpaceHead extends ScopedControlHead {
  readonly member: boolean | null;
  readonly governanceThreshold: number;
  readonly links: readonly { readonly kind: DirectoryKind; readonly id: string }[];
}
export interface SpaceView {
  readonly head: SpaceHead | null;
  /** Null until there is a verified head and a reachability observation. */
  readonly governance: "active" | "orphaned" | "conflict" | null;
}
export interface PublicationHead extends ScopedControlHead {
  readonly discussion: string | null;
}
export type ScopedPublicationPost = {
  readonly id: string;
  readonly publishedAtMs: number;
} & (
  | { readonly state: "published" | "edited"; readonly text: string; readonly reason: null }
  | { readonly state: "retracted"; readonly text: null; readonly reason: string | null }
);
export interface PublicationView {
  readonly head: PublicationHead | null;
  readonly frozen: boolean;
  readonly provenance: "confirmed" | "checking" | "conflict";
  readonly conflicts: readonly (readonly [string, string])[];
  readonly posts: readonly ScopedPublicationPost[];
}

type Close = { readonly done: boolean } | Refusal;
type ControlResult = { readonly outcome: "accepted" | "duplicate" | "forked" } | Refusal;
type EventResult = { readonly outcome: "accepted" | "duplicate" | "equivocation" | "held" } | Refusal;

export interface CommunitiesEngines {
  readonly publication: {
    open(ctx: ProfileExecutionContext, id: string): Done | Refusal;
    close(ctx: ProfileExecutionContext, id: string): Close;
    acceptControl(ctx: ProfileExecutionContext, id: string, signed: string): ControlResult;
    /** Core verifies the signature. The supplied key must open the reference authorized by the control state. */
    acceptEvent(ctx: ProfileExecutionContext, id: string, signed: string, publicKey: string): EventResult;
    view(ctx: ProfileExecutionContext, id: string): PublicationView | Refusal;
  };
  readonly space: {
    open(ctx: ProfileExecutionContext, id: string): Done | Refusal;
    close(ctx: ProfileExecutionContext, id: string): Close;
    acceptState(ctx: ProfileExecutionContext, id: string, signed: string): ControlResult;
    /** Reported reachable account ids are used for display only. */
    view(ctx: ProfileExecutionContext, id: string, liveAccounts?: readonly string[]): SpaceView | Refusal;
  };
  readonly directory: {
    observe(ctx: ProfileExecutionContext, place: DirectoryKind, id: string, details: { name: string; description?: string }): Done | Refusal;
    observeOffer(ctx: ProfileExecutionContext, place: AdmissionPlace, id: string, terms: AdmissionTerms & { incarnation: string }): { readonly target: string } | Refusal;
    withdrawOffer(ctx: ProfileExecutionContext, place: AdmissionPlace, id: string, target: string): Done | Refusal;
    withdraw(ctx: ProfileExecutionContext, place: DirectoryKind, id: string): Close;
    list(ctx: ProfileExecutionContext): { readonly entries: readonly DirectoryEntry[] } | Refusal;
  };
}

export function createCommunitiesEngines(core: Pick<WasmCore, "call">): CommunitiesEngines {
  const q = <T>(command: Record<string, unknown>): T => core.call(command) as T;
  const publication = (ctx: ProfileExecutionContext, handle: string, op: Record<string, unknown>) =>
    q<never>({ cmd: "publication_do", ctx, handle, op });
  const space = (ctx: ProfileExecutionContext, handle: string, op: Record<string, unknown>) =>
    q<never>({ cmd: "space_do", ctx, handle, op });
  const directory = (ctx: ProfileExecutionContext, op: Record<string, unknown>) =>
    q<never>({ cmd: "directory_do", ctx, op });
  return {
    publication: {
      open: (ctx, id) => publication(ctx, id, { kind: "open" }),
      close: (ctx, id) => publication(ctx, id, { kind: "close" }),
      acceptControl: (ctx, id, signed) => publication(ctx, id, { kind: "accept_control", signed }),
      acceptEvent: (ctx, id, signed, publicKey) => publication(ctx, id, { kind: "accept_event", signed, publicKey }),
      view: (ctx, id) => publication(ctx, id, { kind: "view" }),
    },
    space: {
      open: (ctx, id) => space(ctx, id, { kind: "open" }),
      close: (ctx, id) => space(ctx, id, { kind: "close" }),
      acceptState: (ctx, id, signed) => space(ctx, id, { kind: "accept_state", signed }),
      view: (ctx, id, liveAccounts) => space(ctx, id, { kind: "view", liveAccounts }),
    },
    directory: {
      observe: (ctx, place, id, details) => directory(ctx, { ...details, kind: "observe", place, id }),
      observeOffer: (ctx, place, id, terms) => directory(ctx, { ...terms, kind: "observe_offer", place, id }),
      withdrawOffer: (ctx, place, id, target) => directory(ctx, { kind: "withdraw_offer", place, id, target }),
      withdraw: (ctx, place, id) => directory(ctx, { kind: "withdraw", place, id }),
      list: (ctx) => directory(ctx, { kind: "list" }),
    },
  };
}
