// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { VaultWrappingInfo } from "@om/platform-api";
import { lockCopy } from "../copy/en";
import { LockScreen } from "./LockScreen";
import type { LockServices } from "./lockServices";

afterEach(cleanup);

const wrap = (over: Partial<VaultWrappingInfo>): VaultWrappingInfo => ({ method: "device-key", reason: "device_key_requested", reducedAtRestProperty: false, passphraseProtected: false, ...over });
const axeViolations = async (el: Element): Promise<string[]> =>
  (await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations.map((v) => `${v.id}: ${v.help}`);

function fake(info: VaultWrappingInfo | null, over: Partial<LockServices> = {}): LockServices {
  return {
    wrapping: () => Promise.resolve(info), setPassword: vi.fn(() => Promise.resolve({ ok: true as const, wrapping: wrap({ passphraseProtected: true }) })),
    removePassword: vi.fn(() => Promise.resolve({ ok: true as const, wrapping: wrap({}) })), useDevicePrompt: vi.fn(() => Promise.resolve({ ok: true as const, wrapping: wrap({ method: "webauthn-prf", reason: "prf_verified" }) })),
    stopLocking: vi.fn(() => Promise.resolve({ ok: true as const, wrapping: wrap({}) })), lockNow: vi.fn(() => Promise.resolve()), idleMinutes: () => null, setIdleMinutes: vi.fn(), ...over,
  };
}
const GOOD = "correct horse battery staple";

describe("Settings > Lock: what it says", () => {
  it("calls a key kept in this browser 'not locked' and warns in plain words; no lock-time choices without a lock", async () => {
    const { container } = render(<LockScreen services={fake(wrap({}))} />);
    expect(await screen.findByText(lockCopy.modes.none.title)).toBeTruthy();
    expect(screen.getByRole("note").textContent).toBe(lockCopy.warning);
    expect(screen.queryByRole("radio")).toBeNull();
    expect(screen.queryByRole("button", { name: new RegExp(lockCopy.actions.stop.title) })).toBeNull();
    expect(screen.getByText(lockCopy.idle.needsLock)).toBeTruthy();
    expect(await axeViolations(container)).toEqual([]);
  });

  it("says a device prompt, a synced passkey and a password each ask every time, and only the synced one warns", async () => {
    for (const [info, mode] of [
      [wrap({ method: "webauthn-prf", reason: "prf_verified" }), "device"],
      [wrap({ method: "webauthn-prf", reason: "prf_verified", reducedAtRestProperty: true }), "device-shared"],
      [wrap({ passphraseProtected: true }), "password"],
    ] as const) {
      const { unmount } = render(<LockScreen services={fake(info)} />);
      expect(await screen.findByText(lockCopy.modes[mode].title)).toBeTruthy();
      expect(screen.queryByRole("note")).toBeNull();
      expect(screen.getAllByRole("radio")).toHaveLength(5);
      unmount();
    }
  });

  it("says so when it cannot tell", async () => {
    render(<LockScreen services={fake(null)} />);
    expect((await screen.findByRole("alert")).textContent).toBe(lockCopy.failed);
  });
});

describe("Settings > Lock: changing it", () => {
  it("adds a password only when it is long enough and typed twice, and shows a tick that fades", async () => {
    const services = fake(wrap({}));
    const { container } = render(<LockScreen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: new RegExp(lockCopy.actions.addPassword.title) }));
    await userEvent.type(screen.getByLabelText("New password"), "short");
    await userEvent.type(screen.getByLabelText("Type it again"), "short");
    await userEvent.click(screen.getByRole("button", { name: lockCopy.password.save }));
    expect(await screen.findByRole("alert")).toBeTruthy();
    expect(services.setPassword).not.toHaveBeenCalled();
    expect(await axeViolations(container)).toEqual([]);

    await userEvent.clear(screen.getByLabelText("New password"));
    await userEvent.clear(screen.getByLabelText("Type it again"));
    await userEvent.type(screen.getByLabelText("New password"), GOOD);
    await userEvent.type(screen.getByLabelText("Type it again"), GOOD);
    await userEvent.click(screen.getByRole("button", { name: lockCopy.password.save }));
    await waitFor(() => { expect(services.setPassword).toHaveBeenCalledWith(GOOD, undefined); });
    expect(await screen.findByText(lockCopy.modes.password.title)).toBeTruthy();
    expect(screen.getByRole("status").textContent).toBe(lockCopy.done);
  });

  it("a wrong current password stays on the form and says why, in the vault's own words", async () => {
    const services = fake(wrap({ passphraseProtected: true }), { setPassword: vi.fn(() => Promise.resolve({ ok: false as const, reason: "wrong_passphrase" as const })) });
    render(<LockScreen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: new RegExp(lockCopy.actions.changePassword.title) }));
    await userEvent.type(screen.getByLabelText("Current password"), "not it at all");
    await userEvent.type(screen.getByLabelText("New password"), GOOD);
    await userEvent.type(screen.getByLabelText("Type it again"), GOOD);
    await userEvent.click(screen.getByRole("button", { name: lockCopy.password.save }));
    expect((await screen.findByRole("alert")).textContent).toContain("not right");
    expect(screen.getByLabelText("Current password")).toBeTruthy();
  });

  it("stopping every lock warns first, asks for the password, and does nothing until confirmed", async () => {
    const services = fake(wrap({ passphraseProtected: true }));
    const { container } = render(<LockScreen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: new RegExp(lockCopy.actions.stop.title) }));
    expect(screen.getByText(lockCopy.stop.warning)).toBeTruthy();
    expect(services.stopLocking).not.toHaveBeenCalled();
    expect(await axeViolations(container)).toEqual([]);
    await userEvent.click(screen.getByRole("button", { name: lockCopy.stop.cancel }));
    expect(services.stopLocking).not.toHaveBeenCalled();
    await userEvent.click(screen.getByRole("button", { name: new RegExp(lockCopy.actions.stop.title) }));
    await userEvent.type(screen.getByLabelText("Current password"), GOOD);
    await userEvent.click(screen.getByRole("button", { name: lockCopy.stop.confirm }));
    await waitFor(() => { expect(services.stopLocking).toHaveBeenCalledWith(GOOD); });
    expect(await screen.findByText(lockCopy.modes.none.title)).toBeTruthy();
  });

  it("a browser with no device prompt says nothing changed", async () => {
    const services = fake(wrap({}), { useDevicePrompt: vi.fn(() => Promise.resolve({ ok: true as const, wrapping: wrap({}) })) });
    render(<LockScreen services={services} />);
    await userEvent.click(await screen.findByRole("button", { name: new RegExp(lockCopy.actions.devicePrompt.title) }));
    await userEvent.click(screen.getByRole("button", { name: lockCopy.device.start }));
    expect((await screen.findByRole("alert")).textContent).toBe(lockCopy.device.unavailable);
  });

  it("chooses when to lock, and can lock right now", async () => {
    const services = fake(wrap({ passphraseProtected: true }));
    render(<LockScreen services={services} />);
    await userEvent.click(await screen.findByRole("radio", { name: lockCopy.idle.after(15) }));
    expect(services.setIdleMinutes).toHaveBeenCalledWith(15);
    await userEvent.click(screen.getByRole("button", { name: lockCopy.idle.now }));
    expect(services.lockNow).toHaveBeenCalledTimes(1);
  });
});
