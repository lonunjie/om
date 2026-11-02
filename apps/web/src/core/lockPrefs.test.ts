// @vitest-environment jsdom
import { afterEach, describe, expect, it } from "vitest";
import { readIdleMinutes, writeIdleMinutes } from "./lockPrefs";

afterEach(() => { globalThis.localStorage.clear(); });

describe("idle lock preference", () => {
  it("defaults to only locking when OM is closed, and keeps a valid choice", () => {
    expect(readIdleMinutes()).toBeNull();
    writeIdleMinutes(15);
    expect(readIdleMinutes()).toBe(15);
    writeIdleMinutes(null);
    expect(readIdleMinutes()).toBeNull();
  });
  it("ignores anything it did not write itself", () => {
    for (const junk of ["0", "-5", "7", "abc", "1e3", ""]) { globalThis.localStorage.setItem("om.lock.idle-minutes", junk); expect(readIdleMinutes(), junk).toBeNull(); }
  });
});
