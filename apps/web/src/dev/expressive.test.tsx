// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import axe from "axe-core";
import { afterEach, describe, expect, it, vi } from "vitest";
import { createHarness, type SeedExpressivePack } from "@om/core-harness";
import { asConversationId, asMessageId, asOperationId, asPersonId, asProfileId, PACK_FILE_MAX_BYTES, PACK_FILE_MEDIA_TYPE, type ExpressiveMediaApi, type GroupMessageView } from "@om/platform-api";
import { ConversationView, ExpressivePickerControl, GroupChat, GroupComposer, ProfileProvider, SecurityNotices, ThemeProvider, createActionRegistry, createMemoryKeyValue, createProfileScopedState, type ExpressiveSurface } from "@om/ui";
import { conversationCopy, expressiveCopy, groupsCopy } from "../copy/en";
import { Shell } from "../core/Shell";
import { devExpressiveHost, expressiveFixture, expressiveRichText, inlineMessage } from "./expressiveFixture";

afterEach(() => { cleanup(); vi.restoreAllMocks(); vi.unstubAllGlobals(); });

const profile = asProfileId("work");
const conversationId = asConversationId("chat");
const remote = "https://attacker.invalid/spy.webp";
const packFile = Uint8Array.of(0xa4, 1, 2, 3);
const literal = `<style>body{display:none}</style><img src="${remote}">`;

const a11y = async (node: Element) => {
  const { violations } = await axe.run(node, { rules: { "color-contrast": { enabled: false }, region: { enabled: false } } });
  expect(violations.map((v) => v.id)).toEqual([]);
};

function setup(enabled = true, hostile = false, received = false) {
  const fixture: SeedExpressivePack = hostile ? {
    ...expressiveFixture,
    preview: { ...expressiveFixture.preview, description: literal, creatorDisplay: literal, licenseUrl: remote, sourceUrls: [remote] },
  } : expressiveFixture;
  const client = createHarness({ expressiveMedia: enabled });
  const ctx = client.seedProfile({
    profileId: profile, label: "Work", expressivePacks: [{ ...fixture, file: packFile }], conversations: [{
      id: conversationId, personId: asPersonId("ana"), displayName: "Ana",
      messages: [{ text: "Hello", direction: "incoming" }, inlineMessage, ...(received ? [{
        text: "A pack for you", direction: "incoming" as const,
        attachments: [{ filename: "faces.ompack", mediaType: PACK_FILE_MEDIA_TYPE, bytes: packFile }],
      }] : [])],
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
  const surface: ExpressiveSurface = { ...devExpressiveHost(client, profile), copy: expressiveCopy, decoder: { decode },
    files: { pick: () => Promise.resolve([{ name: "faces.ompack", mimeType: "", size: packFile.length, bytes: () => Promise.resolve(packFile.slice()) }]) },
  };
  return { services, surface, fixture, close, play, pause, draw, decode, nextFrame };
}

const button = (name: string | RegExp) => screen.getByRole("button", { name });
const dialog = () => screen.getByRole("dialog");
async function readyRoom({ client, ctx }: ReturnType<typeof setup>["services"]) {
  const created = await client.groups.do(ctx, { kind: "create", name: "Planning", relationshipIds: [] });
  const room = created.ok && created.events.find((event) => event.kind === "created");
  if (!room) throw new Error("test Room unavailable");
  const get = client.groups.get.bind(client.groups);
  vi.spyOn(client.groups, "get").mockImplementation((...args) => {
    const detail = get(...args);
    return detail === null ? null : { ...detail, canUpload: true };
  });
  return room.conversationId;
}

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
  it("shows the Core policy disclosure next to the default-off upload reuse choice", async () => {
    const { services, surface } = setup();
    const api = services.client.expressive;
    const blobExposure = vi.fn(() => ({
      kind: "shared_blob_linkage" as const,
      line: "Shared blob: the BlobStore can link conversations by BlobID.",
    }));
    const prepare = vi.fn(api.prepare.bind(api));
    const client = { ...services.client, expressive: { ...api, blobExposure, prepare } };
    render(<Screen services={{ ...services, client }} surface={surface} />);
    const user = await pick();
    await install(user);
    const option = screen.getByRole<HTMLInputElement>("checkbox", { name: expressiveCopy.allowUploadReuse });
    expect(option.checked).toBe(false);
    const disclosure = screen.getByText(expressiveCopy.blobExposure.shared_blob_linkage);
    expect(option.getAttribute("aria-describedby")).toBe(disclosure.id);
    expect(blobExposure).toHaveBeenCalledWith("shared_blob");
    await a11y(dialog());
    await user.click(option);
    await user.click(button("Send Hello smile"));
    await screen.findByText(expressiveCopy.sent);
    expect(prepare).toHaveBeenCalledWith(services.ctx, expect.anything(), expect.any(Uint8Array), "shared_blob");
    expect(option.checked).toBe(false);
  });

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

describe("pack files", () => {
  it.each(["picker", "drop"])("reviews a host %s file before allowing the staged install", async (source) => {
    window.history.replaceState(null, "", "/");
    const { services, surface, fixture } = setup();
    const { client, ctx } = services;
    const installFile = vi.spyOn(client.expressive, "do");
    const discard = vi.spyOn(client.expressive, "discardPackFile");
    const store = createProfileScopedState(createMemoryKeyValue());
    render(<ProfileProvider client={client} contexts={client} store={store} initialProfileId={profile}>
      <Shell services={services} expressiveHost={surface} />
    </ProfileProvider>);
    const user = userEvent.setup();
    const choose = screen.getByRole("button", { name: expressiveCopy.importFile });
    const open = async () => {
      if (source === "picker") await user.click(choose);
      else fireEvent.drop(screen.getByText(expressiveCopy.dropFile), { dataTransfer: { types: ["Files"], files: [{
        name: "faces.ompack", type: PACK_FILE_MEDIA_TYPE, size: packFile.length,
        arrayBuffer: () => Promise.resolve(packFile.slice().buffer),
      }] } });
      const review = within(await screen.findByRole("dialog", { name: expressiveCopy.review }));
      expect(await review.findByRole("heading", { name: fixture.preview.title })).toBe(document.activeElement);
      expect(review.getByText(expressiveCopy.signedBy)).toBeTruthy();
      expect(review.getByText(/2 items/).textContent).toContain(expressiveCopy.size(fixture.preview.size));
      expect(review.queryByRole("checkbox", { name: expressiveCopy.allowUploadReuse })).toBeNull();
      return review.getByRole("button", { name: expressiveCopy.install(fixture.preview.title) });
    };

    await open();
    await a11y(dialog());
    expect(installFile).not.toHaveBeenCalled();
    expect(await client.expressive.query(ctx, { kind: "packs" })).toEqual([]);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(document.activeElement).toBe(choose);
    expect(discard).toHaveBeenCalledTimes(1);
    expect(await client.expressive.query(ctx, { kind: "packs" })).toEqual([]);

    const refused = await open();
    installFile.mockResolvedValueOnce({ refused: "confirmation_required" });
    await user.click(refused);
    expect(await screen.findByText(expressiveCopy.packChanged)).toBeTruthy();
    expect(await client.expressive.query(ctx, { kind: "packs" })).toEqual([]);
    await user.keyboard("{Escape}");

    await user.click(await open());
    expect(await screen.findByText(expressiveCopy.installed)).toBeTruthy();
    expect(await client.expressive.query(ctx, { kind: "packs" })).toHaveLength(1);
  });

  it.each(["message", "picker", "drop"])("reviews and confirms a pack from the %s without requesting its provenance", async (source) => {
    const { services, surface } = setup(true, true, true);
    const fetch = vi.fn<typeof globalThis.fetch>();
    vi.stubGlobal("fetch", fetch);
    const read = vi.spyOn(services.client.attachments, "open");
    const installFile = vi.spyOn(services.client.expressive, "do");
    render(<Screen {...{ services, surface }} />);
    await screen.findByText("faces.ompack");
    expect(read).not.toHaveBeenCalled();
    const user = userEvent.setup();
    if (source === "message") {
      await user.click(within(screen.getByRole("log")).getByRole("button", { name: "Review pack" }));
      expect((await screen.findByText("File sent by")).textContent).toBe("File sent by Ana");
    } else {
      await user.click(button("Stickers"));
      if (source === "picker") await user.click(await screen.findByRole("button", { name: "Choose pack file" }));
      else {
        const drop = await screen.findByText("Drop a pack file here.");
        fireEvent.drop(drop, { dataTransfer: { types: ["Files"], files: [{
          name: "faces.ompack", type: "", size: packFile.length,
          arrayBuffer: () => Promise.resolve(packFile.slice().buffer),
        }] } });
      }
      expect(screen.queryByText("File sent by")).toBeNull();
    }
    const confirm = await screen.findByRole("button", { name: "Install Little faces" });
    expect(installFile).not.toHaveBeenCalled();
    expect(dialog().querySelector("a, img, iframe")).toBeNull();
    await a11y(dialog());
    await user.click(confirm);
    expect(await screen.findByText(expressiveCopy.installed)).toBeTruthy();
    await a11y(dialog());
    expect(read).toHaveBeenCalledTimes(source === "message" ? 1 : 0);
    expect(fetch).not.toHaveBeenCalled();
    await user.click(await screen.findByRole("button", { name: "Little faces" }));
    await user.click(button("Preview Hello smile"));
    expect(await within(dialog()).findByRole("img", { name: "A smiling sticker" })).toBeTruthy();
    await a11y(dialog());
    expect(fetch).not.toHaveBeenCalled();
  });

  it("states size, read, validation, lineage and save failures in plain words", async () => {
    const { services, surface } = setup();
    if (surface.files === undefined) throw new Error("file picker unavailable");
    const pickFile = vi.spyOn(surface.files, "pick");
    const review = vi.spyOn(services.client.expressive, "reviewPackFile");
    const bytes = vi.fn(() => Promise.resolve(packFile));
    render(<Screen {...{ services, surface }} />);
    const user = await pick();
    const choose = await screen.findByRole("button", { name: "Choose pack file" });
    pickFile.mockResolvedValueOnce([{ name: "big.ompack", size: PACK_FILE_MAX_BYTES + 1, mimeType: "", bytes }]);
    await user.click(choose);
    expect(await screen.findByText(expressiveCopy.packTooLarge)).toBeTruthy();
    expect(bytes).not.toHaveBeenCalled();
    pickFile.mockRejectedValueOnce(new Error("file input"));
    await user.click(choose);
    expect(await screen.findByText(expressiveCopy.reviewFailed)).toBeTruthy();
    review.mockResolvedValueOnce({ refused: "bad_signature" });
    await user.click(choose);
    expect(await screen.findByText(expressiveCopy.reviewFailed)).toBeTruthy();
    review.mockResolvedValueOnce({ refused: "pack_needs_earlier_version" });
    await user.click(choose);
    expect(await screen.findByText(expressiveCopy.packNeedsEarlier)).toBeTruthy();
    await a11y(dialog());
    await user.click(choose);
    vi.spyOn(services.client.expressive, "do").mockResolvedValueOnce({ refused: "pack_storage" });
    await user.click(await screen.findByRole("button", { name: "Install Little faces" }));
    expect(await screen.findByText(expressiveCopy.installFailed)).toBeTruthy();
    await a11y(dialog());
  });
});

describe("Room expressive media", () => {
  it.each([false, true])("uses the Room id when sending and renders Core's fallback choice (%s)", async (fallback) => {
    const { services, surface, decode } = setup();
    const { client, ctx } = services;
    const groupId = await readyRoom(services);
    const messageId = asMessageId(`room/${groupId}/sticker`);
    const messages: GroupMessageView[] = [];
    vi.spyOn(client.groups, "messages").mockImplementation(() => messages);
    const dispatch = vi.spyOn(client, "dispatch").mockImplementation((intent) => {
      messages.push({ id: "sticker", text: "🙂", outgoing: true, senderAccount: "me", senderDevice: "device", atMs: 500, epoch: 1,
        attachmentMessageId: messageId, attachments: [{ index: 0, blobId: "blob", mediaType: "image/webp", size: 15, state: "stored_for_later" }],
        ...(fallback ? { expressiveFallback: true } : {}),
      });
      return Promise.resolve({ operationId: intent.operationId, profileId: ctx.profileId });
    });
    vi.spyOn(client, "observe").mockImplementation((_ctx, _op, listener) => { listener({ kind: "Committed" }); return () => undefined; });
    const query = vi.fn<ExpressiveSurface["describe"]>(() => Promise.resolve({ altText: "A smiling sticker", canOpen: true }));
    const decoded = vi.spyOn(client.expressive, "decode").mockResolvedValue({ state: "placeholder", altText: "A smiling sticker" });
    const roomSurface = { ...surface, describe: query };
    const props = { services, groupId, copy: groupsCopy, attachmentCopy: conversationCopy.attachments, expressive: roomSurface };
    const renderRoom = (refreshKey: number) => <ThemeProvider motion="reduced">
      <GroupChat {...props} refreshKey={refreshKey} chats={[]} onLeft={() => undefined} />
      <GroupComposer {...props} draft="Keep my draft" onDraftChange={() => undefined} />
    </ThemeProvider>;
    const view = render(renderRoom(0));
    const user = await pick();
    await install(user);
    await user.click(button("Send Hello smile"));
    expect(await screen.findByText(expressiveCopy.sent)).toBeTruthy();
    expect(dispatch).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ kind: "SendExpressive", conversationId: groupId, target: "Attachment" }));
    await user.keyboard("{Escape}");
    view.rerender(renderRoom(1));
    const log = within(screen.getByRole("log", { name: "Group messages" }));
    expect(log.getByText("🙂")).toBeTruthy();
    expect(screen.getByRole<HTMLTextAreaElement>("textbox", { name: groupsCopy.chat.composerLabel }).value).toBe("Keep my draft");
    if (fallback) {
      expect(log.getByText(expressiveCopy.roomFallback)).toBeTruthy();
      expect(query).not.toHaveBeenCalled();
    } else {
      expect(log.queryByText(expressiveCopy.roomFallback)).toBeNull();
      await user.click(await log.findByRole("button", { name: expressiveCopy.show }));
      expect(decoded).toHaveBeenCalledWith(ctx, messageId, { kind: "attachment", index: 0 }, { decode }, { reducedMotion: true });
    }
    await a11y(screen.getByRole("log", { name: "Group messages" }));
  });

  it("stages a pack as an ordinary Room attachment until the composer sends it", async () => {
    const { services, surface } = setup();
    const { client, ctx } = services;
    const groupId = await readyRoom(services);
    const blobId = "bc".repeat(32);
    const prepare = vi.spyOn(client.attachments, "prepare").mockResolvedValue({ ok: true, blobId });
    const dispatch = vi.spyOn(client, "dispatch")
      .mockRejectedValueOnce(new Error("lost acknowledgement"))
      .mockImplementation((intent) => Promise.resolve({ operationId: intent.operationId, profileId: ctx.profileId }));
    vi.spyOn(client, "observe").mockImplementation((_ctx, _op, listener) => { listener({ kind: "Committed" }); return () => undefined; });
    const review = vi.spyOn(client.expressive, "reviewPackFile");
    const onSent = vi.fn();
    render(<GroupComposer {...{ services, groupId, onSent }} copy={groupsCopy} expressive={surface} attachmentCopy={conversationCopy.attachments}
      draft="A pack to share" onDraftChange={() => undefined} />);
    const user = userEvent.setup();
    await user.click(button(expressiveCopy.attachPack));
    const tray = await screen.findByRole("list", { name: conversationCopy.attachments.tray });
    expect(within(tray).getByText("faces.ompack")).toBeTruthy();
    expect(prepare).not.toHaveBeenCalled();
    expect(dispatch).not.toHaveBeenCalled();
    const send = screen.getByRole<HTMLButtonElement>("button", { name: groupsCopy.chat.send });
    await user.click(send);
    expect(await screen.findByText(expressiveCopy.roomWaiting)).toBeTruthy();
    await waitFor(() => { expect(send.disabled).toBe(false); });
    await user.click(send);
    await waitFor(() => { expect(onSent).toHaveBeenCalledOnce(); });
    expect(prepare).toHaveBeenCalledWith(ctx, packFile, { filename: "faces.ompack", mediaType: PACK_FILE_MEDIA_TYPE });
    expect(prepare).toHaveBeenCalledTimes(1);
    expect(dispatch).toHaveBeenCalledTimes(2);
    expect(dispatch).toHaveBeenNthCalledWith(1, expect.objectContaining({
      kind: "SendAttachments", conversationId: groupId, draft: { text: "A pack to share" }, attachments: [{ blobId }],
    }));
    expect(dispatch.mock.calls[1]).toEqual(dispatch.mock.calls[0]);
    expect(review).not.toHaveBeenCalled();
    expect(await client.expressive.query(ctx, { kind: "packs" })).toEqual([]);
  });
});
