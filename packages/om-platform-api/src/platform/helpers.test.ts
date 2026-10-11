import { describe, expect, it } from "vitest";
import type { CapabilityReport } from "./adapter";
import type { CapabilityEntry } from "./capability";
import { FAMILY_NAMES } from "./families";
import type { FamilyName } from "./families";
import { degrade, describeLimits, requireCapability } from "./helpers";

const report = (over: Partial<Record<FamilyName, CapabilityEntry>> = {}): CapabilityReport => ({
  families: Object.fromEntries(FAMILY_NAMES.map((f) => [f, over[f] ?? { state: "unavailable" }])) as Record<FamilyName, CapabilityEntry>,
  backgroundPush: "none",
  localDiscovery: "manual_known_endpoint",
  localContacts: "unavailable",
  callSystemIntegration: false,
});

describe("requireCapability / degrade", () => {
  it("reports unavailable without throwing", () => {
    expect(requireCapability(report(), "push")).toEqual({ ok: false, family: "push", reason: "capability_unavailable" });
    expect(requireCapability(report({ push: { state: "partial" } }), "push")).toEqual({ ok: true, state: "partial" });
  });
  it("runs only the explicit branch; partial fails closed to unavailable", () => {
    const b = { available: () => "a", unavailable: () => "u" };
    expect(degrade(report({ share: { state: "available" } }), "share", b)).toBe("a");
    expect(degrade(report({ share: { state: "partial" } }), "share", b)).toBe("u");
    expect(degrade(report({ share: { state: "partial" } }), "share", { ...b, partial: () => "p" })).toBe("p");
    expect(degrade(report(), "share", b)).toBe("u");
  });
});

describe("describeLimits", () => {
  it("lists machine codes only", () => {
    const codes = describeLimits(report({ clipboard: { state: "partial", detail: "sensitive_unsupported" } }));
    expect(codes).toEqual(expect.arrayContaining([
      "secure_surface_unavailable", "backup_exclusion_unavailable", "clipboard_sensitive_unsupported",
      "notification_removal_unavailable", "input_privacy_unavailable", "background_push_none",
      "local_discovery_not_automatic", "call_system_integration_unavailable",
    ]));
    for (const c of codes) expect(c).toMatch(/^[a-z_]+$/);
  });
});
