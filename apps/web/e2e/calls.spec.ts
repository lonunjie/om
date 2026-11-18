import { encodeTurnLease } from "@om/home-wire";
import { expect, test, type Locator, type Page } from "@playwright/test";
import { noPasskeys, onboard, reopen } from "./helpers.ts";

const APP = "http://localhost:4177";

interface CallProbe {
  connections: RTCPeerConnection[];
  videoPrompts: number;
  captured: MediaStreamTrack[];
}

async function probe(page: Page): Promise<void> {
  await page.addInitScript(() => {
    const data: CallProbe = { connections: [], videoPrompts: 0, captured: [] };
    Object.assign(window, { callProbe: data });
    const RTC = RTCPeerConnection;
    window.RTCPeerConnection = class extends RTC {
      constructor(config?: RTCConfiguration) {
        super(config);
        data.connections.push(this);
      }
    };
    const capture = navigator.mediaDevices.getUserMedia.bind(navigator.mediaDevices);
    navigator.mediaDevices.getUserMedia = async (constraints) => {
      if (constraints?.video) data.videoPrompts++;
      const stream = await capture(constraints);
      data.captured.push(...stream.getTracks());
      return stream;
    };
  });
}

const counters = (page: Page) => page.evaluate(() => {
  const { connections, videoPrompts, captured } = (window as unknown as { callProbe: CallProbe }).callProbe;
  return { peers: connections.length, videoPrompts, tracks: captured.map((track) => track.readyState) };
});

const inboundStats = (page: Page) => page.evaluate(async () => {
  const { connections } = (window as unknown as { callProbe: CallProbe }).callProbe;
  const stats = await connections.at(-1)?.getStats();
  let energy = 0;
  let audioPackets = 0;
  let videoFrames = 0;
  stats?.forEach((item: { type: string; kind?: string; totalAudioEnergy?: number; packetsReceived?: number; framesDecoded?: number }) => {
    if (item.type !== "inbound-rtp") return;
    if (item.kind === "audio") {
      energy += item.totalAudioEnergy ?? 0;
      audioPackets += item.packetsReceived ?? 0;
    }
    if (item.kind === "video") videoFrames += item.framesDecoded ?? 0;
  });
  return { energy, audioPackets, videoFrames };
});

const iceCredential = (page: Page) => page.evaluate(() => {
  const { connections } = (window as unknown as { callProbe: CallProbe }).callProbe;
  return connections.at(-1)?.localDescription?.sdp.match(/^a=ice-ufrag:(.+)$/m)?.[1] ?? "";
});

async function pressButton(page: Page, name: string): Promise<void> {
  const button: Locator = page.getByRole("button", { name, exact: true });
  await expect(button).toBeVisible();
  for (let tab = 0; tab < 40; tab++) {
    if (await button.evaluate((element) => document.activeElement === element)) {
      await page.keyboard.press("Enter");
      return;
    }
    await page.keyboard.press("Tab");
  }
  throw new Error(`Could not reach "${name}" by keyboard`);
}

async function establishContact(alice: Page, bob: Page): Promise<void> {
  await Promise.all([probe(alice), probe(bob), noPasskeys(alice), noPasskeys(bob)]);
  await onboard(alice, { stopAt: "invite", name: "Alice" });
  await alice.getByRole("button", { name: "Show my QR" }).click();
  const link = (await alice.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  await alice.getByRole("button", { name: "Skip for now" }).click();
  await onboard(bob, { stopAt: "invite", name: "Bob" });
  await bob.getByText("Have an invite?").click();
  await bob.getByLabel("Invitation link").fill(link);
  await bob.getByRole("button", { name: "Accept invitation" }).click();

  const request = alice.getByRole("region", { name: "Message request" });
  await expect(request).toBeVisible();
  await expect(alice.getByRole("button", { name: "Video call" })).toHaveCount(0);
  expect(await counters(alice)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
  await request.getByRole("button", { name: "Accept", exact: true }).click();
  await alice.getByRole("option").click();
  await alice.getByRole("textbox", { name: /message/i }).fill("Ready to call");
  await alice.getByRole("button", { name: /send/i }).click();
  await bob.getByRole("option").click();
  await expect(bob.getByRole("log").getByText("Ready to call")).toBeVisible();
  await bob.getByRole("textbox", { name: /message/i }).fill("Ready here too");
  await bob.getByRole("button", { name: /send/i }).click();
  await expect(alice.getByRole("log").getByText("Ready here too")).toBeVisible();
}

test("two devices call through Core, delay camera permission until choice, reconnect and release media", async ({ browser }) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone", "camera"] });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone", "camera"] });
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    await establishContact(alice, bob);

    const signalling: string[] = [];
    for (const page of [alice, bob]) {
      page.on("request", (request) => {
        if (request.url().endsWith("/v1/relay/deposit")) signalling.push(request.postData() ?? "");
      });
    }
    await pressButton(alice, "Video call");
    await expect(bob.getByRole("button", { name: "Answer video" })).toBeVisible();
    expect(await counters(bob)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    await pressButton(bob, "Answer audio");
    const aliceCall = alice.getByRole("region", { name: /^Call with/ });
    const bobCall = bob.getByRole("region", { name: /^Call with/ });
    await expect(aliceCall.getByRole("status")).toContainText("Call connected");
    await expect(bobCall.getByRole("status")).toContainText("Call connected");
    await expect.poll(async () => (await inboundStats(alice)).energy).toBeGreaterThan(0);
    await expect.poll(async () => (await inboundStats(bob)).energy).toBeGreaterThan(0);
    await expect.poll(async () => (await inboundStats(bob)).videoFrames).toBeGreaterThan(0);
    expect((await counters(bob)).videoPrompts).toBe(0);
    expect(signalling.length).toBeGreaterThanOrEqual(2);
    expect(signalling.join("")).not.toMatch(/a=(?:candidate|ice-ufrag)|v=0/);

    await pressButton(bob, "Mute microphone");
    await expect(bob.getByRole("button", { name: "Unmute microphone" })).toBeVisible();
    const before = await iceCredential(alice);
    expect(before).not.toBe("");
    await a.setOffline(true);
    await a.setOffline(false);
    await expect.poll(() => iceCredential(alice)).not.toBe(before);
    await expect(aliceCall.getByRole("status")).toContainText("Call connected");
    await expect(bobCall.getByRole("status")).toContainText("Call connected");
    expect((await counters(alice)).peers).toBe(1);
    expect((await counters(bob)).peers).toBe(1);
    await expect(bob.getByRole("button", { name: "Unmute microphone" })).toBeVisible();

    await pressButton(bob, "End call");
    await expect(aliceCall.getByRole("status")).toContainText("Call ended");
    await expect.poll(async () => (await counters(alice)).tracks).toEqual(["ended", "ended"]);
    await expect.poll(async () => (await counters(bob)).tracks).toEqual(["ended"]);

    await alice.evaluate(() => {
      Object.defineProperty(window, "RTCRtpScriptTransform", { value: undefined, configurable: true });
    });
    await pressButton(alice, "Audio call");
    await expect(alice.getByRole("alert")).toHaveText("This browser can’t make protected calls.");
    expect((await counters(alice)).peers).toBe(1);
    expect((await counters(alice)).tracks).toEqual(["ended", "ended"]);
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});

test("encrypted audio reaches the browser with a mismatched receive key but never plays", async ({ browser }) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone"], serviceWorkers: "block" });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone"], serviceWorkers: "block" });
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    // Replace only this test receiver's key before the real worker imports it.
    await b.route("**/assets/sframe-worker-*.js", async (route) => {
      const response = await route.fetch();
      const prelude = 'self.addEventListener("message", ({data}) => { if (data.receive instanceof Uint8Array) data.receive.fill(0xff); });\n';
      await route.fulfill({ response, body: prelude + await response.text() });
    });
    await establishContact(alice, bob);
    await pressButton(alice, "Audio call");
    await pressButton(bob, "Answer audio");
    await expect.poll(async () => (await inboundStats(bob)).audioPackets).toBeGreaterThan(0);
    await expect.poll(async () => (await inboundStats(alice)).energy).toBeGreaterThan(0);
    expect((await inboundStats(bob)).energy).toBe(0);
    await expect(bob.getByRole("region", { name: /^Call with/ }).getByRole("status")).toContainText("couldn’t connect directly", { timeout: 25_000 });
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});

test("wake socket rings while visible, then an opaque push nudges a hidden tab without media work", async ({ browser }, testInfo) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    // Control visibility deterministically, independent of window activation on the browser host.
    await bob.addInitScript(() => {
      Object.defineProperty(document, "visibilityState", { value: "visible", configurable: true });
    });
    let lastSeqFrames = 0;
    let closedSockets = 0;
    bob.on("websocket", (socket) => {
      if (!socket.url().endsWith("/v1/relay/wake")) return;
      socket.on("framereceived", () => { lastSeqFrames++; });
      socket.on("close", () => { closedSockets++; });
    });
    await establishContact(alice, bob);
    const reading = new Set<string>();
    const exposed = { clearDescription: false, callUrl: false };
    for (const page of [alice, bob]) {
      page.on("request", (request) => {
        const url = new URL(request.url());
        if (url.port !== "8788") return;
        reading.add(`${request.method()} ${url.pathname}`);
        exposed.callUrl ||= url.search !== "" || url.hash !== "";
        if (url.pathname === "/v1/relay/deposit") {
          exposed.clearDescription ||= /a=(?:candidate|ice-ufrag)|v=0/.test(request.postDataBuffer()?.toString("latin1") ?? "");
        }
      });
    }
    const before = lastSeqFrames;
    await pressButton(alice, "Audio call");
    await expect(bob.getByRole("button", { name: "Answer audio" })).toBeVisible();
    expect(lastSeqFrames).toBeGreaterThan(before);
    await bob.getByRole("button", { name: "Decline" }).click();
    await expect(alice.getByText("Call declined", { exact: true })).toBeVisible();

    await bob.evaluate(() => {
      Object.defineProperty(document, "visibilityState", { value: "hidden", configurable: true });
      document.dispatchEvent(new Event("visibilitychange"));
    });
    await expect.poll(() => closedSockets).toBeGreaterThan(0);
    const won = alice.waitForResponse((r) => r.url().endsWith("/v1/relay/deposit") && r.ok());
    await pressButton(alice, "Audio call");
    await won;
    await bob.evaluate(() => {
      navigator.serviceWorker.dispatchEvent(new MessageEvent("message", { data: { type: "om.sw/wake" } }));
    });
    await expect(bob.getByRole("button", { name: "Answer audio" })).toBeVisible();
    expect(await counters(bob)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    await bob.getByRole("button", { name: "Decline" }).click();
    await expect(alice.getByText("Call declined", { exact: true })).toBeVisible();
    const permitted = new Set(["POST /v1/relay/deposit", "POST /v1/relay/drain", "POST /v1/relay/ack", "POST /v1/intro/drain"]);
    expect([...reading].filter((operation) => !permitted.has(operation))).toEqual([]);
    expect(reading.has("POST /v1/relay/deposit")).toBe(true);
    expect(exposed).toEqual({ clearDescription: false, callUrl: false });
    await testInfo.attach("direct-call-home-observations", {
      contentType: "application/json", body: JSON.stringify({ operations: [...reading].sort(), ...exposed, receivedWakeFrame: true, hiddenSocketClosed: true }),
    });
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});

test("opening a closed app drains an eligible call and a late offer only leaves a missed call", async ({ browser }) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  try {
    const alice = await a.newPage();
    let bob = await b.newPage();
    await establishContact(alice, bob);

    async function callWhileClosed(late: boolean): Promise<Page> {
      await bob.close();
      const deposited = alice.waitForResponse((r) => r.url().endsWith("/v1/relay/deposit") && r.ok());
      await pressButton(alice, "Audio call");
      await deposited;
      const next = await b.newPage();
      await probe(next);
      await noPasskeys(next);
      // Advance only the recipient. The test Home can still deliver the authenticated envelope.
      if (late) await next.clock.setFixedTime(new Date(Date.now() + 50_000));
      await next.goto("/");
      await reopen(next);
      return next;
    }

    bob = await callWhileClosed(false);
    await expect(bob.getByRole("button", { name: "Answer audio" })).toBeVisible();
    await bob.getByRole("button", { name: "Decline" }).click();
    await expect(alice.getByText("Call declined", { exact: true })).toBeVisible();

    bob = await callWhileClosed(true);
    await bob.getByRole("option").click();
    await expect(bob.getByRole("list", { name: "Missed calls" })).toContainText("Missed call");
    await expect(bob.getByRole("button", { name: "Answer audio" })).toHaveCount(0);
    expect(await counters(bob)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    await bob.reload();
    await reopen(bob);
    await expect(bob.getByRole("list", { name: "Missed calls" })).toContainText("Missed call");
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});

test("an authenticated but disallowed contact offer does not ring or start media", async ({ browser }, testInfo) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    await establishContact(alice, bob);
    await bob.getByRole("button", { name: "About Alice" }).click();
    const allow = bob.getByRole("switch", { name: "Allow calls" });
    await allow.click();
    await expect(allow).not.toBeChecked();
    await bob.getByRole("dialog").getByRole("button", { name: "Close", exact: true }).click();

    let leases = 0;
    bob.on("request", (r) => { if (r.url().endsWith("/v1/media/turn")) leases++; });
    const consumed = bob.waitForResponse((r) => r.url().endsWith("/v1/relay/ack") && r.ok());
    await pressButton(alice, "Audio call");
    // The message following the offer proves the recipient ratchet and application have moved past the drop.
    await alice.getByRole("textbox", { name: /message/i }).fill("After the offer");
    await alice.getByRole("button", { name: /send/i }).click();
    await expect(bob.getByRole("log").getByText("After the offer")).toBeVisible();
    await consumed;
    await expect(bob.getByRole("button", { name: "Answer audio" })).toHaveCount(0);
    const media = await counters(bob);
    expect(media).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    expect(leases).toBe(0);
    await testInfo.attach("call-no-ring-observations", { contentType: "application/json", body: JSON.stringify({ ...media, leases }) });
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});

test("Maximum Privacy gathers only relayed candidates against a local TURN fixture", async ({ browser }, testInfo) => {
  test.skip(process.env["OM_E2E_LOCAL_TURN"] !== "1", "Requires an isolated local coturn fixture");
  const home = "http://127.0.0.1:8788";
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone"], serviceWorkers: "block" });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone"], serviceWorkers: "block" });
  let leases = 0;
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    for (const [page, username] of [[alice, "test-alice"], [bob, "test-bob"]] as const) {
      // Supply only the optional media capability. Real Core signalling still goes through the local Home.
      await page.route(`${home}/.well-known/om-provider-capabilities`, async (route) => {
        const response = await route.fetch();
        const doc = await response.json() as { features: string[] };
        await route.fulfill({ response, json: { ...doc, features: [...doc.features, "om.media.turn.v1"], media: { stun_urls: [] } } });
      });
      await page.route(`${home}/v1/media/turn`, async (route) => {
        expect(route.request().method()).toBe("POST");
        expect(route.request().headers()).toHaveProperty("om-admission");
        leases++;
        await route.fulfill({
          headers: { "access-control-allow-origin": "*", "cache-control": "no-store" },
          contentType: "application/cbor",
          body: Buffer.from(encodeTurnLease({ urls: ["turn:127.0.0.1:34790?transport=udp"], username, credential: "om-e2e-only", expires_at: Math.floor(Date.now() / 1000) + 60 })),
        });
      });
      const seed = `${APP}/test-media-setup`;
      await page.route(seed, (route) => route.fulfill({ contentType: "text/html", body: "<!doctype html><title>Fixture</title>" }));
      await page.goto(seed);
      await page.evaluate((origin) => new Promise<void>((resolve, reject) => {
        const handle = indexedDB.open("om-provider-secrets", 1);
        handle.onupgradeneeded = () => { handle.result.createObjectStore("secrets"); };
        handle.onerror = () => { reject(new Error("Fixture store failed")); };
        handle.onsuccess = () => {
          const db = handle.result;
          const tx = db.transaction("secrets", "readwrite");
          tx.objectStore("secrets").put(new Uint8Array(32).fill(7), `admission-v1:${origin}`);
          tx.oncomplete = () => { db.close(); resolve(); };
          tx.onerror = () => { db.close(); reject(new Error("Fixture admission failed")); };
        };
      }), home);
      await page.unroute(seed);
    }
    await establishContact(alice, bob);
    for (const page of [alice, bob]) {
      await page.goto("/settings/protection");
      await reopen(page);
      await page.getByRole("button", { name: "Use Maximum Privacy" }).click();
      await expect(page.getByText("Now using Maximum Privacy.")).toBeVisible();
      await page.goto("/");
      await reopen(page);
      await page.getByRole("option").click();
    }
    await pressButton(alice, "Audio call");
    await expect(bob.getByRole("button", { name: "Answer audio" })).toBeVisible();
    expect(leases).toBe(1);
    expect(await counters(bob)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    await pressButton(bob, "Answer audio");
    const observations = [];
    for (const page of [alice, bob]) {
      await expect(page.getByRole("region", { name: /^Call with/ }).getByRole("status")).toContainText("Call connected");
      const info = await page.evaluate(() => {
        const { connections } = (window as unknown as { callProbe: CallProbe }).callProbe;
        const peer = connections.at(-1);
        const candidates = [...(peer?.localDescription?.sdp ?? "").matchAll(/^a=candidate:.*\styp (\S+)/gm)].map((match) => match[1]);
        return { candidates, policy: peer?.getConfiguration().iceTransportPolicy };
      });
      expect(info.policy).toBe("relay");
      expect(info.candidates.length).toBeGreaterThan(0);
      expect(new Set(info.candidates)).toEqual(new Set(["relay"]));
      observations.push(info);
    }
    expect(leases).toBe(2);
    await testInfo.attach("relay-only-observations", {
      contentType: "application/json", body: JSON.stringify({ peers: observations, leases }),
    });
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});

test("Maximum Privacy with a direct-only Home refuses before capture or ICE", async ({ browser }) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone"] });
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    await establishContact(alice, bob);
    await alice.goto("/settings/protection");
    await reopen(alice);
    await alice.getByRole("button", { name: "Use Maximum Privacy" }).click();
    await expect(alice.getByText("Now using Maximum Privacy.")).toBeVisible();
    await alice.goto("/");
    await reopen(alice);
    await alice.getByRole("option").click();
    let allocations = 0;
    alice.on("request", (r) => { if (r.url().endsWith("/v1/media/turn")) allocations++; });
    await pressButton(alice, "Audio call");
    await expect(alice.getByRole("alert")).toContainText("Your protection settings need a private route");
    expect(await counters(alice)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    expect(allocations).toBe(0);
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});
