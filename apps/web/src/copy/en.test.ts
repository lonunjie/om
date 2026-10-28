import type { AccountControlStateCode, DeviceCautionCode, DeviceRole, DeviceStateCode, LastSeenClass, LinkOfferState, LinkRequestState, RecoveryCustodianCode, RecoveryPolicyState, RecoveryRequestState, RecoveryScope } from "@om/platform-api";
import type { ClientOriginChoiceCode, HomeObserverCode, LimitCode, RejectReason, SecurityNoticeCode, UserActionCode, VaultErrorReason, VaultStage, WrappingMethod, WrappingReason } from "@om/platform-api";
import { describe, expect, it } from "vitest";
import { appStrings, conversationCopy, deviceScannerCopy, landingCopy, devicesCopy, homeCopy, linkThisDeviceCopy, onboardingCopy, recoveryCopy, scannerCopy, shellLabels, switcherLabels, unlockCopy } from "./en";

/** Every leaf must be a non-empty string; functions are called with sample arguments. */
function leaves(v: unknown, path: string, out: [string, string][]): void {
  if (typeof v === "string") out.push([path, v]);
  else if (typeof v === "function") out.push([path, String((v as (...a: number[]) => unknown)(2, 5))]);
  else if (typeof v === "object" && v !== null) for (const [k, x] of Object.entries(v)) leaves(x, `${path}.${k}`, out);
  else out.push([path, ""]);
}
const all = (name: string, v: unknown): [string, string][] => { const o: [string, string][] = []; leaves(v, name, o); return o; };

// Exhaustive by type: adding a code upstream breaks compilation here until the copy and this list are updated.
const keys = <K extends string>(r: Record<K, true>): readonly K[] => Object.keys(r) as K[];
const REASONS = keys<RejectReason>({ invalid: true, unsupported: true, unauthorized: true, revoked: true, expired: true, stale_state: true, conflict: true, policy_denied: true, quota: true, rate_limited: true, integrity_failure: true, unavailable: true, not_found: true });
const NOTICES = keys<SecurityNoticeCode>({ safety_number_changed: true, device_added: true, unverified_contact: true, session_reset: true, integrity_failure: true });
const ACTIONS = keys<UserActionCode>({ authorize_with_device: true, confirm_security_change: true, reauthenticate: true, update_client: true });
const VAULT = keys<VaultErrorReason>({
  invalid_profile_id: true, invalid_purpose: true, invalid_key: true, not_found: true, corrupt: true, locked: true, already_exists: true, conflict: true, needs_user_gesture: true,
  ceremony_failed: true, prf_unavailable: true, unwrap_failed: true, unsupported_factor: true, passphrase_required: true, wrong_passphrase: true, unsupported_platform: true, malformed: true, unsupported_version: true,
  wrong_purpose: true, integrity_failure: true, crypto_failed: true, storage_failed: true, delete_unverified: true,
});
const WRAP = keys<WrappingReason>({ prf_verified: true, device_key_requested: true, webauthn_absent: true, prf_not_enabled: true, prf_no_output: true, webauthn_create_failed: true, webauthn_get_failed: true });
const METHODS = keys<WrappingMethod>({ "webauthn-prf": true, "device-key": true });
const STAGES = keys<VaultStage>({ enroll_create: true, enroll_verify: true, unlock: true, rotate_unwrap: true });
const LIMITS = keys<LimitCode>({
  secure_surface_unavailable: true, backup_exclusion_unavailable: true, clipboard_sensitive_unsupported: true, clipboard_unavailable: true, notification_removal_unavailable: true,
  notification_removal_partial: true, input_privacy_partial: true, input_privacy_unavailable: true, background_push_none: true, local_discovery_not_automatic: true,
  local_discovery_unavailable: true, call_system_integration_unavailable: true,
});
const OBSERVERS = keys<HomeObserverCode>({ sponsor_sees_metadata: true, host_sees_metadata: true, host_sees_ciphertext: true, no_third_party: true });
const ORIGINS = keys<ClientOriginChoiceCode>({ default_origin: true, independent_origin_available: true, independent_origin_active: true });
const FROZEN = keys<Exclude<AccountControlStateCode, "normal">>({ control_conflict: true, safety_freeze: true, fork_detected: true });
const ROLES = keys<DeviceRole>({ control: true, messaging: true });
const DEVICE_STATES = keys<DeviceStateCode>({ active: true, quarantined: true, revocation_pending: true, revoked: true });
const LAST_SEEN = keys<LastSeenClass>({ now: true, recent: true, this_week: true, this_month: true, long_ago: true, never: true });
const CAUTIONS = keys<DeviceCautionCode>({ suspend_would_freeze: true, last_voting_controller: true, history_sync_unavailable: true, long_idle: true });
const LINK_STATES = keys<LinkRequestState>({ awaiting_approval: true, pending_authorization: true, approved: true, rejected: true, expired: true });
const OFFER_STATES = keys<LinkOfferState>({ waiting_for_scan: true, awaiting_approval: true, linked: true, rejected: true, expired: true });
const POLICIES = keys<RecoveryPolicyState>({ on: true, pending: true, off_explicit: true });
const CUSTODIANS = keys<RecoveryCustodianCode>({ synced_passkey: true, recovery_key: true, trusted_contacts: true, spare_device: true });
const RECOVERY_STATES = keys<RecoveryRequestState>({ waiting: true, completed: true, vetoed: true, cancelled: true });
const SCOPES = keys<RecoveryScope>({ identity: true, history: true, provider: true });

describe("English copy catalog", () => {
  it("has a non-empty string for every key of every copy type", () => {
    const entries = [...all("conversation", conversationCopy), ...all("onboarding", onboardingCopy), ...all("shell", shellLabels), ...all("switcher", switcherLabels), ...all("app", appStrings), ...all("landing", landingCopy), ...all("scanner", scannerCopy), ...all("unlock", unlockCopy),
      ...all("deviceScanner", deviceScannerCopy), ...all("devices", devicesCopy), ...all("linkThisDevice", linkThisDeviceCopy), ...all("recovery", recoveryCopy), ...all("home", homeCopy)];
    expect(entries.length).toBeGreaterThan(200);
    expect(entries.filter(([, s]) => s.trim() === "").map(([p]) => p)).toEqual([]);
  });
  const has = (rec: Readonly<Record<string, string>>, codes: readonly string[]): string[] => codes.filter((c) => (rec[c] ?? "").trim() === "");
  it("words every machine reason code", () => {
    expect(has(conversationCopy.notices, NOTICES)).toEqual([]);
    for (const r of [conversationCopy.delivery.rejected, conversationCopy.composer.rejected, conversationCopy.request.rejected, onboardingCopy.invite.rejected]) expect(has(r, REASONS)).toEqual([]);
    expect(has(conversationCopy.composer.needsAction, ACTIONS)).toEqual([]);
    const p = onboardingCopy.protect;
    expect(has(p.errors, VAULT)).toEqual([]);
    expect(has(p.reason, WRAP)).toEqual([]);
    expect(has(p.method, METHODS)).toEqual([]);
    expect(has(p.needsGesture, STAGES)).toEqual([]);
    expect(has(p.limits, LIMITS)).toEqual([]);
    expect(has(onboardingCopy.home.observers, OBSERVERS)).toEqual([]);
    expect(has(onboardingCopy.home.origin, ORIGINS)).toEqual([]);
  });
  it("words every device, link and recovery code", () => {
    const d = devicesCopy;
    for (const o of [d.outcome, linkThisDeviceCopy.outcome, recoveryCopy.outcome]) { expect(has(o.rejected, REASONS)).toEqual([]); expect(has(o.needsAction, ACTIONS)).toEqual([]); }
    expect(has(d.controlState, FROZEN)).toEqual([]);
    expect(has(d.role, ROLES)).toEqual([]);
    expect(has(d.state, DEVICE_STATES)).toEqual([]);
    expect(has(d.lastSeen, LAST_SEEN)).toEqual([]);
    expect(has(d.cautions, CAUTIONS)).toEqual([]);
    expect(has(d.link.requestState, LINK_STATES)).toEqual([]);
    expect(has(linkThisDeviceCopy.state, OFFER_STATES)).toEqual([]);
    expect(has(recoveryCopy.policy, POLICIES)).toEqual([]);
    expect(has(recoveryCopy.custodian, CUSTODIANS)).toEqual([]);
    expect(has(recoveryCopy.requestState, RECOVERY_STATES)).toEqual([]);
    expect(SCOPES.filter((s) => recoveryCopy.scope[s].title === "" || recoveryCopy.start.consequences[s].length === 0)).toEqual([]);
  });
  it("suspend and permanent removal never share wording", () => {
    expect(devicesCopy.suspend.action).not.toBe(devicesCopy.revoke.action);
    expect(devicesCopy.suspend.confirm).not.toBe(devicesCopy.revoke.confirm);
    expect(devicesCopy.suspend.consequences.filter((c) => devicesCopy.revoke.consequences.includes(c) && !/already on it/.test(c))).toEqual([]);
  });
  it("shows no provider names or raw machine codes to people", () => {
    const text = [...all("c", conversationCopy), ...all("o", onboardingCopy), ...all("a", appStrings), ...all("d", devicesCopy), ...all("l", linkThisDeviceCopy), ...all("r", recoveryCopy)].map(([, s]) => s).join("\n");
    expect(text).not.toMatch(/\b(?:Google|Apple|Microsoft|Cloudflare|WebAuthn|PRF|IndexedDB|[a-z]+_[a-z_]+)\b/);
  });
});
