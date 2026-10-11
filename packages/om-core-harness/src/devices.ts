// DEV/TEST ONLY device, device-link and recovery simulation for the harness. It imitates Core outcomes so the UI can be
// tested; it is not an AccountControlState implementation and enforces none of its cryptography.
import { asDeviceId, parseDeviceLink } from "@om/platform-api";
import type {
  AccountControlStateCode, CoreIntent, DeviceActionCode, DeviceCautionCode, DeviceId, DeviceNoticeView, DeviceRole, DeviceStateCode,
  DeviceSummary, DevicesView, LastSeenClass, LinkOfferState, LinkOfferView, LinkRequestView, OperationId, OutcomeClass, ProfileId,
  RecoveryCustodianCode, RecoveryPolicyState, RecoveryRequestView, RecoveryScope, RecoveryStatusView, RejectReason, TerminalOutcome,
} from "@om/platform-api";

export interface SeedDevice {
  readonly id: string;
  readonly label: string;
  readonly role: DeviceRole;
  readonly state?: DeviceStateCode;
  readonly current?: boolean;
  readonly enrolledAt?: number;
  readonly lastSeen?: LastSeenClass;
  /** `false` marks a device that cannot share device history (another implementation). Default true. */
  readonly historySync?: boolean;
}
export interface SeedIncomingRecovery {
  readonly requestId: string;
  readonly scope: RecoveryScope;
  readonly custodian: RecoveryCustodianCode;
  readonly startedAt?: number;
  readonly activatesAt?: number | null;
}
export interface SeedRecovery {
  readonly policy?: RecoveryPolicyState;
  readonly custodians?: readonly RecoveryCustodianCode[];
  readonly scopes?: readonly RecoveryScope[];
  readonly activationDelayMs?: number;
  readonly recoveryKeySaved?: boolean;
  readonly incoming?: readonly SeedIncomingRecovery[];
}
export interface SeedDevices {
  /** Default: one current control device. */
  readonly devices?: readonly SeedDevice[];
  readonly controlState?: AccountControlStateCode;
  /** Simulates a Hardened profile: approving a link needs a second device (PendingAuthorization). */
  readonly linkNeedsAuthorization?: boolean;
  readonly recovery?: SeedRecovery;
}

/** What one intent would do. The harness decides when (and whether) `commit` runs: faults can defer or replace it. */
export type Staged =
  | { readonly kind: "rejected"; readonly reason: RejectReason }
  | {
    readonly kind: "effect";
    readonly outcome: OutcomeClass;
    readonly commit: () => void;
    /** Present when `outcome` is PendingAuthorization: finalizes or rolls back once authorization resolves. */
    readonly onAuthorize?: (o: TerminalOutcome) => void;
  };

interface DeviceRec { id: DeviceId; label: string; role: DeviceRole; state: DeviceStateCode; current: boolean; enrolledAt: number; lastSeen: LastSeenClass; historySync: boolean }
interface RequestRec { view: LinkRequestView; token: string }
interface OfferRec { owner: ProfileId | null; operationId: OperationId | null; link: string; label: string; code: string; expiresAt: number; state: LinkOfferState }
type RecoveryRec = { -readonly [K in keyof RecoveryRequestView]: RecoveryRequestView[K] };
interface AccountSim {
  devices: Map<DeviceId, DeviceRec>;
  controlState: AccountControlStateCode;
  linkNeedsAuthorization: boolean;
  requests: Map<string, RequestRec>;
  notices: DeviceNoticeView[];
  policy: RecoveryPolicyState;
  custodians: readonly RecoveryCustodianCode[];
  scopes: readonly RecoveryScope[];
  delayMs: number;
  keySaved: boolean;
  recoveries: Map<string, RecoveryRec>;
}

const LINK_TTL_MS = 10 * 60 * 1000;
const DEFAULT_DELAY_MS = 72 * 60 * 60 * 1000;
const B64URL = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-";
const random = (alphabet: string, n: number): string =>
  Array.from(globalThis.crypto.getRandomValues(new Uint8Array(n)), (b) => alphabet.charAt(b % alphabet.length)).join("");
const fakeCode = (): string => { const d = random("0123456789", 6); return `${d.slice(0, 3)} ${d.slice(3)}`; };

export function createDeviceSim(clock: () => number) {
  const accounts = new Map<ProfileId, AccountSim>();
  const offers = new Map<string, OfferRec>(); // by token; offers travel between accounts out of band (QR/link)
  let seq = 0;
  /** Profiles whose views changed during the last staged commit (besides the dispatching one). */
  const touched = new Set<ProfileId>();

  function account(profileId: ProfileId): AccountSim {
    const a = accounts.get(profileId);
    if (!a) throw new Error("device sim: unknown profile");
    return a;
  }

  function seed(profileId: ProfileId, s: SeedDevices = {}): void {
    const now = clock();
    const devices = new Map<DeviceId, DeviceRec>();
    for (const d of s.devices ?? [{ id: `${profileId}-this`, label: "This browser", role: "control" as const, current: true }]) {
      const id = asDeviceId(d.id);
      devices.set(id, {
        id, label: d.label, role: d.role, state: d.state ?? "active", current: d.current ?? false, enrolledAt: d.enrolledAt ?? now,
        lastSeen: d.lastSeen ?? (d.current ? "now" : "recent"), historySync: d.historySync ?? true,
      });
    }
    const r = s.recovery ?? {};
    const delayMs = r.activationDelayMs ?? DEFAULT_DELAY_MS;
    const recoveries = new Map<string, RecoveryRec>();
    for (const i of r.incoming ?? []) {
      const startedAt = i.startedAt ?? now;
      recoveries.set(i.requestId, {
        requestId: i.requestId, operationId: null, direction: "incoming", scope: i.scope, custodian: i.custodian, startedAt,
        activatesAt: i.activatesAt === undefined ? startedAt + delayMs : i.activatesAt, state: "waiting", canStop: true,
      });
    }
    accounts.set(profileId, {
      devices, controlState: s.controlState ?? "normal", linkNeedsAuthorization: s.linkNeedsAuthorization ?? false, requests: new Map(), notices: [],
      policy: r.policy ?? "on", custodians: r.custodians ?? ["recovery_key"], scopes: r.scopes ?? ["identity", "history", "provider"], delayMs,
      keySaved: r.recoveryKeySaved ?? true, recoveries,
    });
  }

  const current = (a: AccountSim): DeviceRec | undefined => [...a.devices.values()].find((d) => d.current);
  const currentIsActiveController = (a: AccountSim): boolean => { const c = current(a); return c?.role === "control" && c.state === "active"; };

  /** Imitates Core's allowed-action rules (ACCOUNT_CONTINUITY_AND_CONTROL.md §4.1, §9, §13) for display. */
  function actionsFor(a: AccountSim, d: DeviceRec): { actions: DeviceActionCode[]; cautions: DeviceCautionCode[] } {
    const cautions: DeviceCautionCode[] = [];
    if (d.lastSeen === "long_ago") cautions.push("long_idle");
    if (!d.historySync) cautions.push("history_sync_unavailable");
    if (a.controlState !== "normal" || d.current || !currentIsActiveController(a) || d.state === "revoked" || d.state === "revocation_pending") return { actions: [], cautions };
    if (d.role === "messaging") return { actions: d.state === "active" ? ["revoke"] : [], cautions };
    const controllers = [...a.devices.values()].filter((x) => x.role === "control" && x.state !== "revoked");
    const voting = controllers.filter((x) => x.state === "active").length;
    const majority = Math.floor(controllers.length / 2) + 1;
    if (d.state === "quarantined") return { actions: ["revoke"], cautions };
    const actions: DeviceActionCode[] = [];
    if (voting - 1 <= 0) cautions.push("last_voting_controller");
    else if (voting - 1 < majority) cautions.push("suspend_would_freeze");
    else actions.push("suspend");
    if (voting - 1 > 0) actions.push("revoke");
    return { actions, cautions };
  }

  function summary(a: AccountSim, d: DeviceRec): DeviceSummary {
    const { actions, cautions } = actionsFor(a, d);
    return { id: d.id, label: d.label, role: d.role, state: d.state, isCurrent: d.current, enrolledAt: d.enrolledAt, lastSeen: d.lastSeen, actions, cautions };
  }

  function expireIfDue(r: RequestRec): void {
    if (r.view.state === "awaiting_approval" && clock() > r.view.expiresAt) r.view = { ...r.view, state: "expired" };
  }

  const queries = {
    devices(profileId: ProfileId): DevicesView {
      const a = account(profileId);
      for (const r of a.requests.values()) expireIfDue(r);
      return {
        controlState: a.controlState,
        devices: [...a.devices.values()].map((d) => summary(a, d)),
        linkRequests: [...a.requests.values()].map((r) => r.view),
        notices: [...a.notices],
      };
    },
    linkRequest(profileId: ProfileId, operationId: OperationId): LinkRequestView | null {
      const r = [...account(profileId).requests.values()].find((x) => x.view.operationId === operationId);
      if (!r) return null;
      expireIfDue(r);
      return r.view;
    },
    linkOffer(profileId: ProfileId, operationId: OperationId): LinkOfferView | null {
      const o = [...offers.values()].find((x) => x.owner === profileId && x.operationId === operationId);
      if (!o) return null;
      const state = o.state === "waiting_for_scan" && clock() > o.expiresAt ? "expired" : o.state;
      return { operationId, link: o.link, expiresAt: o.expiresAt, verificationCode: state === "waiting_for_scan" || state === "expired" ? null : o.code, state };
    },
    recovery(profileId: ProfileId): RecoveryStatusView {
      const a = account(profileId);
      const canVeto = currentIsActiveController(a);
      return {
        policy: a.policy,
        custodians: a.custodians.map((code) => ({ code, distributionBound: code === "synced_passkey" })),
        scopes: a.scopes,
        activationDelayMs: a.delayMs,
        recoveryKeySaved: a.keySaved,
        requests: [...a.recoveries.values()].map((r) => ({ ...r, canStop: r.state === "waiting" && (r.direction === "outgoing" || canVeto) })),
      };
    },
  };

  function newOffer(owner: ProfileId | null, operationId: OperationId | null, label: string): OfferRec {
    const token = random(B64URL, 22);
    const o: OfferRec = { owner, operationId, link: `om:link/${token}`, label, code: fakeCode(), expiresAt: clock() + LINK_TTL_MS, state: "waiting_for_scan" };
    offers.set(token, o);
    return o;
  }
  const touch = (o: OfferRec): void => { if (o.owner !== null) touched.add(o.owner); };
  const offerOf = (r: RequestRec): OfferRec | undefined => offers.get(r.token);

  function link(a: AccountSim, r: RequestRec): void {
    const id = asDeviceId(`dev-${String(++seq)}`);
    const at = clock();
    a.devices.set(id, { id, label: r.view.deviceLabel, role: "messaging", state: "active", current: false, enrolledAt: at, lastSeen: "now", historySync: true });
    // R-166: every enrollment is announced on the account's devices; there is no path that skips this notice.
    a.notices.push({ code: "device_linked", deviceId: id, label: r.view.deviceLabel, at });
    r.view = { ...r.view, state: "approved" };
    const o = offerOf(r);
    if (o) { o.state = "linked"; touch(o); }
  }

  const rejected = (reason: RejectReason): Staged => ({ kind: "rejected", reason });
  const effect = (outcome: OutcomeClass, commit: () => void, onAuthorize?: (o: TerminalOutcome) => void): Staged =>
    onAuthorize ? { kind: "effect", outcome, commit, onAuthorize } : { kind: "effect", outcome, commit };

  /** Returns `null` for intents this module does not own. */
  function stage(profileId: ProfileId, intent: CoreIntent): Staged | null {
    const a = accounts.get(profileId);
    if (!a) return null;
    switch (intent.kind) {
      case "OfferDeviceLink":
        return effect({ kind: "Committed" }, () => { newOffer(profileId, intent.operationId, "New web browser"); });
      case "RequestDeviceLink": {
        const parsed = parseDeviceLink(intent.link);
        if (!parsed.ok) return rejected("invalid");
        const o = offers.get(parsed.token);
        if (!o) return rejected("not_found");
        if (o.owner === profileId) return rejected("invalid"); // a device cannot link itself
        if (clock() > o.expiresAt) return rejected("expired");
        if (o.state !== "waiting_for_scan") return rejected("conflict");
        if (!currentIsActiveController(a)) return rejected("unauthorized"); // a messaging device cannot link another device
        return effect({ kind: "Committed" }, () => {
          const requestId = `lr-${String(++seq)}`;
          a.requests.set(requestId, {
            token: parsed.token,
            view: { requestId, operationId: intent.operationId, deviceLabel: o.label, verificationCode: o.code, expiresAt: o.expiresAt, state: "awaiting_approval" },
          });
          o.state = "awaiting_approval";
          touch(o);
        });
      }
      case "ApproveDeviceLink": {
        const r = a.requests.get(intent.requestId);
        if (!r) return rejected("not_found");
        expireIfDue(r);
        if (r.view.state === "expired") return rejected("expired");
        if (r.view.state !== "awaiting_approval") return rejected("conflict");
        if (!currentIsActiveController(a)) return rejected("unauthorized");
        if (a.linkNeedsAuthorization) {
          return effect({ kind: "PendingAuthorization" }, () => { r.view = { ...r.view, state: "pending_authorization" }; }, (o) => {
            if (o.kind === "Committed") link(a, r);
            else { r.view = { ...r.view, state: "rejected" }; const off = offerOf(r); if (off) { off.state = "rejected"; touch(off); } }
          });
        }
        return effect({ kind: "Committed" }, () => { link(a, r); });
      }
      case "RejectDeviceLink": {
        const r = a.requests.get(intent.requestId);
        if (!r) return rejected("not_found");
        if (r.view.state !== "awaiting_approval") return rejected("conflict");
        return effect({ kind: "Committed" }, () => {
          r.view = { ...r.view, state: "rejected" };
          const o = offerOf(r);
          if (o) { o.state = "rejected"; touch(o); }
        });
      }
      case "SuspendDevice": {
        const d = a.devices.get(intent.deviceId);
        if (!d) return rejected("not_found");
        if (!actionsFor(a, d).actions.includes("suspend")) return rejected("policy_denied");
        return effect({ kind: "Committed" }, () => { d.state = "quarantined"; });
      }
      case "RevokeDevice": {
        const d = a.devices.get(intent.deviceId);
        if (!d) return rejected("not_found");
        if (!actionsFor(a, d).actions.includes("revoke")) return rejected("policy_denied");
        if (d.role === "messaging") return effect({ kind: "Committed" }, () => { d.state = "revoked"; });
        const before = d.state;
        // Revoking a controller needs the parent-state majority: one device alone leaves it pending.
        return effect({ kind: "PendingAuthorization" }, () => { d.state = "revocation_pending"; }, (o) => { d.state = o.kind === "Committed" ? "revoked" : before; });
      }
      case "StartRecovery": {
        if (a.policy === "off_explicit") return rejected("unsupported");
        if (!a.scopes.includes(intent.scope)) return rejected("unsupported");
        if (!a.custodians.includes(intent.custodian)) return rejected("not_found");
        if ([...a.recoveries.values()].some((r) => r.direction === "outgoing" && r.scope === intent.scope && r.state === "waiting")) return rejected("conflict");
        const requestId = `rr-${String(++seq)}`;
        const now = clock();
        const rec: RecoveryRec = {
          requestId, operationId: intent.operationId, direction: "outgoing", scope: intent.scope, custodian: intent.custodian, startedAt: now,
          activatesAt: intent.scope === "identity" ? now + a.delayMs : null, state: intent.scope === "identity" ? "waiting" : "completed", canStop: false,
        };
        const commit = (): void => { a.recoveries.set(requestId, rec); };
        // Identity recovery waits out the activation delay (or a controller co-signature); the others confer no authority.
        if (intent.scope !== "identity") return effect({ kind: "Committed" }, commit);
        return effect({ kind: "PendingAuthorization" }, commit, (o) => { if (rec.state === "waiting") rec.state = o.kind === "Committed" ? "completed" : "cancelled"; });
      }
      case "VetoRecovery": {
        const r = a.recoveries.get(intent.requestId);
        if (!r) return rejected("not_found");
        if (r.state !== "waiting") return rejected("conflict");
        if (r.direction === "incoming" && !currentIsActiveController(a)) return rejected("unauthorized");
        return effect({ kind: "Committed" }, () => { r.state = r.direction === "incoming" ? "vetoed" : "cancelled"; });
      }
      default:
        return null;
    }
  }

  return {
    seed,
    queries,
    stage,
    touched,
    /** A brand-new device (outside any seeded profile) shows this offer; returns what its screen would display. */
    simulateNewDeviceOffer(label: string): { readonly link: string; readonly verificationCode: string } {
      const o = newOffer(null, null, label);
      return { link: o.link, verificationCode: o.code };
    },
    /** The StartRecovery operation behind an outgoing request (for resolving its authorization in tests). */
    recoveryOperation(profileId: ProfileId, requestId: string): OperationId | null {
      return account(profileId).recoveries.get(requestId)?.operationId ?? null;
    },
  };
}

export type DeviceSim = ReturnType<typeof createDeviceSim>;
