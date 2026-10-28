// SPDX-License-Identifier: MPL-2.0
// Strict deterministic CBOR (RFC 8949 section 4.2.1) for the Home API bodies: unsigned integers, byte strings, text
// strings, arrays, maps keyed by unsigned integers or text, booleans and null. Everything else is rejected, as are
// non-shortest encodings, indefinite lengths, duplicate or unsorted map keys, tags, floats and trailing bytes.
// Bounded: nesting depth and total element count are capped before anything is allocated from an attacker length.

export type CborMap = ReadonlyMap<number | string, CborValue>;
export type CborValue = number | bigint | string | boolean | null | Uint8Array | readonly CborValue[] | CborMap;

export class CborError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "CborError";
  }
}

const MAX_DEPTH = 8;
const MAX_ELEMENTS = 4096;
const MAX_UINT = (1n << 64n) - 1n;
const utf8 = new TextEncoder();
const utf8Strict = new TextDecoder("utf-8", { fatal: true, ignoreBOM: true });

function head(major: number, n: number | bigint, out: number[]): void {
  const v = BigInt(n);
  if (v < 0n || v > MAX_UINT) throw new CborError("integer out of range");
  const m = major << 5;
  if (v < 24n) out.push(m | Number(v));
  else if (v < 0x100n) out.push(m | 24, Number(v));
  else if (v < 0x10000n) out.push(m | 25, Number(v >> 8n), Number(v & 0xffn));
  else if (v < 0x100000000n) out.push(m | 26, ...[24n, 16n, 8n, 0n].map((s) => Number((v >> s) & 0xffn)));
  else out.push(m | 27, ...[56n, 48n, 40n, 32n, 24n, 16n, 8n, 0n].map((s) => Number((v >> s) & 0xffn)));
}

function compareBytes(a: Uint8Array, b: Uint8Array): number {
  const n = Math.min(a.length, b.length);
  for (let i = 0; i < n; i++) {
    const d = (a[i] ?? 0) - (b[i] ?? 0);
    if (d !== 0) return d;
  }
  return a.length - b.length;
}

function enc(v: CborValue, out: number[], depth: number): void {
  if (depth > MAX_DEPTH) throw new CborError("nesting too deep");
  if (v === null) out.push(0xf6);
  else if (typeof v === "boolean") out.push(v ? 0xf5 : 0xf4);
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
    const entries = [...(v as CborMap).entries()].map(([k, val]) => {
      const kb: number[] = [];
      enc(k, kb, depth + 1);
      return { k: Uint8Array.from(kb), val };
    });
    entries.sort((a, b) => compareBytes(a.k, b.k));
    for (let i = 1; i < entries.length; i++) {
      if (compareBytes((entries[i - 1] as { k: Uint8Array }).k, (entries[i] as { k: Uint8Array }).k) === 0) throw new CborError("duplicate map key");
    }
    head(5, entries.length, out);
    for (const e of entries) {
      for (const x of e.k) out.push(x);
      enc(e.val, out, depth + 1);
    }
  } else if (Array.isArray(v)) {
    head(4, v.length, out);
    for (const x of v as readonly CborValue[]) enc(x, out, depth + 1);
  } else {
    throw new CborError("unsupported value");
  }
}

/** Deterministic encoding of `v`. Map keys are sorted by their encoded bytes. */
export function encode(v: CborValue): Uint8Array<ArrayBuffer> {
  const out: number[] = [];
  enc(v, out, 0);
  return Uint8Array.from(out);
}

/** Builds a map from `[key, value]` pairs, skipping `undefined` values (absent optional fields). */
export function map(entries: readonly (readonly [number | string, CborValue | undefined])[]): CborMap {
  const m = new Map<number | string, CborValue>();
  for (const [k, v] of entries) {
    if (v === undefined) continue;
    if (m.has(k)) throw new CborError("duplicate map key");
    m.set(k, v);
  }
  return m;
}

class Reader {
  pos = 0;
  elements = 0;
  constructor(readonly b: Uint8Array) {}

  byte(): number {
    const x = this.b[this.pos];
    if (x === undefined) throw new CborError("truncated");
    this.pos += 1;
    return x;
  }

  take(n: number): Uint8Array {
    if (n > this.b.length - this.pos) throw new CborError("truncated");
    const s = this.b.subarray(this.pos, this.pos + n);
    this.pos += n;
    return s;
  }

  arg(info: number): bigint {
    if (info < 24) return BigInt(info);
    const width = info === 24 ? 1 : info === 25 ? 2 : info === 26 ? 4 : info === 27 ? 8 : 0;
    if (width === 0) throw new CborError("indefinite or reserved length");
    let v = 0n;
    for (const x of this.take(width)) v = (v << 8n) | BigInt(x);
    const min = width === 1 ? 24n : width === 2 ? 0x100n : width === 4 ? 0x10000n : 0x100000000n;
    if (v < min) throw new CborError("non-shortest integer");
    return v;
  }

  length(info: number): number {
    const n = this.arg(info);
    if (n > BigInt(this.b.length)) throw new CborError("length exceeds input");
    return Number(n);
  }

  value(depth: number): CborValue {
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
      case 2: return this.take(this.length(info)).slice();
      case 3: {
        try { return utf8Strict.decode(this.take(this.length(info))); } catch { throw new CborError("invalid utf-8"); }
      }
      case 4: {
        const n = this.length(info);
        const a: CborValue[] = [];
        for (let i = 0; i < n; i++) a.push(this.value(depth + 1));
        return a;
      }
      case 5: {
        const n = this.length(info);
        const m = new Map<number | string, CborValue>();
        let prev: Uint8Array | undefined;
        for (let i = 0; i < n; i++) {
          const start = this.pos;
          const k = this.value(depth + 1);
          if (typeof k !== "number" && typeof k !== "string") throw new CborError("map keys must be unsigned integers or text");
          const kb = this.b.subarray(start, this.pos);
          if (prev !== undefined && compareBytes(prev, kb) >= 0) throw new CborError("map keys not sorted or duplicated");
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
}

/** Strict decode of exactly one item. Throws `CborError`. */
export function decode(bytes: Uint8Array): CborValue {
  const r = new Reader(bytes);
  const v = r.value(0);
  if (r.pos !== bytes.length) throw new CborError("trailing bytes");
  return v;
}
