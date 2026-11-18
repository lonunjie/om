// SPDX-License-Identifier: MPL-2.0
// Wire layer of the optional push wake (om.relay.push.v1, ADR-0210): the endpoint rules a Home applies before it will ever send to an address.
import { describe, expect, it } from "vitest";
import { FEATURES, PUSH_FEATURE, PUSH_PATHS, checkPushEndpoint, parsePushRequest, pushSubscriptionId } from "./index";

describe("push feature", () => {
  it("is an advertised optional feature with fixed paths", () => {
    expect(PUSH_FEATURE).toBe("om.relay.push.v1");
    expect(Object.values(FEATURES)).toContain(PUSH_FEATURE);
    expect(PUSH_PATHS).toEqual({ key: "/v1/push/key", subscription: "/v1/push/subscription" });
  });
});

describe("checkPushEndpoint", () => {
  const ok = (u: string, extra: readonly string[] = []) => checkPushEndpoint(u, extra).ok;
  it("accepts the three browser push services over https", () => {
    expect(ok("https://fcm.googleapis.com/fcm/send/abc:def")).toBe(true);
    expect(ok("https://updates.push.services.mozilla.com/wpush/v2/gAAAA")).toBe(true);
    expect(ok("https://web.push.apple.com/QGxyz")).toBe(true);
    expect(ok("https://wns2-par02p.notify.windows.com/w/?token=x")).toBe(true);
  });
  it("refuses everything else: plain http, other hosts, credentials, ports, IP literals, fragments, long values, look-alikes", () => {
    for (const u of [
      "http://fcm.googleapis.com/x", "https://example.com/x", "https://user:pw@fcm.googleapis.com/x", "https://fcm.googleapis.com:8443/x",
      "https://127.0.0.1/x", "https://[::1]/x", "https://169.254.169.254/latest", "https://fcm.googleapis.com/x#frag",
      "https://evilgoogleapis.com/x", "https://fcm.googleapis.com.evil.test/x", "not a url", "", "https://fcm.googleapis.com/" + "a".repeat(600),
    ]) expect(ok(u), u).toBe(false);
  });
  it("lets the owner add exact origins (a local test service, a self-hosted distributor)", () => {
    expect(ok("http://127.0.0.1:4545/p/1", ["http://127.0.0.1:4545"])).toBe(true);
    expect(ok("http://127.0.0.1:4546/p/1", ["http://127.0.0.1:4545"])).toBe(false);
    expect(ok("https://up.example.org/x", ["up.example.org"])).toBe(true);
    expect(ok("https://other.example.org/x", ["up.example.org"])).toBe(false);
  });
});

describe("parsePushRequest and the subscription id", () => {
  it("reads exactly {endpoint}", () => {
    expect(parsePushRequest('{"endpoint":"https://web.push.apple.com/x"}')).toBe("https://web.push.apple.com/x");
    for (const b of ["", "[]", "{}", '{"endpoint":1}', '{"endpoint":"a","x":1}', "{"]) expect(parsePushRequest(b), b).toBeUndefined();
  });
  it("is a 32 hex digit hash of the endpoint, stable and different per endpoint", async () => {
    const a = await pushSubscriptionId("https://web.push.apple.com/a");
    expect(a).toMatch(/^[0-9a-f]{32}$/);
    expect(a).toBe(await pushSubscriptionId("https://web.push.apple.com/a"));
    expect(a).not.toBe(await pushSubscriptionId("https://web.push.apple.com/b"));
  });
});
