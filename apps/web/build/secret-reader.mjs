// The ONE reader for OM release secrets that live in a Bitwarden/Vaultwarden item (used by sign.mjs, root-ceremony.mjs and
// pq-ceremony.mjs). Pure node, no dependencies, no I/O: callers hand it text, it hands back validated key material.
//
// Where the secret lives. NEW (2026-10-28): a secure-note item whose notes are only a non-secret description and whose
// secret is a custom field, type 1 (HIDDEN: masked in the UI, unlike a note body, which is always shown in
// plaintext when the item is opened). LEGACY: the secret is the note body. Input accepted from stdin or a file:
//   1. `bw get item NAME` JSON  -> field `secret` (must be hidden), else the legacy note body
//   2. any other text           -> treated as the secret itself (a `bw get notes` body, a bare value, a key file)
// Accepted encodings:
//   Ed25519 (PKCS8): multi-line PEM; PEM collapsed onto one line (spaces and/or literal \n between the lines); bare base64
//                    PKCS8 DER on one line (the form the generators write: no newline, no spaces).
//   ML-DSA-65 seed:  64 hex chars (32 bytes), or `seed: <64 hex>`; the legacy `-----BEGIN OM PQ ROOT SEED-----` block (pq-ceremony.mjs).
// A hidden-field value must be exactly one secret (nothing else); only the legacy note body may carry surrounding text.
// Secret material is never printed, logged, put in argv or in an error message, and nothing here touches a disk.
import { createHash, createPrivateKey, createPublicKey } from "node:crypto";

/** Legacy field name (accepted as an alias on read). New fields are descriptive and dated: `ed25519-pkcs8-YYYY-MM-DD`, `mldsa65-seed-YYYY-MM-DD`. */
export const LEGACY_FIELD = "secret";
export const SECRET_FIELD = LEGACY_FIELD;
/** Field-name prefix per secret kind (readEd25519 -> "ed25519", readPqSeed -> "pq"). */
export const FIELD_PREFIX = { ed25519: "ed25519-pkcs8", pq: "mldsa65-seed" };
const today = () => new Date().toISOString().slice(0, 10);
/** The field name a generator/migration writes for `kind`, dated so a rotated secret can sit beside the old one. */
export const secretFieldName = (kind, date = today()) => {
  if (!FIELD_PREFIX[kind]) throw new Error(`unknown secret kind ${JSON.stringify(kind)}`);
  return `${FIELD_PREFIX[kind]}-${date}`;
};
const isDatedName = (name, kind) => typeof name === "string" && new RegExp(`^${FIELD_PREFIX[kind]}-\\d{4}-\\d{2}-\\d{2}$`).test(name);
/** True for any name this reader treats as a secret field of `kind` (dated or legacy). */
export const isSecretFieldName = (name, kind) => name === LEGACY_FIELD || isDatedName(name, kind);
export const FIELD_HIDDEN = 1; // Bitwarden custom field types: 0 text, 1 hidden, 2 boolean, 3 linked

const B64 = /^[A-Za-z0-9+/]+={0,2}$/;
const HEX64 = /^[0-9a-fA-F]{64}$/;
const PEM_BLOCK = /-----BEGIN PRIVATE KEY-----([^-]*)-----END PRIVATE KEY-----/;
const MAX_SECRET_CHARS = 16_384;

export const sha256Hex = (bytes) => createHash("sha256").update(bytes).digest("hex");
const isRecord = (v) => typeof v === "object" && v !== null && !Array.isArray(v);

// ---------- locating the secret text ----------

/**
 * Finds the secret text in `input` (a string: item JSON or raw text; or an already parsed item object).
 * Returns { value, strict, source: "field" | "notes" | "raw", notes }:
 *   - "field": a hidden custom field (strict: the value must be exactly one secret). Which one: `opts.fieldName` if given (exact),
 *              else the single dated field of `opts.kind` (`ed25519-pkcs8-*` / `mldsa65-seed-*`); the legacy name `secret` is the
 *              fallback alias when neither is present. Several candidates are an error: name one with `fieldName`.
 *   - "notes": the legacy note body of an item with no secret field (tolerant)
 *   - "raw":   not item JSON; the text itself (tolerant)
 * `allowVisibleField` (migration only) also accepts a `secret` field of the wrong, visible type; otherwise that is an error.
 */
export const locateSecret = (input, { allowVisibleField = false, kind, fieldName } = {}) => {
  let item = input;
  if (typeof input === "string") {
    if (input.length > 4 * MAX_SECRET_CHARS) throw new Error("input is too large to be a vault item");
    if (!input.trimStart().startsWith("{")) return { value: input, strict: false, source: "raw", notes: "" };
    try { item = JSON.parse(input); } catch { throw new Error("input looks like JSON but is not valid JSON"); }
  }
  if (!isRecord(item)) throw new Error("input is not a vault item");
  const notes = typeof item.notes === "string" ? item.notes : "";
  const all = Array.isArray(item.fields) ? item.fields.filter(isRecord) : [];
  const named = (pred) => all.filter((f) => pred(f.name));
  let fields = fieldName !== undefined ? named((n) => n === fieldName) : kind !== undefined ? named((n) => isDatedName(n, kind)) : [];
  if (fields.length === 0) fields = named((n) => n === LEGACY_FIELD);
  if (fields.length > 1) throw new Error(fieldName !== undefined ? `the item has more than one custom field named ${fieldName}` : "the item has more than one candidate secret field; pass the field name explicitly");
  if (fields.length === 1) {
    const f = fields[0];
    if (f.type !== FIELD_HIDDEN && !allowVisibleField) throw new Error(`the custom field ${f.name} is not of type hidden; fix it with migrate-vault-item (a visible field defeats the purpose)`);
    if (typeof f.value !== "string" || f.value.length === 0) throw new Error(`the custom field ${f.name} is empty`);
    return { value: f.value, strict: true, source: "field", notes, fieldType: f.type, fieldName: f.name };
  }
  if (notes.length === 0) throw new Error(`the item has no secret custom field and its note is empty`);
  return { value: notes, strict: false, source: "notes", notes };
};

// ---------- Ed25519 (PKCS8) ----------

const wrap64 = (b64) => b64.match(/.{1,64}/g).join("\n");
/** Canonical PKCS8 PEM text for a base64 body; throws a generic message on anything that is not Ed25519 PKCS8. */
const pemFromBase64 = (b64) => {
  const bad = () => new Error("input is not a PKCS8 Ed25519 private key (accepted: PEM, one-line PEM, base64 DER)");
  if (!B64.test(b64) || b64.length > 400) throw bad();
  const pem = `-----BEGIN PRIVATE KEY-----\n${wrap64(b64)}\n-----END PRIVATE KEY-----\n`;
  let key;
  try { key = createPrivateKey(pem); } catch { throw bad(); }
  if (key.asymmetricKeyType !== "ed25519") throw new Error("input is not an ed25519 key");
  // strict: the DER must be exactly the canonical encoding of that key (no trailing bytes, no non-canonical base64)
  if (key.export({ format: "der", type: "pkcs8" }).toString("base64") !== b64) throw bad();
  return pem;
};
/** Body of a PEM block: whitespace and literal `\n` / `\r` escapes removed. */
const pemBody = (body) => body.replace(/\\+[nr]/g, "").replace(/\s+/g, "");

/**
 * Normalises one Ed25519 PKCS8 secret to canonical multi-line PEM. `strict` means the whole text must be the secret;
 * otherwise a PEM block may sit inside surrounding text (the legacy note body).
 */
export const normalizeEd25519 = (text, { strict = false } = {}) => {
  if (typeof text !== "string" || text.length > MAX_SECRET_CHARS) throw new Error("input is not a PKCS8 Ed25519 private key (accepted: PEM, one-line PEM, base64 DER)");
  const t = text.trim().replace(/^(?:\\+[nr])+|(?:\\+[nr])+$/g, "").trim(); // literal \n escapes around a one-line value
  const m = PEM_BLOCK.exec(t);
  if (m) {
    if (strict && m[0] !== t) throw new Error("the secret field holds more than one key (it must hold exactly the key)");
    return pemFromBase64(pemBody(m[1]));
  }
  if (/-----BEGIN /.test(t)) throw new Error("input is not a PKCS8 Ed25519 private key (accepted: PEM, one-line PEM, base64 DER)");
  return pemFromBase64(t); // bare base64 DER: the whole text must be that one token
};
/** The single-line form the generators store in the hidden field: base64 of the PKCS8 DER. */
export const ed25519FieldValue = (pemOrKey) => (typeof pemOrKey === "string" ? createPrivateKey(pemOrKey) : pemOrKey).export({ format: "der", type: "pkcs8" }).toString("base64");
export const ed25519PublicHex = (pem) => createPublicKey(createPrivateKey(pem)).export({ format: "der", type: "spki" }).subarray(-32).toString("hex");

/** Reads an Ed25519 key from item JSON or raw text. Returns { pem, notes, source }; `notes` is the description to cross-check. */
export const readEd25519 = (input, opts = {}) => {
  const loc = locateSecret(input, { kind: "ed25519", ...opts });
  return { pem: normalizeEd25519(loc.value, { strict: loc.strict }), notes: loc.notes, source: loc.source, fieldName: loc.fieldName };
};

// ---------- ML-DSA-65 seed ----------

/** The legacy block the old generator wrote into the note body (pq-ceremony.mjs parses its metadata lines). */
export const PQ_BLOCK = /-----BEGIN OM PQ ROOT SEED-----\n([\s\S]*?)-----END OM PQ ROOT SEED-----/;
/**
 * Extracts the 32-byte seed (lowercase hex) from bare 64-hex, `seed: <hex>` or (tolerant) the legacy block.
 * Returns { seedHex, meta } where `meta` carries the legacy block's key: value lines (alg, frame, key-id, fingerprint), if any.
 */
export const normalizePqSeed = (text, { strict = false } = {}) => {
  if (typeof text !== "string" || text.length > MAX_SECRET_CHARS) throw new Error("input is not an OM PQ root seed (64 hex chars, or `seed: <64 hex>`)");
  const t = text.trim();
  if (HEX64.test(t)) return { seedHex: t.toLowerCase(), meta: {} };
  const one = /^seed:\s*([0-9a-fA-F]{64})$/.exec(t);
  if (one) return { seedHex: one[1].toLowerCase(), meta: {} };
  const block = PQ_BLOCK.exec(t);
  if (block && !strict) {
    const meta = Object.fromEntries(block[1].split("\n").filter(Boolean).map((l) => { const i = l.indexOf(": "); return i < 0 ? ["", ""] : [l.slice(0, i), l.slice(i + 2)]; }));
    if (!HEX64.test(meta.seed ?? "")) throw new Error("the PQ root note has no valid 32-byte seed");
    return { seedHex: meta.seed.toLowerCase(), meta };
  }
  if (!strict) {
    const line = /^seed:\s*([0-9a-fA-F]{64})\s*$/m.exec(t); // legacy note body with extra lines around it
    if (line) return { seedHex: line[1].toLowerCase(), meta: {} };
  }
  throw new Error("input is not an OM PQ root seed note (64 hex chars, `seed: <64 hex>` or the legacy seed block)");
};
/** Reads a PQ seed from item JSON or raw text. Returns { seedHex, meta, notes, source }. */
export const readPqSeed = (input, opts = {}) => {
  const loc = locateSecret(input, { kind: "pq", ...opts });
  return { ...normalizePqSeed(loc.value, { strict: loc.strict }), notes: loc.notes, source: loc.source, fieldName: loc.fieldName };
};

// ---------- the non-secret description in `notes` ----------

/**
 * `notes` of a migrated/generated item: only public facts, one `label: value` per line. `public-key`, `key-id` and
 * `fingerprint` are cross-checked against the secret on every read, so a damaged or swapped item is caught.
 */
export const describeItem = ({ purpose, keyId, keyIdLabel = "key-id", publicKey, fingerprint, extra = [], fieldName = LEGACY_FIELD }) =>
  [purpose, "", `${keyIdLabel}: ${keyId}`, ...(publicKey ? [`public-key: ${publicKey}`] : []), `fingerprint: sha256 ${fingerprint}`, ...extra, "",
   `The private key is NOT in this note: it is in the hidden custom field "${fieldName}" (single line). Do not paste it here.`].join("\n");
/** Throws if a `public-key:` / `fingerprint:` / `key-id:` line of the description disagrees with what the secret derives. Only claims present are checked. */
export const checkDescription = (notes, { publicKey, fingerprint, keyId }) => {
  const get = (label) => new RegExp(`^${label}: (.+)$`, "m").exec(notes ?? "")?.[1]?.trim();
  const pk = get("public-key");
  if (pk !== undefined && publicKey !== undefined && pk !== publicKey) throw new Error("the item description's public key does not match the secret (wrong or damaged item)");
  const fp = get("fingerprint");
  if (fp !== undefined && fingerprint !== undefined && fp !== `sha256 ${fingerprint}`) throw new Error("the item description's fingerprint does not match the secret (wrong or damaged item)");
  const id = get("key-id");
  if (id !== undefined && keyId !== undefined && id !== keyId) throw new Error("the item description's key id does not match the secret (wrong or damaged item)");
};

/** A Bitwarden secure-note item (CLI JSON) with the secret in a HIDDEN custom field. */
export const secureNoteItem = ({ name, notes, secret, fieldName }) => ({
  type: 2, name, notes, secureNote: { type: 0 }, favorite: false, organizationId: null, folderId: null,
  fields: [{ name: fieldName, value: secret, type: FIELD_HIDDEN, linkedId: null }],
});
