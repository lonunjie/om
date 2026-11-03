// SPDX-License-Identifier: MPL-2.0
import { admissionHash } from "@om/home-wire";
import { describe, expect, it } from "vitest";
import { adoptAdmission, pendingAdmissionHash, readAdmission } from "./admission";
import { createMemorySecretStore } from "./secrets";

const HOME = "https://mine.example";

describe("owner admission secret", () => {
  it("shows the same setup value until a Home is chosen, as lowercase hex of a 32-byte hash", async () => {
    const store = createMemorySecretStore();
    const first = await pendingAdmissionHash(store);
    expect(first).toMatch(/^[0-9a-f]{64}$/);
    expect(await pendingAdmissionHash(store)).toBe(first);
  });

  it("binds the secret that produced the shown value to the address, and to no other", async () => {
    const store = createMemorySecretStore();
    const shown = await pendingAdmissionHash(store);
    expect(await readAdmission(store, HOME)).toBeUndefined();
    expect(await adoptAdmission(store, HOME)).toBe(true);
    const secret = await readAdmission(store, HOME);
    expect(secret?.length).toBe(32);
    expect(await admissionHash(secret ?? new Uint8Array(0))).toBe(shown);
    expect(await readAdmission(store, "https://other.example")).toBeUndefined();
  });

  it("retires the pending secret: the next Home gets a fresh value, and adopting twice binds nothing", async () => {
    const store = createMemorySecretStore();
    const one = await pendingAdmissionHash(store);
    await adoptAdmission(store, HOME);
    expect(await adoptAdmission(store, "https://second.example")).toBe(false);
    const two = await pendingAdmissionHash(store);
    expect(two).not.toBe(one);
    await adoptAdmission(store, "https://second.example");
    const a = await readAdmission(store, HOME);
    const b = await readAdmission(store, "https://second.example");
    expect(a).toBeDefined();
    expect(b).toBeDefined();
    expect(Buffer.from(a ?? []).equals(Buffer.from(b ?? []))).toBe(false);
  });
});
