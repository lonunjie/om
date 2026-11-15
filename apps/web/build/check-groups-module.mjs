// Proves a built site (default dist/) serves groups with the verified module (ADR-0187): the manifest lists both Core modules, the groups one
// lazy; the digest table compiled into the bundle's own code matches both; the real loader (src/update/modules.ts) accepts them, and refuses a
// flipped byte and another release's manifest; and the verified groups module, instantiated for real, reports the group engine present while the
// base module reports it absent. Usage: node check-groups-module.mjs [site-dir]
import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { loadVerifiedModule, parseModuleTable, CORE_WASM_PATH, GROUPS_WASM_PATH } from "../src/update/modules.ts";

const site = process.argv[2] ?? fileURLToPath(new URL("../dist", import.meta.url));
const errors = [];
const need = (c, m) => { if (!c) errors.push(m); };
const manifest = JSON.parse(readFileSync(join(site, "bundle-manifest.json"), "utf8"));
const core = manifest.files.filter((f) => CORE_WASM_PATH.test(f.path));
const groups = manifest.files.filter((f) => GROUPS_WASM_PATH.test(f.path));
need(core.length === 1, `expected one base Core module in the manifest, found ${core.length}`);
need(groups.length === 1, `expected one groups module in the manifest, found ${groups.length}`);
need(groups[0]?.lazy === true, "the groups module is not marked lazy in the manifest");
need(core[0]?.lazy !== true, "the base Core module must not be lazy");

// The table compiled into the bundle's code.
const js = readdirSync(join(site, "assets")).filter((n) => n.endsWith(".js"));
let table = null;
for (const n of js) {
  const q = "[\"'`]";
  const d = (k) => `${k}:\\{sha256:${q}([0-9a-f]{64})${q},bytes:(\\d+)\\}`;
  const m = new RegExp(`version:${q}([^"'\`]+)${q},${d("core")},${d("groups")}`).exec(readFileSync(join(site, "assets", n), "utf8"));
  if (m) { table = parseModuleTable({ version: m[1], core: { sha256: m[2], bytes: Number(m[3]) }, groups: { sha256: m[4], bytes: Number(m[5]) } }); break; }
}
need(table !== null, "no module table is compiled into the bundle's code");
need(table?.version === manifest.version, "the compiled-in table is for another version than the manifest");

const src = (over = {}) => ({
  fetch: async (p) => {
    const text = over[p] ?? (() => { try { return readFileSync(join(site, p)); } catch { return null; } })();
    if (text === null) return { ok: false };
    const buf = Buffer.from(text);
    return { ok: true, text: async () => buf.toString("utf8"), arrayBuffer: async () => buf.buffer.slice(buf.byteOffset, buf.byteOffset + buf.byteLength) };
  },
});
const cores = {};
if (table !== null && core[0] && groups[0]) {
  const okBase = await loadVerifiedModule(table, "core", core[0].path, src(), { release: false });
  const okGroups = await loadVerifiedModule(table, "groups", groups[0].path, src(), { release: true });
  need(okBase.ok, `base module refused: ${okBase.reason}`);
  need(okGroups.ok, `groups module refused: ${okGroups.reason}`);
  const flipped = Buffer.from(readFileSync(join(site, groups[0].path))); flipped[100] ^= 1;
  need((await loadVerifiedModule(table, "groups", groups[0].path, src({ [groups[0].path]: flipped }), { release: true })).reason === "digest_mismatch", "a flipped byte was not refused");
  const other = JSON.stringify({ ...manifest, version: "9.9.9" });
  need((await loadVerifiedModule(table, "groups", groups[0].path, src({ "bundle-manifest.json": other }), { release: true })).reason === "version_skew", "another release's manifest was not refused");
  if (okBase.ok) cores.base = okBase.value;
  if (okGroups.ok) cores.groups = okGroups.value;
}

// Instantiate the verified bytes for real (the same ABI as packages/om-core-wasm/src/wasm.ts) and ask each whether it has the group engine.
async function groupsAvailable(bytes) {
  const holder = {};
  const { instance } = await WebAssembly.instantiate(bytes, { om_host: {
    random_fill: (ptr, len) => { globalThis.crypto.getRandomValues(new Uint8Array(holder.e.memory.buffer, ptr, len)); },
    now_ms: () => Date.now(),
  } });
  holder.e = instance.exports;
  const req = new TextEncoder().encode(JSON.stringify({ cmd: "groups_available" }));
  const inPtr = holder.e.om_in_alloc(req.length); // may grow memory: take the buffer after
  new Uint8Array(holder.e.memory.buffer, inPtr, req.length).set(req);
  const n = holder.e.om_call(req.length);
  return JSON.parse(new TextDecoder().decode(new Uint8Array(holder.e.memory.buffer, holder.e.om_out_ptr(), n))).ok;
}
if (cores.base && cores.groups) {
  need((await groupsAvailable(cores.groups)) === true, "the verified groups module does not report the group engine");
  need((await groupsAvailable(cores.base)) === false, "the base module unexpectedly reports the group engine");
}
if (errors.length > 0) { console.error(`check-groups-module: FAILED (${site})\n${errors.join("\n")}`); process.exit(1); }
console.log(`check-groups-module: ok (${site}): both modules listed and verified against the compiled-in digests, groups lazy, engine present only in the groups module`);
