// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { groupsLoadCopy } from "../copy/en";
import { GroupsGate, isRefusal } from "./GroupsGate";

afterEach(cleanup);
const noop = (): void => undefined;

describe("first use of groups on a device (lazy module)", () => {
  it("shows a calm waiting state while the module is fetched, and asks for it exactly once", async () => {
    const enable = vi.fn(() => new Promise<never>(noop));
    const { baseElement } = render(<GroupsGate newGroup setup={{ enable }} copy={groupsLoadCopy} onClose={noop} />);
    expect(screen.getAllByText(groupsLoadCopy.loading).length).toBeGreaterThan(0);
    expect(screen.getByText(groupsLoadCopy.loadingNote)).toBeTruthy();
    expect(enable).toHaveBeenCalledTimes(1);
    const violations = (await axe.run(baseElement, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } })).violations;
    expect(violations.map((v) => v.id)).toEqual([]);
  });
  it("tells the restart whether the person asked for the New group sheet (a held invitation must not end on that sheet)", () => {
    for (const newGroup of [true, false]) {
      const enable = vi.fn(() => new Promise<never>(noop));
      render(<GroupsGate newGroup={newGroup} setup={{ enable }} copy={groupsLoadCopy} onClose={noop} />);
      expect(enable).toHaveBeenCalledWith({ newGroup });
      cleanup();
    }
  });
  it("says plainly when the files could not be downloaded, and can try again", async () => {
    const enable = vi.fn().mockResolvedValueOnce({ ok: false, reason: "unavailable" }).mockImplementation(() => new Promise<never>(noop));
    render(<GroupsGate newGroup setup={{ enable }} copy={groupsLoadCopy} onClose={noop} />);
    await waitFor(() => { expect(screen.getByText(groupsLoadCopy.failed.unavailable)).toBeTruthy(); });
    fireEvent.click(screen.getByRole("button", { name: groupsLoadCopy.retry }));
    await waitFor(() => { expect(enable).toHaveBeenCalledTimes(2); });
    expect(screen.queryByText(groupsLoadCopy.failed.unavailable)).toBeNull();
  });
  it("says honestly that files which do not match this version were not used", async () => {
    for (const reason of ["digest_mismatch", "version_skew", "digest_skew", "size_mismatch", "table_missing"] as const) {
      cleanup();
      render(<GroupsGate newGroup setup={{ enable: () => Promise.resolve({ ok: false, reason }) }} copy={groupsLoadCopy} onClose={noop} />);
      await waitFor(() => { expect(screen.getByText(groupsLoadCopy.failed.refused)).toBeTruthy(); });
    }
    expect(isRefusal("unavailable")).toBe(false);
    expect(isRefusal("manifest_unavailable")).toBe(false);
  });
  it("treats a thrown error as unavailable, never as success", async () => {
    render(<GroupsGate newGroup setup={{ enable: () => Promise.reject(new Error("x")) }} copy={groupsLoadCopy} onClose={noop} />);
    await waitFor(() => { expect(screen.getByText(groupsLoadCopy.failed.unavailable)).toBeTruthy(); });
  });
});
