// SPDX-License-Identifier: MPL-2.0
// Field accessors over a decoded CBOR map with the CDDL rules of spec/cddl/home-api.cddl: exact key set, bounded sizes,
// no unknown keys (a body that carries one is `invalid_request`, never silently ignored).
import { CborError, type CborValue } from "./cbor";

export class Obj {
  private readonly m: ReadonlyMap<number | string, CborValue>;
  private readonly used = new Set<number | string>();

  constructor(v: CborValue) {
    if (!(v instanceof Map)) throw new CborError("expected a map");
    this.m = v as ReadonlyMap<number | string, CborValue>;
  }

  private take(k: number): CborValue | undefined {
    this.used.add(k);
    return this.m.get(k);
  }

  has(k: number): boolean {
    return this.m.has(k);
  }

  uint(k: number): number {
    const v = this.take(k);
    if (typeof v !== "number") throw new CborError(`field ${String(k)}: expected an unsigned integer`);
    return v;
  }
  optUint(k: number): number | undefined {
    return this.m.has(k) ? this.uint(k) : undefined;
  }
  bool(k: number): boolean {
    const v = this.take(k);
    if (typeof v !== "boolean") throw new CborError(`field ${String(k)}: expected a boolean`);
    return v;
  }
  optBool(k: number): boolean | undefined {
    return this.m.has(k) ? this.bool(k) : undefined;
  }
  bytes(k: number, min: number, max: number): Uint8Array {
    const v = this.take(k);
    if (!(v instanceof Uint8Array) || v.length < min || v.length > max) throw new CborError(`field ${String(k)}: expected ${String(min)}..${String(max)} bytes`);
    return v;
  }
  optBytes(k: number, min: number, max: number): Uint8Array | undefined {
    return this.m.has(k) ? this.bytes(k, min, max) : undefined;
  }
  text(k: number, min: number, max: number): string {
    const v = this.take(k);
    if (typeof v !== "string" || v.length < min || v.length > max) throw new CborError(`field ${String(k)}: expected text of ${String(min)}..${String(max)}`);
    return v;
  }
  optText(k: number, min: number, max: number): string | undefined {
    return this.m.has(k) ? this.text(k, min, max) : undefined;
  }
  /** An array whose every element is a byte string of `min..max` bytes; `1..n` items unless `allowEmpty`. */
  byteList(k: number, minItems: number, maxItems: number, min: number, max: number): Uint8Array[] {
    const v = this.take(k);
    if (!Array.isArray(v) || v.length < minItems || v.length > maxItems) throw new CborError(`field ${String(k)}: expected ${String(minItems)}..${String(maxItems)} items`);
    return (v as CborValue[]).map((x) => {
      if (!(x instanceof Uint8Array) || x.length < min || x.length > max) throw new CborError(`field ${String(k)}: bad item`);
      return x;
    });
  }
  optByteList(k: number, minItems: number, maxItems: number, min: number, max: number): Uint8Array[] | undefined {
    return this.m.has(k) ? this.byteList(k, minItems, maxItems, min, max) : undefined;
  }
  optTextList(k: number, minItems: number, maxItems: number, min: number, max: number): string[] | undefined {
    if (!this.m.has(k)) return undefined;
    const v = this.take(k);
    if (!Array.isArray(v) || v.length < minItems || v.length > maxItems) throw new CborError(`field ${String(k)}: expected ${String(minItems)}..${String(maxItems)} items`);
    return (v as CborValue[]).map((x) => {
      if (typeof x !== "string" || x.length < min || x.length > max) throw new CborError(`field ${String(k)}: bad item`);
      return x;
    });
  }
  /** An array of maps (each wrapped for field access), `0..maxItems` items. */
  mapList(k: number, maxItems: number): Obj[] {
    const v = this.take(k);
    if (!Array.isArray(v) || v.length > maxItems) throw new CborError(`field ${String(k)}: expected at most ${String(maxItems)} maps`);
    return (v as CborValue[]).map((x) => new Obj(x));
  }
  /** `digest = {0: hash-alg(1 = sha-256), 1: bstr .size 32}` (spec/cddl/common.cddl). */
  digest(k: number): Uint8Array {
    const d = new Obj(this.take(k) ?? null);
    if (d.uint(0) !== 1) throw new CborError(`field ${String(k)}: unsupported hash algorithm`);
    const h = d.bytes(1, 32, 32);
    d.done();
    return h;
  }
  /** Optional key 23 `extensions` (never empty when present); accepted and bounded, not interpreted. */
  optExtensions(): void {
    if (!this.m.has(23)) return;
    const v = this.take(23);
    if (!(v instanceof Map) || v.size < 1 || v.size > 16) throw new CborError("bad extensions");
  }
  /** Rejects any key the caller did not read. */
  done(): void {
    for (const k of this.m.keys()) if (!this.used.has(k)) throw new CborError(`unknown field ${String(k)}`);
  }
}
