// @vitest-environment jsdom
import { cleanup, render, screen } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it } from "vitest";
import type { VaultWrappingInfo } from "@om/platform-api";
import { lockCopy } from "../copy/en";
import { LockScreen } from "./LockScreen";

afterEach(cleanup);

const wrap = (over: Partial<VaultWrappingInfo>): VaultWrappingInfo => ({ method: "device-key", reason: "device_key_requested", reducedAtRestProperty: false, passphraseProtected: false, ...over });

describe("Settings > Lock", () => {
  it("calls a key kept in this browser 'not locked' and warns in plain words", async () => {
    const { container } = render(<LockScreen services={{ wrapping: () => Promise.resolve(wrap({})) }} />);
    expect(await screen.findByText(lockCopy.modes.none.title)).toBeTruthy();
    expect(screen.getByRole("note").textContent).toBe(lockCopy.warning);
    expect((await axe.run(container, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations).toEqual([]);
  });
  it("says a device prompt, a synced passkey and a password each ask every time, and only the synced one warns", async () => {
    for (const [info, mode] of [
      [wrap({ method: "webauthn-prf", reason: "prf_verified" }), "device"],
      [wrap({ method: "webauthn-prf", reason: "prf_verified", reducedAtRestProperty: true }), "device-shared"],
      [wrap({ passphraseProtected: true }), "password"],
    ] as const) {
      const { unmount } = render(<LockScreen services={{ wrapping: () => Promise.resolve(info) }} />);
      expect(await screen.findByText(lockCopy.modes[mode].title)).toBeTruthy();
      expect(screen.queryByRole("note")).toBeNull();
      unmount();
    }
  });
  it("says so when it cannot tell", async () => {
    render(<LockScreen services={{ wrapping: () => Promise.resolve(null) }} />);
    expect((await screen.findByRole("alert")).textContent).toBe(lockCopy.failed);
  });
});
