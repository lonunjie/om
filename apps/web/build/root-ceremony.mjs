#!/usr/bin/env node
// OM release root-key ceremony tool (audit B2, PR-005). Pure node, no dependencies. RUN ON AN OFFLINE MACHINE.
// The OFFLINE ROOT key signs a small "delegation" that authorises the online release key (see deploy/KEY-CEREMONY.md).
//
//   gen-root         --out <abs path on removable media>                      new root key; prints ONLY id, public key, fingerprint
//   show-root        --root-key <file>                                        prints id, public key, fingerprint of a key file (to check a backup)
//   sign-delegation  --root-key <file>... --out <file> --expires <t>
//                    [--online-pub <hex> --key-id <id> [--not-before <t>] [--not-after <t>]]
//                    [--prev <delegation.json>] [--version <n>]               new delegation (with --prev: version+1, keys/revoked carried over)
//   revoke           --root-key <file>... --prev <delegation.json> --key-id <id> [--expires <t>] --out <file>
//   cosign           --root-key <file> --in <delegation.json> [--out <file>]  add one more root signature (2-of-N)
//   inspect          --in <delegation.json>                                   human summary (public data)
//
// <t> is YYYY-MM-DD, a full UTC time YYYY-MM-DDTHH:MM:SSZ, or +Nd (N days from now).
// Private key material is never printed, logged, put in argv or in an error message. The key file is read only
// from the path given, must be mode 0600, and is written only by gen-root (mode 0600, never overwritten).
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign } from "node:crypto";
import { chmodSync, existsSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { DELEGATION_FORMAT, delegationPayload, parseDelegation } from "./verify-release.mjs";

const TIME = /^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z$/;
const ID = /^[A-Za-z0-9._-]{1,64}$/;
const HEX64 = /^[0-9a-f]{64}$/;
const MAX_DAYS = 400;

const iso = (ms) => new Date(ms).toISOString().replace(/\.\d{3}Z$/, "Z");
/** `YYYY-MM-DD` | full UTC time | `+Nd` -> canonical `YYYY-MM-DDTHH:MM:SSZ`. */
export const parseTime = (t, now = Date.now()) => {
  if (typeof t !== "string") throw new Error("time missing");
  if (/^\+\d{1,4}d$/.test(t)) return iso(now + Number(t.slice(1, -1)) * 86_400_000);
  if (/^\d{4}-\d{2}-\d{2}$/.test(t)) t += "T00:00:00Z";
  if (!TIME.test(t) || Number.isNaN(Date.parse(t))) throw new Error(`bad time ${JSON.stringify(t)} (use YYYY-MM-DD, YYYY-MM-DDTHH:MM:SSZ or +Nd)`);
  return t;
};

const rawPub = (publicKey) => publicKey.export({ format: "der", type: "spki" }).subarray(-32);
export const fingerprint = (pubHex) => createHash("sha256").update(Buffer.from(pubHex, "hex")).digest("hex");
const grouped = (hex) => hex.match(/.{1,8}/g).join(" ");
/** Deterministic root key id derived from the public key, so a signature entry cannot name the wrong key. */
export const rootKeyId = (pubHex) => `root-${fingerprint(pubHex).slice(0, 16)}`;
export const describeKey = (pubHex) => ({ id: rootKeyId(pubHex), pub: pubHex, fingerprint: grouped(fingerprint(pubHex)) });

/** Generates a root key pair in memory. Returns the PEM (private) and the public description. */
export const generateRoot = () => {
  const { privateKey, publicKey } = generateKeyPairSync("ed25519");
  const pubHex = rawPub(publicKey).toString("hex");
  return { pem: privateKey.export({ format: "pem", type: "pkcs8" }), ...describeKey(pubHex) };
};

/** Loads a root key file (must be mode 0600). Errors never include file content. */
export const loadRoot = (file) => {
  let st;
  try { st = statSync(file); } catch { throw new Error(`cannot read root key file ${file}`); }
  if (!st.isFile()) throw new Error(`${file} is not a file`);
  if ((st.mode & 0o077) !== 0) throw new Error(`${file} has permissions ${(st.mode & 0o777).toString(8)}; it must be 600 (chmod 600 on the offline media)`);
  let key;
  try { key = createPrivateKey(readFileSync(file, "utf8")); } catch { throw new Error(`${file} is not a PKCS8 PEM private key`); }
  if (key.asymmetricKeyType !== "ed25519") throw new Error(`${file} is not an ed25519 key`);
  const pubHex = rawPub(createPublicKey(key)).toString("hex");
  return { key, ...describeKey(pubHex) };
};

/** Adds (or replaces) this root's signature over the delegation payload. Pure; returns a new object. */
export const addRootSignature = (delegation, root) => {
  const d = parseDelegation(delegation);
  if (!d) throw new Error("not a valid delegation document");
  const sig = sign(null, delegationPayload(d), root.key).toString("base64");
  const others = d.signatures.filter((s) => s.keyId !== root.id);
  return { ...d, signatures: [...others, { keyId: root.id, alg: "ed25519", sig }].sort((a, b) => (a.keyId < b.keyId ? -1 : 1)) };
};

/**
 * Builds an UNSIGNED delegation. With `prev`, version is prev+1 and keys/revoked carry over. `addKey` appends a key,
 * `revokeId` moves a key id into `revoked` (and out of `keys`). `expires` is required and must be in the future.
 */
export const buildDelegation = ({ prev, version, expires, addKey, revokeId, now = Date.now(), allowLong = false }) => {
  const p = prev === undefined ? null : parseDelegation(prev);
  if (prev !== undefined && !p) throw new Error("--prev is not a valid delegation document");
  const exp = parseTime(expires, now);
  const expMs = Date.parse(exp);
  if (expMs <= now) throw new Error("--expires is in the past");
  if (!allowLong && expMs - now > MAX_DAYS * 86_400_000) throw new Error(`--expires is more than ${MAX_DAYS} days away; a long-lived delegation weakens revocation (use --allow-long to override)`);
  const v = p ? p.version + 1 : version ?? 1;
  if (!Number.isSafeInteger(v) || v < 1 || (p && version !== undefined && version !== v)) throw new Error("bad or conflicting --version");
  let keys = p ? [...p.keys] : [];
  const revoked = p ? [...p.revoked] : [];
  if (addKey) {
    const { keyId, pub, notBefore, notAfter } = addKey;
    if (!ID.test(keyId ?? "")) throw new Error("--key-id must match [A-Za-z0-9._-]{1,64}");
    if (!HEX64.test(pub ?? "")) throw new Error("--online-pub must be 64 lowercase hex chars (raw Ed25519 public key)");
    if (keys.some((k) => k.keyId === keyId) || revoked.includes(keyId)) throw new Error(`key id ${keyId} already used in this delegation chain (key ids are never reused)`);
    const nb = parseTime(notBefore ?? iso(now), now);
    const na = parseTime(notAfter ?? exp, now);
    if (Date.parse(na) <= Date.parse(nb)) throw new Error("--not-after must be after --not-before");
    keys.push({ keyId, alg: "ed25519", pub, notBefore: nb, notAfter: na });
  }
  if (revokeId !== undefined) {
    if (!keys.some((k) => k.keyId === revokeId) && !revoked.includes(revokeId)) throw new Error(`key id ${revokeId} is not in the delegation`);
    keys = keys.filter((k) => k.keyId !== revokeId);
    if (!revoked.includes(revokeId)) revoked.push(revokeId);
  }
  keys.sort((a, b) => (a.keyId < b.keyId ? -1 : 1));
  revoked.sort();
  return { format: DELEGATION_FORMAT, version: v, expires: exp, keys, revoked, signatures: [] };
};

export const summary = (d) =>
  [`delegation v${d.version}, expires ${d.expires}`,
   ...d.keys.map((k) => `  key ${k.keyId} ${k.alg} ${k.pub}  ${k.notBefore} .. ${k.notAfter}`),
   `  revoked: ${d.revoked.length ? d.revoked.join(", ") : "(none)"}`,
   `  root signatures: ${d.signatures.length ? d.signatures.map((s) => s.keyId).join(", ") : "(none)"}`].join("\n");

// ---------- CLI ----------
const MEDIA = [/^\/Volumes\/[^/]+\//, /^\/media\//, /^\/mnt\//, /^\/run\/media\//];
const parseArgs = (argv) => {
  const o = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) throw new Error(`unexpected argument ${a}`);
    const k = a.slice(2);
    if (k === "allow-long") { o[k] = true; continue; }
    const v = argv[++i];
    if (v === undefined || v.startsWith("--")) throw new Error(`--${k} needs a value`);
    (o[k] ??= []).push(v);
  }
  return o;
};
const one = (o, k, required = false) => {
  const v = o[k];
  if (v === undefined) { if (required) throw new Error(`--${k} is required`); return undefined; }
  if (v.length !== 1) throw new Error(`--${k} given more than once`);
  return v[0];
};
const readJson = (f) => { try { return JSON.parse(readFileSync(f, "utf8")); } catch { throw new Error(`cannot read JSON from ${f}`); } };
const emit = (d, out) => {
  const text = JSON.stringify(d, null, 2) + "\n";
  if (out) writeFileSync(out, text);
  else process.stdout.write(text);
};
const insideGit = (dir) => spawnSync("git", ["-C", dir, "rev-parse", "--show-toplevel"], { encoding: "utf8" }).status === 0;

const cmds = {
  "gen-root"(o) {
    const out = one(o, "out", true);
    if (!isAbsolute(out)) throw new Error("--out must be an absolute path on the offline/removable media");
    const dir = dirname(out);
    if (!existsSync(dir)) throw new Error(`directory ${dir} does not exist (is the media mounted?)`);
    const real = realpathSync(dir) + sep;
    const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), "../../..") + sep;
    if (real.startsWith(repoRoot) || insideGit(dir)) throw new Error("refusing to write the root key inside a git work tree; name a path on removable media");
    if (existsSync(out) || existsSync(`${out}.pub`)) throw new Error(`${out} (or ${out}.pub) already exists; refusing to overwrite a key`);
    if (!MEDIA.some((re) => re.test(real))) console.error(`WARNING: ${dir} does not look like removable media. The root key must live only on offline media.`);
    const r = generateRoot();
    writeFileSync(out, r.pem, { mode: 0o600, flag: "wx" });
    chmodSync(out, 0o600);
    writeFileSync(`${out}.pub`, `${r.id}\n${r.pub}\nsha256 ${r.fingerprint}\n`, { flag: "wx" });
    console.log(`root key id:  ${r.id}\npublic key:   ${r.pub}\nfingerprint:  sha256 ${r.fingerprint}\nprivate key written to ${out} (mode 600); it is not shown anywhere. Back it up per deploy/KEY-CEREMONY.md.`);
  },
  "show-root"(o) {
    const r = loadRoot(one(o, "root-key", true));
    console.log(`root key id:  ${r.id}\npublic key:   ${r.pub}\nfingerprint:  sha256 ${r.fingerprint}`);
  },
  "sign-delegation"(o) {
    const roots = (o["root-key"] ?? []).map(loadRoot);
    if (roots.length === 0) throw new Error("--root-key is required");
    const pub = one(o, "online-pub");
    const prevFile = one(o, "prev");
    if (!pub && !prevFile) throw new Error("--online-pub is required (or pass --prev to renew an existing delegation)");
    const version = one(o, "version") === undefined ? undefined : Number(one(o, "version"));
    let d = buildDelegation({
      prev: prevFile ? readJson(prevFile) : undefined, version, expires: one(o, "expires", true), allowLong: o["allow-long"] === true,
      addKey: pub ? { keyId: one(o, "key-id", true), pub, notBefore: one(o, "not-before"), notAfter: one(o, "not-after") } : undefined,
    });
    for (const r of roots) d = addRootSignature(d, r);
    console.error(summary(d));
    emit(d, one(o, "out"));
  },
  revoke(o) {
    const roots = (o["root-key"] ?? []).map(loadRoot);
    if (roots.length === 0) throw new Error("--root-key is required");
    const prev = readJson(one(o, "prev", true));
    const p = parseDelegation(prev);
    if (!p) throw new Error("--prev is not a valid delegation document");
    let d = buildDelegation({ prev, expires: one(o, "expires") ?? p.expires, revokeId: one(o, "key-id", true), allowLong: o["allow-long"] === true });
    for (const r of roots) d = addRootSignature(d, r);
    console.error(summary(d));
    emit(d, one(o, "out"));
  },
  cosign(o) {
    const root = loadRoot(one(o, "root-key", true));
    const inFile = one(o, "in", true);
    const d = parseDelegation(readJson(inFile));
    if (!d) throw new Error("--in is not a valid delegation document");
    console.error(`Signing as ${root.id}. Check this is what you intend to authorise:\n${summary(d)}`);
    emit(addRootSignature(d, root), one(o, "out") ?? inFile);
  },
  inspect(o) {
    const d = parseDelegation(readJson(one(o, "in", true)));
    if (!d) throw new Error("not a valid delegation document");
    console.log(summary(d));
  },
};

if (process.argv[1] && realpathSync(process.argv[1]) === realpathSync(fileURLToPath(import.meta.url))) {
  const [cmd, ...rest] = process.argv.slice(2);
  try {
    if (!cmds[cmd]) throw new Error(`usage: root-ceremony.mjs <${Object.keys(cmds).join("|")}> ... (see the header of this file and deploy/KEY-CEREMONY.md)`);
    cmds[cmd](parseArgs(rest));
  } catch (e) {
    console.error(`root-ceremony: ${e.message}`);
    process.exit(1);
  }
}
