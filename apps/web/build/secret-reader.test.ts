// The shared release-secret reader (secret-reader.mjs) and `migrate-vault-item`: every accepted encoding, the legacy note-body
// fallback, the hidden-field rules, strictness, no secret echo, and a full migration against a FAKE `bw` (throwaway keys only;
// nothing here touches a real vault).
import { spawnSync } from "node:child_process";
import { generateKeyPairSync } from "node:crypto";
import { chmodSync, existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { afterAll, describe, expect, it } from "vitest";
import { generatePqRoot, loadPqRootNote, formatPqNote, pqFingerprint } from "./pq-ceremony.mjs";
import { fingerprint, generateRoot, loadRootPem } from "./root-ceremony.mjs";
import { ed25519FieldValue, locateSecret, normalizeEd25519, normalizePqSeed, readEd25519, secureNoteItem } from "./secret-reader.mjs";
import { pemFromSecretText, signManifest } from "./sign.mjs";
import { planMigration } from "./vault-migrate.mjs";
import { delegate, mkOnline, mkRoot, pinOf } from "./testkit";

const tmp = mkdtempSync(join(tmpdir(), "om-secret-test-"));
afterAll(() => {
  rmSync(tmp, { recursive: true, force: true });
});
const CEREMONY = join(import.meta.dirname, "root-ceremony.mjs");

const key = generateRoot();
const b64 = ed25519FieldValue(key.pem);
const body = key.pem.replace(/-----[A-Z ]+-----|\s/g, "");
const lines = key.pem.trim().split("\n");
const encodings: Record<string, string> = {
  "multi-line PEM": key.pem,
  "multi-line PEM, CRLF, no final newline": key.pem.trim().replace(/\n/g, "\r\n"),
  "one-line PEM, spaces": `${lines.at(0) ?? ""} ${body} ${lines.at(-1) ?? ""}`,
  "one-line PEM, literal \\n": lines.join("\\n"),
  "one-line PEM, literal \\n and a trailing one": `${lines.join("\\n")}\\n`,
  "bare base64 PKCS8 DER": b64,
  "bare base64 PKCS8 DER, padded with whitespace": `  ${b64}\n`,
};
const hiddenItem = (value: string, notes = "public description") => ({ id: "i1", type: 2, name: "n", notes, fields: [{ name: "secret", value, type: 1 }] });

describe("Ed25519 encodings", () => {
  it.each(Object.entries(encodings))("raw text: %s", (_n, text) => {
    expect(normalizeEd25519(text)).toBe(key.pem);
    expect(normalizeEd25519(text, { strict: true })).toBe(key.pem);
    expect(loadRootPem(text).pub).toBe(key.pub);
  });
  it.each(Object.entries(encodings))("hidden field of bw item JSON: %s", (_n, text) => {
    expect(readEd25519(JSON.stringify(hiddenItem(text)))).toMatchObject({ pem: key.pem, source: "field" });
    expect(loadRootPem(JSON.stringify(hiddenItem(text))).id).toBe(key.id);
  });
  it("legacy fallback: the note body of an item without a secret field (and a note with text around the PEM)", () => {
    const legacy = { id: "i1", type: 2, name: "n", notes: key.pem, fields: [] };
    expect(readEd25519(JSON.stringify(legacy))).toMatchObject({ pem: key.pem, source: "notes" });
    expect(readEd25519({ ...legacy, fields: undefined, notes: `my key\n${key.pem}\nend` }).pem).toBe(key.pem);
    expect(readEd25519(`my key\n${key.pem}\nend`).source).toBe("raw"); // `bw get notes` output
  });
  it("the hidden field wins over the note, and a visible `secret` field is refused", () => {
    const other = generateRoot();
    expect(readEd25519({ ...hiddenItem(b64), notes: other.pem }).pem).toBe(key.pem);
    expect(() => readEd25519({ ...hiddenItem(b64), fields: [{ name: "secret", value: b64, type: 0 }] })).toThrow(/not of type hidden/);
    expect(locateSecret({ ...hiddenItem(b64), fields: [{ name: "secret", value: b64, type: 0 }] }, { allowVisibleField: true }).source).toBe("field");
    expect(() => readEd25519({ ...hiddenItem(b64), fields: [{ name: "secret", value: b64, type: 1 }, { name: "secret", value: b64, type: 1 }] })).toThrow(/more than one/);
    expect(() => readEd25519({ ...hiddenItem(b64), fields: [{ name: "secret", value: "", type: 1 }] })).toThrow(/empty/);
    expect(() => readEd25519({ id: "i", type: 2, notes: "", fields: [] })).toThrow(/no secret custom field/);
  });
  it("a hidden field must be exactly one key; the legacy note may have text around it", () => {
    expect(() => readEd25519(hiddenItem(`note ${key.pem}`))).toThrow(/more than one key|exactly the key/);
    expect(() => readEd25519(hiddenItem(`${b64} x`))).toThrow(/not a PKCS8/);
  });
  it.each([
    ["empty", ""],
    ["words", "SECRET-LOOKING-TEXT"],
    ["truncated DER", b64.slice(0, 60)],
    ["extra DER bytes", `${b64}AAAA`],
    ["bad base64 chars", `${b64.slice(0, 10)}!${b64.slice(11)}`],
    ["encrypted PEM", "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----"],
    ["EC PEM", "-----BEGIN EC PRIVATE KEY-----\nAAAA\n-----END EC PRIVATE KEY-----"],
    ["unterminated PEM", `-----BEGIN PRIVATE KEY-----\n${body}`],
    ["two tokens", `${b64} ${b64}`],
  ])("rejects %s without echoing it", (_n, text) => {
    let msg = "";
    try { normalizeEd25519(text); } catch (e) { msg = (e as Error).message; }
    expect(msg).toMatch(/not a PKCS8|not an ed25519/);
    if (text.length > 8) expect(msg).not.toContain(text.slice(0, 20));
  });
  it("refuses a PKCS8 key of another type (EC is not Ed25519)", () => {
    const rsa = generateKeyPairSync("ec", { namedCurve: "P-256" }).privateKey.export({ format: "pem", type: "pkcs8" });
    expect(() => normalizeEd25519(rsa)).toThrow(/not an ed25519 key/);
  });
  it("the description's public data is cross-checked against the secret", () => {
    const good = hiddenItem(b64, `x\nkey-id: ${key.id}\npublic-key: ${key.pub}\nfingerprint: sha256 ${"0".repeat(64)}`);
    expect(() => loadRootPem(JSON.stringify(good))).toThrow(/fingerprint does not match/);
    const wrongPub = hiddenItem(b64, `x\npublic-key: ${"a".repeat(64)}`);
    expect(() => loadRootPem(JSON.stringify(wrongPub))).toThrow(/public key does not match/);
    const wrongId = hiddenItem(b64, `x\nkey-id: root-0000000000000000`);
    expect(() => loadRootPem(JSON.stringify(wrongId))).toThrow(/key id does not match/);
  });
});

describe("ML-DSA-65 seed encodings", () => {
  const pq = generatePqRoot();
  const hex = Buffer.from(pq.seed).toString("hex");
  const item = (value: string, extra: Record<string, unknown> = {}) => ({ id: "p1", type: 2, name: "n", notes: "d", fields: [{ name: "secret", value, type: 1 }], ...extra });
  it.each([
    ["bare 64 hex", hex],
    ["bare 64 hex, uppercase", hex.toUpperCase()],
    ["seed: <hex>", `seed: ${hex}`],
    ["seed: <hex>, whitespace", `  seed:   ${hex}\n`],
  ])("raw text and hidden field: %s", (_n, text) => {
    expect(normalizePqSeed(text).seedHex).toBe(hex);
    expect(loadPqRootNote(text).pub).toBe(pq.pub);
    expect(loadPqRootNote(JSON.stringify(item(text))).pub).toBe(pq.pub);
    expect(loadPqRootNote(JSON.stringify(item(text))).source).toBe("field");
  });
  it("legacy fallback: the old note-body block, as item notes or as raw `bw get notes` output", () => {
    const note = formatPqNote(pq);
    expect(loadPqRootNote(note).pub).toBe(pq.pub);
    expect(loadPqRootNote(JSON.stringify({ id: "p", type: 2, notes: note, fields: [] }))).toMatchObject({ pub: pq.pub, source: "notes" });
    expect(loadPqRootNote(`intro\nseed: ${hex}\n`).pub).toBe(pq.pub); // a note with extra lines
  });
  it.each([
    ["63 hex", hex.slice(1)],
    ["65 hex", `${hex}0`],
    ["non-hex", `${hex.slice(0, 63)}g`],
    ["seed: with 2 values", `seed: ${hex} ${hex}`],
    ["words", "SECRET-LOOKING-TEXT"],
  ])("rejects %s without echoing it", (_n, text) => {
    let msg = "";
    try { loadPqRootNote(JSON.stringify(item(text))); } catch (e) { msg = (e as Error).message; }
    expect(msg).toMatch(/not an OM PQ root seed/);
    expect(msg).not.toContain(text.slice(0, 20));
  });
  it("a hidden field must be strict: the legacy block is not accepted inside it", () => {
    expect(() => loadPqRootNote(JSON.stringify(item(formatPqNote(pq))))).toThrow(/not an OM PQ root seed/);
  });
  it("the description's key id and fingerprint must match the seed", () => {
    const other = generatePqRoot();
    const mism = item(hex, { notes: `key-id: ${other.keyId}\nfingerprint: sha256 ${pqFingerprint(other.pub)}` });
    expect(() => loadPqRootNote(JSON.stringify(mism))).toThrow(/does not match the secret/);
  });
});

describe("sign.mjs reads the online key from a key file path in every encoding", () => {
  const online = mkOnline();
  const root = mkRoot();
  const real = Date.now();
  const delegation = delegate({ roots: [root], online, keyId: "online-1", now: real });
  const trust = { roots: pinOf([root]).keys, threshold: 1, now: real };
  const manifest = { version: "1.0.0", delegation };
  const reference = signManifest(manifest, online.pem, "online-1", trust);
  const onlineB64 = ed25519FieldValue(online.pem);
  it.each([
    ["PEM file", online.pem],
    ["one-line PEM file", online.pem.trim().replace(/\n/g, " ")],
    ["bare DER base64 file", `${onlineB64}\n`],
    ["bw item JSON file", JSON.stringify(secureNoteItem({ name: "k", notes: "public description", secret: onlineB64 }))],
    ["legacy bw item JSON file (note body)", JSON.stringify({ id: "i", type: 2, notes: online.pem, fields: [] })],
  ])("%s", (n, text) => {
    const file = join(tmp, `online-${n.replace(/\W+/g, "-")}.key`);
    writeFileSync(file, text, { mode: 0o600 });
    const pem = pemFromSecretText(readFileSync(file, "utf8"));
    expect(signManifest(manifest, pem, "online-1", trust)).toEqual(reference); // Ed25519 is deterministic
  });
  it("garbage is refused without echo", () => {
    expect(() => pemFromSecretText("SECRET-LOOKING-TEXT")).toThrow(/not a PKCS8/);
  });
});

describe("generators write the secret only into a single-line hidden field", () => {
  it("gen-root and gen-pq-root", () => {
    for (const cmd of ["gen-root", "gen-pq-root"]) {
      const r = spawnSync("node", [CEREMONY, cmd, "--to-stdin-for-vault", "--name", "T"], { encoding: "utf8" });
      expect(r.status, r.stderr).toBe(0);
      const it = JSON.parse(r.stdout) as { notes: string; fields: { name: string; value: string; type: number }[] };
      expect(it.fields).toHaveLength(1);
      const [f] = it.fields as [{ name: string; value: string; type: number; linkedId: null }];
      expect({ name: f.name, type: f.type, linkedId: f.linkedId }).toEqual({ name: "secret", type: 1, linkedId: null });
      expect(f.value).toMatch(/^[A-Za-z0-9+/]+=*$/);
      expect(f.value).not.toMatch(/\s/);
      expect(it.notes).not.toContain(f.value);
      expect(it.notes).toMatch(/hidden custom field "secret"/);
      expect(it.notes).toMatch(/fingerprint: sha256 [0-9a-f]{64}/);
      expect(r.stderr).not.toContain(f.value);
    }
  });
});

// ---------- migrate-vault-item against a fake bw ----------
const FAKE_BW = `#!/usr/bin/env node
const fs = require("node:fs");
const [state, log] = [process.env.FAKE_BW_STATE, process.env.FAKE_BW_LOG];
const args = process.argv.slice(2);
fs.appendFileSync(log, JSON.stringify(args) + "\\n");
const item = () => JSON.parse(fs.readFileSync(state, "utf8"));
if (args[0] === "sync") { process.stdout.write("Syncing complete.\\n"); }
else if (args[0] === "get" && args[1] === "item") {
  const it = item();
  if (args[2] !== it.name && args[2] !== it.id) { process.stderr.write("Not found."); process.exit(1); }
  process.stdout.write(JSON.stringify(it));
} else if (args[0] === "edit" && args[1] === "item") {
  const next = JSON.parse(Buffer.from(fs.readFileSync(0, "utf8").trim(), "base64").toString("utf8"));
  const cur = item();
  // like Vaultwarden: a write built on an older copy than the server holds is refused, and every accepted write bumps the revision
  if ((cur.revisionDate ?? 0) !== (next.revisionDate ?? 0)) { process.stderr.write("The client copy of this cipher is out of date."); process.exit(1); }
  next.revisionDate = (cur.revisionDate ?? 0) + 1;
  if (process.env.FAKE_BW_CORRUPT) next.fields = next.fields.map((f) => ({ ...f, value: f.value.replace(/^./, (c) => (c === "A" ? "B" : "A")) }));
  fs.writeFileSync(state, JSON.stringify(next));
  process.stdout.write(JSON.stringify(next)); // real bw echoes the item
} else { process.exit(2); }
`;
const fakeBw = join(tmp, "fake-bw.cjs");
writeFileSync(fakeBw, FAKE_BW);
chmodSync(fakeBw, 0o755);

const migrate = (item: unknown, kind: string, name: string, ...extra: string[]) => {
  const state = join(tmp, `state-${Math.random().toString(36).slice(2)}.json`);
  const log = `${state}.log`;
  writeFileSync(state, JSON.stringify(item));
  const r = spawnSync("node", [CEREMONY, "migrate-vault-item", "--kind", kind, "--item", name, "--bw-command", fakeBw, ...extra], {
    encoding: "utf8", env: { ...process.env, FAKE_BW_STATE: state, FAKE_BW_LOG: log},
  });
  return { r, after: JSON.parse(readFileSync(state, "utf8")) as { notes: string; fields?: { name: string; value: string; type: number }[] }, calls: existsSync(log) ? readFileSync(log, "utf8") : "" };
};
const legacyEd = (name: string, notes: string) => ({ id: "item-1", type: 2, name, notes, fields: [{ name: "other", value: "keep me", type: 0 }], secureNote: { type: 0 } });

describe("migrate-vault-item", () => {
  it.each([
    ["online-ed25519", "OM release signing key (ed25519) - lonunjie"],
    ["root-ed25519", "OM release root (software signer) 2026-10-28"],
  ])("%s: note-body PEM -> hidden field, note blanked, same public key, nothing secret printed or in argv", (kind, name) => {
    const k = generateRoot();
    const { r, after, calls } = migrate(legacyEd(name, k.pem), kind, name);
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toContain(`public key:   ${k.pub}`);
    expect(r.stdout).toMatch(/fingerprint: {2}sha256 [0-9a-f]{64}/);
    expect(r.stdout).toMatch(/migrated in 2 verified edit/);
    expect(after.fields).toContainEqual({ name: "secret", value: ed25519FieldValue(k.pem), type: 1, linkedId: null });
    expect(after.fields).toContainEqual({ name: "other", value: "keep me", type: 0 }); // other fields survive
    expect(after.notes).not.toMatch(/PRIVATE KEY/);
    expect(after.notes).toContain(`public-key: ${k.pub}`);
    expect(loadRootPem(after).pub).toBe(k.pub); // the tool's own loader on the migrated item
    const secrets = [ed25519FieldValue(k.pem), k.pem.replace(/-----[A-Z ]+-----|\s/g, "")];
    for (const s of secrets) {
      expect(r.stdout + r.stderr).not.toContain(s);
      expect(calls).not.toContain(s); // argv of every bw call
    }
    expect(calls).toContain('["edit","item","item-1"]');
    // idempotent: a second run writes nothing
    const again = migrate(after, kind, name);
    expect(again.r.stdout).toMatch(/already migrated/);
    expect(again.calls).not.toContain('"edit"');
  });
  it("accepts a one-line PEM note and a visible (text) `secret` field, rewriting it as hidden", () => {
    const k = generateRoot();
    const oneLine = k.pem.trim().replace(/\n/g, " ");
    const a = migrate(legacyEd("n", oneLine), "root-ed25519", "n");
    expect(a.r.status, a.r.stderr).toBe(0);
    expect(loadRootPem(a.after).pub).toBe(k.pub);
    const b = migrate({ ...legacyEd("n", "d"), fields: [{ name: "secret", value: ed25519FieldValue(k.pem), type: 0 }] }, "root-ed25519", "n");
    expect(b.r.status, b.r.stderr).toBe(0);
    expect(b.after.fields).toEqual([{ name: "secret", value: ed25519FieldValue(k.pem), type: 1, linkedId: null }]);
    expect(b.after.notes).not.toMatch(/PRIVATE KEY/);
  });
  it("pq-root: the legacy seed block -> hidden field with the bare seed; same fingerprint", () => {
    const pq = generatePqRoot();
    const name = "OM release PQ root (ML-DSA-65) 2026-10-28";
    const { r, after, calls } = migrate({ id: "item-2", type: 2, name, notes: formatPqNote(pq), fields: [], secureNote: { type: 0 } }, "pq-root", name);
    expect(r.status, r.stderr).toBe(0);
    const seed = Buffer.from(pq.seed).toString("hex");
    expect(r.stdout).toContain(`sha256 ${pqFingerprint(pq.pub)}`);
    expect(r.stdout + r.stderr).not.toContain(seed);
    expect(calls).not.toContain(seed);
    expect(after.fields).toEqual([{ name: "secret", value: seed, type: 1, linkedId: null }]);
    expect(after.notes).not.toContain(seed);
    expect(loadPqRootNote(after).pub).toBe(pq.pub);
  });
  it("--dry-run writes nothing", () => {
    const k = generateRoot();
    const { r, after, calls } = migrate(legacyEd("n", k.pem), "root-ed25519", "n", "--dry-run");
    expect(r.status, r.stderr).toBe(0);
    expect(r.stdout).toMatch(/dry run: 2 edit/);
    expect(after.notes).toBe(k.pem);
    expect(calls).not.toContain('"edit"');
  });
  it("refuses wrong kind data, non-notes, conflicting note/field, missing item, bw failure; never echoes the secret", () => {
    const k = generateRoot();
    const pq = generatePqRoot();
    const wrongKind = migrate(legacyEd("n", k.pem), "pq-root", "n");
    expect(wrongKind.r.status).toBe(1);
    expect(wrongKind.after.notes).toBe(k.pem); // untouched
    const login = migrate({ ...legacyEd("n", k.pem), type: 1 }, "root-ed25519", "n");
    expect(login.r.stderr).toMatch(/not a secure note/);
    const other = generateRoot();
    const conflict = migrate({ ...legacyEd("n", k.pem), fields: [{ name: "secret", value: ed25519FieldValue(other.pem), type: 1 }] }, "root-ed25519", "n");
    expect(conflict.r.stderr).toMatch(/DIFFERENT keys/);
    const missing = migrate(legacyEd("n", k.pem), "root-ed25519", "nope");
    expect(missing.r.status).toBe(1);
    expect(missing.r.stderr).toMatch(/bw get item failed/);
    expect(missing.r.stderr).not.toContain(ed25519FieldValue(k.pem));
    const empty = migrate({ ...legacyEd("n", ""), fields: [] }, "pq-root", "n");
    expect(empty.r.stderr).toMatch(/no secret/);
    const badKind = migrate(legacyEd("n", k.pem), "nonsense", "n");
    expect(badKind.r.stderr).toMatch(/--kind must be one of/);
    expect(JSON.stringify([wrongKind.r, login.r, conflict.r, missing.r, empty.r]).includes(Buffer.from(pq.seed).toString("hex"))).toBe(false);
  });
  it("a bad write is caught by the read-back; the note keeps the key after the first edit", () => {
    const k = generateRoot();
    const state = join(tmp, "state-corrupt.json");
    writeFileSync(state, JSON.stringify(legacyEd("n", k.pem)));
    const r = spawnSync("node", [CEREMONY, "migrate-vault-item", "--kind", "root-ed25519", "--item", "n", "--bw-command", fakeBw], {
      encoding: "utf8", env: { ...process.env, FAKE_BW_STATE: state, FAKE_BW_LOG: `${state}.log`, FAKE_BW_CORRUPT: "1" },
    });
    expect(r.status).toBe(1);
    expect(r.stderr).toMatch(/read-back|not a PKCS8/);
    expect((JSON.parse(readFileSync(state, "utf8")) as { notes: string }).notes).toBe(k.pem); // step 1 keeps the legacy note
    expect(r.stdout + r.stderr).not.toContain(ed25519FieldValue(k.pem));
  });
  it("planMigration is pure: an already-migrated item plans no steps", () => {
    const k = generateRoot();
    const derive = (i: string | object, o?: { allowVisibleField?: boolean }) => { const x = loadRootPem(i, o); return { id: x.id, pub: x.pub, fingerprint: fingerprint(x.pub), value: x.value }; };
    const first = planMigration(legacyEd("n", k.pem), "root-ed25519", derive);
    expect(first.steps).toHaveLength(2);
    expect(planMigration(first.steps[1], "root-ed25519", derive).steps).toHaveLength(0);
  });
});
