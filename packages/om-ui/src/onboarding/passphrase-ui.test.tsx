// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asProfileId } from "@om/platform-api";
import type { CreateVaultOptions, DeviceVault, UnlockResult, VaultWrappingInfo } from "@om/platform-api";
import { createHarness } from "@om/core-harness";
import { createActionRegistry } from "../actions/registry";
import { testCopy, testUnlockCopy } from "./__fixtures__/copy";
import { checkNewPassphrase, PASSPHRASE_MIN_CODE_POINTS } from "./passphrase";
import { ProtectDevice } from "./ProtectDevice";
import { UnlockVault } from "./UnlockVault";

afterEach(cleanup);

const PID = asProfileId("me");
const WRAP: VaultWrappingInfo = { method: "device-key", reason: "device_key_requested", reducedAtRestProperty: false, passphraseProtected: true };
const axeViolations = async (el: Element): Promise<string[]> =>
  (await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations.map((v) => `${v.id}: ${v.help}`);

const PASS = "correct horse battery";

describe("checkNewPassphrase", () => {
  it("counts code points, requires the minimum, a maximum, and a matching repeat", () => {
    expect(checkNewPassphrase("short", "short")).toBe("too_short");
    expect(checkNewPassphrase("😀".repeat(PASSPHRASE_MIN_CODE_POINTS), "😀".repeat(PASSPHRASE_MIN_CODE_POINTS))).toBeNull();
    expect(checkNewPassphrase("😀".repeat(PASSPHRASE_MIN_CODE_POINTS - 1), "x")).toBe("too_short");
    expect(checkNewPassphrase(PASS, "other passphrase!")).toBe("mismatch");
    expect(checkNewPassphrase("a".repeat(257), "a".repeat(257))).toBe("too_long");
  });
});

describe("ProtectDevice passphrase option", () => {
  function setup(createProfileVault: DeviceVault["createProfileVault"]) {
    const h = createHarness({ env: {}, clock: () => 1000 });
    const ctx = h.seedProfile({ profileId: PID, label: "pending" });
    const vault = { createProfileVault } as unknown as DeviceVault;
    render(<ProtectDevice services={{ client: h, registry: createActionRegistry(), ctx }} vault={vault} copy={testCopy.protect} onProtected={vi.fn()} />);
  }

  it("is off by default and creates the vault without an additional factor", async () => {
    const create = vi.fn<DeviceVault["createProfileVault"]>().mockResolvedValue({ ok: true, wrapping: { ...WRAP, passphraseProtected: false } });
    setup(create);
    expect(screen.queryByLabelText("Passphrase")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Protect this device" }));
    await waitFor(() => { expect(create).toHaveBeenCalledTimes(1); });
    expect((create.mock.calls[0]?.[1] as CreateVaultOptions).additionalFactor).toBeUndefined();
  });

  it("warns that a forgotten passphrase is unrecoverable and blocks short or mismatched input before touching the vault", async () => {
    const create = vi.fn<DeviceVault["createProfileVault"]>().mockResolvedValue({ ok: true, wrapping: WRAP });
    setup(create);
    await userEvent.click(screen.getByRole("checkbox", { name: /Also use a passphrase/ }));
    expect(screen.getByText(/cannot be recovered/)).toBeTruthy();
    expect(await axeViolations(document.body)).toEqual([]);
    await userEvent.type(screen.getByLabelText("Passphrase"), "short");
    await userEvent.type(screen.getByLabelText("Repeat passphrase"), "short");
    await userEvent.click(screen.getByRole("button", { name: "Protect this device" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Use at least 12 characters.");
    await userEvent.clear(screen.getByLabelText("Passphrase"));
    await userEvent.type(screen.getByLabelText("Passphrase"), PASS);
    await userEvent.click(screen.getByRole("button", { name: "Protect this device" }));
    expect((await screen.findByRole("alert")).textContent).toBe("The passphrases do not match.");
    expect(create).not.toHaveBeenCalled();
  });

  it("sends the passphrase as UTF-8 bytes in additionalFactor and clears the fields", async () => {
    const create = vi.fn<DeviceVault["createProfileVault"]>().mockResolvedValue({ ok: true, wrapping: WRAP });
    setup(create);
    await userEvent.click(screen.getByRole("checkbox", { name: /Also use a passphrase/ }));
    await userEvent.type(screen.getByLabelText("Passphrase"), PASS);
    await userEvent.type(screen.getByLabelText("Repeat passphrase"), PASS);
    await userEvent.click(screen.getByRole("button", { name: "Protect this device" }));
    await waitFor(() => { expect(create).toHaveBeenCalledTimes(1); });
    const factor = (create.mock.calls[0]?.[1] as CreateVaultOptions).additionalFactor;
    expect(factor?.kind).toBe("passphrase");
    expect(new TextDecoder().decode(factor?.secret)).toBe(PASS);
  });
});

describe("UnlockVault", () => {
  function setup(protectedVault: boolean, unlock: DeviceVault["unlock"]) {
    const onUnlocked = vi.fn();
    const onNeedsRecovery = vi.fn();
    const vault = {
      wrapping: vi.fn().mockResolvedValue({ ok: true, wrapping: { ...WRAP, passphraseProtected: protectedVault } }),
      unlock,
    } as unknown as DeviceVault;
    render(<UnlockVault vault={vault} profileId={PID} copy={testUnlockCopy} onUnlocked={onUnlocked} onNeedsRecovery={onNeedsRecovery} />);
    return { onUnlocked, onNeedsRecovery };
  }

  it("asks for no passphrase when none is set", async () => {
    const unlock = vi.fn<DeviceVault["unlock"]>().mockResolvedValue({ ok: true });
    const { onUnlocked } = setup(false, unlock);
    await screen.findByRole("button", { name: "Unlock" });
    expect(screen.queryByLabelText("Passphrase")).toBeNull();
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    await waitFor(() => { expect(onUnlocked).toHaveBeenCalled(); });
    expect(unlock).toHaveBeenCalledWith(PID, undefined);
  });

  it("sends the passphrase, clears the field, reports a wrong one, and unlocks on retry", async () => {
    const results: UnlockResult[] = [{ ok: false, reason: "wrong_passphrase" }, { ok: true }];
    const unlock = vi.fn<DeviceVault["unlock"]>().mockImplementation(() => Promise.resolve(results.shift() ?? { ok: true }));
    const { onUnlocked } = setup(true, unlock);
    const field = await screen.findByLabelText<HTMLInputElement>("Passphrase");
    expect(await axeViolations(document.body)).toEqual([]);
    await userEvent.type(field, "wrong guess here");
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Error: wrong_passphrase");
    expect(new TextDecoder().decode(unlock.mock.calls[0]?.[1]?.passphrase?.secret)).toBe("wrong guess here");
    expect(screen.getByLabelText<HTMLInputElement>("Passphrase").value).toBe("");
    expect(onUnlocked).not.toHaveBeenCalled();
    await userEvent.type(screen.getByLabelText("Passphrase"), PASS);
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    await waitFor(() => { expect(onUnlocked).toHaveBeenCalled(); });
  });

  it("does not call the vault with an empty passphrase, and offers recovery when data is missing or damaged", async () => {
    const unlock = vi.fn<DeviceVault["unlock"]>().mockResolvedValue({ ok: false, reason: "corrupt" });
    const { onNeedsRecovery } = setup(true, unlock);
    await screen.findByLabelText("Passphrase");
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    expect((await screen.findByRole("alert")).textContent).toBe("Error: passphrase_required");
    expect(unlock).not.toHaveBeenCalled();
    await userEvent.type(screen.getByLabelText("Passphrase"), PASS);
    await userEvent.click(screen.getByRole("button", { name: "Unlock" }));
    await userEvent.click(await screen.findByRole("button", { name: "Open recovery" }));
    expect(onNeedsRecovery).toHaveBeenCalled();
  });
});
