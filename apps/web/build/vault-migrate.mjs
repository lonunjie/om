// Moves an OM release secret from a Bitwarden/Vaultwarden NOTE BODY (always shown in plaintext when the item is opened) into
// a descriptively named, dated HIDDEN custom field (`ed25519-pkcs8-YYYY-MM-DD` / `mldsa65-seed-YYYY-MM-DD`, type 1, masked in the UI; a legacy field named `secret` is renamed), and blanks the note to a non-secret description.
// Used by `root-ceremony.mjs migrate-vault-item` for all three item kinds, in either vault. See deploy/KEY-CEREMONY.md.
//
// The tool never unlocks anything: the caller supplies the `bw` environment (BW_SESSION, BITWARDENCLI_APPDATA_DIR, server).
// Safety order, so a failure can never lose the key:
//   1. read the item, derive the PUBLIC key from the old secret with the ceremony tool's own loader;
//   2. write the item with the hidden field ADDED and the note still intact; sync; read back; the field must derive the same key;
//   3. only then write the item with the note blanked; sync; read back; the note must hold no secret and the field the same key.
// Secret material is only ever in this process's memory and on the stdin of `bw edit item` (never in argv, a file or a log);
// bw's own output (which echoes the whole item) is captured and discarded.
import { spawnSync } from "node:child_process";
import { FIELD_HIDDEN, isSecretFieldName, secretFieldName, describeItem } from "./secret-reader.mjs";

const SHORT = {
  "online-ed25519": "OM release signing key (ed25519) - lonunjie",
  "root-ed25519": "OM release root (software signer)",
  "pq-root": "OM release PQ root (ML-DSA-65)",
};
/** Per kind: the non-secret description written into `notes`. `info` = { id, pub, fingerprint } from the key's own derivation. */
export const KINDS = {
  "online-ed25519": {
    family: "ed25519",
    describe: (info) => describeItem({
      purpose: "OM release signing key (ed25519): the delegated ONLINE key that signs OM web bundle manifests under the root-signed delegation (apps/web/build/sign.mjs --sign-from-vault).",
      keyId: "om-release-2026-10-28", keyIdLabel: "release-key-id", publicKey: info.pub, fingerprint: info.fingerprint, fieldName: info.fieldName,
    }),
  },
  "root-ed25519": {
    family: "ed25519",
    describe: (info) => describeItem({
      purpose: "OM release root, software signer (ed25519, frame raw): 1 of the 3 signers of the 2-of-3 root policy that signs the release-key delegation (deploy/KEY-CEREMONY.md). Personal vault only.",
      keyId: info.id, publicKey: info.pub, fingerprint: info.fingerprint, fieldName: info.fieldName,
    }),
  },
  "pq-root": {
    family: "pq",
    describe: (info) => describeItem({
      purpose: "OM release PQ root (ML-DSA-65, FIPS 204): the post-quantum co-signer of the release-root delegation (ADR-0182, deploy/KEY-CEREMONY-PQ.md). The secret is the 32-byte seed. Personal vault only.",
      keyId: info.id, fingerprint: info.fingerprint, fieldName: info.fieldName, extra: ["alg: ml-dsa-65", "frame: om-pq-v1"],
    }),
  },
};
export const KIND_NAMES = Object.keys(KINDS);
export const defaultItemName = (kind) => SHORT[kind];

const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);
const isSecret = (f, family, name) => isRecord(f) && (name !== undefined ? f.name === name : isSecretFieldName(f.name, family));
const others = (item, family, name) => (Array.isArray(item.fields) ? item.fields.filter((f) => !isSecret(f, family, name)) : []);
/** The secret field: `name` if given, else the dated one of the family (preferred), else the legacy `secret`. */
const secretField = (item, family, name) => {
  const all = Array.isArray(item.fields) ? item.fields.filter((f) => isSecret(f, family, name)) : [];
  return all.find((f) => f.name !== "secret") ?? all[0];
};
const withSecret = (item, family, value, name) => ({ ...item, fields: [...others(item, family), { name, value, type: FIELD_HIDDEN, linkedId: null }] });

/**
 * Pure planning. `derive(input, opts)` is the ceremony tool's own loader for the kind: it takes item JSON (object or string) or
 * raw text and returns { id, pub, fingerprint, value } (value = the canonical single-line secret) or throws a content-free error.
 * Returns { info, steps: [item, ...], summary }: the items to write, in order (0, 1 or 2).
 */
export const planMigration = (item, kind, derive, { fieldName, today } = {}) => {
  const K = KINDS[kind];
  if (!K) throw new Error(`unknown --kind ${JSON.stringify(kind)} (one of ${KIND_NAMES.join(", ")})`);
  if (!isRecord(item) || typeof item.id !== "string" || item.id === "") throw new Error("bw returned no usable item (no id)");
  if (item.type !== 2) throw new Error("the item is not a secure note (type 2); refusing to touch it");
  const family = K.family;
  const field = secretField(item, family, fieldName);
  const target = fieldName ?? (field && field.name !== "secret" ? field.name : secretFieldName(family, today));
  // Legacy: the note body is the secret. Strip the field so the loader looks at the notes.
  let legacy = null;
  let legacyErr = null;
  try { legacy = derive({ ...item, fields: others(item, family) }); } catch (e) { legacyErr = e; }
  let held = null;
  let heldErr = null;
  if (field) { try { held = derive(item, { allowVisibleField: true, fieldName: field.name }); } catch (e) { heldErr = e; } }
  if (legacy && held && legacy.pub !== held.pub) throw new Error("the note body and the custom field hold DIFFERENT keys; refusing to choose (inspect the item by hand)");
  const info = legacy ?? held;
  if (!info) throw heldErr ?? legacyErr ?? new Error("the item holds no secret");
  const wantNotes = K.describe({ ...info, fieldName: target });
  const steps = [];
  const hiddenOk = field && field.name === target && field.type === FIELD_HIDDEN && held !== null;
  let cur = item;
  if (!hiddenOk) { cur = withSecret(item, family, legacy ? legacy.value : held.value, target); steps.push(cur); }
  if (cur.notes !== wantNotes) steps.push({ ...cur, notes: wantNotes });
  return { info, steps, fieldName: target };
};

const bwFail = (args, r) => new Error(`bw ${args.slice(0, 2).join(" ")} failed (${r.error ? r.error.code ?? "spawn error" : `exit ${r.status}`}); its output is withheld because it can carry the secret`);

/** Runs the migration through `bw`. `opts.bw` = the bw executable (default "bw"); `opts.spawn` is for tests. */
export const migrateViaBw = ({ itemName, kind, derive, bw = "bw", dryRun = false, spawn = spawnSync, fieldName, today }) => {
  const run = (args, input) => {
    const r = spawn(bw, args, { input, encoding: "utf8", maxBuffer: 64 * 1024 * 1024, env: process.env });
    if (r.error || r.status !== 0) throw bwFail(args, r);
    return r.stdout;
  };
  const read = (ref) => {
    let item;
    try { item = JSON.parse(run(["get", "item", ref])); } catch (e) {
      if (e instanceof SyntaxError) throw new Error("bw get item returned no JSON (vault locked, or the name is not unique?)");
      throw e;
    }
    return item;
  };
  run(["sync"]);
  const item = read(itemName);
  const { info, steps, fieldName: target } = planMigration(item, kind, derive, { fieldName, today });
  const family = KINDS[kind].family;
  if (dryRun || steps.length === 0) return { info, written: 0, planned: steps.length, fieldName: target };
  let n = 0;
  for (const [i, planned] of steps.entries()) {
    // Every edit bumps the item's revisionDate and the server refuses a write built on an older copy, so steps after the
    // first are re-applied (notes only) to a fresh read instead of replaying the copy read before step 1.
    const step = i === 0 ? planned : { ...read(item.id), notes: planned.notes };
    run(["edit", "item", item.id], Buffer.from(JSON.stringify(step), "utf8").toString("base64")); // output discarded (it echoes the item)
    run(["sync"]);
    const back = read(item.id);
    const f = secretField(back, family, target);
    const got = derive(back, { fieldName: target }); // throws unless the hidden field derives a key
    if (!f || f.type !== FIELD_HIDDEN) throw new Error(`read-back: the field ${target} is not hidden after writing`);
    if (got.pub !== info.pub) throw new Error("read-back: the stored field derives a DIFFERENT public key; the previous content may still be in the note, check the item before doing anything else");
    n++;
  }
  // final state: the note holds no secret any more
  const fin = read(item.id);
  try { derive({ ...fin, fields: others(fin, family) }); throw new Error("read-back: the note body still holds a secret"); } catch (e) { if (e.message.startsWith("read-back")) throw e; }
  return { info, written: n, planned: steps.length, fieldName: target };
};
