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
  const count = o.optUint(3);
  o.done();
  return { requestId, params, ...count === void 0 ? {} : { count } };
}
__name(decodeIntroTicketRequest, "decodeIntroTicketRequest");
function decodeTicketParams(b) {
  const o = new Obj(decode(b));
  const ttlSeconds = o.uint(0);
  const maxUses = o.uint(1);
  const payloadLimitBytes = o.uint(2);
  const allowAttachments = o.optBool(3);
  const allowCalls = o.optBool(4);
  o.done();
  return { ttlSeconds, maxUses, payloadLimitBytes, ...allowAttachments === void 0 ? {} : { allowAttachments }, ...allowCalls === void 0 ? {} : { allowCalls } };
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

// ../../packages/om-home-wire/src/index.ts
var FEATURES = {
  relayQueues: "om.relay.queues.v1",
  relayWake: "om.relay.wake.v1",
  /** The Home lets only its owner create resources (queues, descriptors, key pools, intro services); see `admissionHash`. */
  admission: "om.admission.v1",
  keysPool: "om.keys.pool.v1",
  keysAnonymousFetch: "om.keys.fetch-throttled.v1",
  discoveryPut: "om.discovery.put.v1",
  discoveryDelete: "om.discovery.delete.v1",
  introGate: "om.intro.v1"
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
  /** Key pool. */
  maxOneTimePerKind: 100,
  maxBundleBytes: 65536,
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
  submitRefillPerSecond: 2
};

// src/capabilities.ts
var wakeOn = /* @__PURE__ */ __name((env) => env.OM_WAKE !== "0", "wakeOn");
var admissionHashes = /* @__PURE__ */ __name((env) => (env.OM_ADMISSION ?? "").split(",").map((h) => h.trim().toLowerCase()).filter((h) => /^[0-9a-f]{64}$/.test(h)), "admissionHashes");
var admissionConfigured = /* @__PURE__ */ __name((env) => (env.OM_ADMISSION ?? "").trim() !== "" || env.OM_ADMISSION_REQUIRED === "1", "admissionConfigured");
function capabilitiesDocument(env) {
  return {
    schema_version: 1,
    provider_id: env.OM_PROVIDER_ID ?? "om-reference-home",
    provider_kinds: ["relay", "key_material", "discovery_relay", "intro_gate"],
    durability_class: "durable",
    features: Object.values(FEATURES).filter((f) => (f !== FEATURES.relayWake || wakeOn(env)) && (f !== FEATURES.admission || admissionHashes(env).length > 0)).sort(),
    api_versions: ["1"],
    health: "healthy",
    limits: {
      max_deposit_bytes: LIMITS.maxDepositBytes,
      max_queue_items: LIMITS.maxQueueItems,
      max_queue_bytes: LIMITS.maxQueueBytes,
      max_retention_seconds: LIMITS.maxRetentionSeconds,
      drain_page_items: LIMITS.drainPageItems,
      idempotency_horizon_seconds: LIMITS.idempotencySeconds,
      rotate_overlap_seconds: LIMITS.rotateOverlapSeconds,
      max_deposit_capabilities: LIMITS.maxDepositCapabilities,
      max_descriptor_bytes: LIMITS.maxDescriptorBytes,
      max_ticket_uses: LIMITS.maxTicketUses,
      ...wakeOn(env) ? { wake_sockets_per_queue: LIMITS.maxWakeSockets, wake_auth_seconds: WAKE_AUTH_SECONDS } : {}
    },
    abuse_controls: {
      per_capability_deposit_rate: { burst: LIMITS.depositBurst, per_second: LIMITS.depositRefillPerSecond },
      one_time_prekey_fetch_throttle: { burst: LIMITS.fetchBurst, refill_seconds: LIMITS.fetchRefillSeconds },
      introgate_submit_rate: { burst: LIMITS.submitBurst, per_second: LIMITS.submitRefillPerSecond },
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
    cost_state: { hard_free_tier_limits: env.OM_HARD_FREE_TIER === "1" }
  };
}
__name(capabilitiesDocument, "capabilitiesDocument");

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

// src/http.ts
var CBOR = "application/cbor";
var BASE_HEADERS = {
  "Cache-Control": "no-store",
  "Referrer-Policy": "no-referrer",
  "X-Content-Type-Options": "nosniff",
  "Cross-Origin-Resource-Policy": "cross-origin",
  "Access-Control-Allow-Origin": "*",
  "Access-Control-Expose-Headers": "Retry-After",
  "Vary": "Origin"
};
var preflight = /* @__PURE__ */ __name(() => new Response(null, {
  status: 204,
  headers: { ...BASE_HEADERS, "Access-Control-Allow-Methods": "GET, POST, PUT, DELETE, OPTIONS", "Access-Control-Allow-Headers": "authorization, content-type, om-admission", "Access-Control-Max-Age": "86400" }
}), "preflight");
function ok(body2, contentType = CBOR) {
  return new Response(body2.length === 0 ? null : body2, { status: body2.length === 0 ? 204 : 200, headers: { ...BASE_HEADERS, "Content-Type": contentType } });
}
__name(ok, "ok");
function json(value) {
  return new Response(JSON.stringify(value), { status: 200, headers: { ...BASE_HEADERS, "Content-Type": "application/json", "Cache-Control": "public, max-age=300" } });
}
__name(json, "json");
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

// src/descriptors.ts
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

// src/descriptors.ts
var SCHEMA = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;
var DescriptorStore = class extends DurableObject {
  static {
    __name(this, "DescriptorStore");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA);
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
import { DurableObject as DurableObject2 } from "cloudflare:workers";

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
  if (u.items >= LIMITS.maxQueueItems) throw new HomeError("quota", { detail: quotaDetail(1, input.hard) });
  if (u.bytes + input.bytes.length > LIMITS.maxQueueBytes) throw new HomeError("quota", { detail: quotaDetail(2, input.hard) });
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
var SCHEMA2 = `${MAILBOX_SCHEMA}
CREATE TABLE IF NOT EXISTS tickets (id BLOB PRIMARY KEY, expires_at INTEGER NOT NULL, max_uses INTEGER NOT NULL, used INTEGER NOT NULL DEFAULT 0, payload_limit INTEGER NOT NULL, revoked INTEGER NOT NULL DEFAULT 0);
CREATE TABLE IF NOT EXISTS uses (ticket_id BLOB NOT NULL, fp TEXT NOT NULL, PRIMARY KEY (ticket_id, fp));
`;
var IntroGate = class extends DurableObject2 {
  static {
    __name(this, "IntroGate");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA2);
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
      const count = req.count ?? 1;
      if (count < 1 || count > 16) throw new HomeError("invalid_request");
      if (params.ttlSeconds < 1 || params.ttlSeconds > LIMITS.maxTicketSeconds || params.maxUses < 1 || params.maxUses > LIMITS.maxTicketUses || params.payloadLimitBytes < 1 || params.payloadLimitBytes > LIMITS.maxTicketPayload) throw new HomeError("policy_denied");
      await this.serviceAuth(cap, true);
      const prefix = cap.slice(0, 8);
      const now = nowSeconds();
      const active = one(this.sql, "SELECT COUNT(*) AS n FROM tickets WHERE revoked = 0 AND expires_at + ? >= ?", LIMITS.skewSeconds, now)?.n ?? 0;
      const out = [];
      const binding = (await sha256(prefix)).slice(0, 16);
      for (let i = 0; i < count; i++) {
        const idTail = (await sha256(prefix, req.requestId, new Uint8Array([i]))).slice(0, 8);
        const id = concat(prefix, idTail);
        const row = one(this.sql, "SELECT expires_at, max_uses, payload_limit FROM tickets WHERE id = ?", id);
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
      const q = enqueue(this.sql, { bytes: request, fp, tupleKey: void 0, expiresAt: Math.min(ticket.expires_at, now + LIMITS.maxRetentionSeconds), capHash: void 0, hard: this.hard });
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
import { DurableObject as DurableObject3 } from "cloudflare:workers";
var SCHEMA3 = `
CREATE TABLE IF NOT EXISTS meta (k TEXT PRIMARY KEY, v);
CREATE TABLE IF NOT EXISTS prekeys (id INTEGER PRIMARY KEY AUTOINCREMENT, kind INTEGER NOT NULL, item BLOB NOT NULL);
CREATE TABLE IF NOT EXISTS ops (request_id TEXT PRIMARY KEY, fp TEXT NOT NULL, expires_at INTEGER NOT NULL);
`;
var KeyPool = class extends DurableObject3 {
  static {
    __name(this, "KeyPool");
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
      if (facts) {
        if (facts.deviceId.length !== deviceId.length || facts.deviceId.some((b, i) => b !== deviceId[i])) throw new HomeError("invalid_request", { hint: "bundle is for another device" });
        if (facts.expiresAt + LIMITS.skewSeconds < now) throw new HomeError("expired");
        if (facts.generation < metaNumber(this.sql, "highest_generation")) throw new HomeError("stale_generation");
        setMeta(this.sql, "highest_generation", facts.generation);
        setMeta(this.sql, "bundle_generation", facts.generation);
      }
      const ec = one(this.sql, "SELECT COUNT(*) AS n FROM prekeys WHERE kind = 1")?.n ?? 0;
      const pq = one(this.sql, "SELECT COUNT(*) AS n FROM prekeys WHERE kind = 2")?.n ?? 0;
      if (ec + req.opks.length > LIMITS.maxOneTimePerKind || pq + req.pqopks.length > LIMITS.maxOneTimePerKind) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
      setMeta(this.sql, "bundle", req.bundle);
      for (const k of req.opks) run(this.sql, "INSERT INTO prekeys (kind, item) VALUES (1, ?)", k);
      for (const k of req.pqopks) run(this.sql, "INSERT INTO prekeys (kind, item) VALUES (2, ?)", k);
      run(this.sql, "DELETE FROM ops WHERE expires_at < ?", now);
      run(this.sql, "INSERT INTO ops (request_id, fp, expires_at) VALUES (?, ?, ?)", rid, fp, now + LIMITS.idempotencySeconds);
      return new Uint8Array(0);
    });
  }
  /**
   * Returns `[bundle, one-time item?, pq one-time item?]`. Consumption is throttled per device, so a stranger cannot
   * drain the pool faster than it refills; a throttled fetch gets the signed bundle without one-time items, which is
   * the specified degraded (not weaker) handshake.
   */
  fetchItems() {
    return reply(() => {
      const bundle = getBundle(this.sql);
      if (!bundle) throw new HomeError("not_found");
      const now = nowSeconds();
      const items = [bundle];
      const t = takeToken(metaNumber(this.sql, "tokens", LIMITS.fetchBurst), metaNumber(this.sql, "tokens_at", now), now, LIMITS.fetchBurst, 1 / LIMITS.fetchRefillSeconds);
      if (t.allowed) {
        for (const kind of [1, 2]) {
          const row = one(this.sql, "SELECT id, item FROM prekeys WHERE kind = ? ORDER BY id LIMIT 1", kind);
          if (!row) continue;
          run(this.sql, "DELETE FROM prekeys WHERE id = ?", row.id);
          items.push(new Uint8Array(row.item));
        }
        setMeta(this.sql, "tokens", t.tokens);
        setMeta(this.sql, "tokens_at", now);
      }
      return Promise.resolve(encodeKeyFetchResponse({ items }));
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
import { DurableObject as DurableObject4 } from "cloudflare:workers";

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
var SCHEMA4 = `${MAILBOX_SCHEMA}
CREATE TABLE IF NOT EXISTS dcaps (hash TEXT PRIMARY KEY, id BLOB NOT NULL, generation INTEGER NOT NULL, expires_at INTEGER, revoked INTEGER NOT NULL DEFAULT 0, max_items INTEGER, tokens REAL NOT NULL, tokens_at INTEGER NOT NULL);
CREATE UNIQUE INDEX IF NOT EXISTS dcaps_id ON dcaps (id);
`;
var MailboxQueue = class extends DurableObject4 {
  static {
    __name(this, "MailboxQueue");
  }
  sql;
  constructor(ctx, env) {
    super(ctx, env);
    this.sql = ctx.storage.sql;
    this.sql.exec(SCHEMA4);
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
      const q = enqueue(this.sql, { bytes, fp: wrapperFp, tupleKey, expiresAt, capHash: cap.hash, hard: this.hard });
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
  mint(adminCap, req) {
    return reply(async () => {
      await this.adminAuth(adminCap);
      const fp = await fingerprint(req.label, new TextEncoder().encode(`${String(req.ttlSeconds)}:${String(req.maxItems)}`));
      const done = await this.replay(req.requestId, fp);
      if (done) return done;
      const count = one(this.sql, "SELECT COUNT(*) AS n FROM dcaps WHERE revoked = 0")?.n ?? 0;
      const rows = one(this.sql, "SELECT COUNT(*) AS n FROM dcaps")?.n ?? 0;
      if (count >= LIMITS.maxDepositCapabilities || rows >= LIMITS.maxCapabilityRows) throw new HomeError("quota", { detail: quotaDetail(4, this.hard) });
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

// src/index.ts
var hex = /* @__PURE__ */ __name((b) => Array.from(b, (x) => x.toString(16).padStart(2, "0")).join(""), "hex");
var queueOf = /* @__PURE__ */ __name((env, cap) => env.QUEUES.get(env.QUEUES.idFromName(`q:${hex(cap.slice(0, 16))}`)), "queueOf");
var introOf = /* @__PURE__ */ __name((env, cap) => env.INTRO.get(env.INTRO.idFromName(`i:${hex(cap.slice(0, 8))}`)), "introOf");
var BODY_MAX = LIMITS.maxDepositBytes + 2048;
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
async function route(request, env) {
  const url = new URL(request.url);
  const path = url.pathname;
  const m = request.method;
  if (m === "OPTIONS") return preflight();
  if (path === "/.well-known/om-provider-capabilities") return m === "GET" ? json(capabilitiesDocument(env)) : fail("invalid_request");
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
  const gen = /^\/v1\/keys\/bundles\/([0-9a-f]{32})\/generations\/(\d{1,15})$/.exec(path);
  if (gen) {
    if (m !== "DELETE") return fail("invalid_request");
    const cap = bearer(request);
    if (!cap) return fail("unauthorized");
    return fromReply(await env.KEYS.get(env.KEYS.idFromName(`k:${gen[1] ?? ""}`)).revokeGeneration(cap, Number(gen[2])));
  }
  if (path === "/v1/keys/fetch") {
    const req = parse(decodeKeyFetch, await body(request, 4096));
    const items = [];
    for (const d of req.devices) {
      if (d.length !== 16) return fail("invalid_request");
      const r = await env.KEYS.get(env.KEYS.idFromName(`k:${hex(d)}`)).fetchItems();
      if (!r.ok) return fromReply(r);
      items.push(...decodeKeyFetchResponse(r.body).items);
    }
    return ok(encodeKeyFetchResponse({ items }));
  }
  if (path === "/v1/intro/tickets") {
    const cap = bearer(request);
    if (!cap || !wellFormed(cap, 8) || !await admitted(request, env)) return fail("unauthorized");
    return fromReply(await introOf(env, cap).issueTickets(cap, parse(decodeIntroTicketRequest, await body(request, 1024))));
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
      return await route(request, env);
    } catch (e) {
      if (e instanceof Bad) return fail(e.code);
      if (e instanceof HomeError) return fail(e.errorName);
      return fail("retryable", { retryAfterSeconds: 2 });
    }
  }
};
export {
  DescriptorStore,
  IntroGate,
  KeyPool,
  MailboxQueue,
  index_default as default
};
//# sourceMappingURL=index.js.map
