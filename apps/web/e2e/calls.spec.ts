import { expect, test, type Page } from "@playwright/test";
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

const audioEnergy = (page: Page) => page.evaluate(async () => {
  const { connections } = (window as unknown as { callProbe: CallProbe }).callProbe;
  const stats = await connections.at(-1)?.getStats();
  let energy = 0;
  stats?.forEach((item: { type: string; kind?: string; totalAudioEnergy?: number }) => {
    if (item.type === "inbound-rtp" && item.kind === "audio") energy = item.totalAudioEnergy ?? 0;
  });
  return energy;
});

test("two devices call through Core, delay camera permission until choice, reconnect and release media", async ({ browser }) => {
  const a = await browser.newContext({ baseURL: APP, permissions: ["microphone", "camera"] });
  const b = await browser.newContext({ baseURL: APP, permissions: ["microphone", "camera"] });
  try {
    const alice = await a.newPage();
    const bob = await b.newPage();
    await Promise.all([probe(alice), probe(bob), noPasskeys(alice), noPasskeys(bob)]);
    await onboard(alice, { stopAt: "invite" });
    await alice.getByRole("button", { name: "Show my QR" }).click();
    const link = (await alice.locator("code[translate=no]").first().textContent())?.trim() ?? "";
    await alice.getByRole("button", { name: "Skip for now" }).click();
    await onboard(bob, { stopAt: "invite" });
    await bob.getByText("Have an invite?").click();
    await bob.getByLabel("Invitation link").fill(link);
    await bob.getByRole("button", { name: "Accept invitation" }).click();

    const request = alice.getByRole("region", { name: "Message request" });
    await expect(request.getByRole("button", { name: "Video call" })).toHaveCount(0);
    expect(await counters(alice)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    await request.getByRole("button", { name: "Accept", exact: true }).click();
    await alice.getByRole("option").click();
    await alice.getByRole("textbox", { name: /message/i }).fill("Ready to call");
    await alice.getByRole("button", { name: /send/i }).click();
    await bob.getByRole("option").click();
    await expect(bob.getByRole("log").getByText("Ready to call")).toBeVisible();

    const signalling: string[] = [];
    for (const page of [alice, bob]) {
      page.on("request", (request) => {
        if (request.url().endsWith("/v1/relay/deposit")) signalling.push(request.postData() ?? "");
      });
    }
    await alice.getByRole("button", { name: "Video call" }).press("Enter");
    await expect(bob.getByRole("button", { name: "Answer video" })).toBeVisible();
    expect(await counters(bob)).toEqual({ peers: 0, videoPrompts: 0, tracks: [] });
    await bob.getByRole("button", { name: "Answer audio" }).press("Enter");
    await expect(alice.getByText("Call connected", { exact: true })).toBeVisible();
    await expect(bob.getByText("Call connected", { exact: true })).toBeVisible();
    await expect.poll(() => audioEnergy(alice)).toBeGreaterThan(0);
    await expect.poll(() => audioEnergy(bob)).toBeGreaterThan(0);
    expect((await counters(bob)).videoPrompts).toBe(0);
    expect(signalling.length).toBeGreaterThanOrEqual(2);
    expect(signalling.join("")).not.toMatch(/a=(?:candidate|ice-ufrag)|v=0/);

    await bob.getByRole("button", { name: "Mute microphone" }).press("Enter");
    await expect(bob.getByRole("button", { name: "Unmute microphone" })).toBeVisible();
    await a.setOffline(true);
    await a.setOffline(false);
    await expect(alice.getByText("Call connected", { exact: true })).toBeVisible();
    expect((await counters(alice)).peers).toBe(1);
    expect((await counters(bob)).peers).toBe(1);
    await expect(bob.getByRole("button", { name: "Unmute microphone" })).toBeVisible();

    await bob.getByRole("button", { name: "End call" }).press("Enter");
    await expect(alice.getByText("Call ended", { exact: true })).toBeVisible();
    await expect.poll(async () => (await counters(alice)).tracks).toEqual(["ended", "ended"]);
    await expect.poll(async () => (await counters(bob)).tracks).toEqual(["ended"]);
  } finally {
    await Promise.all([a.close(), b.close()]);
  }
});
