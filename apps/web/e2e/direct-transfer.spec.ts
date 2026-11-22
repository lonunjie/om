// Direct file transfer on the REAL Core (om_wasm.wasm) in a real browser, over the in-page loopback channel (ADR-0158 section 4, ADR-0194;
// D120: the product has no byte transport, so the e2e build wires the loopback and a spec drives it the way two devices would). Two identities
// become contacts through the shell; then, in the page, Alice sends a file that is too big for the Home straight to Bob, and Bob fetches it.
// Checked: the bytes are equal, only ciphertext was kept, the durable write comes before the receipt, the sender ends confirmed, and a cut
// mid-transfer resumes instead of starting over.
import { expect, test, type Page } from "@playwright/test";
import { addIdentity, noPasskeys, onboard, walkSetup } from "./helpers.ts";

test.use({ baseURL: "http://localhost:4176" });

interface Item { readonly messageId?: string; readonly attachments?: readonly { readonly state?: string }[] }
interface Frame { readonly kind: string }
interface Loop {
  readonly direct: {
    readonly rt: {
      contextFor(id: string): unknown;
      openProfile(id: string): Promise<unknown>;
      flush(id: string): Promise<void>;
      query(q: unknown): Promise<unknown>;
      dispatch(c: unknown): Promise<unknown>;
      attachments: { refreshLimits(): Promise<unknown>; prepareDirect(ctx: unknown, bytes: Uint8Array, meta: { mediaType: string; filename: string }): Promise<{ ok: boolean; blobId?: string }> };
      directDriver: { fetch(ctx: unknown, messageId: string, index: number): Promise<{ ok: boolean; bytes?: Uint8Array; receiptSent?: boolean; reason?: string }> };
    };
    readonly faults: { cutAfter(n: number): void; opened(): number; readonly log: readonly { readonly dir: string; readonly frame: Frame }[] };
    readonly kept: { length: number }[];
    readonly profiles: string[];
    setBlobLimit(n: number): void;
  };
}

interface Outcome { readonly ok: boolean; readonly reason?: string; readonly equal: boolean; readonly receiptSent: boolean; readonly kept: number[]; readonly keptBeforeReceipt: boolean; readonly opened: number; readonly senderState: string | undefined }

/** Alice and Bob in one browser, contacts of each other (the same steps as core.spec.ts), with no message sent yet. */
async function contacts(page: Page): Promise<void> {
  await noPasskeys(page);
  await onboard(page, { stopAt: "invite" });
  await page.getByRole("button", { name: "Show my QR" }).click();
  const link = (await page.locator("code[translate=no]").first().textContent())?.trim() ?? "";
  expect(link).toMatch(/\/#invite\//);
  await page.getByRole("button", { name: "Skip for now" }).click();
  await addIdentity(page);
  await walkSetup(page, { stopAt: "invite" });
  await page.getByText("Have an invite?").click();
  await page.getByLabel("Invitation link").fill(link);
  await page.getByRole("button", { name: "Accept invitation" }).click();
  await expect(page.getByRole("button", { name: "Settings" })).toBeVisible();
  await page.getByRole("region", { name: "Message request" }).getByRole("button", { name: "Accept", exact: true }).click();
  await expect(page.getByRole("option")).toHaveCount(1, { timeout: 30_000 });
}

/** One transfer, run in the page against the running Core: Alice offers 3000 bytes (over the 1 KiB the Home takes), Bob fetches them. */
function transfer(page: Page, cut: boolean): Promise<Outcome> {
  return page.evaluate(async (cutIt) => {
    const loop = (globalThis as unknown as { __OM_E2E_LOOPBACK__: Loop }).__OM_E2E_LOOPBACK__.direct;
    const rt = loop.rt;
    const [alice, bob] = loop.profiles;
    if (alice === undefined || bob === undefined) throw new Error("both identities should be open");
    const ctxOf = async (id: string): Promise<unknown> => { try { return rt.contextFor(id); } catch { return rt.openProfile(id); } };
    const ca = await ctxOf(alice);
    const cb = await ctxOf(bob);
    loop.setBlobLimit(1024);
    await rt.attachments.refreshLimits(); // Core learns the Home's new limit before it plans the file
    const file = Uint8Array.from({ length: 3000 }, (_, i) => (i * 31 + 5) & 255);
    const prep = await rt.attachments.prepareDirect(ca, file, { mediaType: "video/mp4", filename: "clip.mp4" });
    if (!prep.ok || prep.blobId === undefined) throw new Error(`prepare refused: ${JSON.stringify(prep)}`);
    const list = async (ctx: unknown): Promise<{ id: string }[]> => (await rt.query({ kind: "GetConversationList", ctx })) as { id: string }[];
    const last = async (ctx: unknown, id: string): Promise<Item | undefined> => ((await rt.query({ kind: "GetConversationView", ctx, conversationId: id })) as { timeline: Item[] }).timeline.at(-1);
    const aConv = (await list(ca))[0]?.id;
    if (aConv === undefined) throw new Error("no conversation for the sender");
    await rt.dispatch({ kind: "SendAttachmentsDirect", ctx: ca, operationId: `direct-${cutIt ? "cut" : "plain"}`, conversationId: aConv, draft: { text: "big one" }, attachments: [{ blobId: prep.blobId }] });
    const settle = async (): Promise<void> => { for (let i = 0; i < 20; i++) { await rt.flush(alice); await rt.flush(bob); } };
    let theirs: Item | undefined;
    for (let i = 0; i < 40 && theirs?.messageId === undefined; i++) {
      await settle();
      const bConv = (await list(cb))[0]?.id;
      theirs = bConv === undefined ? undefined : await last(cb, bConv);
      if (theirs?.messageId === undefined) await new Promise((r) => setTimeout(r, 250));
    }
    if (theirs?.messageId === undefined) throw new Error("the offer never reached the receiver");
    const before = loop.kept.length;
    const opened = loop.faults.opened();
    if (cutIt) loop.faults.cutAfter(2);
    const logBefore = loop.faults.log.length;
    const r = await rt.directDriver.fetch(cb, theirs.messageId, 0);
    const receiptAt = loop.faults.log.findIndex((l, i) => i >= logBefore && l.frame.kind === "receipt");
    await settle();
    const mine = await last(ca, aConv);
    const bytes = r.bytes;
    return {
      ok: r.ok, ...(r.reason === undefined ? {} : { reason: r.reason }),
      equal: bytes !== undefined && bytes.length === file.length && bytes.every((b, i) => b === file[i]),
      receiptSent: r.receiptSent === true,
      kept: loop.kept.slice(before).map((k) => k.length),
      keptBeforeReceipt: receiptAt > -1,
      opened: loop.faults.opened() - opened,
      senderState: mine?.attachments?.[0]?.state,
    };
  }, cut);
}

test("a file too big for the Home goes straight to the other device: equal bytes, ciphertext kept, sender confirmed", async ({ page }) => {
  test.setTimeout(150_000);
  await contacts(page);
  const o = await transfer(page, false);
  expect(o.reason).toBeUndefined();
  expect(o.ok).toBe(true);
  expect(o.equal).toBe(true);
  expect(o.receiptSent).toBe(true);
  expect(o.keptBeforeReceipt).toBe(true);
  expect(o.kept).toEqual([3000 + 16]); // the ciphertext (file plus its tag), written once
  expect(o.senderState).toBe("delivered_locally");
});

test("a channel cut mid-transfer resumes on a new channel and still ends with equal bytes and a confirmed sender", async ({ page }) => {
  test.setTimeout(150_000);
  await contacts(page);
  const o = await transfer(page, true);
  expect(o.reason).toBeUndefined();
  expect(o.ok).toBe(true);
  expect(o.equal).toBe(true);
  expect(o.opened).toBeGreaterThan(1); // the first channel was cut; the fetch opened another
  expect(o.kept).toEqual([3000 + 16]);
  expect(o.senderState).toBe("delivered_locally");
});
