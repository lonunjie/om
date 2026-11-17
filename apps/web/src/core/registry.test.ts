import { IDBFactory } from "fake-indexeddb";
import { afterEach, describe, expect, it, vi } from "vitest";
import { asProfileId } from "@om/platform-api";
import { createProfileRegistry } from "./registry";

describe("the identity registry", () => {
  afterEach(() => { vi.useRealTimers(); });

  it("lists identities in the order they were created, whatever their ids sort like", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    const registry = createProfileRegistry(new IDBFactory());
    const ids = ["p-zulu", "p-alpha", "p-mike"].map(asProfileId);
    for (const id of ids) {
      await registry.add(id);
      vi.advanceTimersByTime(5);
    }
    expect(await registry.list()).toEqual(ids);
  });

  it("breaks a tie on the creation time by id so the order is still stable", async () => {
    vi.useFakeTimers({ toFake: ["Date"] });
    const registry = createProfileRegistry(new IDBFactory());
    await registry.add(asProfileId("p-b"));
    await registry.add(asProfileId("p-a"));
    expect(await registry.list()).toEqual(["p-a", "p-b"]);
  });
});
