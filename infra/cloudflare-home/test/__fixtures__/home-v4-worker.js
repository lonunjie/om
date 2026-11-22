var __defProp = Object.defineProperty;
var __name = (target, value) => __defProp(target, "name", { value, configurable: true });

// ../../packages/om-home-wire/src/cbor.ts
var CborError = class extends Error {
  static {
    __name(this, "CborError");
  }
  constructor(message) {
    super(message);
    this.name = "CborError";
  }
};
var MAX_DEPTH = 8;
var MAX_ELEMENTS = 4096;
var MAX_UINT = (1n << 64n) - 1n;
var utf8 = new TextEncoder();
var utf8Strict = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });
function head(major, n, out) {
  const v = BigInt(n);
  if (v < 0n || v > MAX_UINT) throw new CborError("integer out of range");
  const m = major << 5;
  if (v < 24n) out.push(m | Number(v));
  else if (v < 0x100n) out.push(m | 24, Number(v));
  else if (v < 0x10000n) out.push(m | 25, Number(v >> 8n), Number(v & 0xffn));
  else if (v < 0x100000000n) out.push(m | 26, ...[24n, 16n, 8n, 0n].map((s) => Number(v >> s & 0xffn)));
  else out.push(m | 27, ...[56n, 48n, 40n, 32n, 24n, 16n, 8n, 0n].map((s) => Number(v >> s & 0xffn)));
}
__name(head, "head");
function compareBytes(a, b) {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d;
  }
  return a.length - b.length;
}
__name(compareBytes, "compareBytes");
function enc(v, out, depth) {
  if (depth > MAX_DEPTH) throw new CborError("nesting too deep");
  if (v === null) out.push(246);
  else if (typeof v === "boolean") out.push(v ? 245 : 244);
  else if (typeof v === "number" || typeof v === "bigint") {
    if (typeof v === "number" && !Number.isSafeInteger(v)) throw new CborError("not an integer");
    head(0, v, out);
  } else if (typeof v === "string") {
    const b = utf8.encode(v);
    head(3, b.length, out);
    for (const x of b) out.push(x);
  } else if (v instanceof Uint8Array) {
    head(2, v.length, out);
    for (const x of v) out.push(x);
  } else if (v instanceof Map) {
    const entries = [...v.entries()].map(([k, val]) => {
      const kb = [];
      enc(k, kb, depth + 1);
      return { k: Uint8Array.from(kb), val };
    });
    entries.sort((a, b) => compareBytes(a.k, b.k));
    for (let i = 1; i < entries.length; i++) {
      if (compareBytes(entries[i - 1].k, entries[i].k) === 0) throw new CborError("duplicate map key");
    }
    head(5, entries.length, out);
    for (const e of entries) {
      for (const x of e.k) out.push(x);
      enc(e.val, out, depth + 1);
    }
  } else if (Array.isArray(v)) {
    head(4, v.length, out);
    for (const x of v) enc(x, out, depth + 1);
  } else {
    throw new CborError("unsupported value");
  }
}
__name(enc, "enc");
function encode(v) {
  const out = [];
  enc(v, out, 0);
  return Uint8Array.from(out);
}
__name(encode, "encode");
function map(entries) {
  const m = /* @__PURE__ */ new Map();
  for (const [k, v] of entries) {
    if (v === void 0) continue;
    if (m.has(k)) throw new CborError("duplicate map key");
    m.set(k, v);
  }
  return m;
}
__name(map, "map");
var Reader = class {
  constructor(b) {
    this.b = b;
  }
  static {
    __name(this, "Reader");
  }
  pos = 0;
  elements = 0;
  byte() {
    const x = this.b[this.pos];
    if (x === void 0) throw new CborError("truncated");
    this.pos += 1;
    return x;
  }
  take(n) {
    if (n > this.b.length - this.pos) throw new CborError("truncated");
    const s = this.b.subarray(this.pos, this.pos + n);
    this.pos += n;
    return s;
  }
  arg(info) {
    if (info < 24) return BigInt(info);
    const width = info === 24 ? 1 : info === 25 ? 2 : info === 26 ? 4 : info === 27 ? 8 : 0;
    if (width === 0) throw new CborError("indefinite or reserved length");
    let v = 0n;
    for (const x of this.take(width)) v = v << 8n | BigInt(x);
    const min = width === 1 ? 24n : width === 2 ? 0x100n : width === 4 ? 0x10000n : 0x100000000n;
    if (v < min) throw new CborError("non-shortest integer");
    return v;
  }
  length(info) {
    const n = this.arg(info);
    if (n > BigInt(this.b.length)) throw new CborError("length exceeds input");
    return Number(n);
  }
  value(depth) {
    if (depth > MAX_DEPTH) throw new CborError("nesting too deep");
    this.elements += 1;
    if (this.elements > MAX_ELEMENTS) throw new CborError("too many elements");
    const ib = this.byte();
    const major = ib >> 5;
    const info = ib & 31;
    switch (major) {
      case 0: {
        const n = this.arg(info);
        return n <= BigInt(Number.MAX_SAFE_INTEGER) ? Number(n) : n;
      }
      case 2:
        return this.take(this.length(info)).slice();
      case 3: {
        try {
          return utf8Strict.decode(this.take(this.length(info)));
        } catch {
          throw new CborError("invalid utf-8");
        }
      }
      case 4: {
        const n = this.length(info);
        const a = [];
        for (let i = 0; i < n; i++) a.push(this.value(depth + 1));
        return a;
      }
      case 5: {
        const n = this.length(info);
        const m = /* @__PURE__ */ new Map();
        let prev;
        for (let i = 0; i < n; i++) {
          const start = this.pos;
          const k = this.value(depth + 1);
          if (typeof k !== "number" && typeof k !== "string") throw new CborError("map keys must be unsigned integers or text");
          const kb = this.b.subarray(start, this.pos);
          if (prev !== void 0 && compareBytes(prev, kb) >= 0) throw new CborError("map keys not sorted or duplicated");
          prev = kb;
          m.set(k, this.value(depth + 1));
        }
        return m;
      }
      case 7:
        if (info === 20) return false;
        if (info === 21) return true;
        if (info === 22) return null;
        throw new CborError("unsupported simple value or float");
      default:
        throw new CborError(major === 1 ? "negative integers are not used" : "tags are not used");
    }
  }
};
function decode(bytes) {
  const r = new Reader(bytes);
  const v = r.value(0);
  if (r.pos !== bytes.length) throw new CborError("trailing bytes");
  return v;
}
__name(decode, "decode");

// ../../packages/om-home-wire/src/bytes.ts
var HEX = "0123456789abcdef";
function toHex(b) {
  let s = "";
  for (const x of b) s += HEX.charAt(x >> 4) + HEX.charAt(x & 15);
  return s;
}
__name(toHex, "toHex");
function fromHex(s) {
  if (s.length % 2 !== 0 || !/^[0-9a-f]*$/.test(s)) throw new Error("bad hex");
  const out = new Uint8Array(s.length / 2);
  for (let i = 0; i < out.length; i++) out[i] = parseInt(s.slice(2 * i, 2 * i + 2), 16);
  return out;
}
__name(fromHex, "fromHex");
function toBase64Url(b) {
  let s = "";
  for (const x of b) s += String.fromCharCode(x);
  return btoa(s).replaceAll("+", "-").replaceAll("/", "_").replace(/=+$/, "");
}
__name(toBase64Url, "toBase64Url");
function fromBase64Url(s) {
  if (!/^[A-Za-z0-9_-]*$/.test(s)) throw new Error("bad base64url");
  const bin = atob(s.replaceAll("-", "+").replaceAll("_", "/") + "=".repeat((4 - s.length % 4) % 4));
  const out = new Uint8Array(bin.length);
  for (let i = 0; i < bin.length; i++) out[i] = bin.charCodeAt(i);
  return out;
}
__name(fromBase64Url, "fromBase64Url");
function concat(...parts) {
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}
__name(concat, "concat");
function equalBytes(a, b) {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= (a[i] ?? 0) ^ (b[i] ?? 0);
  return d === 0;
}
__name(equalBytes, "equalBytes");
async function sha256(...parts) {
  return new Uint8Array(await crypto.subtle.digest("SHA-256", concat(...parts)));
}
__name(sha256, "sha256");
function utf82(s) {
  return new TextEncoder().encode(s);
}
__name(utf82, "utf8");

// ../../packages/om-home-wire/src/version.ts
var API_MAJOR = 1;

// ../../packages/om-home-wire/src/fields.ts
var Obj = class _Obj {
  static {
    __name(this, "Obj");
  }
  m;
  used = /* @__PURE__ */ new Set();
  constructor(v) {
    if (!(v instanceof Map)) throw new CborError("expected a map");
    this.m = v;
  }
  take(k) {
    this.used.add(k);
    return this.m.get(k);
  }
  has(k) {
    return this.m.has(k);
  }
  uint(k) {
    const v = this.take(k);
    if (typeof v !== "number") throw new CborError(`field ${String(k)}: expected an unsigned integer`);
    return v;
  }
  optUint(k) {
    return this.m.has(k) ? this.uint(k) : void 0;
  }
  bool(k) {
    const v = this.take(k);
    if (typeof v !== "boolean") throw new CborError(`field ${String(k)}: expected a boolean`);
    return v;
  }
  optBool(k) {
    return this.m.has(k) ? this.bool(k) : void 0;
  }
  bytes(k, min, max) {
    const v = this.take(k);
    if (!(v instanceof Uint8Array) || v.length < min || v.length > max) throw new CborError(`field ${String(k)}: expected ${String(min)}..${String(max)} bytes`);
    return v;
  }
  optBytes(k, min, max) {
    return this.m.has(k) ? this.bytes(k, min, max) : void 0;
  }
  text(k, min, max) {
    const v = this.take(k);
    if (typeof v !== "string" || v.length < min || v.length > max) throw new CborError(`field ${String(k)}: expected text of ${String(min)}..${String(max)}`);
    return v;
  }
  optText(k, min, max) {
    return this.m.has(k) ? this.text(k, min, max) : void 0;
  }
  /** An array whose every element is a byte string of `min..max` bytes; `1..n` items unless `allowEmpty`. */
  byteList(k, minItems, maxItems, min, max) {
    const v = this.take(k);
    if (!Array.isArray(v) || v.length < minItems || v.length > maxItems) throw new CborError(`field ${String(k)}: expected ${String(minItems)}..${String(maxItems)} items`);
    return v.map((x) => {
      if (!(x instanceof Uint8Array) || x.length < min || x.length > max) throw new CborError(`field ${String(k)}: bad item`);
      return x;
    });
  }
  optByteList(k, minItems, maxItems, min, max) {
    return this.m.has(k) ? this.byteList(k, minItems, maxItems, min, max) : void 0;
  }
  optTextList(k, minItems, maxItems, min, max) {
    if (!this.m.has(k)) return void 0;
    const v = this.take(k);
    if (!Array.isArray(v) || v.length < minItems || v.length > maxItems) throw new CborError(`field ${String(k)}: expected ${String(minItems)}..${String(maxItems)} items`);
    return v.map((x) => {
      if (typeof x !== "string" || x.length < min || x.length > max) throw new CborError(`field ${String(k)}: bad item`);
      return x;
    });
  }
  /** An array of maps (each wrapped for field access), `0..maxItems` items. */
  mapList(k, maxItems) {
    const v = this.take(k);
    if (!Array.isArray(v) || v.length > maxItems) throw new CborError(`field ${String(k)}: expected at most ${String(maxItems)} maps`);
    return v.map((x) => new _Obj(x));
  }
  /** `digest = {0: hash-alg(1 = sha-256), 1: bstr .size 32}` (spec/cddl/common.cddl). */
  digest(k) {
    const d = new _Obj(this.take(k) ?? null);
    if (d.uint(0) !== 1) throw new CborError(`field ${String(k)}: unsupported hash algorithm`);
    const h = d.bytes(1, 32, 32);
    d.done();
    return h;
  }
  /** Optional key 23 `extensions` (never empty when present); accepted and bounded, not interpreted. */
  optExtensions() {
    if (!this.m.has(23)) return;
    const v = this.take(23);
    if (!(v instanceof Map) || v.size < 1 || v.size > 16) throw new CborError("bad extensions");
  }
  /** Rejects any key the caller did not read. */
  done() {
    for (const k of this.m.keys()) if (!this.used.has(k)) throw new CborError(`unknown field ${String(k)}`);
  }
};

// ../../packages/om-home-wire/src/errors.ts
var HOME_ERROR_CODES = {
  invalid_request: 1,
  unsupported_version: 2,
  unauthorized: 3,
  revoked: 4,
  expired: 5,
  not_found: 6,
  stale_generation: 7,
  invalid_signature: 8,
  quota: 9,
  rate_limited: 10,
  retryable: 11,
  moved: 12,
  payload_too_large: 13,
  policy_denied: 14,
  admission_denied: 15,
  spent_token: 16
};
var RETRYABLE = /* @__PURE__ */ new Set(["rate_limited", "retryable"]);
var NAME_OF = new Map(Object.entries(HOME_ERROR_CODES).map(([n, c]) => [c, n]));
var isRetryable = /* @__PURE__ */ __name((name) => RETRYABLE.has(name), "isRetryable");
var HTTP_STATUS = {
  invalid_request: 400,
  unsupported_version: 400,
  unauthorized: 401,
  revoked: 403,
  expired: 410,
  not_found: 404,
  stale_generation: 409,
  invalid_signature: 422,
  quota: 429,
  rate_limited: 429,
  retryable: 503,
  moved: 410,
  payload_too_large: 413,
  policy_denied: 403,
  admission_denied: 403,
  spent_token: 409
};
var HomeError = class extends Error {
  constructor(errorName, options = {}) {
    super(errorName);
    this.errorName = errorName;
    this.options = options;
    this.name = "HomeError";
  }
  static {
    __name(this, "HomeError");
  }
};
function quotaDetail(cls, hard) {
  return encode(map([[0, cls], [1, hard]]));
}
__name(quotaDetail, "quotaDetail");
function encodeHomeError(e) {
  const retryable = isRetryable(e.errorName);
  const retryAfter = retryable ? e.options.retryAfterSeconds : void 0;
  return encode(map([
    [0, API_MAJOR],
    [1, HOME_ERROR_CODES[e.errorName]],
    [2, e.options.hint],
    [3, retryAfter],
    [4, e.options.detail]
  ]));
}
__name(encodeHomeError, "encodeHomeError");

// ../../packages/om-home-wire/src/bodies.ts
var ID = 16;
var CAP_MIN = 16;
var CAP_MAX = 96;
function open(bytes) {
  const o = new Obj(decode(bytes));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  return o;
}
__name(open, "open");
function decodeDepositRequest(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const envelope = o.bytes(3, 1, 262144);
  const expiresAt = o.optUint(4);
  o.done();
  return { requestId, cap, envelope, ...expiresAt === void 0 ? {} : { expiresAt } };
}
__name(decodeDepositRequest, "decodeDepositRequest");
var encodeDepositReceipt = /* @__PURE__ */ __name((r) => encode(map([[0, API_MAJOR], [1, r.itemId], [2, r.seq]])), "encodeDepositReceipt");
function decodeDrainRequest(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const after = o.optUint(3);
  o.done();
  return { requestId, cap, ...after === void 0 ? {} : { after } };
}
__name(decodeDrainRequest, "decodeDrainRequest");
function encodeDrainResponse(r) {
  const items = r.items.map((i) => map([[0, i.id], [1, i.bytes], [2, i.seq]]));
  return encode(map([[0, API_MAJOR], [1, items], [2, r.next]]));
}
__name(encodeDrainResponse, "encodeDrainResponse");
function decodeAckRequest(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const itemIds = o.byteList(3, 1, 256, ID, ID);
  o.done();
  return { requestId, cap, itemIds };
}
__name(decodeAckRequest, "decodeAckRequest");
function decodeRotateRequest(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  o.done();
  return { requestId, cap };
}
__name(decodeRotateRequest, "decodeRotateRequest");
var encodeRotateResponse = /* @__PURE__ */ __name((r) => encode(map([[0, API_MAJOR], [1, r.receiveCap], [2, r.depositCap], [3, r.overlapUntil]])), "encodeRotateResponse");
function decodeMintRequest(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const label = o.bytes(2, 0, 64);
  const ttlSeconds = o.optUint(3);
  const maxItems = o.optUint(4);
  o.done();
  return { requestId, label, ...ttlSeconds === void 0 ? {} : { ttlSeconds }, ...maxItems === void 0 ? {} : { maxItems } };
}
__name(decodeMintRequest, "decodeMintRequest");
var encodeDepositCapabilityRecord = /* @__PURE__ */ __name((r) => encode(map([[0, API_MAJOR], [1, r.id], [2, r.cap], [3, r.expiresAt]])), "encodeDepositCapabilityRecord");
function decodeQueueCreate(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const receiveCap = o.bytes(2, CAP_MIN, CAP_MAX);
  const depositCap = o.bytes(3, CAP_MIN, CAP_MAX);
  const adminCap = o.optBytes(4, CAP_MIN, CAP_MAX);
  o.done();
  return { requestId, receiveCap, depositCap, ...adminCap === void 0 ? {} : { adminCap } };
}
__name(decodeQueueCreate, "decodeQueueCreate");
function decodeKeyFetchGrant(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const caps = o.byteList(2, 1, 8, CAP_MIN, CAP_MAX);
  o.done();
  return { requestId, caps };
}
__name(decodeKeyFetchGrant, "decodeKeyFetchGrant");
function decodeKeyFetch(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const cap = o.bytes(2, CAP_MIN, CAP_MAX);
  const devices = o.byteList(3, 1, 64, 16, 64);
  o.done();
  return { requestId, cap, devices };
}
__name(decodeKeyFetch, "decodeKeyFetch");
var encodeKeyFetchResponse = /* @__PURE__ */ __name((r) => encode(map([[0, API_MAJOR], [1, [...r.items]]])), "encodeKeyFetchResponse");
function decodeKeyFetchResponse(b) {
  const o = open(b);
  const items = o.byteList(1, 0, 400, 1, 262144);
  o.done();
  return { items };
}
__name(decodeKeyFetchResponse, "decodeKeyFetchResponse");
function decodeKeyPublish(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const bundle = o.bytes(2, 1, 65536);
  const opks = o.byteList(3, 0, 100, 1, 8192);
  const pqopks = o.byteList(4, 0, 100, 1, 8192);
  o.done();
  return { requestId, bundle, opks, pqopks };
}
__name(decodeKeyPublish, "decodeKeyPublish");
function decodeDescriptorPut(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const body2 = o.bytes(2, 1, 32768);
  const expiresAt = o.uint(3);
  o.done();
  return { requestId, body: body2, expiresAt };
}
__name(decodeDescriptorPut, "decodeDescriptorPut");
var encodeDescriptorGet = /* @__PURE__ */ __name((r) => encode(map([[0, API_MAJOR], [1, r.body], [2, r.expiresAt]])), "encodeDescriptorGet");
function decodeIntroTicketRequest(b) {
  const o = open(b);
  const requestId = o.bytes(1, ID, ID);
  const params = o.bytes(2, 1, 512);
  const count2 = o.optUint(3);
  o.done();
  return { requestId, params, ...count2 === void 0 ? {} : { count: count2 } };
}
__name(decodeIntroTicketRequest, "decodeIntroTicketRequest");
function decodeTicketParams(b) {
  const o = new Obj(decode(b));
  const ttlSeconds = o.uint(0);
  const maxUses = o.uint(1);
  const payloadLimitBytes = o.uint(2);
  const allowAttachments = o.optBool(3);
  const allowCalls = o.optBool(4);
  const ticketId = o.optBytes(5, 16, 16);
  o.done();
  return { ttlSeconds, maxUses, payloadLimitBytes, ...allowAttachments === void 0 ? {} : { allowAttachments }, ...allowCalls === void 0 ? {} : { allowCalls }, ...ticketId === void 0 ? {} : { ticketId } };
}
__name(decodeTicketParams, "decodeTicketParams");
var encodeIntroductionTicket = /* @__PURE__ */ __name((t) => encode(map([[0, 15], [1, 1], [2, t.ticketId], [3, t.serviceBinding], [4, t.expiresAt], [5, t.maxUses], [6, t.payloadLimitBytes], [7, t.allowAttachments], [8, t.allowCalls], [9, t.nonce]])), "encodeIntroductionTicket");
var encodeTicketList = /* @__PURE__ */ __name((tickets) => encode(map([[0, API_MAJOR], [1, tickets.map((t) => encodeIntroductionTicket(t))]])), "encodeTicketList");
var decodeIntroDrainRequest = decodeDrainRequest;
var WAKE_SUBPROTOCOL = "om.relay.wake.v1";
var WAKE_LABEL_PREFIX = "om.q.";
var WAKE_PING = "ping";
var WAKE_PONG = "pong";
var WAKE_CLOSE = { badFrame: 4400, unauthorized: 4401, authTimeout: 4408, superseded: 4409, tooMany: 4429 };
var WAKE_AUTH_SECONDS = 10;
var WAKE_FRAME_MAX = 256;
function parseWakeProtocols(header) {
  if (header === null) return void 0;
  const parts = header.split(",").map((x) => x.trim());
  if (!parts.includes(WAKE_SUBPROTOCOL)) return void 0;
  const labels = parts.filter((x) => x.startsWith(WAKE_LABEL_PREFIX));
  const label = labels[0];
  if (labels.length !== 1 || label === void 0) return void 0;
  const hex2 = label.slice(WAKE_LABEL_PREFIX.length);
  if (!/^[0-9a-f]{32}$/.test(hex2)) return void 0;
  const out = new Uint8Array(16);
  for (let i = 0; i < 16; i++) out[i] = parseInt(hex2.slice(2 * i, 2 * i + 2), 16);
  return { queueId: out };
}
__name(parseWakeProtocols, "parseWakeProtocols");
function decodeWakeAuth(b) {
  const o = open(b);
  const cap = o.bytes(1, CAP_MIN, CAP_MAX);
  o.done();
  return { cap };
}
__name(decodeWakeAuth, "decodeWakeAuth");
var encodeWakeEvent = /* @__PURE__ */ __name((e) => encode(map([[0, API_MAJOR], [1, e.kind === "ready" ? 1 : 2], [2, e.seq]])), "encodeWakeEvent");
var ADMISSION_HEADER = "om-admission";
async function admissionHash(secret) {
  const h = await sha256(utf82("OM-RELAY-ADMISSION-v1"), secret);
  return Array.from(h, (x) => x.toString(16).padStart(2, "0")).join("");
}
__name(admissionHash, "admissionHash");

// ../../packages/om-home-wire/src/sealed.ts
var PROFILE1_CIPHERTEXT_LENGTHS = [1040, 4112, 16400, 65552, 131088];
var PROFILE1_HPKE_ENC_LENGTH = 1120;
function parseDeposit(bytes) {
  const o = new Obj(decode(bytes));
  const type = o.uint(0);
  if (o.uint(1) !== 1) throw new CborError("unsupported object version");
  if (type === 23) {
    const cryptoProfile = o.uint(2);
    if (cryptoProfile < 1 || cryptoProfile > 65535) throw new CborError("crypto profile out of range");
    const deliveryKeyId = o.bytes(3, 16, 16);
    const deliveryId = o.bytes(4, 16, 16);
    const enc2 = o.bytes(5, 32, 2048);
    const ct = o.bytes(6, 17, 131088);
    const expiresAt = o.uint(7);
    o.optExtensions();
    o.done();
    if (cryptoProfile === 1 && (enc2.length !== PROFILE1_HPKE_ENC_LENGTH || !PROFILE1_CIPHERTEXT_LENGTHS.includes(ct.length))) throw new CborError("profile-1 lengths");
    return { kind: "sealed", cryptoProfile, deliveryKeyId, deliveryId, expiresAt };
  }
  if (type === 19) {
    const ticketId = o.bytes(2, 16, 16);
    o.digest(3);
    const profile = o.uint(4);
    if (profile < 1 || profile > 65535) throw new CborError("crypto profile out of range");
    o.bytes(5, 32, 2048);
    o.bytes(6, 17, 65552);
    o.optBytes(7, 1, 4096);
    o.optExtensions();
    o.done();
    return { kind: "message_request", ticketId };
  }
  throw new CborError("not a SealedDeliveryEnvelope or MessageRequest");
}
__name(parseDeposit, "parseDeposit");

// ../../packages/om-home-wire/src/bundle.ts
function readBundleFacts(bundle) {
  try {
    const outer = new Obj(decode(bundle));
    const body2 = outer.bytes(1, 1, 65536);
    const inner = decode(body2);
    if (!(inner instanceof Map)) return void 0;
    const m = inner;
    const [type, deviceId, generation, expiresAt] = [m.get(0), m.get(2), m.get(6), m.get(13)];
    if (type !== 9 || !(deviceId instanceof Uint8Array) || typeof generation !== "number" || typeof expiresAt !== "number") return void 0;
    return { deviceId, generation, expiresAt };
  } catch {
    return void 0;
  }
}
__name(readBundleFacts, "readBundleFacts");

// ../../packages/om-home-wire/src/blob.ts
var BLOB_CIPHERTEXT_LABEL = "OM-BLOB-CIPHERTEXT-HASH-v1";
var BLOB_DIGEST_HEADER = "om-ciphertext-digest";
var ID2 = 16;
var CAP_MIN2 = 32;
var CAP_MAX2 = 96;
var BLOB_WIRE_MAX = 16 * 1024 * 1024;
function bstrHead(n) {
  if (n < 24) return Uint8Array.of(64 | n);
  if (n < 256) return Uint8Array.of(88, n);
  if (n < 65536) return Uint8Array.of(89, n >> 8, n & 255);
  if (n < 4294967296) return Uint8Array.of(90, n >>> 24 & 255, n >>> 16 & 255, n >>> 8 & 255, n & 255);
  throw new CborError("blob too large");
}
__name(bstrHead, "bstrHead");
async function blobCiphertextDigest(ciphertext) {
  return sha256(Uint8Array.of(130), encode(BLOB_CIPHERTEXT_LABEL), bstrHead(ciphertext.length), ciphertext);
}
__name(blobCiphertextDigest, "blobCiphertextDigest");
function parseBlobDigestHeader(value) {
  if (value === null) return void 0;
  try {
    const d = fromBase64Url(value.trim());
    return d.length === 32 ? d : void 0;
  } catch {
    return void 0;
  }
}
__name(parseBlobDigestHeader, "parseBlobDigestHeader");
var blobLocatorPattern = /^[0-9a-f]{32}$/;
function decodeBlobPutRequest(b) {
  const o = new Obj(decode(b));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  const requestId = o.bytes(1, ID2, ID2);
  const cap = o.bytes(2, CAP_MIN2, CAP_MAX2);
  const ciphertext = o.bytes(3, 1, BLOB_WIRE_MAX);
  const retentionSeconds = o.optUint(4);
  o.done();
  return { requestId, cap, ciphertext, ...retentionSeconds === void 0 ? {} : { retentionSeconds } };
}
__name(decodeBlobPutRequest, "decodeBlobPutRequest");
var encodeBlobPutResponse = /* @__PURE__ */ __name((r) => encode(map([[0, API_MAJOR], [1, r.readCap], [2, r.digest], [3, r.expiresAt]])), "encodeBlobPutResponse");

// ../../packages/om-home-wire/src/usage.ts
var USAGE_FEATURE = "om.relay.usage.v1";
var USAGE_PATH = "/v1/relay/usage";

// ../../packages/om-home-wire/src/recovery.ts
var RECOVERY_FEATURE = "om.recovery.store.v1";
var RECOVERY_RECORD_MAX_BYTES = 1048576;
var RECOVERY_SIGNED_OBJECT_MAX_BYTES = 16384;
var RECOVERY_CAP_BYTES = 32;
var REQUEST_ID_HEADER = "om-request-id";
var ID3 = 16;
var LABEL_LOCATOR = "OM-RECOVERY-RECORD-LOCATOR-v1";
var LABEL_INBOX = "OM-RECOVERY-INBOX-ID-v1";
var LABEL_REQUEST = "OM-RECOVERY-REQUEST-HASH-v1";
function open2(bytes) {
  const o = new Obj(decode(bytes));
  if (o.uint(0) !== API_MAJOR) throw new CborError("unsupported api major");
  return o;
}
__name(open2, "open");
var dsHashValue = /* @__PURE__ */ __name(async (label, body2) => sha256(encode([label, body2])), "dsHashValue");
var recoveryLocator = /* @__PURE__ */ __name((readCap) => dsHashValue(LABEL_LOCATOR, readCap), "recoveryLocator");
var recoveryInboxId = /* @__PURE__ */ __name((accountId) => dsHashValue(LABEL_INBOX, accountId), "recoveryInboxId");
var recoveryRequestDigest = /* @__PURE__ */ __name((requestBody) => dsHashValue(LABEL_REQUEST, requestBody), "recoveryRequestDigest");
function decodeRecordPut(b) {
  const o = open2(b);
  const requestId = o.bytes(1, ID3, ID3);
  const envelope = o.bytes(2, 1, RECOVERY_RECORD_MAX_BYTES);
  o.done();
  return { requestId, envelope };
}
__name(decodeRecordPut, "decodeRecordPut");
var encodeRecordGetResponse = /* @__PURE__ */ __name((envelope) => encode(map([[0, API_MAJOR], [1, envelope]])), "encodeRecordGetResponse");
function checkRecordEnvelope(b) {
  if (b.length > RECOVERY_RECORD_MAX_BYTES) throw new CborError("envelope too large");
  const o = new Obj(decode(b));
  if (o.uint(0) !== 35 || o.uint(1) !== 1) throw new CborError("not a RecoveryRecordEnvelope");
  const profile = o.uint(2);
  if (profile < 1 || profile > 65535) throw new CborError("profile id");
  o.bytes(3, 12, 12);
  o.bytes(4, 16400, 1048016);
  o.optExtensions();
  o.done();
}
__name(checkRecordEnvelope, "checkRecordEnvelope");
function signedParts(bytes) {
  if (bytes.length < 1 || bytes.length > RECOVERY_SIGNED_OBJECT_MAX_BYTES) throw new CborError("signed object size");
  const o = new Obj(decode(bytes));
  const body2 = o.bytes(1, 1, RECOVERY_SIGNED_OBJECT_MAX_BYTES);
  const sigs = o.mapList(2, 16);
  if (sigs.length < 1) throw new CborError("no signatures");
  o.done();
  return body2;
}
__name(signedParts, "signedParts");
function checkSignedRequest(bytes) {
  const body2 = signedParts(bytes);
  const o = new Obj(decode(body2));
  if (o.uint(0) !== 4 || o.uint(1) !== 1) throw new CborError("not a RecoveryRequest");
  const accountId = o.bytes(2, 32, 32);
  o.uint(3);
  o.bytes(4, 16, 16);
  o.digest(5);
  o.digest(6);
  o.bytes(7, 16, 16);
  o.optExtensions();
  o.done();
  return { accountId, body: body2 };
}
__name(checkSignedRequest, "checkSignedRequest");
function checkSignedVeto(bytes) {
  const body2 = signedParts(bytes);
  const o = new Obj(decode(body2));
  if (o.uint(0) !== 5 || o.uint(1) !== 1) throw new CborError("not a RecoveryVeto");
  const accountId = o.bytes(2, 32, 32);
  o.uint(3);
  const requestDigest = o.digest(4);
  o.optExtensions();
  o.done();
  return { accountId, requestDigest, body: body2 };
}
__name(checkSignedVeto, "checkSignedVeto");
function decodeFanout(b) {
  const o = open2(b);
  const requestId = o.bytes(1, ID3, ID3);
  const signed = o.bytes(2, 1, RECOVERY_SIGNED_OBJECT_MAX_BYTES);
  o.done();
  return { requestId, signed };
}
__name(decodeFanout, "decodeFanout");
function decodeInboxRegister(b) {
  const o = open2(b);
  const requestId = o.bytes(1, ID3, ID3);
  const cap = o.bytes(2, 16, 96);
  o.done();
  return { requestId, cap };
}
__name(decodeInboxRegister, "decodeInboxRegister");
var INBOX_DRAIN_PAGE_MAX = 16;
function decodeInboxDrain(b) {
  const o = open2(b);
  const requestId = o.bytes(1, ID3, ID3);
  const cap = o.bytes(2, 16, 96);
  const pageSize = o.optUint(3);
  if (pageSize !== void 0 && (pageSize < 1 || pageSize > INBOX_DRAIN_PAGE_MAX)) throw new CborError("page size");
  o.done();
  return { requestId, cap, ...pageSize === void 0 ? {} : { pageSize } };
}
__name(decodeInboxDrain, "decodeInboxDrain");
function encodeInboxDrainResponse(r) {
  const items = r.items.map((i) => map([[0, i.id], [1, i.kind], [2, i.bytes]]));
  return encode(map([[0, API_MAJOR], [1, items], [2, r.cursor]]));
}
__name(encodeInboxDrainResponse, "encodeInboxDrainResponse");
function decodeInboxAck(b) {
  const o = open2(b);
  const requestId = o.bytes(1, ID3, ID3);
  const cap = o.bytes(2, 16, 96);
  const itemIds = o.byteList(3, 1, INBOX_DRAIN_PAGE_MAX, ID3, ID3);
  o.done();
  return { requestId, cap, itemIds };
}
__name(decodeInboxAck, "decodeInboxAck");
var encodeVetoList = /* @__PURE__ */ __name((vetoes) => encode(map([[0, API_MAJOR], [1, [...vetoes]]])), "encodeVetoList");

// ../../packages/om-home-wire/src/index.ts
var FEATURES = {
  relayQueues: "om.relay.queues.v1",
  relayWake: "om.relay.wake.v1",
  /** The Home lets only its owner create resources (queues, descriptors, key pools, intro services); see `admissionHash`. */
  admission: "om.admission.v1",
  keysPool: "om.keys.pool.v1",
  keysAnonymousFetch: "om.keys.fetch-throttled.v1",
  /** Fetching a device's bundle needs a capability its owner granted for one relationship (D4). */
  keysRelationshipCaps: "om.keys.relationship-caps.v1",
  discoveryPut: "om.discovery.put.v1",
  discoveryDelete: "om.discovery.delete.v1",
  introGate: "om.intro.v1",
  /** A bounded encrypted blob spool (put, get, delete by capability); offered only where the Home has owner admission (docs/reports/2026-11-04-attachments-home-status.md). */
  blobSpool: "om.blob.spool.v1",
  /** The usage read of the personal limits profile (admin capability; see usage.ts). */
  relayUsage: USAGE_FEATURE,
  /** RecoveryRecord store, recovery inbox and veto board (ADR-0201); offered only where the Home has owner admission. */
  recoveryStore: RECOVERY_FEATURE
};

// src/limits.ts
var DAY = 86400;
var LIMITS = {
  /** Largest SealedDeliveryEnvelope: 131088-byte ciphertext, 1120-byte HPKE enc and a few dozen bytes of framing. */
  maxDepositBytes: 133e3,
  maxQueueItems: 512,
  maxQueueBytes: 16 * 1024 * 1024,
  /** Retention bound; a sealed wrapper's own authenticated expiry may be shorter, never longer (SEALED_SENDER_DELIVERY_PROFILE section 7). */
  maxRetentionSeconds: 14 * DAY,
  defaultRetentionSeconds: 7 * DAY,
  /** spec/registries/om-time-fields.json default skew. */
  skewSeconds: 300,
  /** Per deposit capability token bucket. */
  depositBurst: 30,
  depositRefillPerSecond: 1,
  /** Wake channel (feature om.relay.wake.v1): sockets one queue holds at once, authenticated or not. */
  maxWakeSockets: 8,
  drainPageItems: 64,
  drainPageBytes: 1024 * 1024,
  maxDepositCapabilities: 64,
  /** Rows of the replay set and of the request_id idempotency table kept per queue (oldest dropped first): a hostile but valid depositor cannot grow storage without bound. */
  maxReplayRows: 8192,
  maxCapabilityRows: 512,
  /** request_id idempotency horizon across restarts (home-api.md section 5). */
  idempotencySeconds: 7 * DAY,
  rotateOverlapSeconds: DAY,
  rotateResultSeconds: 600,
  /**
   * Blob spool (feature om.blob.spool.v1), deliberately small: a free-plan Home keeps no card and no R2, so the spool is a
   * temporary hand-over area for small attachments, not storage. 32 MiB holds a few photos for a week; one object is at most
   * 4 MiB so a put stays well inside a Worker's 128 MB and a Durable Object's request budget. Larger files travel directly
   * between the two devices or through a BlobStore the person enables (STORAGE_AND_BLOB_ARCHITECTURE). Operational values, never protocol constants.
   */
  maxBlobSpoolBytes: 32 * 1024 * 1024,
  maxBlobBytes: 4 * 1024 * 1024,
  maxBlobs: 64,
  /** Key pool. */
  maxOneTimePerKind: 100,
  maxBundleBytes: 65536,
  /** Relationship fetch capabilities one device pool holds (one per accepted relationship). */
  maxFetchCapabilities: 256,
  fetchBurst: 8,
  fetchRefillSeconds: 600,
  /** Descriptor store. */
  maxDescriptorBytes: 32768,
  maxDescriptorSeconds: 30 * DAY,
  descriptorReadBurst: 20,
  descriptorReadRefillSeconds: 5,
  /** IntroGate. */
  maxTicketsPerService: 256,
  maxTicketUses: 16,
  maxTicketSeconds: 14 * DAY,
  maxTicketPayload: 65536,
  maxPendingRequests: 256,
  submitBurst: 20,
  submitRefillPerSecond: 2,
  /**
   * RecoveryStore (feature om.recovery.store.v1, ADR-0201, home-api.md section 8.4). Starting points scaled from the numbers above,
   * not measurements: one record per recovery lineage (a few at most), a put pace of the fetch refill, an inbox that holds a handful of
   * signed objects. Operational values, never protocol constants; advertised in ProviderCapabilities.limits.
   */
  maxRecoveryRecordBytes: 1048576,
  maxRecoveryRecords: 16,
  maxRecoveryInboxes: 16,
  recoveryPutBurst: 6,
  recoveryPutRefillSeconds: 600,
  recoveryGetBurst: 10,
  recoveryGetRefillSeconds: 30,
  /** Failed gets (wrong capability or unknown locator), whole Home: bounds guessing without per-locator state for locators that do not exist. */
  recoveryFailedGetBurst: 30,
  recoveryFailedGetRefillSeconds: 10,
  /** Per inbox. */
  recoveryRequestsPerDay: 3,
  recoveryPendingRequests: 8,
  recoveryPendingVetoes: 32,
  recoveryPendingVetoesPerDigest: 16,
  recoveryVetoesPerHour: 8,
  /** Fan-out and veto posts, whole Home (the only recovery limits that answer `rate_limited` to a poster). */
  recoveryPostBurst: 10,
  recoveryPostRefillSeconds: 60,
  recoveryDrainBurst: 20,
  recoveryDrainRefillSeconds: 5,
  recoveryVetoReadBurst: 10,
  recoveryVetoReadRefillSeconds: 10,
  /** A signed RecoveryRequest or RecoveryVeto (the registry maximum of object types 4 and 5). */
  maxRecoveryObjectBytes: 16384,
  recoveryRecordRetentionSeconds: 400 * DAY,
  recoveryInboxRetentionSeconds: 400 * DAY
};
var PERSONAL_LIMITS = {
  maxQueueItems: 128,
  maxQueueBytes: 8 * 1024 * 1024,
  maxDepositCapabilities: 16,
  maxBlobSpoolBytes: 16 * 1024 * 1024,
  maxBlobs: 32,
  maxRecoveryRecords: 4,
  maxRecoveryInboxes: 4
};
var USAGE_CONFIG = {
  workerRequestsPerDay: 1e5,
  durableObjectRequestsPerDay: 1e5,
  softRatio: 0.8,
  hardRatio: 0.95
};
var profileOf = /* @__PURE__ */ __name((env) => {
  const p = (env.OM_PROFILE ?? "").trim().toLowerCase();
  return p === "" || p === "default" ? "default" : "personal";
}, "profileOf");
var limitsOf = /* @__PURE__ */ __name((env) => profileOf(env) === "personal" ? { ...LIMITS, ...PERSONAL_LIMITS } : LIMITS, "limitsOf");

// src/usage.ts
import { DurableObject } from "cloudflare:workers";

// src/sql.ts
function all(sql, query, ...binds) {
  return sql.exec(query, ...binds).toArray();
}
__name(all, "all");
function one(sql, query, ...binds) {
  return all(sql, query, ...binds)[0];
}
__name(one, "one");
function run(sql, query, ...binds) {
  sql.exec(query, ...binds);
}
__name(run, "run");
function getMeta(sql, key) {
  return one(sql, "SELECT v FROM meta WHERE k = ?", key)?.v;
}
__name(getMeta, "getMeta");
function setMeta(sql, key, value) {
  run(sql, "INSERT INTO meta (k, v) VALUES (?, ?) ON CONFLICT(k) DO UPDATE SET v = excluded.v", key, value);
}
__name(setMeta, "setMeta");
function metaNumber(sql, key, fallback = 0) {
  const v = getMeta(sql, key);
  return typeof v === "number" ? v : fallback;
}
__name(metaNumber, "metaNumber");
function metaString(sql, key) {
  const v = getMeta(sql, key);
  return typeof v === "string" ? v : void 0;
}
__name(metaString, "metaString");

// src/util.ts
var nowSeconds = /* @__PURE__ */ __name(() => Math.floor(Date.now() / 1e3), "nowSeconds");
function randomBytes(n) {
  const b = new Uint8Array(n);
  crypto.getRandomValues(b);
  return b;
}
__name(randomBytes, "randomBytes");
async function capHash(cap) {
  return toHex(await sha256(cap));
}
__name(capHash, "capHash");
async function fingerprint(...parts) {
  return toHex(await sha256(...parts));
}
__name(fingerprint, "fingerprint");
async function reply(fn) {
  try {
    return { ok: true, body: await fn() };
  } catch (e) {
    if (e instanceof HomeError) {
      return {
        ok: false,
        error: e.errorName,
        ...e.options.retryAfterSeconds === void 0 ? {} : { retryAfterSeconds: e.options.retryAfterSeconds },
        ...e.options.detail === void 0 ? {} : { detail: e.options.detail }
      };
    }
    throw e;
  }
}
__name(reply, "reply");
function errorResponse(name, opts = {}) {
  return encodeHomeError(new HomeError(name, opts));
}
__name(errorResponse, "errorResponse");
function takeToken(tokens, at, now, burst, refillPerSecond) {
  const t = Math.min(burst, tokens + Math.max(0, now - at) * refillPerSecond);
  if (t >= 1) return { allowed: true, tokens: t - 1, waitSeconds: 0 };
  return { allowed: false, tokens: t, waitSeconds: Math.max(1, Math.ceil((1 - t) / refillPerSecond)) };
}
__name(takeToken, "takeToken");
var MAX_SAFE = Number.MAX_SAFE_INTEGER;

// src/usage.ts
var usageOn = /* @__PURE__ */ __name((env) => profileOf(env) === "personal" && env.USAGE !== void 0, "usageOn");
var ceilingOf = /* @__PURE__ */ __name((raw, fallback) => {
  const n = Number(raw);
  return raw !== void 0 && Number.isSafeInteger(n) && n > 0 ? n : fallback;
}, "ceilingOf");
var ceilings = /* @__PURE__ */ __name((env) => ({
  worker: ceilingOf(env.OM_USAGE_CEILING_WORKER, USAGE_CONFIG.workerRequestsPerDay),
  durable: ceilingOf(env.OM_USAGE_CEILING_DO, USAGE_CONFIG.durableObjectRequestsPerDay)
}), "ceilings");
function classify(s, c) {
  const ratio = Math.max(s.worker / c.worker, s.durable / c.durable);
  return ratio >= USAGE_CONFIG.hardRatio ? "hard" : ratio >= USAGE_CONFIG.softRatio ? "soft" : "healthy";
}
__name(classify, "classify");
function usageDocument(env, s) {
  const profile = profileOf(env);
  if (s === void 0) return { state: "unknown", profile };
  const c = ceilings(env);
  const metrics = [
    { name: "worker_requests", used: s.worker, ceiling: c.worker },
    { name: "durable_object_requests", used: s.durable, ceiling: c.durable }
  ];
  return { state: classify(s, c), billable_overage: "never", budget_remaining: 0, window_starts_at: s.day * DAY, resets_at: (s.day + 1) * DAY, metrics, profile };
}
__name(usageDocument, "usageDocument");
var UsageMeter = class extends DurableObject {
  static {
    __name(this, "UsageMeter");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec("CREATE TABLE IF NOT EXISTS usage_day (day INTEGER PRIMARY KEY, worker INTEGER NOT NULL, durable INTEGER NOT NULL)");
  }
  today() {
    return Math.floor(nowSeconds() / DAY);
  }
  snapshot(day) {
    const r = all(this.sql, "SELECT worker, durable FROM usage_day WHERE day = ?", day)[0];
    return { day, worker: r?.worker ?? 0, durable: r?.durable ?? 0 };
  }
  /** Adds one served request (and its Durable Object calls) to today's count and returns the count including it. */
  note(durableCalls) {
    const day = this.today();
    run(this.sql, "INSERT INTO usage_day (day, worker, durable) VALUES (?, 1, ?) ON CONFLICT(day) DO UPDATE SET worker = worker + 1, durable = durable + excluded.durable", day, durableCalls);
    run(this.sql, "DELETE FROM usage_day WHERE day < ?", day - 7);
    return this.snapshot(day);
  }
  /** Adds Durable Object calls a request made after it was noted (the per-device fan-out of a key fetch). Counts no Worker request. */
  charge(durableCalls) {
    if (!Number.isSafeInteger(durableCalls) || durableCalls <= 0) return;
    const day = this.today();
    run(this.sql, "INSERT INTO usage_day (day, worker, durable) VALUES (?, 0, ?) ON CONFLICT(day) DO UPDATE SET durable = durable + excluded.durable", day, durableCalls);
  }
};

// src/capabilities.ts
var wakeOn = /* @__PURE__ */ __name((env) => env.OM_WAKE !== "0", "wakeOn");
var blobsOn = /* @__PURE__ */ __name((env) => env.BLOBS !== void 0 && env.OM_BLOB_SPOOL !== "0" && admissionHashes(env).length > 0, "blobsOn");
var recoveryOn = /* @__PURE__ */ __name((env) => env.RECOVERY !== void 0 && env.OM_RECOVERY_STORE !== "0" && admissionHashes(env).length > 0, "recoveryOn");
var admissionHashes = /* @__PURE__ */ __name((env) => (env.OM_ADMISSION ?? "").split(",").map((h) => h.trim().toLowerCase()).filter((h) => /^[0-9a-f]{64}$/.test(h)), "admissionHashes");
var admissionConfigured = /* @__PURE__ */ __name((env) => (env.OM_ADMISSION ?? "").trim() !== "" || env.OM_ADMISSION_REQUIRED === "1", "admissionConfigured");
function capabilitiesDocument(env, recoveryScopeId) {
  const L = limitsOf(env);
  return {
    schema_version: 1,
    provider_id: env.OM_PROVIDER_ID ?? "om-reference-home",
    provider_kinds: ["relay", "key_material", "discovery_relay", "intro_gate", ...blobsOn(env) ? ["blob"] : [], ...recoveryOn(env) ? ["recovery"] : []],
    durability_class: "durable",
    features: Object.values(FEATURES).filter((f) => (f !== FEATURES.relayWake || wakeOn(env)) && (f !== FEATURES.admission || admissionHashes(env).length > 0) && (f !== FEATURES.blobSpool || blobsOn(env)) && (f !== FEATURES.relayUsage || usageOn(env)) && (f !== FEATURES.recoveryStore || recoveryOn(env))).sort(),
    api_versions: ["1"],
    health: "healthy",
    ...recoveryOn(env) && recoveryScopeId !== void 0 ? { recovery_scope_id: recoveryScopeId } : {},
    limits: {
      max_deposit_bytes: LIMITS.maxDepositBytes,
      max_queue_items: L.maxQueueItems,
      max_queue_bytes: L.maxQueueBytes,
      max_retention_seconds: LIMITS.maxRetentionSeconds,
      drain_page_items: LIMITS.drainPageItems,
      idempotency_horizon_seconds: LIMITS.idempotencySeconds,
      rotate_overlap_seconds: LIMITS.rotateOverlapSeconds,
      max_deposit_capabilities: L.maxDepositCapabilities,
      max_descriptor_bytes: LIMITS.maxDescriptorBytes,
      max_ticket_uses: LIMITS.maxTicketUses,
      ...blobsOn(env) ? { max_blob_spool_bytes: L.maxBlobSpoolBytes, max_blob_bytes: LIMITS.maxBlobBytes, max_blobs: L.maxBlobs, blob_default_retention_seconds: LIMITS.defaultRetentionSeconds } : {},
      ...recoveryOn(env) ? {
        max_recovery_record_bytes: L.maxRecoveryRecordBytes,
        max_recovery_records: L.maxRecoveryRecords,
        max_recovery_inboxes: L.maxRecoveryInboxes,
        recovery_put_rate: { burst: L.recoveryPutBurst, refill_seconds: L.recoveryPutRefillSeconds },
        recovery_get_rate: { burst: L.recoveryGetBurst, refill_seconds: L.recoveryGetRefillSeconds },
        recovery_failed_get_rate: { burst: L.recoveryFailedGetBurst, refill_seconds: L.recoveryFailedGetRefillSeconds },
        recovery_requests_per_day: L.recoveryRequestsPerDay,
        recovery_pending_requests: L.recoveryPendingRequests,
        recovery_pending_vetoes: L.recoveryPendingVetoes,
        recovery_pending_vetoes_per_request: L.recoveryPendingVetoesPerDigest,
        recovery_vetoes_per_hour: L.recoveryVetoesPerHour,
        recovery_post_rate: { burst: L.recoveryPostBurst, refill_seconds: L.recoveryPostRefillSeconds },
        recovery_drain_rate: { burst: L.recoveryDrainBurst, refill_seconds: L.recoveryDrainRefillSeconds },
        recovery_veto_read_rate: { burst: L.recoveryVetoReadBurst, refill_seconds: L.recoveryVetoReadRefillSeconds },
        max_recovery_object_bytes: L.maxRecoveryObjectBytes,
        recovery_item_retention_seconds: L.maxRetentionSeconds,
        recovery_record_retention_seconds: L.recoveryRecordRetentionSeconds,
        recovery_inbox_retention_seconds: L.recoveryInboxRetentionSeconds
      } : {},
      ...wakeOn(env) ? { wake_sockets_per_queue: LIMITS.maxWakeSockets, wake_auth_seconds: WAKE_AUTH_SECONDS } : {}
    },
    abuse_controls: {
      per_capability_deposit_rate: { burst: LIMITS.depositBurst, per_second: LIMITS.depositRefillPerSecond },
      one_time_prekey_fetch_throttle: { burst: LIMITS.fetchBurst, refill_seconds: LIMITS.fetchRefillSeconds },
      introgate_submit_rate: { burst: LIMITS.submitBurst, per_second: LIMITS.submitRefillPerSecond },
      ...blobsOn(env) ? { blob_put: "needs owner admission; the ciphertext digest is verified; bytes and count are bounded; the read capability is minted by the Home" } : {},
      object_shape_check: "deposits must be a SealedDeliveryEnvelope or a MessageRequest",
      replay: "exact-retry idempotent; a different object under the same (delivery_key_id, delivery_id) is rejected"
    },
    privacy_properties: {
      logs: "none: the Home keeps no request log and reads no network address",
      stores: "capability hashes, sealed ciphertext, signed public key material, opaque descriptors",
      never_stores: ["plaintext", "sender identity", "network address", "AccountID"]
    },
    security_properties: {
      authority: "none: a stored object is never account, device or relationship authority",
      bearer_capabilities: "carried in the request body or Authorization header only, never in a URL, cookie or log"
    },
    cost_state: { hard_free_tier_limits: env.OM_HARD_FREE_TIER === "1", ...profileOf(env) === "personal" ? { limits_profile: "personal" } : {} }
  };
}
__name(capabilitiesDocument, "capabilitiesDocument");

// src/http.ts
var CBOR = "application/cbor";
var BASE_HEADERS = {
  "Cache-Control": "no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Expose-Headers": "Retry-After, ETag",
  "Vary": "Origin"
};
var preflight = /* @__PURE__ */ __name(() => new Response(null, {
  status: 204,
  headers: { ...BASE_HEADERS, "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS", "Access-Control-Allow-Headers": "authorization, content-type, om-admission, om-ciphertext-digest, if-match, if-none-match, om-request-id", "Access-Control-Max-Age": "86400" }
}), "preflight");
function ok(body2, contentType = CBOR) {
  return new Response(body2.length === 0 ? null : body2, { status: body2.length === 0 ? 204 : 200, headers: { ...BASE_HEADERS, "Content-Type": contentType } });
}
__name(ok, "ok");
function okEtag(body2, etag, contentType = CBOR) {
  return new Response(body2.length === 0 ? null : body2, { status: body2.length === 0 ? 204 : 200, headers: { ...BASE_HEADERS, "Content-Type": contentType, ETag: `"${etag}"` } });
}
__name(okEtag, "okEtag");
function json(value) {
  return new Response(JSON.stringify(value), { status: 200, headers: { ...BASE_HEADERS, "Content-Type": "application/json", "Cache-Control": "public, max-age=300" } });
}
__name(json, "json");
function privateJson(value) {
  return new Response(JSON.stringify(value), { status: 200, headers: { ...BASE_HEADERS, "Content-Type": "application/json" } });
}
__name(privateJson, "privateJson");
function fail(name, opts = {}) {
  const headers = { ...BASE_HEADERS, "Content-Type": CBOR };
  if (opts.retryAfterSeconds !== void 0 && (name === "rate_limited" || name === "retryable")) headers["Retry-After"] = String(opts.retryAfterSeconds);
  return new Response(errorResponse(name, opts), { status: HTTP_STATUS[name], headers });
}
__name(fail, "fail");
function fromReply(r) {
  return r.ok ? ok(r.body) : fail(r.error, { ...r.retryAfterSeconds === void 0 ? {} : { retryAfterSeconds: r.retryAfterSeconds }, ...r.detail === void 0 ? {} : { detail: r.detail } });
}
__name(fromReply, "fromReply");
var DISCARD_BUDGET = 1024 * 1024;
async function readBody(request, max) {
  if (request.body === null) return new Uint8Array(0);
  const reader = request.body.getReader();
  const chunks = [];
  let total = 0;
  for (; ; ) {
    const { done, value } = await reader.read();
    if (done) break;
    total += value.length;
    if (total > max) {
      while (total < DISCARD_BUDGET) {
        const rest = await reader.read();
        if (rest.done) break;
        total += rest.value.length;
      }
      await reader.cancel();
      return void 0;
    }
    chunks.push(value);
  }
  const out = new Uint8Array(total);
  let o = 0;
  for (const c of chunks) {
    out.set(c, o);
    o += c.length;
  }
  return out;
}
__name(readBody, "readBody");
function bearer(request) {
  const h = request.headers.get("authorization");
  if (h === null || !h.startsWith("Bearer ")) return void 0;
  try {
    const cap = fromBase64Url(h.slice(7).trim());
    return cap.length >= 16 && cap.length <= 96 ? cap : void 0;
  } catch {
    return void 0;
  }
}
__name(bearer, "bearer");

// src/blobs.ts
import { DurableObject as DurableObject2 } from "cloudflare:workers";
var CHUNK = 1024 * 1024;
var SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS blobs (id BLOB PRIMARY KEY, owner_hash TEXT NOT NULL, read_hash TEXT NOT NULL, len INTEGER NOT NULL, chunks INTEGER NOT NULL, digest BLOB NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS chunks (id BLOB NOT NULL, idx INTEGER NOT NULL, bytes BLOB NOT NULL, PRIMARY KEY (id, idx));
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;
var BlobSpool = class extends DurableObject2 {
  static {
    __name(this, "BlobSpool");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
  }
  get hard() {
    return this.env.OM_HARD_FREE_TIER === "1";
  }
  /** The per-spool secret the read capabilities derive from; created on first use and never leaves this object. */
  readSecret() {
    const row = one(this.sql, "SELECT v FROM meta WHERE k = 'spool_key'");
    if (row?.v) return new Uint8Array(row.v);
    const key = randomBytes(32);
    run(this.sql, "INSERT INTO meta (k, v) VALUES ('spool_key', ?)", key);
    return key;
  }
  async readCapFor(id, ownerHash) {
    const k = await crypto.subtle.importKey("raw", this.readSecret(), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
    const mac = new Uint8Array(await crypto.subtle.sign("HMAC", k, concat(utf82("OM-BLOB-READ-CAP-v1"), id, utf82(ownerHash))));
    return concat(id, mac);
  }
  usage() {
    const r = one(this.sql, "SELECT COUNT(*) AS n, SUM(len) AS b FROM blobs");
    return { count: r?.n ?? 0, bytes: r?.b ?? 0 };
  }
  purge(now) {
    run(this.sql, "DELETE FROM chunks WHERE id IN (SELECT id FROM blobs WHERE expires_at + ? < ?)", LIMITS.skewSeconds, now);
    run(this.sql, "DELETE FROM blobs WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
    run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
  }
  async schedule() {
    const t = one(this.sql, "SELECT MIN(expires_at) + ? AS t FROM blobs", LIMITS.skewSeconds)?.t;
    if (t === null || t === void 0) {
      await this.ctx.storage.deleteAlarm();
      return;
    }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1e3, t * 1e3 + 1e3));
  }
  async alarm() {
    this.purge(nowSeconds());
    await this.schedule();
  }
  drop(id) {
    run(this.sql, "DELETE FROM chunks WHERE id = ?", id);
    run(this.sql, "DELETE FROM blobs WHERE id = ?", id);
  }
  async putResponse(row) {
    return encodeBlobPutResponse({ readCap: await this.readCapFor(new Uint8Array(row.id), row.owner_hash), digest: new Uint8Array(row.digest), expiresAt: row.expires_at });
  }
  /**
   * Stores one ciphertext. `expected` is the digest the sender computed over it (OM-BLOB-CIPHERTEXT-HASH-v1); the Home
   * recomputes and refuses a mismatch, so a truncated or altered upload is never stored. The same owner capability with the
   * same bytes is an exact retry and returns the original answer; anything else under an existing locator is `invalid_request`.
   */
  put(req, expected) {
    return reply(async () => {
      const len = req.ciphertext.length;
      if (len > LIMITS.maxBlobBytes) throw new HomeError("payload_too_large");
      let ttl = LIMITS.defaultRetentionSeconds;
      if (req.retentionSeconds !== void 0) {
        if (req.retentionSeconds < 1 || req.retentionSeconds > LIMITS.maxRetentionSeconds) throw new HomeError("invalid_request", { hint: "retention outside this Home's bound" });
        ttl = req.retentionSeconds;
      }
      const id = req.cap.slice(0, 16);
      const ownerHash = await capHash(req.cap);
      const fp = await fingerprint(req.cap, expected);
      const rid = await fingerprint(req.requestId);
      const seen = one(this.sql, "SELECT fp FROM ops WHERE request_id = ?", rid);
      if (seen && seen.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
      const now = nowSeconds();
      this.purge(now);
      const existing = one(this.sql, "SELECT * FROM blobs WHERE id = ?", id);
      if (existing) {
        if (existing.owner_hash === ownerHash && equalBytes(new Uint8Array(existing.digest), expected)) return this.putResponse(existing);
        throw new HomeError("invalid_request", { hint: "locator in use" });
      }
      const got = await blobCiphertextDigest(req.ciphertext);
      if (!equalBytes(got, expected)) throw new HomeError("invalid_request", { hint: "ciphertext digest mismatch" });
      const u = this.usage();
      if (u.count >= limitsOf(this.env).maxBlobs || u.bytes + len > limitsOf(this.env).maxBlobSpoolBytes) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      const readCap = await this.readCapFor(id, ownerHash);
      const row = { id, owner_hash: ownerHash, read_hash: await capHash(readCap), len, chunks: Math.ceil(len / CHUNK), digest: got, expires_at: now + ttl };
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "INSERT INTO blobs (id, owner_hash, read_hash, len, chunks, digest, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?)", row.id, row.owner_hash, row.read_hash, row.len, row.chunks, row.digest, row.expires_at);
        for (let i = 0; i < row.chunks; i++) run(this.sql, "INSERT INTO chunks (id, idx, bytes) VALUES (?, ?, ?)", id, i, req.ciphertext.slice(i * CHUNK, (i + 1) * CHUNK));
        run(this.sql, "INSERT OR REPLACE INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
        run(this.sql, "DELETE FROM ops WHERE rowid <= (SELECT MAX(rowid) FROM ops) - ?", LIMITS.maxReplayRows);
      });
      await this.schedule();
      return encodeBlobPutResponse({ readCap, digest: got, expiresAt: row.expires_at });
    });
  }
  /** The row a presented capability opens, and which kind it is. A wrong capability and an absent locator answer alike (`not_found`): no existence oracle. */
  async open(locator, cap) {
    this.purge(nowSeconds());
    const row = one(this.sql, "SELECT * FROM blobs WHERE id = ?", locator);
    const h = await capHash(cap);
    if (!row || !equalBytes(cap.slice(0, 16), locator)) throw new HomeError("not_found");
    const owner = row.owner_hash === h;
    if (!owner && row.read_hash !== h) throw new HomeError("not_found");
    return { row, owner };
  }
  /** The ciphertext, re-hashed against the digest stored at put: a blob that no longer matches is deleted and reported absent. */
  get(locator, cap) {
    return reply(async () => {
      const { row } = await this.open(locator, cap);
      if (nowSeconds() > row.expires_at) throw new HomeError("expired");
      const parts = all(this.sql, "SELECT bytes FROM chunks WHERE id = ? ORDER BY idx", locator).map((c) => new Uint8Array(c.bytes));
      const bytes = concat(...parts);
      if (bytes.length !== row.len || !equalBytes(await blobCiphertextDigest(bytes), new Uint8Array(row.digest))) {
        this.drop(locator);
        await this.schedule();
        throw new HomeError("not_found");
      }
      return bytes;
    });
  }
  /** Owner only. Deleting a blob that is already gone is success-equivalent (home-api: idempotent). */
  remove(locator, cap) {
    return reply(async () => {
      let opened;
      try {
        opened = await this.open(locator, cap);
      } catch (e) {
        if (e instanceof HomeError && e.errorName === "not_found") return new Uint8Array(0);
        throw e;
      }
      if (!opened.owner) throw new HomeError("unauthorized");
      this.drop(locator);
      await this.schedule();
      return new Uint8Array(0);
    });
  }
};

// src/descriptors.ts
import { DurableObject as DurableObject3 } from "cloudflare:workers";
var SCHEMA2 = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;
var DescriptorStore = class extends DurableObject3 {
  static {
    __name(this, "DescriptorStore");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA2);
  }
  async schedule() {
    const until = metaNumber(this.sql, "keep_until", 0);
    if (until > 0) await this.ctx.storage.setAlarm(until * 1e3 + LIMITS.skewSeconds * 1e3);
  }
  async alarm() {
    if (metaNumber(this.sql, "keep_until", 0) + LIMITS.skewSeconds <= nowSeconds()) {
      await this.ctx.storage.deleteAll();
    }
  }
  async auth(cap, claim) {
    const h = await capHash(cap);
    const owner = metaString(this.sql, "owner");
    if (owner === void 0) {
      if (!claim) throw new HomeError("unauthorized");
      setMeta(this.sql, "owner", h);
      return;
    }
    if (owner !== h) throw new HomeError("unauthorized");
  }
  put(cap, req) {
    return reply(async () => {
      const fp = await fingerprint(req.body, new TextEncoder().encode(String(req.expiresAt)));
      const rid = await fingerprint(req.requestId);
      const prior = one(this.sql, "SELECT fp FROM ops WHERE request_id = ?", rid);
      await this.auth(cap, true);
      if (metaNumber(this.sql, "revoked") === 1) throw new HomeError("revoked");
      if (prior) {
        if (prior.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
        return new Uint8Array(0);
      }
      const now = nowSeconds();
      if (req.body.length > LIMITS.maxDescriptorBytes) throw new HomeError("payload_too_large");
      if (req.expiresAt + LIMITS.skewSeconds < now) throw new HomeError("expired");
      const expiresAt = Math.min(req.expiresAt, now + LIMITS.maxDescriptorSeconds);
      setMeta(this.sql, "body", req.body);
      setMeta(this.sql, "expires_at", expiresAt);
      setMeta(this.sql, "keep_until", expiresAt);
      run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
      run(this.sql, "INSERT INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
      await this.schedule();
      return new Uint8Array(0);
    });
  }
  /** Public read. Missing, expired and revoked are all `not_found`, so a reader learns nothing about a revoked invite. */
  get() {
    return reply(() => {
      const now = nowSeconds();
      const t = takeToken(metaNumber(this.sql, "read_tokens", LIMITS.descriptorReadBurst), metaNumber(this.sql, "read_at", now), now, LIMITS.descriptorReadBurst, 1 / LIMITS.descriptorReadRefillSeconds);
      setMeta(this.sql, "read_tokens", t.tokens);
      setMeta(this.sql, "read_at", now);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      const body2 = one(this.sql, "SELECT v FROM meta WHERE k = 'body'")?.v;
      const expiresAt = metaNumber(this.sql, "expires_at");
      if (!body2 || metaNumber(this.sql, "revoked") === 1 || expiresAt + LIMITS.skewSeconds < now) throw new HomeError("not_found");
      return Promise.resolve(encodeDescriptorGet({ body: new Uint8Array(body2), expiresAt }));
    });
  }
  revoke(cap) {
    return reply(async () => {
      await this.auth(cap, true);
      setMeta(this.sql, "revoked", 1);
      run(this.sql, "DELETE FROM meta WHERE k = 'body'");
      if (metaNumber(this.sql, "keep_until", 0) === 0) setMeta(this.sql, "keep_until", nowSeconds() + LIMITS.maxDescriptorSeconds);
      await this.schedule();
      return new Uint8Array(0);
    });
  }
};

// src/intro.ts
import { DurableObject as DurableObject4 } from "cloudflare:workers";

// src/mailbox.ts
var MAILBOX_SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS items (seq INTEGER PRIMARY KEY, id BLOB NOT NULL, bytes BLOB NOT NULL, len INTEGER NOT NULL, fp TEXT NOT NULL, expires_at INTEGER NOT NULL, cap_hash TEXT);
CREATE UNIQUE INDEX IF NOT EXISTS items_id ON items (id);
CREATE TABLE IF NOT EXISTS seen (fp TEXT PRIMARY KEY, tuple_key TEXT, item_id BLOB NOT NULL, seq INTEGER NOT NULL, expires_at INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS seen_tuple ON seen (tuple_key) WHERE tuple_key IS NOT NULL;
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, result BLOB NOT NULL, expires_at INTEGER NOT NULL);
`;
function usage(sql) {
  const r = one(sql, "SELECT COUNT(*) AS n, SUM(len) AS b FROM items");
  return { items: r?.n ?? 0, bytes: r?.b ?? 0 };
}
__name(usage, "usage");
function purgeExpired(sql, now) {
  run(sql, "DELETE FROM items WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
  run(sql, "DELETE FROM seen WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
  run(sql, "DELETE FROM ops WHERE expires_at < ?", now);
}
__name(purgeExpired, "purgeExpired");
function nextExpiry(sql) {
  const r = one(sql, `SELECT MIN(t) AS t FROM (
    SELECT MIN(expires_at) + ${String(LIMITS.skewSeconds)} AS t FROM items UNION ALL SELECT MIN(expires_at) + ${String(LIMITS.skewSeconds)} FROM seen UNION ALL SELECT MIN(expires_at) FROM ops)`);
  return r?.t ?? void 0;
}
__name(nextExpiry, "nextExpiry");
function enqueue(sql, input) {
  const prior = one(sql, "SELECT item_id, seq FROM seen WHERE fp = ?", input.fp);
  if (prior) return { id: new Uint8Array(prior.item_id), seq: prior.seq, duplicate: true };
  if (input.tupleKey !== void 0 && one(sql, "SELECT 1 AS x FROM seen WHERE tuple_key = ?", input.tupleKey)) throw new HomeError("invalid_request", { hint: "replayed delivery id" });
  const u = usage(sql);
  if (u.items >= (input.maxItems ?? LIMITS.maxQueueItems)) throw new HomeError("quota", { detail: quotaDetail(1, input.hard) });
  if (u.bytes + input.bytes.length > (input.maxBytes ?? LIMITS.maxQueueBytes)) throw new HomeError("quota", { detail: quotaDetail(2, input.hard) });
  const seq = metaNumber(sql, "next_seq", 1);
  const id = new Uint8Array(16);
  crypto.getRandomValues(id);
  setMeta(sql, "next_seq", seq + 1);
  run(sql, "INSERT INTO items (seq, id, bytes, len, fp, expires_at, cap_hash) VALUES (?, ?, ?, ?, ?, ?, ?)", seq, id, input.bytes, input.bytes.length, input.fp, input.expiresAt, input.capHash ?? null);
  run(sql, "INSERT INTO seen (fp, tuple_key, item_id, seq, expires_at) VALUES (?, ?, ?, ?, ?)", input.fp, input.tupleKey ?? null, id, seq, input.expiresAt);
  run(sql, "DELETE FROM seen WHERE seq <= ?", seq - LIMITS.maxReplayRows);
  return { id, seq, duplicate: false };
}
__name(enqueue, "enqueue");
function page(sql, after, now) {
  const rows = all(
    sql,
    "SELECT seq, id, bytes, expires_at, len FROM items WHERE seq > ? AND expires_at + ? >= ? ORDER BY seq LIMIT ?",
    after,
    LIMITS.skewSeconds,
    now,
    LIMITS.drainPageItems + 1
  );
  const items = [];
  let bytes = 0;
  let more = false;
  for (const r of rows) {
    if (items.length >= LIMITS.drainPageItems || items.length > 0 && bytes + r.len > LIMITS.drainPageBytes) {
      more = true;
      break;
    }
    bytes += r.len;
    items.push({ seq: r.seq, id: new Uint8Array(r.id), bytes: new Uint8Array(r.bytes), expires_at: r.expires_at });
  }
  return { items, more };
}
__name(page, "page");
function ack(sql, ids) {
  let known = 0;
  for (const id of ids) {
    const had = one(sql, "SELECT 1 AS x FROM items WHERE id = ?", id) !== void 0;
    if (had) run(sql, "DELETE FROM items WHERE id = ?", id);
    if (had || one(sql, "SELECT 1 AS x FROM seen WHERE item_id = ?", id) !== void 0) known += 1;
  }
  return known;
}
__name(ack, "ack");
var cursorOf = /* @__PURE__ */ __name((seq) => {
  const b = new Uint8Array(8);
  new DataView(b.buffer).setBigUint64(0, BigInt(seq));
  return b;
}, "cursorOf");

// src/intro.ts
var SCHEMA3 = `${MAILBOX_SCHEMA}
CREATE TABLE IF NOT EXISTS tickets (id BLOB PRIMARY KEY, expires_at INTEGER NOT NULL, max_uses INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0, payload_limit INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS uses (ticket_id BLOB NOT NULL, fp TEXT NOT NULL, PRIMARY KEY (ticket_id, fp));
`;
var IntroGate = class extends DurableObject4 {
  static {
    __name(this, "IntroGate");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA3);
  }
  get hard() {
    return this.env.OM_HARD_FREE_TIER === "1";
  }
  async schedule() {
    const t = nextExpiry(this.sql);
    const tickets = one(this.sql, "SELECT MIN(expires_at) AS t FROM tickets")?.t ?? void 0;
    const soonest = [t, tickets].filter((x) => x !== void 0);
    if (soonest.length === 0) {
      await this.ctx.storage.deleteAlarm();
      return;
    }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1e3, Math.min(...soonest) * 1e3 + 1e3));
  }
  async alarm() {
    const now = nowSeconds();
    purgeExpired(this.sql, now);
    run(this.sql, "DELETE FROM uses WHERE ticket_id IN (SELECT id FROM tickets WHERE expires_at + ? < ?)", LIMITS.skewSeconds, now);
    run(this.sql, "DELETE FROM tickets WHERE expires_at + ? < ?", LIMITS.skewSeconds, now);
    await this.schedule();
  }
  /** Recipient authority over this service: first use claims it, every later use must present the same capability. */
  async serviceAuth(cap, claim) {
    const h = await capHash(cap);
    const owner = metaString(this.sql, "owner");
    if (owner === void 0) {
      if (!claim) throw new HomeError("unauthorized");
      setMeta(this.sql, "owner", h);
      return;
    }
    if (owner !== h) throw new HomeError("unauthorized");
  }
  issueTickets(cap, req) {
    return reply(async () => {
      let params;
      try {
        params = decodeTicketParams(req.params);
      } catch {
        throw new HomeError("invalid_request");
      }
      const count2 = req.count ?? 1;
      if (count2 < 1 || count2 > 16) throw new HomeError("invalid_request");
      if (params.ttlSeconds < 1 || params.ttlSeconds > LIMITS.maxTicketSeconds || params.maxUses < 1 || params.maxUses > LIMITS.maxTicketUses || params.payloadLimitBytes < 1 || params.payloadLimitBytes > LIMITS.maxTicketPayload) throw new HomeError("policy_denied");
      await this.serviceAuth(cap, true);
      const prefix = cap.slice(0, 8);
      const now = nowSeconds();
      const active = one(this.sql, "SELECT COUNT(*) AS n FROM tickets WHERE revoked = 0 AND expires_at + ? >= ?", LIMITS.skewSeconds, now)?.n ?? 0;
      const out = [];
      const binding = (await sha256(prefix)).slice(0, 16);
      if (params.ticketId && (count2 !== 1 || !params.ticketId.subarray(0, 8).every((b, i) => b === prefix[i]))) throw new HomeError("invalid_request");
      for (let i = 0; i < count2; i++) {
        const idTail = (await sha256(prefix, req.requestId, new Uint8Array([i]))).slice(0, 8);
        const id = params.ticketId ?? concat(prefix, idTail);
        const row = one(this.sql, "SELECT expires_at, max_uses, payload_limit FROM tickets WHERE id = ?", id);
        if (row && params.ticketId && (row.max_uses !== params.maxUses || row.payload_limit !== params.payloadLimitBytes)) throw new HomeError("invalid_request");
        if (!row) {
          if (active + out.length >= LIMITS.maxTicketsPerService) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
          run(this.sql, "INSERT INTO tickets (id, expires_at, max_uses, payload_limit) VALUES (?, ?, ?, ?)", id, now + params.ttlSeconds, params.maxUses, params.payloadLimitBytes);
        }
        out.push({
          ticketId: id,
          serviceBinding: binding,
          expiresAt: row?.expires_at ?? now + params.ttlSeconds,
          maxUses: row?.max_uses ?? params.maxUses,
          payloadLimitBytes: row?.payload_limit ?? params.payloadLimitBytes,
          nonce: (await sha256(id, new Uint8Array([1]))).slice(0, 16),
          ...params.allowAttachments === void 0 ? {} : { allowAttachments: params.allowAttachments },
          ...params.allowCalls === void 0 ? {} : { allowCalls: params.allowCalls }
        });
      }
      await this.schedule();
      return encodeTicketList(out);
    });
  }
  /** Revoke one ticket (idempotent). */
  revokeTicket(cap, ticketId) {
    return reply(async () => {
      await this.serviceAuth(cap, false);
      run(this.sql, "UPDATE tickets SET revoked = 1 WHERE id = ?", ticketId);
      return new Uint8Array(0);
    });
  }
  /** `POST /v1/intro/requests`: no credential beyond the ticket inside the MessageRequest. */
  submit(request) {
    return reply(async () => {
      const now = nowSeconds();
      const mr = (() => {
        try {
          return parseDeposit(request);
        } catch {
          return void 0;
        }
      })();
      if (mr?.kind !== "message_request") throw new HomeError("invalid_request");
      const ticket = one(
        this.sql,
        "SELECT expires_at, max_uses, used, payload_limit, revoked FROM tickets WHERE id = ?",
        mr.ticketId
      );
      if (!ticket || ticket.revoked) throw new HomeError("admission_denied");
      if (ticket.expires_at + LIMITS.skewSeconds < now) throw new HomeError("expired");
      const t = takeToken(metaNumber(this.sql, "tokens", LIMITS.submitBurst), metaNumber(this.sql, "tokens_at", now), now, LIMITS.submitBurst, LIMITS.submitRefillPerSecond);
      setMeta(this.sql, "tokens", t.tokens);
      setMeta(this.sql, "tokens_at", now);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      if (request.length > ticket.payload_limit + 1024) throw new HomeError("payload_too_large");
      const fp = await fingerprint(request);
      const retry = one(this.sql, "SELECT 1 AS x FROM uses WHERE ticket_id = ? AND fp = ?", mr.ticketId, fp) !== void 0;
      if (!retry && ticket.used >= ticket.max_uses) throw new HomeError("spent_token");
      purgeExpired(this.sql, now);
      const pending = one(this.sql, "SELECT COUNT(*) AS n FROM items")?.n ?? 0;
      if (!retry && pending >= LIMITS.maxPendingRequests) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
      const q = enqueue(this.sql, { bytes: request, fp, tupleKey: void 0, expiresAt: Math.min(ticket.expires_at, now + LIMITS.maxRetentionSeconds), capHash: void 0, hard: this.hard, maxItems: limitsOf(this.env).maxQueueItems, maxBytes: limitsOf(this.env).maxQueueBytes });
      if (!retry) {
        run(this.sql, "INSERT INTO uses (ticket_id, fp) VALUES (?, ?)", mr.ticketId, fp);
        run(this.sql, "UPDATE tickets SET used = used + 1 WHERE id = ?", mr.ticketId);
      }
      await this.schedule();
      return encodeDepositReceipt({ itemId: q.id, seq: q.seq });
    });
  }
  /** Drain accepted requests with sequence greater than `after`; items at or below `after` are acknowledged and deleted. */
  drain(req) {
    return reply(async () => {
      await this.serviceAuth(req.cap, false);
      const after = req.after ?? 0;
      run(this.sql, "DELETE FROM items WHERE seq <= ?", after);
      const p = page(this.sql, after, nowSeconds());
      const last = p.items.at(-1);
      await this.schedule();
      return encodeDrainResponse({ items: p.items.map((i) => ({ id: i.id, bytes: i.bytes, seq: i.seq })), ...p.more && last ? { next: cursorOf(last.seq) } : {} });
    });
  }
};

// src/keys.ts
import { DurableObject as DurableObject5 } from "cloudflare:workers";
var SCHEMA4 = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS prekeys (id INTEGER PRIMARY KEY AUTOINCREMENT, kind INTEGER NOT NULL, item BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS fetch_caps (h TEXT PRIMARY KEY, tokens REAL NOT NULL, tokens_at INTEGER NOT NULL);
`;
var KeyPool = class extends DurableObject5 {
  static {
    __name(this, "KeyPool");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA4);
    if (!all(this.sql, "SELECT name FROM pragma_table_info('prekeys')").some((c) => c.name === "gen")) {
      this.sql.exec("ALTER TABLE prekeys ADD COLUMN gen INTEGER NOT NULL DEFAULT 0");
    }
  }
  get hard() {
    return this.env.OM_HARD_FREE_TIER === "1";
  }
  /** key_publish_cap: the first publish for a DeviceID claims it; every later publish/revoke must present the same capability. */
  async publishAuth(cap, claim) {
    const h = await capHash(cap);
    const owner = metaString(this.sql, "owner");
    if (owner === void 0) {
      if (!claim) throw new HomeError("unauthorized");
      setMeta(this.sql, "owner", h);
      return;
    }
    if (owner !== h) throw new HomeError("unauthorized");
  }
  publish(cap, deviceId, req) {
    return reply(async () => {
      const fp = await fingerprint(req.bundle, ...req.opks, ...req.pqopks);
      const rid = await fingerprint(req.requestId);
      const prior = one(this.sql, "SELECT fp FROM ops WHERE request_id = ?", rid);
      await this.publishAuth(cap, true);
      if (prior) {
        if (prior.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
        return new Uint8Array(0);
      }
      const facts = readBundleFacts(req.bundle);
      const now = nowSeconds();
      let dropBelow = 0;
      if (facts) {
        if (facts.deviceId.length !== deviceId.length || facts.deviceId.some((b, i) => b !== deviceId[i])) throw new HomeError("invalid_request", { hint: "bundle is for another device" });
        if (facts.expiresAt + LIMITS.skewSeconds < now) throw new HomeError("expired");
        if (facts.generation < metaNumber(this.sql, "highest_generation")) throw new HomeError("stale_generation");
        if (facts.generation > metaNumber(this.sql, "highest_generation")) dropBelow = facts.generation;
        setMeta(this.sql, "highest_generation", facts.generation);
        setMeta(this.sql, "bundle_generation", facts.generation);
      }
      if (dropBelow > 0) run(this.sql, "DELETE FROM prekeys WHERE gen < ?", dropBelow);
      const gen = facts?.generation ?? 0;
      const ec = one(this.sql, "SELECT COUNT(*) AS n FROM prekeys WHERE kind = 1")?.n ?? 0;
      const pq = one(this.sql, "SELECT COUNT(*) AS n FROM prekeys WHERE kind = 2")?.n ?? 0;
      if (ec + req.opks.length > LIMITS.maxOneTimePerKind || pq + req.pqopks.length > LIMITS.maxOneTimePerKind) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      setMeta(this.sql, "bundle", req.bundle);
      for (const k of req.opks) run(this.sql, "INSERT INTO prekeys (kind, item, gen) VALUES (1, ?, ?)", k, gen);
      for (const k of req.pqopks) run(this.sql, "INSERT INTO prekeys (kind, item, gen) VALUES (2, ?, ?)", k, gen);
      run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
      run(this.sql, "INSERT INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
      return new Uint8Array(0);
    });
  }
  /**
   * `PUT /v1/keys/bundles/{device}/fetch-caps` (feature om.keys.relationship-caps.v1): the owner (holder of the device's
   * key_publish_cap) grants fetch capabilities, one per accepted relationship. Only hashes are kept. A retry grants the
   * same capability again, which changes nothing.
   */
  grantFetch(cap, caps) {
    return reply(async () => {
      await this.publishAuth(cap, false);
      const now = nowSeconds();
      const have = one(this.sql, "SELECT COUNT(*) AS n FROM fetch_caps")?.n ?? 0;
      for (const c of caps) {
        const h = await capHash(c);
        if (one(this.sql, "SELECT 1 AS x FROM fetch_caps WHERE h = ?", h)) continue;
        if (have + 1 > LIMITS.maxFetchCapabilities) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
        run(this.sql, "INSERT INTO fetch_caps (h, tokens, tokens_at) VALUES (?, ?, ?)", h, LIMITS.fetchBurst, now);
      }
      return new Uint8Array(0);
    });
  }
  /**
   * Returns `[bundle, one-time item?, pq one-time item?]` to the holder of a relationship fetch capability the owner
   * granted (ASYNC_KEY_MATERIAL 2.1); anyone else gets `unauthorized`. Consumption is throttled per capability and per
   * device, so one capability cannot drain the pool faster than it refills; a throttled fetch gets the signed bundle
   * without one-time items, which is the specified degraded (not weaker) handshake.
   */
  fetchItems(cap) {
    return reply(async () => {
      const h = await capHash(cap);
      const granted = one(this.sql, "SELECT tokens, tokens_at FROM fetch_caps WHERE h = ?", h);
      if (!granted) throw new HomeError("unauthorized");
      const bundle = getBundle(this.sql);
      if (!bundle) throw new HomeError("not_found");
      const now = nowSeconds();
      const items = [bundle];
      const refill = 1 / LIMITS.fetchRefillSeconds;
      const device = takeToken(metaNumber(this.sql, "tokens", LIMITS.fetchBurst), metaNumber(this.sql, "tokens_at", now), now, LIMITS.fetchBurst, refill);
      const held = takeToken(granted.tokens, granted.tokens_at, now, LIMITS.fetchBurst, refill);
      if (device.allowed && held.allowed) {
        for (const kind of [1, 2]) {
          const row = one(this.sql, "SELECT id, item FROM prekeys WHERE kind = ? ORDER BY id LIMIT 1", kind);
          if (!row) continue;
          run(this.sql, "DELETE FROM prekeys WHERE id = ?", row.id);
          items.push(new Uint8Array(row.item));
        }
        setMeta(this.sql, "tokens", device.tokens);
        setMeta(this.sql, "tokens_at", now);
        run(this.sql, "UPDATE fetch_caps SET tokens = ?, tokens_at = ? WHERE h = ?", held.tokens, now, h);
      }
      return encodeKeyFetchResponse({ items });
    });
  }
  /** Best-effort cleanup after rotation or compromise: idempotent, and never evidence that an older generation is safe (PC-K06). */
  revokeGeneration(cap, generation) {
    return reply(async () => {
      await this.publishAuth(cap, false);
      if (metaNumber(this.sql, "bundle_generation", -1) === generation) {
        run(this.sql, "DELETE FROM meta WHERE k = 'bundle'");
        run(this.sql, "DELETE FROM prekeys");
      }
      return new Uint8Array(0);
    });
  }
  inventory() {
    const rows = all(this.sql, "SELECT kind, COUNT(*) AS n FROM prekeys GROUP BY kind");
    return { ec: rows.find((r) => r.kind === 1)?.n ?? 0, pq: rows.find((r) => r.kind === 2)?.n ?? 0 };
  }
};
function getBundle(sql) {
  const r = one(sql, "SELECT v FROM meta WHERE k = 'bundle'");
  return r?.v ? new Uint8Array(r.v) : void 0;
}
__name(getBundle, "getBundle");

// src/queue.ts
import { DurableObject as DurableObject6 } from "cloudflare:workers";

// src/rules.ts
function depositCapStatus(cap, queue, now) {
  if (cap.revoked) return "revoked";
  if (cap.expiresAt !== null && cap.expiresAt + LIMITS.skewSeconds < now) return "expired";
  if (cap.generation < queue.current && queue.overlapUntil + LIMITS.skewSeconds < now) return "expired";
  return "ok";
}
__name(depositCapStatus, "depositCapStatus");
function receiveStatus(presented, current, previous, overlapUntil, now) {
  if (current !== void 0 && presented === current) return "current";
  if (previous !== void 0 && presented === previous) return overlapUntil + LIMITS.skewSeconds >= now ? "previous_in_overlap" : "previous_expired";
  return "unknown";
}
__name(receiveStatus, "receiveStatus");
function sealedExpiry(expiresAt, now) {
  if (expiresAt + LIMITS.skewSeconds < now) return { ok: false, error: "expired" };
  if (expiresAt > now + LIMITS.maxRetentionSeconds + LIMITS.skewSeconds) return { ok: false, error: "invalid_request" };
  return { ok: true, retainUntil: Math.min(expiresAt, now + LIMITS.maxRetentionSeconds) };
}
__name(sealedExpiry, "sealedExpiry");

// src/queue.ts
var SCHEMA5 = `${MAILBOX_SCHEMA}
CREATE TABLE IF NOT EXISTS dcaps (hash TEXT PRIMARY KEY, id BLOB NOT NULL, generation INTEGER NOT NULL, expires_at INTEGER, revoked INTEGER NOT NULL DEFAULT 0, max_items INTEGER, tokens REAL NOT NULL, tokens_at INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS dcaps_id ON dcaps (id);
`;
var MailboxQueue = class extends DurableObject6 {
  static {
    __name(this, "MailboxQueue");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA5);
    ctx.setWebSocketAutoResponse(new WebSocketRequestResponsePair(WAKE_PING, WAKE_PONG));
  }
  get hard() {
    return this.env.OM_HARD_FREE_TIER === "1";
  }
  exists() {
    return metaString(this.sql, "receive") !== void 0;
  }
  /** Idempotency: the same request_id and the same operation fingerprint returns the stored outcome; a different one is invalid_request. */
  async replay(requestId, fp) {
    const rid = await fingerprint(requestId);
    const r = one(this.sql, "SELECT fp, result FROM ops WHERE request_id = ?", rid);
    if (!r) return void 0;
    if (r.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
    return new Uint8Array(r.result);
  }
  async remember(requestId, fp, result, ttl) {
    const rid = await fingerprint(requestId);
    run(this.sql, "INSERT OR REPLACE INTO ops (request_id, fp, result, expires_at) VALUES (?, ?, ?, ?)", rid, fp, result, nowSeconds() + ttl);
    run(this.sql, "DELETE FROM ops WHERE rowid <= (SELECT MAX(rowid) FROM ops) - ?", LIMITS.maxReplayRows);
  }
  async schedule() {
    const t = nextExpiry(this.sql);
    if (t === void 0) {
      await this.ctx.storage.deleteAlarm();
      return;
    }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1e3, t * 1e3 + 1e3));
  }
  async alarm() {
    purgeExpired(this.sql, nowSeconds());
    run(this.sql, "DELETE FROM dcaps WHERE expires_at IS NOT NULL AND expires_at + ? < ? AND revoked = 0", LIMITS.skewSeconds, nowSeconds());
    await this.schedule();
  }
  // ---------------------------------------------------------------------------------------------------- create
  create(req) {
    return reply(async () => {
      const fp = await fingerprint(req.receiveCap, req.depositCap, req.adminCap ?? new Uint8Array(0));
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const [rh, dh, ah] = [await capHash(req.receiveCap), await capHash(req.depositCap), req.adminCap ? await capHash(req.adminCap) : void 0];
      if (this.exists()) {
        const same = metaString(this.sql, "receive") === rh && one(this.sql, "SELECT 1 AS x FROM dcaps WHERE hash = ? AND generation = 0", dh) !== void 0 && (metaString(this.sql, "admin") ?? void 0) === ah;
        if (!same) throw new HomeError("unauthorized");
      } else {
        setMeta(this.sql, "receive", rh);
        setMeta(this.sql, "generation", 0);
        if (ah !== void 0) setMeta(this.sql, "admin", ah);
        run(this.sql, "INSERT INTO dcaps (hash, id, generation, tokens, tokens_at) VALUES (?, ?, 0, ?, ?)", dh, randomBytes(16), LIMITS.depositBurst, nowSeconds());
      }
      const out = new Uint8Array(0);
      await this.remember(req.requestId, fp, out, LIMITS.idempotencySeconds);
      return out;
    });
  }
  // ---------------------------------------------------------------------------------------------------- deposit
  deposit(req) {
    return reply(async () => {
      const bytes = req.envelope;
      const fp = await fingerprint(req.cap, bytes);
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const now = nowSeconds();
      const full = /* @__PURE__ */ __name(() => new HomeError("quota", { detail: quotaDetail(1, this.hard) }), "full");
      if (!this.exists()) throw full();
      const cap = one(this.sql, "SELECT * FROM dcaps WHERE hash = ?", await capHash(req.cap));
      if (!cap) throw full();
      const status = depositCapStatus({ revoked: cap.revoked === 1, expiresAt: cap.expires_at, generation: cap.generation }, { current: metaNumber(this.sql, "generation"), overlapUntil: metaNumber(this.sql, "overlap_until") }, now);
      if (status !== "ok") throw new HomeError(status);
      if (bytes.length > LIMITS.maxDepositBytes) throw new HomeError("payload_too_large");
      let view;
      try {
        view = parseDeposit(bytes);
      } catch {
        throw new HomeError("invalid_request", { hint: "not a sealed delivery envelope or message request" });
      }
      let expiresAt = now + LIMITS.defaultRetentionSeconds;
      let tupleKey;
      if (view.kind === "sealed") {
        const e = sealedExpiry(view.expiresAt, now);
        if (!e.ok) throw new HomeError(e.error);
        expiresAt = e.retainUntil;
        tupleKey = `${await fingerprint(view.deliveryKeyId)}:${await fingerprint(view.deliveryId)}`;
      }
      const wrapperFp = await fingerprint(bytes);
      const known = one(this.sql, "SELECT 1 AS x FROM seen WHERE fp = ?", wrapperFp) !== void 0;
      if (!known) {
        if (cap.max_items !== null) {
          const n = one(this.sql, "SELECT COUNT(*) AS n FROM items WHERE cap_hash = ?", cap.hash)?.n ?? 0;
          if (n >= cap.max_items) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
        }
        const t = takeToken(cap.tokens, cap.tokens_at, now, LIMITS.depositBurst, LIMITS.depositRefillPerSecond);
        if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
        run(this.sql, "UPDATE dcaps SET tokens = ?, tokens_at = ? WHERE hash = ?", t.tokens, now, cap.hash);
      }
      purgeExpired(this.sql, now);
      const q = enqueue(this.sql, { bytes, fp: wrapperFp, tupleKey, expiresAt, capHash: cap.hash, hard: this.hard, maxItems: limitsOf(this.env).maxQueueItems, maxBytes: limitsOf(this.env).maxQueueBytes });
      if (!q.duplicate) this.wake(q.seq);
      const out = encodeDepositReceipt({ itemId: q.id, seq: q.seq });
      await this.remember(req.requestId, fp, out, LIMITS.idempotencySeconds);
      await this.schedule();
      return out;
    });
  }
  // ---------------------------------------------------------------------------------------------------- receive side
  /** Current receive capability always; the previous one only during the rotation overlap. */
  async receiveAuth(cap, forAck) {
    const h = await capHash(cap);
    if (!this.exists()) throw new HomeError("unauthorized");
    if (metaNumber(this.sql, "revoked") === 1) throw new HomeError("revoked");
    const s = receiveStatus(h, metaString(this.sql, "receive"), metaString(this.sql, "prev_receive"), metaNumber(this.sql, "overlap_until"), nowSeconds());
    if (s === "current" || s === "previous_in_overlap") return;
    if (s === "previous_expired") throw new HomeError(forAck ? "stale_generation" : "revoked");
    throw new HomeError("unauthorized");
  }
  drain(req) {
    return reply(async () => {
      await this.receiveAuth(req.cap, false);
      const now = nowSeconds();
      const p = page(this.sql, req.after ?? 0, now);
      const last = p.items.at(-1);
      return encodeDrainResponse({
        items: p.items.map((i) => ({ id: i.id, bytes: i.bytes, seq: i.seq })),
        ...p.more && last ? { next: cursorOf(last.seq) } : {}
      });
    });
  }
  ack(req) {
    return reply(async () => {
      const fp = await fingerprint(req.cap, ...req.itemIds);
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      await this.receiveAuth(req.cap, true);
      if (ack(this.sql, req.itemIds) === 0) throw new HomeError("not_found");
      const out = new Uint8Array(0);
      await this.remember(req.requestId, fp, out, LIMITS.idempotencySeconds);
      await this.schedule();
      return out;
    });
  }
  rotate(req) {
    return reply(async () => {
      const fp = await fingerprint(req.cap);
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const h = await capHash(req.cap);
      if (!this.exists() || metaString(this.sql, "receive") !== h) {
        if (this.exists() && metaString(this.sql, "prev_receive") === h) throw new HomeError("stale_generation");
        throw new HomeError(metaNumber(this.sql, "revoked") === 1 ? "revoked" : "unauthorized");
      }
      const prefix = req.cap.slice(0, 16);
      const gen = metaNumber(this.sql, "generation") + 1;
      const receiveCap = concat(prefix, randomBytes(32));
      const depositCap = concat(prefix, randomBytes(32));
      const overlapUntil = nowSeconds() + LIMITS.rotateOverlapSeconds;
      setMeta(this.sql, "prev_receive", h);
      setMeta(this.sql, "receive", await capHash(receiveCap));
      setMeta(this.sql, "generation", gen);
      setMeta(this.sql, "overlap_until", overlapUntil);
      run(this.sql, "INSERT INTO dcaps (hash, id, generation, tokens, tokens_at) VALUES (?, ?, ?, ?, ?)", await capHash(depositCap), randomBytes(16), gen, LIMITS.depositBurst, nowSeconds());
      this.closeWake(WAKE_CLOSE.superseded);
      const out = encodeRotateResponse({ receiveCap, depositCap, overlapUntil });
      await this.remember(req.requestId, fp, out, LIMITS.rotateResultSeconds);
      return out;
    });
  }
  // ---------------------------------------------------------------------------------------------------- wake (om.relay.wake.v1)
  //
  // A hibernatable WebSocket per connected client. The object sleeps between messages, so an idle queue costs nothing; a
  // deposit sends the awake sockets one small frame (the new sequence) and the client drains with the ordinary `drain`.
  // The frame carries no content. The only authority is the receive capability in the client's first frame.
  /** The Worker has already checked the upgrade and the routing label; this accepts the socket or refuses it with a typed answer. */
  fetch() {
    const sockets = this.ctx.getWebSockets();
    if (sockets.length >= LIMITS.maxWakeSockets) {
      const stale = sockets.map((s) => ({ s, st: this.wakeState(s) })).filter((x) => !x.st.authed).sort((a, b) => a.st.at - b.st.at)[0];
      if (!stale) return fail("rate_limited", { retryAfterSeconds: 30 });
      stale.s.close(WAKE_CLOSE.tooMany, "replaced");
    }
    const [client, server] = Object.values(new WebSocketPair());
    this.ctx.acceptWebSocket(server);
    server.serializeAttachment({ authed: false, at: Date.now() });
    return new Response(null, { status: 101, webSocket: client, headers: { "Sec-WebSocket-Protocol": WAKE_SUBPROTOCOL, "Cache-Control": "no-store" } });
  }
  wakeState(ws) {
    const a = ws.deserializeAttachment();
    return { authed: a?.authed === true, at: typeof a?.at === "number" ? a.at : 0 };
  }
  async webSocketMessage(ws, message) {
    const st = this.wakeState(ws);
    if (typeof message === "string" || message.byteLength > WAKE_FRAME_MAX || st.authed) {
      ws.close(WAKE_CLOSE.badFrame, "bad frame");
      return;
    }
    if (Date.now() > st.at + WAKE_AUTH_SECONDS * 1e3) {
      ws.close(WAKE_CLOSE.authTimeout, "too late");
      return;
    }
    let cap;
    try {
      cap = decodeWakeAuth(new Uint8Array(message)).cap;
    } catch (e) {
      if (e instanceof CborError) {
        ws.close(WAKE_CLOSE.badFrame, "bad frame");
        return;
      }
      throw e;
    }
    try {
      await this.receiveAuth(cap, false);
    } catch (e) {
      if (e instanceof HomeError) {
        ws.close(WAKE_CLOSE.unauthorized, "unauthorized");
        return;
      }
      throw e;
    }
    ws.serializeAttachment({ authed: true, at: st.at });
    ws.send(encodeWakeEvent({ kind: "ready", seq: metaNumber(this.sql, "next_seq", 1) - 1 }));
  }
  webSocketClose(ws, code) {
    try {
      ws.close(code >= 1e3 && code < 5e3 && code !== 1005 && code !== 1006 ? code : 1e3, "bye");
    } catch {
    }
  }
  webSocketError(ws) {
    try {
      ws.close(1011, "error");
    } catch {
    }
  }
  /** Tells every authenticated socket that item `seq` arrived. A socket that cannot be written to is dropped; the client falls back to its slow poll. */
  wake(seq) {
    const frame = encodeWakeEvent({ kind: "wake", seq });
    for (const ws of this.ctx.getWebSockets()) {
      if (!this.wakeState(ws).authed) continue;
      try {
        ws.send(frame);
      } catch {
        try {
          ws.close(1011, "error");
        } catch {
        }
      }
    }
  }
  closeWake(code) {
    for (const ws of this.ctx.getWebSockets()) {
      try {
        ws.close(code, "superseded");
      } catch {
      }
    }
  }
  // ---------------------------------------------------------------------------------------------------- admin
  async adminAuth(cap) {
    const admin = metaString(this.sql, "admin");
    if (!this.exists() || admin === void 0 || admin !== await capHash(cap)) throw new HomeError("unauthorized");
  }
  /** True admin proof only; used to gate reads that are for the queue's owner (the usage read). */
  adminCheck(adminCap) {
    return reply(async () => {
      await this.adminAuth(adminCap);
      return new Uint8Array(0);
    });
  }
  mint(adminCap, req) {
    return reply(async () => {
      await this.adminAuth(adminCap);
      const fp = await fingerprint(req.label, new TextEncoder().encode(`${String(req.ttlSeconds)}:${String(req.maxItems)}`));
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const count2 = one(this.sql, "SELECT COUNT(*) AS n FROM dcaps WHERE revoked = 0")?.n ?? 0;
      const rows = one(this.sql, "SELECT COUNT(*) AS n FROM dcaps")?.n ?? 0;
      if (count2 >= limitsOf(this.env).maxDepositCapabilities || rows >= LIMITS.maxCapabilityRows) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      const now = nowSeconds();
      const cap = concat(adminCapPrefix(adminCap), randomBytes(32));
      const id = randomBytes(16);
      const expiresAt = req.ttlSeconds === void 0 ? null : now + Math.min(req.ttlSeconds, LIMITS.maxRetentionSeconds * 4);
      run(
        this.sql,
        "INSERT INTO dcaps (hash, id, generation, expires_at, max_items, tokens, tokens_at) VALUES (?, ?, ?, ?, ?, ?, ?)",
        await capHash(cap),
        id,
        metaNumber(this.sql, "generation"),
        expiresAt,
        req.maxItems ?? null,
        LIMITS.depositBurst,
        now
      );
      const rec = { id, cap, ...expiresAt === null ? {} : { expiresAt } };
      const out = encodeDepositCapabilityRecord(rec);
      await this.remember(req.requestId, fp, out, LIMITS.rotateResultSeconds);
      return out;
    });
  }
  revokeDepositCapability(adminCap, id) {
    return reply(async () => {
      await this.adminAuth(adminCap);
      run(this.sql, "UPDATE dcaps SET revoked = 1 WHERE id = ?", id);
      return new Uint8Array(0);
    });
  }
};
var adminCapPrefix = /* @__PURE__ */ __name((cap) => cap.slice(0, 16), "adminCapPrefix");

// src/recovery.ts
import { DurableObject as DurableObject7 } from "cloudflare:workers";
var SCHEMA6 = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS records (locator BLOB PRIMARY KEY, write_hash BLOB NOT NULL, env BLOB NOT NULL, etag BLOB NOT NULL, len INTEGER NOT NULL, put_tokens REAL NOT NULL, put_at INTEGER NOT NULL, get_tokens REAL NOT NULL, get_at INTEGER NOT NULL, expires_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS inboxes (inbox_id BLOB PRIMARY KEY, cap_hash BLOB NOT NULL, expires_at INTEGER NOT NULL, drain_tokens REAL NOT NULL, drain_at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS items (seq INTEGER PRIMARY KEY AUTOINCREMENT, id BLOB NOT NULL UNIQUE, inbox_id BLOB NOT NULL, kind INTEGER NOT NULL, bytes BLOB NOT NULL, expires_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS items_inbox ON items (inbox_id, seq);
CREATE TABLE IF NOT EXISTS reqs (inbox_id BLOB NOT NULL, digest BLOB NOT NULL, body_hash BLOB NOT NULL, at INTEGER NOT NULL, expires_at INTEGER NOT NULL, read_tokens REAL NOT NULL, read_at INTEGER NOT NULL, PRIMARY KEY (inbox_id, digest));
CREATE INDEX IF NOT EXISTS reqs_digest ON reqs (digest);
CREATE TABLE IF NOT EXISTS vetoes (seq INTEGER PRIMARY KEY AUTOINCREMENT, inbox_id BLOB NOT NULL, digest BLOB NOT NULL, hash BLOB NOT NULL, bytes BLOB NOT NULL, expires_at INTEGER NOT NULL);
CREATE INDEX IF NOT EXISTS vetoes_digest ON vetoes (digest, seq);
CREATE TABLE IF NOT EXISTS veto_log (inbox_id BLOB NOT NULL, at INTEGER NOT NULL);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, result BLOB NOT NULL, expires_at INTEGER NOT NULL);
`;
var DAY_SECONDS = 86400;
var HOUR_SECONDS = 3600;
var EMPTY = new Uint8Array(0);
var bytesOf = /* @__PURE__ */ __name((v) => new Uint8Array(v), "bytesOf");
var RecoveryStore = class extends DurableObject7 {
  static {
    __name(this, "RecoveryStore");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA6);
  }
  get hard() {
    return this.env.OM_HARD_FREE_TIER === "1";
  }
  // ------------------------------------------------------------------------------------------------ housekeeping
  /** Drops what is past its retention. Records and inboxes go 400 days after their last replace or register; items and board rows after the mailbox retention. */
  purge(now) {
    const skew = LIMITS.skewSeconds;
    run(this.sql, "DELETE FROM items WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM vetoes WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM reqs WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM veto_log WHERE at < ?", now - HOUR_SECONDS);
    run(this.sql, "DELETE FROM records WHERE expires_at + ? < ?", skew, now);
    run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
    for (const gone of all(this.sql, "SELECT inbox_id FROM inboxes WHERE expires_at + ? < ?", skew, now)) {
      for (const t of ["items", "reqs", "vetoes", "veto_log", "inboxes"]) run(this.sql, `DELETE FROM ${t} WHERE inbox_id = ?`, gone.inbox_id);
    }
  }
  /** One alarm at the earliest moment something is due to be purged. (Separate queries: the Durable Object SQLite build limits the terms of a compound SELECT.) */
  async schedule() {
    let t;
    for (const table of ["items", "vetoes", "reqs", "records", "inboxes"]) {
      const r = one(this.sql, `SELECT MIN(expires_at) AS t FROM ${table}`)?.t;
      if (r !== null && r !== void 0) t = Math.min(t ?? Infinity, r + LIMITS.skewSeconds);
    }
    const ops = one(this.sql, "SELECT MIN(expires_at) AS t FROM ops")?.t;
    if (ops !== null && ops !== void 0) t = Math.min(t ?? Infinity, ops);
    if (t === void 0) {
      await this.ctx.storage.deleteAlarm();
      return;
    }
    await this.ctx.storage.setAlarm(Math.max(Date.now() + 1e3, t * 1e3 + 1e3));
  }
  async alarm() {
    this.purge(nowSeconds());
    await this.schedule();
  }
  /** The public scope id of this store (32 random bytes made on first use, stable for the life of the store). */
  scopeId() {
    const have = getMeta(this.sql, "scope_id");
    if (have !== void 0 && have !== null && typeof have !== "string" && typeof have !== "number") return Promise.resolve(new Uint8Array(have));
    const fresh = randomBytes(32);
    setMeta(this.sql, "scope_id", fresh);
    return Promise.resolve(fresh);
  }
  /** The determined outcome of an earlier request with this id, if any; the same id with other bytes is `invalid_request` (home-api.md section 5). */
  prior(rid, fp) {
    const p = one(this.sql, "SELECT fp, result FROM ops WHERE request_id = ?", rid);
    if (!p) return void 0;
    if (p.fp !== fp) throw new HomeError("invalid_request", { hint: "request_id reused with different operation" });
    return bytesOf(p.result);
  }
  remember(rid, fp, result, now) {
    run(this.sql, "INSERT OR REPLACE INTO ops (request_id, fp, result, expires_at) VALUES (?, ?, ?, ?)", rid, fp, result, now + LIMITS.idempotencySeconds);
    run(this.sql, "DELETE FROM ops WHERE rowid <= (SELECT MAX(rowid) FROM ops) - ?", LIMITS.maxReplayRows);
  }
  /** A Home-wide bucket kept in `meta`; throws `rate_limited` with the wait when empty. */
  homeBucket(name, now, burst, refillSeconds) {
    const t = takeToken(metaNumber(this.sql, `${name}_tokens`, burst), metaNumber(this.sql, `${name}_at`, now), now, burst, 1 / refillSeconds);
    setMeta(this.sql, `${name}_tokens`, t.tokens);
    setMeta(this.sql, `${name}_at`, now);
    if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
  }
  // ------------------------------------------------------------------------------------------------ records
  /**
   * Put (create or compare-and-swap replace). Returns the new ETag (32 bytes). A create needs the owner's admission (checked by the
   * Worker) and binds SHA-256(write_cap) to the locator; a replace proves the bound capability and `If-Match`. A capability that
   * does not match and a record that is not there are the same `unauthorized`.
   */
  putRecord(a) {
    return reply(async () => {
      const writeHash = await sha256(a.writeCap);
      const etag = await sha256(a.envelope);
      const rid = toHex(await sha256(a.requestId));
      const fp = toHex(await sha256(utf82("recovery-put"), a.locator, writeHash, etag, new Uint8Array([a.create ? 1 : 0]), a.ifMatch ?? EMPTY));
      const now = nowSeconds();
      this.purge(now);
      const L = limitsOf(this.env);
      if (a.envelope.length > L.maxRecoveryRecordBytes) throw new HomeError("payload_too_large");
      const row = one(this.sql, "SELECT * FROM records WHERE locator = ?", a.locator);
      const bound = row !== void 0 && equalBytes(bytesOf(row.write_hash), writeHash);
      if (a.create) {
        if (row) {
          if (!bound) throw new HomeError("unauthorized");
          const done3 = this.prior(rid, fp);
          if (done3) return done3;
          throw new HomeError("stale_generation");
        }
        const done2 = this.prior(rid, fp);
        if (done2) return done2;
        if ((one(this.sql, "SELECT COUNT(*) AS n FROM records")?.n ?? 0) >= L.maxRecoveryRecords) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
        this.ctx.storage.transactionSync(() => {
          run(
            this.sql,
            "INSERT INTO records (locator, write_hash, env, etag, len, put_tokens, put_at, get_tokens, get_at, expires_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            a.locator,
            writeHash,
            a.envelope,
            etag,
            a.envelope.length,
            LIMITS.recoveryPutBurst - 1,
            now,
            LIMITS.recoveryGetBurst,
            now,
            now + LIMITS.recoveryRecordRetentionSeconds
          );
          this.remember(rid, fp, etag, now);
        });
        await this.schedule();
        return etag;
      }
      if (!row || !bound) throw new HomeError("unauthorized");
      const done = this.prior(rid, fp);
      if (done) return done;
      if (a.ifMatch === void 0 || !equalBytes(bytesOf(row.etag), a.ifMatch)) throw new HomeError("stale_generation");
      const t = takeToken(row.put_tokens, row.put_at, now, LIMITS.recoveryPutBurst, 1 / LIMITS.recoveryPutRefillSeconds);
      if (!t.allowed) {
        run(this.sql, "UPDATE records SET put_tokens = ?, put_at = ? WHERE locator = ?", t.tokens, now, a.locator);
        throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      }
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "UPDATE records SET env = ?, etag = ?, len = ?, put_tokens = ?, put_at = ?, expires_at = ? WHERE locator = ?", a.envelope, etag, a.envelope.length, t.tokens, now, now + LIMITS.recoveryRecordRetentionSeconds, a.locator);
        this.remember(rid, fp, etag, now);
      });
      await this.schedule();
      return etag;
    });
  }
  /**
   * Get. Returns `etag (32 bytes) || RecoveryRecordGetResponse`. The read capability is proved by DS-HASH(read_cap) = locator. A wrong
   * capability, an unknown locator and a locator whose record is not there are one answer (`not_found`), and each spends from the
   * Home-wide failed-get bucket, so guessing is bounded without any per-locator state for locators that do not exist.
   */
  getRecord(locator, readCap) {
    return reply(async () => {
      const derived = await recoveryLocator(readCap);
      const now = nowSeconds();
      this.purge(now);
      const match = equalBytes(derived, locator);
      const row = match ? one(this.sql, "SELECT * FROM records WHERE locator = ?", locator) : void 0;
      if (!row) {
        this.homeBucket("failed_get", now, LIMITS.recoveryFailedGetBurst, LIMITS.recoveryFailedGetRefillSeconds);
        throw new HomeError("not_found");
      }
      const t = takeToken(row.get_tokens, row.get_at, now, LIMITS.recoveryGetBurst, 1 / LIMITS.recoveryGetRefillSeconds);
      run(this.sql, "UPDATE records SET get_tokens = ?, get_at = ? WHERE locator = ?", t.tokens, now, locator);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      return concat(bytesOf(row.etag), encodeRecordGetResponse(bytesOf(row.env)));
    });
  }
  /** Delete; idempotent. A wrong capability and an absent record answer alike (`not_found`, which a client treats as success). */
  deleteRecord(locator, writeCap) {
    return reply(async () => {
      const writeHash = await sha256(writeCap);
      const now = nowSeconds();
      this.purge(now);
      const row = one(this.sql, "SELECT write_hash FROM records WHERE locator = ?", locator);
      if (!row || !equalBytes(bytesOf(row.write_hash), writeHash)) throw new HomeError("not_found");
      run(this.sql, "DELETE FROM records WHERE locator = ?", locator);
      await this.schedule();
      return EMPTY;
    });
  }
  // ------------------------------------------------------------------------------------------------ inbox
  /** Registers (or refreshes) the inbox of one account. The Worker has required the owner's admission. A different capability for a registered inbox is `unauthorized`. */
  registerInbox(inboxId, cap, requestId) {
    return reply(async () => {
      const capHash2 = await sha256(cap);
      const rid = toHex(await sha256(requestId));
      const fp = toHex(await sha256(utf82("recovery-register"), inboxId, capHash2));
      const now = nowSeconds();
      this.purge(now);
      const L = limitsOf(this.env);
      const row = one(this.sql, "SELECT * FROM inboxes WHERE inbox_id = ?", inboxId);
      if (row) {
        if (!equalBytes(bytesOf(row.cap_hash), capHash2)) throw new HomeError("unauthorized");
        if (this.prior(rid, fp)) return EMPTY;
        this.remember(rid, fp, EMPTY, now);
        run(this.sql, "UPDATE inboxes SET expires_at = ? WHERE inbox_id = ?", now + LIMITS.recoveryInboxRetentionSeconds, inboxId);
        await this.schedule();
        return EMPTY;
      }
      if (this.prior(rid, fp)) return EMPTY;
      if ((one(this.sql, "SELECT COUNT(*) AS n FROM inboxes")?.n ?? 0) >= L.maxRecoveryInboxes) throw new HomeError("quota", { detail: quotaDetail(1, this.hard) });
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "INSERT INTO inboxes (inbox_id, cap_hash, expires_at, drain_tokens, drain_at) VALUES (?, ?, ?, ?, ?)", inboxId, capHash2, now + LIMITS.recoveryInboxRetentionSeconds, LIMITS.recoveryDrainBurst, now);
        this.remember(rid, fp, EMPTY, now);
      });
      await this.schedule();
      return EMPTY;
    });
  }
  /** The inbox a capability opens; an absent inbox and a wrong capability are the same `unauthorized`. */
  async authInbox(inboxId, cap) {
    const capHash2 = await sha256(cap);
    const row = one(this.sql, "SELECT * FROM inboxes WHERE inbox_id = ?", inboxId);
    const same = equalBytes(row ? bytesOf(row.cap_hash) : new Uint8Array(32), capHash2);
    if (!row || !same) throw new HomeError("unauthorized");
    return row;
  }
  /** Items in arrival order, `{item_id, kind, bytes}` and no timestamp; ack removes. A cursor (the last sequence, 8 bytes) is present only when more items wait. */
  drain(inboxId, cap, pageSize) {
    return reply(async () => {
      const now = nowSeconds();
      this.purge(now);
      const row = await this.authInbox(inboxId, cap);
      const t = takeToken(row.drain_tokens, row.drain_at, now, LIMITS.recoveryDrainBurst, 1 / LIMITS.recoveryDrainRefillSeconds);
      run(this.sql, "UPDATE inboxes SET drain_tokens = ?, drain_at = ? WHERE inbox_id = ?", t.tokens, now, inboxId);
      if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      const n = Math.min(pageSize ?? INBOX_DRAIN_PAGE_MAX, INBOX_DRAIN_PAGE_MAX);
      const rows = all(this.sql, "SELECT seq, id, kind, bytes FROM items WHERE inbox_id = ? ORDER BY seq LIMIT ?", inboxId, n + 1);
      const page2 = rows.slice(0, n);
      const items = page2.map((r) => ({ id: bytesOf(r.id), kind: r.kind === 2 ? 2 : 1, bytes: bytesOf(r.bytes) }));
      const last = page2[page2.length - 1];
      let cursor;
      if (rows.length > n && last) {
        cursor = new Uint8Array(8);
        new DataView(cursor.buffer).setBigUint64(0, BigInt(last.seq));
      }
      return encodeInboxDrainResponse({ items, ...cursor ? { cursor } : {} });
    });
  }
  /** Removes the named items; an id that is not there is success-equivalent. */
  ack(inboxId, cap, ids) {
    return reply(async () => {
      this.purge(nowSeconds());
      await this.authInbox(inboxId, cap);
      for (const id of ids) run(this.sql, "DELETE FROM items WHERE inbox_id = ? AND id = ?", inboxId, id);
      await this.schedule();
      return EMPTY;
    });
  }
  // ------------------------------------------------------------------------------------------------ fan-out and veto (shape-checked posts)
  /**
   * Appends a signed request to the inbox of its account when one is registered. Always `Empty`: the answer is the same whether or not
   * an inbox exists, and when a per-inbox limit or a full inbox drops the item. Only the Home-wide post bucket answers `rate_limited`.
   */
  fanout(a) {
    return reply(async () => {
      const bodyHash = await sha256(a.body);
      const digest = await recoveryRequestDigest(a.body);
      const inboxId = await recoveryInboxId(a.accountId);
      const rid = toHex(await sha256(a.requestId));
      const fp = toHex(await sha256(utf82("recovery-fanout"), bodyHash));
      const now = nowSeconds();
      this.purge(now);
      if (this.prior(rid, fp)) return EMPTY;
      this.homeBucket("post", now, LIMITS.recoveryPostBurst, LIMITS.recoveryPostRefillSeconds);
      this.remember(rid, fp, EMPTY, now);
      if (!one(this.sql, "SELECT 1 AS x FROM inboxes WHERE inbox_id = ?", inboxId)) return EMPTY;
      if (one(this.sql, "SELECT 1 AS x FROM reqs WHERE inbox_id = ? AND body_hash = ?", inboxId, bodyHash)) return EMPTY;
      if ((one(this.sql, "SELECT COUNT(*) AS n FROM reqs WHERE inbox_id = ? AND at > ?", inboxId, now - DAY_SECONDS)?.n ?? 0) >= LIMITS.recoveryRequestsPerDay) return EMPTY;
      if ((one(this.sql, "SELECT COUNT(*) AS n FROM items WHERE inbox_id = ? AND kind = 1", inboxId)?.n ?? 0) >= LIMITS.recoveryPendingRequests) return EMPTY;
      const expires = now + LIMITS.maxRetentionSeconds;
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "INSERT INTO reqs (inbox_id, digest, body_hash, at, expires_at, read_tokens, read_at) VALUES (?, ?, ?, ?, ?, ?, ?)", inboxId, digest, bodyHash, now, expires, LIMITS.recoveryVetoReadBurst, now);
        run(this.sql, "INSERT INTO items (id, inbox_id, kind, bytes, expires_at) VALUES (?, ?, 1, ?, ?)", randomBytes(16), inboxId, a.signed, expires);
      });
      await this.schedule();
      return EMPTY;
    });
  }
  /**
   * Accepts a veto only when the inbox of its account holds, or has held within the item retention and since had acknowledged, a
   * request with that digest (the digest is kept after an ack so a controller that drained a request can still veto it). Otherwise it is
   * dropped and answered like a success.
   */
  postVeto(a) {
    return reply(async () => {
      const hash = await sha256(a.body);
      const inboxId = await recoveryInboxId(a.accountId);
      const rid = toHex(await sha256(a.requestId));
      const fp = toHex(await sha256(utf82("recovery-veto"), hash));
      const now = nowSeconds();
      this.purge(now);
      if (this.prior(rid, fp)) return EMPTY;
      this.homeBucket("post", now, LIMITS.recoveryPostBurst, LIMITS.recoveryPostRefillSeconds);
      this.remember(rid, fp, EMPTY, now);
      if (!one(this.sql, "SELECT 1 AS x FROM inboxes WHERE inbox_id = ?", inboxId)) return EMPTY;
      if (!one(this.sql, "SELECT 1 AS x FROM reqs WHERE inbox_id = ? AND digest = ?", inboxId, a.requestDigest)) return EMPTY;
      if (one(this.sql, "SELECT 1 AS x FROM vetoes WHERE digest = ? AND hash = ?", a.requestDigest, hash)) return EMPTY;
      const n = /* @__PURE__ */ __name((q, ...b) => one(this.sql, q, ...b)?.n ?? 0, "n");
      if (n("SELECT COUNT(*) AS n FROM veto_log WHERE inbox_id = ? AND at > ?", inboxId, now - HOUR_SECONDS) >= LIMITS.recoveryVetoesPerHour) return EMPTY;
      if (n("SELECT COUNT(*) AS n FROM vetoes WHERE digest = ?", a.requestDigest) >= LIMITS.recoveryPendingVetoesPerDigest) return EMPTY;
      if (n("SELECT COUNT(*) AS n FROM vetoes WHERE inbox_id = ?", inboxId) >= LIMITS.recoveryPendingVetoes) return EMPTY;
      if (n("SELECT COUNT(*) AS n FROM items WHERE inbox_id = ? AND kind = 2", inboxId) >= LIMITS.recoveryPendingVetoes) return EMPTY;
      const expires = now + LIMITS.maxRetentionSeconds;
      this.ctx.storage.transactionSync(() => {
        run(this.sql, "INSERT INTO veto_log (inbox_id, at) VALUES (?, ?)", inboxId, now);
        run(this.sql, "INSERT INTO vetoes (inbox_id, digest, hash, bytes, expires_at) VALUES (?, ?, ?, ?, ?)", inboxId, a.requestDigest, hash, a.signed, expires);
        run(this.sql, "INSERT INTO items (id, inbox_id, kind, bytes, expires_at) VALUES (?, ?, 2, ?, ?)", randomBytes(16), inboxId, a.signed, expires);
      });
      await this.schedule();
      return EMPTY;
    });
  }
  /**
   * The veto board: the vetoes stored for one request digest, at most 16, to anyone who knows the digest (public_read). The bucket is
   * kept per digest only for a digest an inbox has taken, so an arbitrary digest creates no state; an unknown digest and a known one
   * without vetoes both answer an empty list.
   */
  vetoes(digest) {
    return reply(() => {
      const now = nowSeconds();
      this.purge(now);
      const row = one(this.sql, "SELECT inbox_id, read_tokens, read_at FROM reqs WHERE digest = ? LIMIT 1", digest);
      if (row) {
        const t = takeToken(row.read_tokens, row.read_at, now, LIMITS.recoveryVetoReadBurst, 1 / LIMITS.recoveryVetoReadRefillSeconds);
        run(this.sql, "UPDATE reqs SET read_tokens = ?, read_at = ? WHERE inbox_id = ? AND digest = ?", t.tokens, now, row.inbox_id, digest);
        if (!t.allowed) throw new HomeError("rate_limited", { retryAfterSeconds: t.waitSeconds });
      }
      const list = all(this.sql, "SELECT bytes FROM vetoes WHERE digest = ? ORDER BY seq LIMIT ?", digest, LIMITS.recoveryPendingVetoesPerDigest).map((r) => bytesOf(r.bytes));
      return Promise.resolve(encodeVetoList(list));
    });
  }
};

// src/index.ts
var hex = /* @__PURE__ */ __name((b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join(""), "hex");
var queueOf = /* @__PURE__ */ __name((env, cap) => env.QUEUES.get(env.QUEUES.idFromName(`q:${hex(cap.slice(0, 16))}`)), "queueOf");
var introOf = /* @__PURE__ */ __name((env, cap) => env.INTRO.get(env.INTRO.idFromName(`i:${hex(cap.slice(0, 8))}`)), "introOf");
var BODY_MAX = LIMITS.maxDepositBytes + 2048;
var BLOB_BODY_MAX = LIMITS.maxBlobBytes + 2048;
var spoolOf = /* @__PURE__ */ __name((env) => env.BLOBS?.get(env.BLOBS.idFromName("spool")), "spoolOf");
var recoveryOf = /* @__PURE__ */ __name((env) => env.RECOVERY?.get(env.RECOVERY.idFromName("store")), "recoveryOf");
var recoveryScope;
async function scopeOf(env) {
  if (!recoveryOn(env)) return void 0;
  if (recoveryScope !== void 0) return recoveryScope;
  const store = recoveryOf(env);
  if (!store) return void 0;
  try {
    recoveryScope = toBase64Url(await store.scopeId());
    await chargeCalls(env, 1);
  } catch {
  }
  return recoveryScope;
}
__name(scopeOf, "scopeOf");
var wellFormed = /* @__PURE__ */ __name((cap, prefix) => cap.length >= prefix + 16 && cap.length <= 96, "wellFormed");
var Bad = class extends Error {
  constructor(code) {
    super(code);
    this.code = code;
  }
  static {
    __name(this, "Bad");
  }
};
async function body(request, max) {
  if (request.method !== "POST" && request.method !== "PUT") throw new Bad("invalid_request");
  const b = await readBody(request, max);
  if (b === void 0) throw new Bad("payload_too_large");
  return b;
}
__name(body, "body");
function parse(f, b) {
  try {
    return f(b);
  } catch (e) {
    if (e instanceof CborError) throw new Bad("invalid_request");
    throw e;
  }
}
__name(parse, "parse");
function sameHex(a, b) {
  if (a.length !== b.length) return false;
  let d = 0;
  for (let i = 0; i < a.length; i++) d |= a.charCodeAt(i) ^ b.charCodeAt(i);
  return d === 0;
}
__name(sameHex, "sameHex");
async function admitted(request, env) {
  const hashes = admissionHashes(env);
  if (hashes.length === 0) return !admissionConfigured(env);
  const h = request.headers.get(ADMISSION_HEADER);
  if (h === null) return false;
  let secret;
  try {
    secret = fromBase64Url(h.trim());
  } catch {
    return false;
  }
  if (secret.length !== 32) return false;
  const got = await admissionHash(secret);
  let ok2 = false;
  for (const want of hashes) ok2 = sameHex(got, want) || ok2;
  return ok2;
}
__name(admitted, "admitted");
var KEY_FETCH_PATH = "/v1/keys/fetch";
var routedCalls = /* @__PURE__ */ __name((request, path) => request.method !== "OPTIONS" && path.startsWith("/v1/") && path !== KEY_FETCH_PATH ? 1 : 0, "routedCalls");
async function chargeCalls(env, made) {
  if (made <= 0 || !usageOn(env) || env.USAGE === void 0) return;
  try {
    await env.USAGE.get(env.USAGE.idFromName("meter")).charge(made + 1);
  } catch {
  }
}
__name(chargeCalls, "chargeCalls");
function addsWork(request, path) {
  return request.method === "POST" && (path === "/v1/relay/queues" || path === "/v1/relay/deposit" || path === "/v1/intro/requests" || path === "/v1/blobs");
}
__name(addsWork, "addsWork");
async function count(request, env) {
  if (!usageOn(env) || env.USAGE === void 0) return void 0;
  try {
    const snapshot = await env.USAGE.get(env.USAGE.idFromName("meter")).note(1 + routedCalls(request, new URL(request.url).pathname));
    return { state: classify(snapshot, ceilings(env)), snapshot };
  } catch {
    return { state: "unknown", snapshot: void 0 };
  }
}
__name(count, "count");
async function route(request, env, counted) {
  const url = new URL(request.url);
  const path = url.pathname;
  const m = request.method;
  if (m === "OPTIONS") return preflight();
  if (path === "/.well-known/om-provider-capabilities") return m === "GET" ? json(capabilitiesDocument(env, await scopeOf(env))) : fail("invalid_request");
  if (counted?.state === "hard" && addsWork(request, path)) return fail("quota", { detail: quotaDetail(3, true) });
  if (path === USAGE_PATH) {
    if (!usageOn(env)) return fail("not_found");
    if (m !== "GET") return fail("invalid_request");
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    const r = await queueOf(env, admin).adminCheck(admin);
    return r.ok ? privateJson(usageDocument(env, counted?.snapshot)) : fromReply(r);
  }
  if (path === "/" || path === "/healthz") return m === "GET" ? ok(new TextEncoder().encode("OM reference Home\n"), "text/plain; charset=utf-8") : fail("invalid_request");
  if (path === "/v1/relay/queues") {
    if (!await admitted(request, env)) return fail("unauthorized");
    const req = parse(decodeQueueCreate, await body(request, 1024));
    if (![req.receiveCap, req.depositCap, ...req.adminCap ? [req.adminCap] : []].every((c) => wellFormed(c, 16))) return fail("invalid_request");
    const prefix = hex(req.receiveCap.slice(0, 16));
    if (hex(req.depositCap.slice(0, 16)) !== prefix || req.adminCap && hex(req.adminCap.slice(0, 16)) !== prefix) return fail("invalid_request");
    return fromReply(await queueOf(env, req.receiveCap).create(req));
  }
  if (path === "/v1/relay/wake") {
    if (!wakeOn(env)) return fail("not_found");
    if (m !== "GET" || request.headers.get("upgrade")?.toLowerCase() !== "websocket") return fail("invalid_request");
    const label = parseWakeProtocols(request.headers.get("sec-websocket-protocol"));
    if (!label) return fail("invalid_request");
    return env.QUEUES.get(env.QUEUES.idFromName(`q:${hex(label.queueId)}`)).fetch(request);
  }
  if (path === "/v1/relay/deposit") {
    const req = parse(decodeDepositRequest, await body(request, BODY_MAX));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).deposit(req));
  }
  if (path === "/v1/relay/drain") {
    const req = parse(decodeDrainRequest, await body(request, 1024));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).drain(req));
  }
  if (path === "/v1/relay/ack") {
    const req = parse(decodeAckRequest, await body(request, 8192));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).ack(req));
  }
  if (path === "/v1/relay/rotate") {
    const req = parse(decodeRotateRequest, await body(request, 1024));
    return fromReply(await queueOf(env, wellFormed(req.cap, 16) ? req.cap : new Uint8Array(16)).rotate(req));
  }
  if (path === "/v1/relay/deposit-capabilities") {
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).mint(admin, parse(decodeMintRequest, await body(request, 1024))));
  }
  const revokeMatch = /^\/v1\/relay\/deposit-capabilities\/([0-9a-f]{32})$/.exec(path);
  if (revokeMatch) {
    if (m !== "DELETE") return fail("invalid_request");
    const admin = bearer(request);
    if (!admin || !wellFormed(admin, 16)) return fail("unauthorized");
    return fromReply(await queueOf(env, admin).revokeDepositCapability(admin, fromHex(revokeMatch[1] ?? "")));
  }
  if (path === "/v1/blobs" || path.startsWith("/v1/blobs/")) {
    const spool = blobsOn(env) ? spoolOf(env) : void 0;
    if (!spool) return fail("not_found");
    if (path === "/v1/blobs") {
      if (m !== "POST") return fail("invalid_request");
      if (!await admitted(request, env)) return fail("unauthorized");
      const expected = parseBlobDigestHeader(request.headers.get(BLOB_DIGEST_HEADER));
      if (!expected) return fail("invalid_request");
      const req = parse(decodeBlobPutRequest, await body(request, BLOB_BODY_MAX));
      if (!wellFormed(req.cap, 16)) return fail("invalid_request");
      return fromReply(await spool.put(req, expected));
    }
    const locator = path.slice("/v1/blobs/".length);
    if (!blobLocatorPattern.test(locator)) return fail("not_found");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if (m === "GET") {
      const r = await spool.get(fromHex(locator), cap);
      return r.ok ? ok(r.body, "application/octet-stream") : fromReply(r);
    }
    if (m === "DELETE") return fromReply(await spool.remove(fromHex(locator), cap));
    return fail("invalid_request");
  }
  if (path.startsWith("/v1/recovery/")) {
    const store = recoveryOn(env) ? recoveryOf(env) : void 0;
    if (!store) return fail("not_found");
    return recovery(request, path, store, env);
  }
  const desc = /^\/v1\/discovery\/descriptors\/([0-9a-f]{32})$/.exec(path);
  if (desc) {
    const stub = env.DESCRIPTORS.get(env.DESCRIPTORS.idFromName(`d:${desc[1] ?? ""}`));
    if (m === "GET") return fromReply(await stub.get());
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if ((m === "PUT" || m === "DELETE") && !await admitted(request, env)) return fail("unauthorized");
    if (m === "PUT") return fromReply(await stub.put(cap, parse(decodeDescriptorPut, await body(request, LIMITS.maxDescriptorBytes + 1024))));
    if (m === "DELETE") return fromReply(await stub.revoke(cap));
    return fail("invalid_request");
  }
  const bundle = /^\/v1\/keys\/bundles\/([0-9a-f]{32})$/.exec(path);
  if (bundle && m === "PUT") {
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    if (!await admitted(request, env)) return fail("unauthorized");
    const device = fromHex(bundle[1] ?? "");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${bundle[1] ?? ""}`)).publish(cap, device, parse(decodeKeyPublish, await body(request, 4 * LIMITS.maxBundleBytes))));
  }
  const grant = /^\/v1\/keys\/bundles\/([0-9a-f]{32})\/fetch-caps$/.exec(path);
  if (grant) {
    if (m !== "PUT") return fail("invalid_request");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    const req = parse(decodeKeyFetchGrant, await body(request, 2048));
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${grant[1] ?? ""}`)).grantFetch(cap, req.caps));
  }
  const gen = /^\/v1\/keys\/bundles\/([0-9a-f]{32})\/generations\/(\d{1,15})$/.exec(path);
  if (gen) {
    if (m !== "DELETE") return fail("invalid_request");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${gen[1] ?? ""}`)).revokeGeneration(cap, Number(gen[2])));
  }
  if (path === KEY_FETCH_PATH) {
    const req = parse(decodeKeyFetch, await body(request, 4096));
    const items = [];
    let made = 0;
    try {
      for (const d of req.devices) {
        if (d.length !== 16) return fail("invalid_request");
        made++;
        const r = await env.KEYS.get(env.KEYS.idFromName(`k:${hex(d)}`)).fetchItems(req.cap);
        if (!r.ok) return fromReply(r);
        items.push(...decodeKeyFetchResponse(r.body).items);
      }
      return ok(encodeKeyFetchResponse({ items }));
    } finally {
      await chargeCalls(env, made);
    }
  }
  if (path === "/v1/intro/tickets") {
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 8) || !await admitted(request, env)) return fail("unauthorized");
    return fromReply(await introOf(env, cap).issueTickets(cap, parse(decodeIntroTicketRequest, await body(request, 1024))));
  }
  const ticket = /^\/v1\/intro\/tickets\/([0-9a-f]{32})$/.exec(path);
  if (ticket && m === "DELETE") {
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 8) || !await admitted(request, env)) return fail("unauthorized");
    return fromReply(await introOf(env, cap).revokeTicket(cap, fromHex(ticket[1] ?? "")));
  }
  if (path === "/v1/intro/requests") {
    const b = await body(request, LIMITS.maxTicketPayload + 4096);
    const prefix = ticketPrefix(b);
    if (!prefix) return fail("invalid_request");
    return fromReply(await env.INTRO.get(env.INTRO.idFromName(`i:${hex(prefix)}`)).submit(b));
  }
  if (path === "/v1/intro/drain") {
    const req = parse(decodeIntroDrainRequest, await body(request, 1024));
    if (!wellFormed(req.cap, 8)) return fail("unauthorized");
    return fromReply(await introOf(env, req.cap).drain(req));
  }
  return fail("not_found");
}
__name(route, "route");
function id32(s) {
  if (!/^[A-Za-z0-9_-]{43}$/.test(s)) return void 0;
  try {
    const b = fromBase64Url(s);
    return b.length === 32 && toBase64Url(b) === s ? b : void 0;
  } catch {
    return void 0;
  }
}
__name(id32, "id32");
var storeCap = /* @__PURE__ */ __name((request) => {
  const c = bearer(request);
  return c !== void 0 && c.length === RECOVERY_CAP_BYTES ? c : void 0;
}, "storeCap");
async function boundedBody(request, payloadMax) {
  const b = await body(request, payloadMax + 64);
  if (b.length > payloadMax + 27) throw new Bad("payload_too_large");
  return b;
}
__name(boundedBody, "boundedBody");
function parseEtag(h) {
  const m = /^"([A-Za-z0-9_-]{43})"$/.exec(h.trim());
  return m?.[1] === void 0 ? void 0 : id32(m[1]);
}
__name(parseEtag, "parseEtag");
function requestIdHeaderOk(request) {
  const h = request.headers.get(REQUEST_ID_HEADER);
  if (h === null) return true;
  try {
    return fromBase64Url(h.trim()).length === 16;
  } catch {
    return false;
  }
}
__name(requestIdHeaderOk, "requestIdHeaderOk");
async function recovery(request, path, store, env) {
  const m = request.method;
  const rec = /^\/v1\/recovery\/records\/([^/]+)$/.exec(path);
  if (rec) {
    const locator = id32(rec[1] ?? "");
    if (!locator) return fail("not_found");
    if (m === "GET") {
      const cap = storeCap(request);
      if (!cap) return fail("not_found");
      if (!requestIdHeaderOk(request)) return fail("invalid_request");
      const r = await store.getRecord(locator, cap);
      return r.ok ? okEtag(r.body.slice(32), toBase64Url(r.body.slice(0, 32))) : fromReply(r);
    }
    if (m === "PUT") {
      const cap = storeCap(request);
      if (!cap) return fail("unauthorized");
      const inm = request.headers.get("if-none-match");
      const im = request.headers.get("if-match");
      if (inm === null === (im === null)) return fail("invalid_request");
      let ifMatch;
      if (inm !== null) {
        if (inm.trim() !== "*") return fail("invalid_request");
        if (!await admitted(request, env)) return fail("admission_denied");
      } else {
        ifMatch = parseEtag(im ?? "");
        if (!ifMatch) return fail("invalid_request");
      }
      const req = parse(decodeRecordPut, await boundedBody(request, RECOVERY_RECORD_MAX_BYTES));
      parse(checkRecordEnvelope, req.envelope);
      const r = await store.putRecord({ locator, writeCap: cap, requestId: req.requestId, envelope: req.envelope, create: inm !== null, ...ifMatch ? { ifMatch } : {} });
      return r.ok ? okEtag(new Uint8Array(0), toBase64Url(r.body)) : fromReply(r);
    }
    if (m === "DELETE") {
      const cap = storeCap(request);
      return cap ? fromReply(await store.deleteRecord(locator, cap)) : fail("not_found");
    }
    return fail("invalid_request");
  }
  if (path === "/v1/recovery/requests") {
    if (m !== "POST") return fail("invalid_request");
    const req = parse(decodeFanout, await boundedBody(request, RECOVERY_SIGNED_OBJECT_MAX_BYTES));
    const shape = parse(checkSignedRequest, req.signed);
    return fromReply(await store.fanout({ requestId: req.requestId, signed: req.signed, accountId: shape.accountId, body: shape.body }));
  }
  if (path === "/v1/recovery/vetoes") {
    if (m !== "POST") return fail("invalid_request");
    const req = parse(decodeFanout, await boundedBody(request, RECOVERY_SIGNED_OBJECT_MAX_BYTES));
    const shape = parse(checkSignedVeto, req.signed);
    return fromReply(await store.postVeto({ requestId: req.requestId, signed: req.signed, accountId: shape.accountId, requestDigest: shape.requestDigest, body: shape.body }));
  }
  const board = /^\/v1\/recovery\/requests\/([^/]+)\/vetoes$/.exec(path);
  if (board) {
    if (m !== "GET") return fail("invalid_request");
    const digest = id32(board[1] ?? "");
    if (!digest) return fail("not_found");
    const r = await store.vetoes(digest);
    return r.ok ? ok(r.body) : fromReply(r);
  }
  const inbox = /^\/v1\/recovery\/inbox\/([^/]+?)(\/drain|\/ack)?$/.exec(path);
  if (inbox) {
    const inboxId = id32(inbox[1] ?? "");
    if (!inboxId) return fail("not_found");
    const sub = inbox[2];
    if (sub === void 0) {
      if (m !== "PUT") return fail("invalid_request");
      if (!await admitted(request, env)) return fail("admission_denied");
      const req2 = parse(decodeInboxRegister, await body(request, 512));
      if (req2.cap.length !== RECOVERY_CAP_BYTES) return fail("invalid_request");
      return fromReply(await store.registerInbox(inboxId, req2.cap, req2.requestId));
    }
    if (m !== "POST") return fail("invalid_request");
    if (sub === "/drain") {
      const req2 = parse(decodeInboxDrain, await body(request, 512));
      return req2.cap.length === RECOVERY_CAP_BYTES ? fromReply(await store.drain(inboxId, req2.cap, req2.pageSize)) : fail("unauthorized");
    }
    const req = parse(decodeInboxAck, await body(request, 2048));
    return req.cap.length === RECOVERY_CAP_BYTES ? fromReply(await store.ack(inboxId, req.cap, req.itemIds)) : fail("unauthorized");
  }
  return fail("not_found");
}
__name(recovery, "recovery");
function ticketPrefix(b) {
  try {
    const o = new Obj(decode(b));
    o.uint(0);
    o.uint(1);
    return o.bytes(2, 16, 16).slice(0, 8);
  } catch {
    return void 0;
  }
}
__name(ticketPrefix, "ticketPrefix");
var index_default = {
  async fetch(request, env) {
    try {
      return await route(request, env, await count(request, env));
    } catch (e) {
      if (e instanceof Bad) return fail(e.code);
      if (e instanceof HomeError) return fail(e.errorName);
      return fail("retryable", { retryAfterSeconds: 2 });
    }
  }
};
export {
  BlobSpool,
  DescriptorStore,
  IntroGate,
  KeyPool,
  MailboxQueue,
  RecoveryStore,
  UsageMeter,
  index_default as default
};
