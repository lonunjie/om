// @vitest-environment jsdom
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createHarness, type SeedExpressivePack } from "@om/core-harness";
import { asConversationId, asOperationId, asPersonId, asProfileId, type ExpressiveMediaApi } from "@om/platform-api";
import { ConversationView, ExpressivePickerControl, SecurityNotices, ThemeProvider, createActionRegistry, type ExpressiveSurface } from "@om/ui";
import { conversationCopy, expressiveCopy } from "../copy/en";
import { devExpressiveHost, expressiveFixture, expressiveRichText, inlineMessage } from "./expressiveFixture";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

const profile = asProfileId("work");
const conversationId = asConversationId("chat");
const remote = "https://attacker.invalid/spy.webp";
const literal = `<style>body{display:none}</style><img src="${remote}">`;

const a11y = async (node: Element) => {
  const { violations } = await axe.run(node, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  expect(violations.map((v) => v.id)).toEqual([]);
};

function setup(enabled = true, hostile = false) {
  const fixture: SeedExpressivePack = hostile ? {
    ...expressiveFixture,
    preview: { ...expressiveFixture.preview, description: literal, creatorDisplay: literal, licenseUrl: remote, sourceUrls: [remote] },
  } : expressiveFixture;
  const client = createHarness({ expressiveMedia: enabled });
  const ctx = client.seedProfile({
    profileId: profile, label: "Work", expressivePacks: [fixture], conversations: [{
      id: conversationId, personId: asPersonId("ana"), displayName: "Ana",
      messages: [{ text: "Hello", direction: "incoming" }, inlineMessage],
    }],
  });
  let sequence = 0;
  const services = { client, ctx, registry: createActionRegistry({ newOperationId: () => asOperationId(`op-${String(++sequence)}`) }) };
  const close = vi.fn();
  const play = vi.fn();
  const pause = vi.fn();
  const frame = { rgba: new Uint8ClampedArray(24 * 24 * 4), durationUs: 100_000 };
  const nextFrame = vi.fn(() => Promise.resolve({ state: "frame" as const, frame }));
  const decode = vi.fn<ExpressiveMediaApi["decode"]>((asset) => Promise.resolve({
    state: "decoded", facts: { width: asset.facts.width, height: asset.facts.height, frameCount: asset.facts.frames }, frame,
    ...(asset.kind === "sticker" ? { playback: { close, play, pause, nextFrame } } : {}),
  }));
  const draw = vi.fn();
  vi.spyOn(HTMLCanvasElement.prototype, "getContext").mockReturnValue({ putImageData: draw } as unknown as CanvasRenderingContext2D);
  vi.stubGlobal("ImageData", function imageData(data: Uint8ClampedArray, width: number, height: number) {
    return { data, width, height };
  });
  const surface: ExpressiveSurface = { ...devExpressiveHost(client, profile), copy: expressiveCopy, decoder: { decode } };
  return { services, surface, fixture, close, play, pause, draw, decode, nextFrame };
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });
const dialog = () => screen.getByRole("dialog");
const pick = async () => {
  const user = userEvent.setup();
  await user.click(button("Stickers"));
  return user;
};
const install = async (user: ReturnType<typeof userEvent.setup>) => {
  await user.click(await screen.findByRole("button", { name: "Review pack" }));
  await user.click(await screen.findByRole("button", { name: "Install Little faces" }));
  await user.click(await screen.findByRole("button", { name: "Little faces" }));
};

function Screen({ services, surface }: Pick<ReturnType<typeof setup>, "services" | "surface">) {
  return <ThemeProvider motion="reduced">
    <ExpressivePickerControl {...{ services, surface, conversationId }} />
    <ConversationView {...{ services, conversationId }} copy={conversationCopy} expressive={surface} richTextFor={expressiveRichText} />
  </ThemeProvider>;
}

describe("expressive media", () => {
  it("omits the picker and skips the decoder when the Core capability is off", async () => {
    const { services, surface, decode } = setup(false);
    render(<Screen {...{ services, surface }} />);
    await screen.findByText("Hello");
    expect(screen.queryByRole("button", { name: "Stickers" })).toBeNull();
    expect(decode).not.toHaveBeenCalled();
  });

  it("confirms the reviewed pack, keeps favourites and recents local, sends and removes it", async () => {
    const { services, surface, play, pause, nextFrame, draw, close } = setup();
    const { container, unmount } = render(<Screen {...{ services, surface }} />);
    const user = await pick();
    expect(await screen.findByText(expressiveCopy.empty)).toBeTruthy();
    await a11y(dialog());
    const mutate = vi.spyOn(services.client.expressive, "do");
    await user.click(button("Review pack"));
    expect(await screen.findByText("Signed by", { exact: true })).toBeTruthy();
    expect(screen.getByText("000102030405060708090a0b0c0d0e0f")).toBeTruthy();
    expect(screen.getByText(/approval from 1 of 1/)).toBeTruthy();
    expect(screen.getByText(/CC0-1.0/)).toBeTruthy();
    expect(mutate).not.toHaveBeenCalled();
    await a11y(dialog());
    await user.click(button("Install Little faces"));
    expect(mutate).toHaveBeenCalledWith(services.ctx, expect.objectContaining({ kind: "install_pack", confirmation: {
      releaseId: expressiveFixture.preview.releaseId, controlDigest: expressiveFixture.preview.controlDigest,
    } }));
    await user.click(await screen.findByRole("button", { name: "Little faces" }));
    await user.click(await screen.findByRole("button", { name: "Add Hello smile to favourites" }));
    const favourites = await screen.findByRole("region", { name: "Favourites" });
    await a11y(dialog());
    await user.click(within(favourites).getByRole("button", { name: "Send Hello smile" }));
    const log = screen.getByRole("log", { name: "Messages" });
    expect(await within(log).findByRole("img", { name: "A smiling sticker" })).toBeTruthy();
    expect(await screen.findByRole("region", { name: "Recent" })).toBeTruthy();
    expect(play).not.toHaveBeenCalled();
    expect(nextFrame).not.toHaveBeenCalled();
    expect(draw).toHaveBeenCalled();
    await user.keyboard("{Escape}");
    expect(document.activeElement).toBe(button("Stickers"));
    await user.click(button("Play"));
    expect(play).toHaveBeenCalledOnce();
    await waitFor(() => { expect(nextFrame).toHaveBeenCalled(); });
    await user.click(button("Pause"));
    expect(pause).toHaveBeenCalledOnce();
    await a11y(log);
    await user.click(button(/^React: Hello/));
    await user.click(button("Custom reactions"));
    await user.click(await screen.findByRole("button", { name: "Little faces" }));
    await user.click(await screen.findByRole("button", { name: "React with Cheer" }));
    expect(await within(log).findByRole("img", { name: "A cheering reaction" })).toBeTruthy();
    expect(await within(log).findByRole("img", { name: "A small smiling face" })).toBeTruthy();
    await user.keyboard("{Escape}");
    await user.click(button("Stickers"));
    await user.click(await screen.findByRole("button", { name: "Little faces" }));
    await user.click(await screen.findByRole("button", { name: "About Little faces" }));
    await a11y(dialog());
    await user.click(button("Remove Little faces"));
    expect(await screen.findByText("Remove Little faces from this device?")).toBeTruthy();
    await a11y(dialog());
    await user.click(button("Remove Little faces"));
    await screen.findByText(expressiveCopy.removed);
    expect(await within(log).findByRole("img", { name: "A smiling sticker" })).toBeTruthy();
    expect(container.querySelector(".om-expressive-asset [src], .om-expressive-asset [srcset]")).toBeNull();
    unmount();
    expect(close).toHaveBeenCalled();
  });

  it("exposes load, review, install, item, favourite, send and removal failures without machine copy", async () => {
    const { services, surface } = setup();
    const read = services.client.expressive.query.bind(services.client.expressive);
    let fail: "packs" | "preview_pack" | null = "packs";
    vi.spyOn(services.client.expressive, "query").mockImplementation((ctx, query) => {
      if (query.kind === fail) {
        fail = null;
        return Promise.reject(new Error("private machine details"));
      }
      return read(ctx, query);
    });
    render(<Screen {...{ services, surface }} />);
    const user = await pick();
    await screen.findByText(expressiveCopy.loadFailed);
    await a11y(dialog());
    await user.click(button("Try again"));
    fail = "preview_pack";
    await user.click(await screen.findByRole("button", { name: "Review pack" }));
    await screen.findByText(expressiveCopy.reviewFailed);
    await a11y(dialog());
    await user.click(button("Review pack"));
    const mutate = vi.spyOn(services.client.expressive, "do");
    mutate.mockResolvedValueOnce({ refused: "invalid" });
    await user.click(await screen.findByRole("button", { name: "Install Little faces" }));
    await screen.findByText(expressiveCopy.installFailed);
    await a11y(dialog());
    await user.click(button("Install Little faces"));
    await user.click(await screen.findByRole("button", { name: "Little faces" }));
    mutate.mockResolvedValueOnce({ refused: "pack_missing" });
    await user.click(await screen.findByRole("button", { name: "Add Hello smile to favourites" }));
    await screen.findByText(expressiveCopy.favouriteFailed);
    await a11y(dialog());
    vi.spyOn(surface, "readCached").mockResolvedValueOnce(null);
    await user.click(button("Send Hello smile"));
    await screen.findByText(expressiveCopy.missingItem);
    await a11y(dialog());
    vi.spyOn(services.client.expressive, "prepare").mockResolvedValueOnce({ ok: false, refusal: { error: "integrity_failure" } });
    await user.click(button("Send Hello smile"));
    await screen.findByText(expressiveCopy.sendFailed);
    await a11y(dialog());
    await user.click(button("About Little faces"));
    await user.click(button("Remove Little faces"));
    mutate.mockResolvedValueOnce({ refused: "pack_missing" });
    await user.click(button("Remove Little faces"));
    await screen.findByText(expressiveCopy.removeFailed);
    await a11y(dialog());
    expect(screen.queryByText("private machine details")).toBeNull();
  });

  it("announces loading and preserves an uncertain send without enabling a duplicate", async () => {
    const { services, surface } = setup();
    const query = services.client.expressive.query.bind(services.client.expressive);
    let releaseLoad: () => void = () => {};
    const wait = new Promise<void>((resolve) => { releaseLoad = resolve; });
    vi.spyOn(services.client.expressive, "query").mockImplementation(async (ctx, value) => {
      if (value.kind === "packs") await wait;
      return query(ctx, value);
    });
    render(<Screen {...{ services, surface }} />);
    const user = await pick();
    expect(within(dialog()).getByRole("status").textContent).toBe(expressiveCopy.loading);
    await a11y(dialog());
    releaseLoad();
    await install(user);
    const dispatch = vi.spyOn(services.client, "dispatch").mockRejectedValueOnce(new Error("acknowledgement unavailable"));
    const send = await screen.findByRole<HTMLButtonElement>("button", { name: "Send Hello smile" });
    await user.click(send);
    await screen.findByText(expressiveCopy.pending);
    expect(send.disabled).toBe(true);
    await user.click(send);
    expect(dispatch).toHaveBeenCalledOnce();
    await a11y(dialog());
  });

  it("uses an alt-text fallback for a decoder refusal", async () => {
    const { services, surface, decode } = setup();
    decode.mockResolvedValue({ state: "fallback", reason: "decode_failed" });
    render(<Screen {...{ services, surface }} />);
    const user = await pick();
    await install(user);
    await user.click(button("Send Hello smile"));
    await user.keyboard("{Escape}");
    const log = screen.getByRole("log", { name: "Messages" });
    const alt = await within(log).findByText("A smiling sticker");
    const sticker = alt.closest("li");
    if (sticker === null) throw new Error("missing sticker");
    expect(await within(sticker).findByText(expressiveCopy.unavailable)).toBeTruthy();
    expect(within(sticker).queryByRole("img", { name: "A smiling sticker" })).toBeNull();
    await a11y(log);
  });

  it("keeps hostile pack strings out of URL, style and security surfaces", async () => {
    const { services, surface } = setup(true, true);
    const { container } = render(<ThemeProvider>
      <SecurityNotices notices={[{ code: "safety_number_changed" }]} copy={conversationCopy.notices} />
      <ExpressivePickerControl {...{ services, surface, conversationId }} />
      <ConversationView {...{ services, conversationId }} copy={conversationCopy} expressive={surface} richTextFor={(item) => ({
        version: 1, blocks: [{ type: "paragraph", children: [
          { type: "link", destination: "https://example.net/actual", children: [{ type: "text", text: "External page" }] },
          ...(item.text === inlineMessage.text ? [{ type: "custom_emoji", index: 0, fallbackText: literal, style: "position:fixed;inset:0", src: remote }] : []),
        ] }],
      })} />
    </ThemeProvider>);
    const notice = container.querySelector('[data-reason-code="safety_number_changed"]');
    const link = (await screen.findAllByRole("link", { name: /External page/ }))[0];
    if (notice === null || link === undefined) throw new Error("missing security surfaces");
    const outer = [notice.outerHTML, link.parentElement?.outerHTML];
    const user = await pick();
    await user.click(await screen.findByRole("button", { name: "Review pack" }));
    expect(await screen.findAllByText(literal)).not.toHaveLength(0);
    expect([notice.outerHTML, link.parentElement?.outerHTML]).toEqual(outer);
    expect(notice.closest(".om-expressive-picker, .om-expressive-asset")).toBeNull();
    expect(link.closest(".om-expressive-picker, .om-expressive-asset")).toBeNull();
    expect(container.querySelector(`[src^="${remote}"], [href^="${remote}"], [srcset], .om-expressive-picker [style], .om-expressive-asset [style], .om-expressive-picker style`)).toBeNull();
    expect(link.getAttribute("href")).toBe("https://example.net/actual");
    await a11y(dialog());
    await user.keyboard("{Escape}");
    await waitFor(() => { expect(document.activeElement).toBe(button("Stickers")); });
  });
});
