import { describe, expect, it } from "vitest";
import { CHATS, parsePlace, placeTitle, placeUrl, samePlace, type Place } from "./place";

describe("places and addresses", () => {
  const places: readonly Place[] = [
    CHATS,
    { kind: "chats", chat: "c-1" },
    { kind: "chats", chat: "a b/ü?#x" },
    { kind: "settings", page: "menu" },
    { kind: "settings", page: "devices" },
    { kind: "settings", page: "recovery" },
    { kind: "settings", page: "delivery" },
    { kind: "settings", page: "lock" },
    { kind: "settings", page: "help" },
    { kind: "safety-code" },
  ];
  it("every place round-trips through its address", () => {
    for (const p of places) {
      const url = placeUrl(p);
      const at = url.indexOf("#");
      expect(parsePlace(at < 0 ? url : url.slice(0, at), at < 0 ? "" : url.slice(at))).toEqual(p);
    }
  });
  it("keeps a chat's id out of the path", () => {
    expect(placeUrl({ kind: "chats", chat: "secret-id" })).toBe("/chats#secret-id");
  });
  it("sends unknown and old addresses to the root, never to an error", () => {
    for (const path of ["/", "/index.html", "/nope", "/settings/nope", "/chats/extra"]) expect(parsePlace(path, "")).toEqual(CHATS);
  });
  it("tolerates a trailing slash, a bare /chats, a bad fragment and an invitation fragment", () => {
    expect(parsePlace("/settings/", "")).toEqual({ kind: "settings", page: "menu" });
    expect(parsePlace("/chats", "")).toEqual(CHATS);
    expect(parsePlace("/chats", "#%E0%A4%A")).toEqual(CHATS);
    expect(parsePlace("/", "#invite/abc")).toEqual(CHATS);
    expect(parsePlace("/chats", "#invite/abc")).toEqual(CHATS);
  });
  it("compares by address and titles a tab without naming a chat", () => {
    expect(samePlace({ kind: "chats", chat: "x" }, { kind: "chats", chat: "x" })).toBe(true);
    expect(samePlace(CHATS, { kind: "settings", page: "menu" })).toBe(false);
    const names = { app: "OM", chats: "Chats", settings: "Settings", safetyCode: "Safety code" };
    expect(placeTitle({ kind: "chats", chat: "Alice" }, names)).toBe("Chats · OM");
    expect(placeTitle({ kind: "settings", page: "devices" }, names)).toBe("Settings · OM");
  });
});
