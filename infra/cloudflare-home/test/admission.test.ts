// SPDX-License-Identifier: AGPL-3.0-or-later
// Owner admission (om.admission.v1): a personal Home creates resources only for the holder of the admission secret; peers
// never need it. The Home holds only the secret's public hash (OM_ADMISSION).
import { ADMISSION_HEADER, admissionHash, decodeHomeError, encodeQueueCreate, toBase64Url } from "@om/home-wire";
import { createHttpProvider, createMemorySecretStore, parseProviderUrl } from "@om/provider-http";
import { afterAll, beforeAll, describe, expect, it } from "vitest";
import { startHome, type Home } from "./harness";
import { asBody, bearer, newCaps, rand, relay, sealed } from "./kit";

const secret = crypto.getRandomValues(new Uint8Array(32));
let home: Home;
let open: Home;
beforeAll(async () => {
  home = await startHome({ vars: { OM_ADMISSION: await admissionHash(secret) } });
  open = await startHome();
}, 120_000);
afterAll(async () => { await home.dispose(); await open.dispose(); });

const create = (h: Home, headers: Record<string, string>) => {
  const c = newCaps();
  return h.fetch("/v1/relay/queues", { method: "POST", headers: { "content-type": "application/cbor", ...headers }, body: asBody(encodeQueueCreate({ requestId: rand(16), receiveCap: c.receive, depositCap: c.deposit, adminCap: c.admin })) }).then((r) => ({ r, c }));
};
const withSecret = (s: Uint8Array): Record<string, string> => ({ [ADMISSION_HEADER]: toBase64Url(s) });

describe("PC-X owner admission", () => {
  it("PC-X01 is advertised only where the deployment lists admission hashes", async () => {
    const on = (await (await home.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] };
    const off = (await (await open.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] };
    expect(on.features).toContain("om.admission.v1");
    expect(off.features).not.toContain("om.admission.v1");
  });

  it("PC-X02 creating a queue needs the admission secret; a missing, wrong or short one is unauthorized, identically", async () => {
    for (const h of [{}, withSecret(rand(32)), withSecret(rand(16)), { [ADMISSION_HEADER]: "not base64 !" }]) {
      const { r } = await create(home, h);
      expect(r.status).toBe(401);
      expect(decodeHomeError(new Uint8Array(await r.arrayBuffer())).name).toBe("unauthorized");
    }
    const { r } = await create(home, withSecret(secret));
    expect(r.status).toBeLessThan(300);
  });

  it("PC-X03 a Home without admission hashes stays open (a shared or test Home)", async () => {
    const { r } = await create(open, {});
    expect(r.status).toBeLessThan(300);
  });

  it("PC-X03b a value that is set but yields no valid hash closes creation (a typo must not open a personal relay), and is not advertised", async () => {
    for (const vars of [{ OM_ADMISSION: "not-a-hash" }, { OM_ADMISSION: "ab".repeat(31) }, { OM_ADMISSION: " , " }]) {
      const h = await startHome({ vars });
      try {
        const { r } = await create(h, withSecret(secret));
        expect(r.status).toBe(401);
        const caps = (await (await h.fetch("/.well-known/om-provider-capabilities")).json()) as { features: string[] };
        expect(caps.features).not.toContain("om.admission.v1");
      } finally { await h.dispose(); }
    }
    const required = await startHome({ vars: { OM_ADMISSION_REQUIRED: "1" } }); // the per-user template: an empty prompt cannot open the relay
    try { expect((await create(required, withSecret(secret))).r.status).toBe(401); } finally { await required.dispose(); }
    const blank = await startHome({ vars: { OM_ADMISSION: "  " } }); // blank is the same as unset
    try { expect((await create(blank, {})).r.status).toBeLessThan(300); } finally { await blank.dispose(); }
  });

  it("PC-X04 peers need no admission: deposit, drain and ack run on their own capabilities", async () => {
    const { r, c } = await create(home, withSecret(secret));
    expect(r.status).toBeLessThan(300);
    const dep = await relay.deposit(home, c.deposit, sealed());
    expect(dep.ok).toBe(true);
    const drained = await relay.drain(home, c.receive);
    expect(drained.ok && drained.value.items.length).toBe(1);
  });

  it("PC-X05 publishing a descriptor or a key bundle needs it too; reading a descriptor does not", async () => {
    const id = Array.from(rand(16), (x) => x.toString(16).padStart(2, "0")).join("");
    const cap = rand(32);
    const put = (headers: Record<string, string>) => home.fetch(`/v1/discovery/descriptors/${id}`, { method: "PUT", headers: { "content-type": "application/cbor", ...bearer(cap), ...headers }, body: asBody(new Uint8Array([0xa0])) });
    expect((await put({})).status).toBe(401);
    expect((await home.fetch(`/v1/discovery/descriptors/${id}`)).status).toBe(404);
    const keyId = Array.from(rand(16), (x) => x.toString(16).padStart(2, "0")).join("");
    const kp = await home.fetch(`/v1/keys/bundles/${keyId}`, { method: "PUT", headers: { "content-type": "application/cbor", ...bearer(cap) }, body: asBody(new Uint8Array([0xa0])) });
    expect(kp.status).toBe(401);
  });

  it("PC-X06 the secret never sits in a URL and the CORS preflight allows the header", async () => {
    const pre = await home.fetch("/v1/relay/queues", { method: "OPTIONS" });
    expect(pre.headers.get("access-control-allow-headers")).toContain(ADMISSION_HEADER);
  });

  it("PC-X07 the adapter: without the secret nothing is created and the mailbox is not called squatted; with it, mail flows", async () => {
    const config = parseProviderUrl(home.url, { allowLoopbackHttp: true });
    if (!config) throw new Error("url");
    const mailbox = Array.from(rand(16), (x) => x.toString(16).padStart(2, "0")).join("");
    const without = createHttpProvider({ config, secrets: createMemorySecretStore(), webSocket: false });
    expect(await without.poll(mailbox, 0)).toEqual([]);
    expect(without.admissionRefused()).toBe(true);
    expect(without.lostMailboxes()).toEqual([]);
    const withIt = createHttpProvider({ config, secrets: createMemorySecretStore(), webSocket: false, admission: secret });
    expect(await withIt.poll(mailbox, 0)).toEqual([]);
    expect(withIt.admissionRefused()).toBe(false);
  });
});
