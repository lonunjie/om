#!/usr/bin/env node
// OM release root-key ceremony tool (audit B2, PR-005). Pure node, no dependencies. RUN ON AN OFFLINE MACHINE.
// The OFFLINE ROOT key signs a small "delegation" that authorises the online release key (see deploy/KEY-CEREMONY.md).
//
//   gen-root         --out <abs path on removable media>                      new software root key file; prints ONLY id, public key, fingerprint
//   gen-root         --to-stdin-for-vault [--name <item name>]                new software root key, NEVER on disk: stdout is a Bitwarden secure-note item
//                                                                             (JSON): notes = public description, the key = HIDDEN custom field `ed25519-pkcs8-YYYY-MM-DD`
//                                                                             (one line); pipe into `bw encode | bw create item` of the operator's PERSONAL
//                                                                             vault; stderr gets only id, public key, fingerprint
//   show-root        --root-key <file> | --root-key-stdin                     prints id, public key, fingerprint of a key (to check a backup); stdin takes
//                                                                             `bw get item NAME` JSON (or a legacy note body, or a bare key)
//   migrate-vault-item --kind online-ed25519|root-ed25519|pq-root [--item <name>] [--field <name>] [--dry-run] [--bw-command <bw>]
//                                                                             moves a legacy NOTE-body secret into a dated hidden field (or renames the legacy `secret` field) through `bw`
//                                                                             (BW_SESSION / BITWARDENCLI_APPDATA_DIR supplied by the caller), verifying the
//                                                                             same public key before and after each edit; prints only public data
//   draft            --expires <t> [--online-pub <hex> --key-id <id> [--not-before <t>] [--not-after <t>]]
//                    [--prev <delegation.json>] [--version <n>] [--revoke-key-id <id>] [--out <file>]   UNSIGNED delegation (for external signers)
//   sign-delegation  (--root-key <file> | --root-key-stdin)... --out <file> --expires <t>
//                    [--online-pub <hex> --key-id <id> [--not-before <t>] [--not-after <t>]]
//                    [--prev <delegation.json>] [--version <n>]               new delegation (with --prev: version+1, keys/revoked carried over)
//   revoke           (--root-key <file> | --root-key-stdin)... --prev <delegation.json> --key-id <id> [--expires <t>] --out <file>
//   cosign           (--root-key <file> | --root-key-stdin) --in <delegation.json> [--out <file>]  add one more software root signature
//   external-payload --in <delegation.json> --signer-pub <hex> [--frame solana-offchain-v0|raw] --out <file>
//                                                                             writes the EXACT bytes a hardware signer must sign + a human-checkable digest
//   external-attach  --in <delegation.json> (--pub <hex> --sig <base64|hex> | --from-helper <ledger-sign.mjs --out file>) [--frame ...] [--out <file>]
//                                                                             ingests a hardware signer's public key + signature; verified before accepted
//   pin-snippet      --root <pub>=<frame> --root <pub>=<frame> --root <pub>=<frame> [--threshold 2]   paste-ready ROOT_KEYS / PINNED_ROOTS / roots file
//   verify           --in <delegation.json> --roots-file <file>               check the signatures against a roots file ({roots:{id:desc},threshold})
//   inspect          --in <delegation.json>                                   human summary (public data)
//
// Post-quantum co-signature (ADR-0182, deploy/KEY-CEREMONY-PQ.md): an ML-DSA-65 software root, seed in the PERSONAL vault only.
//   gen-pq-root      --to-stdin-for-vault [--name <item name>] [--pub-out <file>]   new PQ root; stdout is a Bitwarden secure-note item whose hidden field
//                                                                             `secret` holds the 32-byte seed (never a file); stderr gets only id, fingerprint; --pub-out writes the PUBLIC descriptor
//   show-pq-root     --pq-root-stdin [--pub-out <file>]                       id, fingerprint and the paste-ready pin literals of a vault copy
//   pq-sign-delegation --pq-root-stdin --in <delegation.json> [--out <file>] [--roots-file <file>]   sign + attach the PQ co-signature (works before or after the Ed25519 signatures)
//   pq-payload       --in <delegation.json> --pq-root-file <descriptor.json> --out <file>   exact bytes an external PQ signer must sign
//   pq-attach        --in <delegation.json> --pq-root-file <descriptor.json> --sig <base64|hex> [--out <file>]   ingest an external PQ signature (verified first)
//   draft/sign-delegation/revoke accept --pq-required: the delegation becomes the PQ epoch (needs fresh Ed25519 signatures); once set it is carried over by --prev.
//   verify           also checks the PQ co-signature when the roots file has "pq"; --require-pq makes it mandatory
//
// <t> is YYYY-MM-DD, a full UTC time YYYY-MM-DDTHH:MM:SSZ, or +Nd (N days from now).
// Private key material is never printed, logged, put in argv or in an error message. A key file is read only from
// the path given, must be mode 0600, and is written only by `gen-root --out` (mode 0600, never overwritten).
// `--root-key-stdin` reads one key from stdin (for a key that lives only in a password vault); nothing touches disk.
import { createHash, createPrivateKey, createPublicKey, generateKeyPairSync, sign, verify } from "node:crypto";
import { chmodSync, existsSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import { dirname, isAbsolute, resolve, sep } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { addPqSignature, checkDescriptor, externalPqAttach, generatePqRoot, loadPqRootNote, pqFingerprint, pqPayload, pqVaultItem } from "./pq-ceremony.mjs";
import { checkDescription, ed25519FieldValue, readEd25519, secretFieldName, secureNoteItem } from "./secret-reader.mjs";
import { KIND_NAMES, KINDS, defaultItemName, migrateViaBw } from "./vault-migrate.mjs";
import { DELEGATION_FORMAT, FRAME_RAW, FRAME_SOLANA_OFFCHAIN_V0, KNOWN_FRAMES, describeRoot, frameDelegationPayload, ledgerText, delegationPayload, parseDelegation, verifyDelegation } from "./verify-release.mjs";

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

/**
 * Parses an Ed25519 root key from vault-item JSON (hidden field `ed25519-pkcs8-*`, legacy name `secret`, else the legacy note body) or raw text: multi-line PEM,
 * one-line PEM or base64 PKCS8 DER (secret-reader.mjs). Errors never include the content.
 */
export const loadRootPem = (input, opts = {}) => {
  const s = readEd25519(input, opts);
  const key = createPrivateKey(s.pem);
  const pubHex = rawPub(createPublicKey(key)).toString("hex");
  const d = describeKey(pubHex);
  checkDescription(s.notes, { publicKey: pubHex, fingerprint: fingerprint(pubHex), keyId: d.id });
  return { key, frame: FRAME_RAW, ...d, value: ed25519FieldValue(key), source: s.source };
};

/** Loads a root key file (must be mode 0600). Errors never include file content. */
export const loadRoot = (file) => {
  let st;
  try { st = statSync(file); } catch { throw new Error(`cannot read root key file ${file}`); }
  if (!st.isFile()) throw new Error(`${file} is not a file`);
  if ((st.mode & 0o077) !== 0) throw new Error(`${file} has permissions ${(st.mode & 0o777).toString(8)}; it must be 600 (chmod 600 on the offline media)`);
  let pem;
  try { pem = readFileSync(file, "utf8"); } catch { throw new Error(`cannot read root key file ${file}`); }
  try { return loadRootPem(pem); } catch (e) { throw new Error(e.message.replace("input", file)); }
};

/** Verifies one externally produced signature (hardware signer) for `frame`; returns the signature entry or throws. */
export const externalEntry = (delegation, { pub, sig, frame = FRAME_SOLANA_OFFCHAIN_V0 }) => {
  const d = parseDelegation(delegation);
  if (!d) throw new Error("not a valid delegation document");
  if (!HEX64.test(pub ?? "")) throw new Error("--pub must be 64 lowercase hex chars (raw Ed25519 public key)");
  if (!KNOWN_FRAMES.includes(frame)) throw new Error(`unknown frame ${JSON.stringify(frame)} (known: ${KNOWN_FRAMES.join(", ")})`);
  const bytes = parseSig(sig);
  const framed = frameDelegationPayload(frame, d, pub);
  const ok = (() => {
    try {
      const k = createPublicKey({ key: Buffer.concat([Buffer.from("302a300506032b6570032100", "hex"), Buffer.from(pub, "hex")]), format: "der", type: "spki" });
      return verify(null, framed, k, bytes);
    } catch { return false; }
  })();
  if (!ok) throw new Error(`the signature does not verify for this delegation, public key ${pub} and frame ${frame}; nothing was written (wrong device/path/app, edited delegation, or a different payload was signed)`);
  return { keyId: rootKeyId(pub), alg: "ed25519", sig: bytes.toString("base64"), ...(frame === FRAME_RAW ? {} : { frame }) };
};
/** 64-byte signature as 128 hex chars or base64. */
const parseSig = (t) => {
  if (typeof t !== "string") throw new Error("--sig is required");
  const b = /^[0-9a-fA-F]{128}$/.test(t) ? Buffer.from(t, "hex") : Buffer.from(t, "base64");
  if (b.length !== 64) throw new Error("--sig must be a 64-byte Ed25519 signature (128 hex chars or base64)");
  return b;
};

/**
 * Adds (or replaces) this root's signature over the delegation payload, framed as `root.frame` (default "raw"; a software
 * key can emulate a Ledger frame, which tests use). The entry declares its frame unless raw. Pure; returns a new object.
 */
export const addRootSignature = (delegation, root) => {
  const d = parseDelegation(delegation);
  if (!d) throw new Error("not a valid delegation document");
  const frame = root.frame ?? FRAME_RAW;
  const framed = frameDelegationPayload(frame, d, root.pub);
  if (!framed) throw new Error(`unknown frame ${JSON.stringify(frame)} (known: ${KNOWN_FRAMES.join(", ")})`);
  const sig = sign(null, framed, root.key).toString("base64");
  return attachSignature(d, { keyId: root.id, alg: "ed25519", sig, ...(frame === FRAME_RAW ? {} : { frame }) });
};
/** Adds (or replaces) a ready-made signature entry; sorted by keyId. */
export const attachSignature = (d, entry) => ({ ...d, signatures: [...d.signatures.filter((s) => s.keyId !== entry.keyId), entry].sort((a, b) => (a.keyId < b.keyId ? -1 : 1)) });

/**
 * Builds an UNSIGNED delegation. With `prev`, version is prev+1 and keys/revoked carry over. `addKey` appends a key,
 * `revokeId` moves a key id into `revoked` (and out of `keys`). `expires` is required and must be in the future.
 */
export const buildDelegation = ({ prev, version, expires, addKey, revokeId, pqRequired = false, now = Date.now(), allowLong = false }) => {
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
  // ADR-0182: the PQ epoch marker is sticky; a renewal can never drop it.
  return { format: DELEGATION_FORMAT, version: v, expires: exp, keys, revoked, ...(pqRequired || p?.pqRequired === true ? { pqRequired: true } : {}), signatures: [] };
};

export const summary = (d) =>
  [`delegation v${d.version}, expires ${d.expires}`,
   ...d.keys.map((k) => `  key ${k.keyId} ${k.alg} ${k.pub}  ${k.notBefore} .. ${k.notAfter}`),
   `  revoked: ${d.revoked.length ? d.revoked.join(", ") : "(none)"}`,
   ...(d.pqRequired === true ? ["  pq required: yes (needs the ML-DSA-65 co-signature of the pinned PQ root)"] : []),
   `  root signatures: ${d.signatures.length ? d.signatures.map((s) => s.keyId).join(", ") : "(none)"}`].join("\n");

// ---------- CLI ----------
const MEDIA = [/^\/Volumes\/[^/]+\//, /^\/media\//, /^\/mnt\//, /^\/run\/media\//];
const FLAGS = new Set(["dry-run", "allow-long", "to-stdin-for-vault", "root-key-stdin", "pq-root-stdin", "pq-required", "require-pq"]);
const parseArgs = (argv) => {
  const o = {};
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith("--")) throw new Error(`unexpected argument ${a}`);
    const k = a.slice(2);
    if (FLAGS.has(k)) { o[k] = true; continue; }
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
/** Software roots named on the command line: key files plus at most one secret on stdin (never echoed): `bw get item` JSON or a raw key. */
const loadRoots = (o) => {
  const roots = (o["root-key"] ?? []).map(loadRoot);
  if (o["root-key-stdin"]) {
    let text;
    try { text = readFileSync(0, "utf8"); } catch { throw new Error("cannot read a key from stdin"); }
    try { roots.push(loadRootPem(text)); } catch (e) { throw new Error(`stdin: ${e.message}`); }
  }
  return roots;
};
const readDelegation = (o, k = "in") => {
  const d = parseDelegation(readJson(one(o, k, true)));
  if (!d) throw new Error(`--${k} is not a valid delegation document`);
  return d;
};
/** One PQ root note on stdin (never echoed). */
const readPqRoot = () => {
  let text;
  try { text = readFileSync(0, "utf8"); } catch { throw new Error("cannot read the PQ root note from stdin"); }
  try { return loadPqRootNote(text); } catch (e) { throw new Error(`stdin: ${e.message}`); }
};
/** --pub-out: the PUBLIC descriptor of a PQ root (not secret) for pq-payload/pq-attach and for pinning. */
const writePub = (o, r) => {
  const f = one(o, "pub-out");
  if (f) writeFileSync(f, JSON.stringify({ keyId: r.keyId, alg: r.alg, pub: r.pub, frame: r.frame }, null, 2) + "\n");
};
const frameOf = (o) => one(o, "frame") ?? FRAME_SOLANA_OFFCHAIN_V0;
const draftOf = (o, now = Date.now()) => {
  const pub = one(o, "online-pub");
  const prevFile = one(o, "prev");
  const revokeId = one(o, "revoke-key-id");
  if (!pub && !prevFile) throw new Error("--online-pub is required (or pass --prev to renew an existing delegation)");
  const version = one(o, "version") === undefined ? undefined : Number(one(o, "version"));
  return buildDelegation({
    prev: prevFile ? readJson(prevFile) : undefined, version, expires: one(o, "expires", true), allowLong: o["allow-long"] === true, now, revokeId, pqRequired: o["pq-required"] === true,
    addKey: pub ? { keyId: one(o, "key-id", true), pub, notBefore: one(o, "not-before"), notAfter: one(o, "not-after") } : undefined,
  });
};
/** What the operator checks on the computer before approving on the device. */
export const payloadReport = (d, signerPub, frame) => {
  const payloadSha = createHash("sha256").update(delegationPayload(d)).digest("hex");
  const framed = frameDelegationPayload(frame, d, signerPub);
  return {
    framed,
    lines: [
      `frame:                      ${frame}`,
      `signer public key:          ${signerPub}`,
      `signer root key id:         ${rootKeyId(signerPub)}`,
      summary(d),
      `delegation payload sha256:  ${payloadSha}`,
      ...(frame === FRAME_SOLANA_OFFCHAIN_V0 ? [`text the Ledger will show:    ${ledgerText(d, payloadSha)}`] : []),
      `bytes to sign:              ${framed.length} bytes, sha256 ${createHash("sha256").update(framed).digest("hex")}`,
    ],
  };
};

const cmds = {
  "gen-root"(o) {
    if (o["to-stdin-for-vault"]) {
      if (o["out"]) throw new Error("--to-stdin-for-vault never writes a file; drop --out");
      if (process.stdout.isTTY) throw new Error("refusing to print a private key to a terminal; pipe stdout straight into `bw encode | bw create item` (see deploy/KEY-CEREMONY.md)");
      const r = generateRoot();
      const name = one(o, "name") ?? `OM release root (software signer) ${r.id}`;
      // Bitwarden CLI item JSON: type 2 = secure note. The notes are a public description; the key is the HIDDEN custom field `ed25519-pkcs8-YYYY-MM-DD`
      // (single line, base64 PKCS8 DER), masked in the UI. Read back with `bw get item <name> | ... --root-key-stdin`.
      const fieldName = secretFieldName("ed25519");
      process.stdout.write(JSON.stringify(secureNoteItem({ name, fieldName, notes: KINDS["root-ed25519"].describe({ id: r.id, pub: r.pub, fingerprint: fingerprint(r.pub), fieldName }), secret: ed25519FieldValue(r.pem) })));
      console.error(`root key id:  ${r.id}\npublic key:   ${r.pub}\nfingerprint:  sha256 ${r.fingerprint}\npin as:       "${r.id}": "${r.pub}"   (frame raw)\nThe private key went to stdout only (no file, no log). Confirm the vault item exists before closing this terminal.`);
      return;
    }
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
    const roots = o["root-key-stdin"] ? loadRoots({ "root-key-stdin": true }) : [loadRoot(one(o, "root-key", true))];
    const r = roots[0];
    console.log(`root key id:  ${r.id}\npublic key:   ${r.pub}\nfingerprint:  sha256 ${r.fingerprint}`);
  },
  draft(o) {
    const d = draftOf(o);
    console.error(summary(d));
    emit(d, one(o, "out"));
  },
  "sign-delegation"(o) {
    const roots = loadRoots(o);
    if (roots.length === 0) throw new Error("--root-key or --root-key-stdin is required");
    let d = draftOf(o);
    for (const r of roots) d = addRootSignature(d, r);
    console.error(summary(d));
    emit(d, one(o, "out"));
  },
  revoke(o) {
    const roots = loadRoots(o);
    if (roots.length === 0) throw new Error("--root-key or --root-key-stdin is required");
    const prev = readJson(one(o, "prev", true));
    const p = parseDelegation(prev);
    if (!p) throw new Error("--prev is not a valid delegation document");
    let d = buildDelegation({ prev, expires: one(o, "expires") ?? p.expires, revokeId: one(o, "key-id", true), pqRequired: o["pq-required"] === true, allowLong: o["allow-long"] === true });
    for (const r of roots) d = addRootSignature(d, r);
    console.error(summary(d));
    emit(d, one(o, "out"));
  },
  cosign(o) {
    const roots = loadRoots(o);
    if (roots.length !== 1) throw new Error("cosign takes exactly one of --root-key / --root-key-stdin");
    const inFile = one(o, "in", true);
    const d = readDelegation(o);
    console.error(`Signing as ${roots[0].id}. Check this is what you intend to authorise:\n${summary(d)}`);
    emit(addRootSignature(d, roots[0]), one(o, "out") ?? inFile);
  },
  "external-payload"(o) {
    const d = readDelegation(o);
    const pub = one(o, "signer-pub", true);
    if (!HEX64.test(pub)) throw new Error("--signer-pub must be 64 lowercase hex chars (raw Ed25519 public key)");
    const frame = frameOf(o);
    if (!KNOWN_FRAMES.includes(frame)) throw new Error(`unknown frame ${JSON.stringify(frame)} (known: ${KNOWN_FRAMES.join(", ")})`);
    if (d.signatures.length > 0) console.error(`note: the delegation already carries signatures from ${d.signatures.map((s) => s.keyId).join(", ")}; they are not part of the signed bytes`);
    const out = one(o, "out", true);
    const r = payloadReport(d, pub, frame);
    writeFileSync(out, r.framed, { flag: "w" });
    console.log(`${r.lines.join("\n")}\nwritten to:                 ${out}\nCompare the delegation summary and the sha256 on the Ledger screen with the lines above before approving.`);
  },
  "external-attach"(o) {
    const d = readDelegation(o);
    let pub = one(o, "pub");
    let sig = one(o, "sig");
    const helperFile = one(o, "from-helper");
    if (helperFile) {
      // the JSON ledger-sign.mjs wrote (--out): { pub, sigBase64, ... }; --pub, if also given, must agree
      const j = readJson(helperFile);
      if (pub !== undefined && pub !== j.pub) throw new Error(`--pub does not match the public key in ${helperFile}`);
      if (sig !== undefined) throw new Error("give --sig or --from-helper, not both");
      pub = j.pub;
      sig = j.sigBase64;
    }
    if (pub === undefined || sig === undefined) throw new Error("--pub and --sig (or --from-helper <file>) are required");
    const entry = externalEntry(d, { pub, sig, frame: frameOf(o) });
    const next = attachSignature(d, entry);
    console.error(`verified: ${entry.keyId} (${entry.frame ?? FRAME_RAW}) signed this delegation. Signatures now: ${next.signatures.map((s) => s.keyId).join(", ")}`);
    emit(next, one(o, "out") ?? one(o, "in"));
  },
  "pin-snippet"(o) {
    const specs = o["root"] ?? [];
    const keys = {};
    for (const spec of specs) {
      const m = /^([0-9a-f]{64})=([A-Za-z0-9._/-]{1,64})$/.exec(spec);
      if (!m) throw new Error(`--root must look like <64 hex public key>=<frame> (got ${JSON.stringify(spec)})`);
      if (!KNOWN_FRAMES.includes(m[2])) throw new Error(`unknown frame ${JSON.stringify(m[2])} (known: ${KNOWN_FRAMES.join(", ")})`);
      const id = rootKeyId(m[1]);
      if (keys[id]) throw new Error(`public key ${m[1]} given twice`);
      keys[id] = m[2] === FRAME_RAW ? m[1] : { pub: m[1], frame: m[2] };
    }
    const threshold = one(o, "threshold") === undefined ? 2 : Number(one(o, "threshold"));
    if (Object.keys(keys).length !== 3 || threshold !== 2) throw new Error("the operator policy is exactly 3 distinct roots with threshold 2 (pass three --root values)");
    const lit = JSON.stringify(keys, null, 2);
    console.log(`// apps/web/src/update/release-key.ts\nexport const ROOT_KEYS: Readonly<Record<string, RootKeyDescriptor>> = ${lit};\n\n// apps/web/build/verify-release.mjs\nexport const PINNED_ROOTS = ${lit};\n\n// roots file for \`verify --roots-file\`\n${JSON.stringify({ roots: keys, threshold })}`);
  },
  verify(o) {
    const d = readDelegation(o);
    const f = readJson(one(o, "roots-file", true));
    if (!f || typeof f !== "object" || !f.roots || typeof f.roots !== "object") throw new Error("--roots-file must be {\"roots\":{id:descriptor},\"threshold\":n}");
    for (const [id, v] of Object.entries(f.roots)) if (!describeRoot(v)) throw new Error(`roots file: bad descriptor for ${id}`);
    const pq = f.pq === undefined || f.pq === null ? null : checkDescriptor(f.pq);
    const r = verifyDelegation(d, { keys: f.roots, threshold: f.threshold, pq, pqRequired: f.pqRequired === true }, { now: Date.now(), pqFloor: o["require-pq"] === true });
    const pqNote = !r.ok || pq === null ? "" : d.signatures.some((s) => s.keyId === pq.keyId) ? `; PQ co-signature ${pq.keyId} verifies` : "; NO PQ co-signature (Ed25519-only chain, allowed until a pqRequired delegation)";
    console.log(r.ok ? `OK: ${f.threshold}-of-${Object.keys(f.roots).length} root signatures verify (delegation v${d.version}, expires ${d.expires}${d.pqRequired === true ? ", pqRequired" : ""})${pqNote}` : `FAILED: ${r.reason}`);
    if (!r.ok) process.exitCode = 1;
  },
  "gen-pq-root"(o) {
    if (!o["to-stdin-for-vault"]) throw new Error("gen-pq-root only runs with --to-stdin-for-vault: the PQ root seed never touches a disk (deploy/KEY-CEREMONY-PQ.md)");
    if (o["out"]) throw new Error("--to-stdin-for-vault never writes a key file; drop --out");
    if (process.stdout.isTTY) throw new Error("refusing to print a private key to a terminal; pipe stdout straight into `bw encode | bw create item` (see deploy/KEY-CEREMONY-PQ.md)");
    const r = generatePqRoot();
    const name = one(o, "name") ?? `OM release PQ root (ML-DSA-65) ${r.keyId}`;
    process.stdout.write(JSON.stringify(pqVaultItem(r, name)));
    writePub(o, r);
    console.error(`pq root key id: ${r.keyId}\nalgorithm:      ${r.alg} (FIPS 204), frame ${r.frame}\nfingerprint:    sha256 ${r.fingerprint}\nThe seed went to stdout only (no file, no log). Confirm the vault item exists before closing this terminal.`);
  },
  "migrate-vault-item"(o) {
    const kind = one(o, "kind", true);
    if (!KINDS[kind]) throw new Error(`--kind must be one of ${KIND_NAMES.join(", ")}`);
    const itemName = one(o, "item") ?? defaultItemName(kind);
    const derive = KINDS[kind].family === "pq"
      ? (input, opts) => { const r = loadPqRootNote(input, opts); return { id: r.keyId, pub: r.pub, fingerprint: pqFingerprint(r.pub), value: r.value }; }
      : (input, opts) => { const k = loadRootPem(input, opts); return { id: k.id, pub: k.pub, fingerprint: fingerprint(k.pub), value: k.value }; };
    const r = migrateViaBw({ itemName, kind, derive, bw: one(o, "bw-command") ?? "bw", dryRun: o["dry-run"] === true, fieldName: one(o, "field") });
    const pub = KINDS[kind].family === "pq" ? "" : `public key:   ${r.info.pub}\n`;
    const state = r.planned === 0 ? "already migrated (a dated hidden field holds the key, note is a description); nothing written"
      : o["dry-run"] ? `dry run: ${r.planned} edit(s) would be written; nothing written`
      : `migrated in ${r.written} verified edit(s): the key now lives in the hidden custom field \`${r.fieldName}\`, the note is a non-secret description`;
    console.log(`item:         ${itemName}\nkey id:       ${r.info.id}\n${pub}fingerprint:  sha256 ${r.info.fingerprint}\n${state}`);
  },
  "show-pq-root"(o) {
    if (!o["pq-root-stdin"]) throw new Error("show-pq-root needs --pq-root-stdin (pipe the vault note in)");
    const r = readPqRoot();
    writePub(o, r);
    const lit = JSON.stringify({ keyId: r.keyId, alg: r.alg, pub: r.pub, frame: r.frame }, null, 2);
    console.log(`pq root key id: ${r.keyId}\nalgorithm:      ${r.alg}, frame ${r.frame}\nfingerprint:    sha256 ${r.fingerprint}\n\n// apps/web/src/update/pq-root.ts\nexport const PQ_ROOT: PqRootPin | null = ${lit};\n\n// apps/web/build/verify-release.mjs\nexport const PINNED_PQ_ROOT = ${lit};`);
  },
  "pq-sign-delegation"(o) {
    if (!o["pq-root-stdin"]) throw new Error("pq-sign-delegation needs --pq-root-stdin (pipe the vault note in)");
    const inFile = one(o, "in", true);
    const d = readDelegation(o);
    const r = readPqRoot();
    console.error(`PQ-signing as ${r.keyId}. Check this is what you intend to authorise:\n${summary(d)}`);
    const next = addPqSignature(d, r);
    emit(next, one(o, "out") ?? inFile);
    const rootsFile = one(o, "roots-file");
    if (rootsFile) cmds.verify({ in: [one(o, "out") ?? inFile], "roots-file": [rootsFile], "require-pq": true });
  },
  "pq-payload"(o) {
    const d = readDelegation(o);
    const pin = checkDescriptor(readJson(one(o, "pq-root-file", true)));
    const bytes = pqPayload(d, pin);
    writeFileSync(one(o, "out", true), bytes);
    console.log(`${summary(d)}\npq root: ${pin.keyId} (${pin.alg}, ${pin.frame})\nbytes to sign: ${bytes.length} bytes, sha256 ${createHash("sha256").update(bytes).digest("hex")}\nwritten to: ${one(o, "out", true)}`);
  },
  "pq-attach"(o) {
    const d = readDelegation(o);
    const pin = checkDescriptor(readJson(one(o, "pq-root-file", true)));
    const next = externalPqAttach(d, pin, one(o, "sig", true));
    console.error(`verified: ${pin.keyId} signed this delegation. Signatures now: ${next.signatures.map((s) => s.keyId).join(", ")}`);
    emit(next, one(o, "out") ?? one(o, "in"));
  },
  inspect(o) {
    console.log(summary(readDelegation(o)));
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
