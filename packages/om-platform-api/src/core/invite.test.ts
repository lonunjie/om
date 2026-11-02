import { describe, expect, it } from "vitest";
import { canonicalInviteLink, inviteTokenFromFragment, parseInviteLink, webInviteLink } from "./invite";

const T = "AAAAAAAAAAAAAAAAAAAAAA";

describe("invitation links", () => {
  it("accepts the canonical form and the web form with the same token", () => {
    for (const link of [`om:invite/${T}`, `https://om.lonunjie.org/#invite/${T}`, `https://om.lonunjie.org#invite/${T}`, `http://localhost:4173/#invite/${T}`]) {
      expect(parseInviteLink(link)).toEqual({ ok: true, token: T });
    }
  });

  it("finds a link inside a pasted message and ignores surrounding space", () => {
    expect(parseInviteLink(`\n Join me on OM: https://om.lonunjie.org/#invite/${T}  thanks!\n`)).toEqual({ ok: true, token: T });
    expect(parseInviteLink(`here: om:invite/${T}.`)).toEqual({ ok: true, token: T });
  });

  it("rejects everything else", () => {
    expect(parseInviteLink("")).toEqual({ ok: false, reason: "empty" });
    expect(parseInviteLink("   ")).toEqual({ ok: false, reason: "empty" });
    expect(parseInviteLink("x".repeat(2001))).toEqual({ ok: false, reason: "too_long" });
    for (const bad of ["om:invite/short", "om:other/AAAAAAAAAAAAAAAAAAAAAA", "https://x.example/a", `https://x.example/invite/${T}`, `xom:invite/${T}`, "om:invite/!!!!!!!!!!!!!!!!!!!!!!", `javascript:alert(1)//#invite/${T}`, `ftp://h/#invite/${T}`]) {
      expect(parseInviteLink(bad), bad).toEqual({ ok: false, reason: "malformed" });
    }
  });

  it("builds the shareable web link and the canonical link, and reads a page fragment", () => {
    expect(webInviteLink(`om:invite/${T}`, "https://om.lonunjie.org/")).toBe(`https://om.lonunjie.org/#invite/${T}`);
    expect(webInviteLink("not a link", "https://om.lonunjie.org")).toBe("not a link");
    expect(canonicalInviteLink(T)).toBe(`om:invite/${T}`);
    expect(inviteTokenFromFragment(`#invite/${T}`)).toBe(T);
    expect(inviteTokenFromFragment(`#/invite/${T}`)).toBe(T);
    expect(inviteTokenFromFragment(`#invite/${T}/x`)).toBeNull();
    expect(inviteTokenFromFragment("#other")).toBeNull();
    expect(inviteTokenFromFragment("")).toBeNull();
  });
});
