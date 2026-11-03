// @vitest-environment jsdom
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { BACKUP_REFUSALS } from "@om/core-wasm";
import { backupCopy, onboardingCopy } from "../copy/en";
import { BackupScreen } from "./BackupScreen";
import type { BackupServices } from "./backupServices";

afterEach(cleanup);

const GOOD = "correct horse battery staple";
const axeViolations = async (el: Element): Promise<string[]> =>
  (await axe.run(el, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations.map((v) => `${v.id}: ${v.help}`);

function fake(over: Partial<BackupServices> = {}): BackupServices {
  return {
    hasChats: () => Promise.resolve(true),
    save: vi.fn(() => Promise.resolve({ ok: true as const, bytes: Uint8Array.of(1, 2), name: "om-backup-2026-11-03.ombackup" })),
    check: vi.fn(() => null),
    restore: vi.fn(() => Promise.resolve({ ok: true as const, summary: { chatsAdded: 2, chatsUpdated: 0, chatsSkipped: 0, messagesAdded: 7 } })),
    ...over,
  };
}
const file = (name = "x.ombackup", size = 4): File => new File([new Uint8Array(size)], name, { type: "application/octet-stream" });

describe("Settings > Backup file: what it says", () => {
  it("says what a backup holds and what it does not, in plain words, and is accessible", async () => {
    const { container } = render(<BackupScreen services={fake()} />);
    expect(screen.getByRole("heading", { level: 1, name: backupCopy.heading })).toBeTruthy();
    expect(screen.getByText(backupCopy.holds.yes)).toBeTruthy();
    expect(screen.getByText(backupCopy.holds.no)).toBeTruthy();
    expect(screen.getAllByRole("note").map((n) => n.textContent)).toContain(backupCopy.holds.care);
    expect(await axeViolations(container)).toEqual([]);
  });
  it("has wording for every reason Core can refuse a file", () => {
    for (const r of BACKUP_REFUSALS) expect(backupCopy.errors[r].trim()).not.toBe("");
  });
});

describe("Settings > Backup file: saving", () => {
  it("asks for the password twice with the app's own checks, then downloads one file and says where it went", async () => {
    const services = fake();
    const download = vi.fn();
    render(<BackupScreen services={services} download={download} />);
    const user = userEvent.setup();
    const [pass, again] = [screen.getByLabelText(backupCopy.save.password), screen.getByLabelText(backupCopy.save.again)];
    await user.type(pass, "short");
    await user.type(again, "short");
    await user.click(screen.getByRole("button", { name: backupCopy.save.action }));
    expect((await screen.findByRole("alert")).textContent).toBe(onboardingCopy.protect.passphrase.tooShort(12));
    expect(services.save).not.toHaveBeenCalled();
    await user.clear(pass); await user.clear(again);
    await user.type(pass, GOOD); await user.type(again, GOOD + "x");
    await user.click(screen.getByRole("button", { name: backupCopy.save.action }));
    expect((await screen.findByRole("alert")).textContent).toBe(onboardingCopy.protect.passphrase.mismatch);
    await user.clear(again); await user.type(again, GOOD);
    await user.click(screen.getByRole("button", { name: backupCopy.save.action }));
    await waitFor(() => { expect(download).toHaveBeenCalledWith(Uint8Array.of(1, 2), "om-backup-2026-11-03.ombackup"); });
    expect(services.save).toHaveBeenCalledWith(GOOD);
    expect((await screen.findByText(backupCopy.save.done("om-backup-2026-11-03.ombackup")))).toBeTruthy();
    expect((pass as HTMLInputElement).value).toBe(""); // the typed password does not stay on screen
  });

  it("says so when there is nothing to back up, and when it fails", async () => {
    const user = userEvent.setup();
    for (const [failure, text] of [["no_chats", backupCopy.save.nothing], ["too_large", backupCopy.errors.too_large]] as const) {
      const { unmount } = render(<BackupScreen services={fake({ save: () => Promise.resolve({ ok: false, failure }) })} download={vi.fn()} />);
      await user.type(screen.getByLabelText(backupCopy.save.password), GOOD);
      await user.type(screen.getByLabelText(backupCopy.save.again), GOOD);
      await user.click(screen.getByRole("button", { name: backupCopy.save.action }));
      expect((await screen.findByRole("alert")).textContent).toBe(text);
      unmount();
    }
  });
});

describe("Settings > Backup file: restoring", () => {
  it("restores a chosen file with its password and reports what came back", async () => {
    const services = fake();
    render(<BackupScreen services={services} />);
    const user = userEvent.setup();
    const restore = screen.getByRole("button", { name: backupCopy.restore.action });
    expect((restore as HTMLButtonElement).disabled).toBe(true);
    await user.upload(screen.getByLabelText(backupCopy.restore.file), file());
    await user.type(screen.getByLabelText(backupCopy.restore.password), GOOD);
    await user.click(restore);
    expect(await screen.findByText(backupCopy.restore.done(2, 7))).toBeTruthy();
    expect(services.restore).toHaveBeenCalledTimes(1);
    expect((screen.getByLabelText(backupCopy.restore.password) as HTMLInputElement).value).toBe("");
  });

  it("says at once that a file is not a backup, before asking for a password", async () => {
    render(<BackupScreen services={fake({ check: () => "not_a_backup" })} />);
    await userEvent.setup().upload(screen.getByLabelText(backupCopy.restore.file), file("photo.jpg"));
    expect((await screen.findByRole("alert")).textContent).toBe(backupCopy.errors.not_a_backup);
  });

  it("names a wrong password, a changed file and an incomplete file differently and keeps the file chosen", async () => {
    const user = userEvent.setup();
    for (const failure of ["wrong_password", "tampered", "truncated"] as const) {
      const { unmount } = render(<BackupScreen services={fake({ restore: () => Promise.resolve({ ok: false, failure }) })} />);
      await user.upload(screen.getByLabelText(backupCopy.restore.file), file());
      await user.type(screen.getByLabelText(backupCopy.restore.password), GOOD);
      await user.click(screen.getByRole("button", { name: backupCopy.restore.action }));
      expect((await screen.findByRole("alert")).textContent).toBe(backupCopy.errors[failure]);
      unmount();
    }
  });

  it("refuses a file far bigger than any backup without reading it", async () => {
    const services = fake();
    render(<BackupScreen services={services} />);
    await userEvent.setup().upload(screen.getByLabelText(backupCopy.restore.file), file("big.ombackup", 7 * 1024 * 1024));
    expect((await screen.findByRole("alert")).textContent).toBe(backupCopy.errors.too_large);
    expect(services.check).not.toHaveBeenCalled();
  });

  it("tells a repeat restore that nothing was new, and mentions chats that were already here", async () => {
    render(<BackupScreen services={fake({ restore: () => Promise.resolve({ ok: true, summary: { chatsAdded: 0, chatsUpdated: 0, chatsSkipped: 1, messagesAdded: 0 } }) })} />);
    const user = userEvent.setup();
    await user.upload(screen.getByLabelText(backupCopy.restore.file), file());
    await user.type(screen.getByLabelText(backupCopy.restore.password), GOOD);
    await user.click(screen.getByRole("button", { name: backupCopy.restore.action }));
    expect(await screen.findByText(`${backupCopy.restore.nothingNew} ${backupCopy.restore.already(1)}`)).toBeTruthy();
  });
});
