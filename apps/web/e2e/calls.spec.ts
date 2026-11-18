import { expect, test, type Locator, type Page } from "@playwright/test";
import { noPasskeys, onboard } from "./helpers.ts";

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
