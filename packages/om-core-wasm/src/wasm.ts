import type { ProfileExecutionContext } from "@om/platform-api";
// Loads om_wasm.wasm and exposes its single JSON entry point. Entropy comes from crypto.getRandomValues; the clock is
// a display/expiry hint only. A trap (panic = abort) kills the instance: every later call fails closed.
import { CoreCallError } from "./types";

interface Exports {
  readonly memory: WebAssembly.Memory;
  readonly om_in_alloc: (len: number) => number;
  readonly om_call: (len: number) => number;
  readonly om_out_ptr: () => number;
  readonly om_call_keys?: (len: number) => number;
}

export interface WasmHost {
  /** Fill `buf` with cryptographically secure random bytes. Default: `crypto.getRandomValues`. */
  readonly randomFill?: (buf: Uint8Array) => void;
  readonly nowMs?: () => number;
  /** Runtime-segment milliseconds for scoped freshness. Default: performance.now(). */
  readonly monotonicMs?: () => number;
}

export interface WasmCore {
  /** One synchronous JSON command. Throws `CoreCallError` for a Core error response or a dead instance. */
  call(command: Readonly<Record<string, unknown>>): unknown;
  /** Binary-only, one-shot key export. The caller owns and must wipe the returned bytes. */
  takeCallKeys?(ctx: ProfileExecutionContext, callSessionId: string): Uint8Array | null;
}

const MAX_RANDOM_CHUNK = 65536;
export const MAX_WASM_COMMAND_BYTES = 16 << 20; // om_in_alloc caps its reservation at this size.
const encoder = new TextEncoder();
const decoder = new TextDecoder("utf-8", { fatal: true });

function isExports(x: WebAssembly.Exports): x is WebAssembly.Exports & Exports {
  return x["memory"] instanceof WebAssembly.Memory && typeof x["om_in_alloc"] === "function" && typeof x["om_call"] === "function" && typeof x["om_out_ptr"] === "function";
}

export async function instantiateCore(source: BufferSource, host: WasmHost = {}): Promise<WasmCore> {
  const random = host.randomFill ?? ((b: Uint8Array) => { globalThis.crypto.getRandomValues(b as Uint8Array<ArrayBuffer>); });
  const now = host.nowMs ?? (() => Date.now());
  const monotonic = host.monotonicMs ?? (() => globalThis.performance?.now() ?? Number.NaN);
  const loaded: { exports?: Exports } = {};
  const importObject: WebAssembly.Imports = {
    om_host: {
      random_fill: (ptr: number, len: number): void => {
        const exports = loaded.exports;
        if (!exports) throw new Error("random_fill before instantiation finished");
        // getRandomValues is limited to 64 KiB per call; re-read the buffer each time (memory may grow).
        for (let off = 0; off < len; off += MAX_RANDOM_CHUNK) {
          random(new Uint8Array(exports.memory.buffer, ptr + off, Math.min(MAX_RANDOM_CHUNK, len - off)));
        }
      },
      now_ms: (): number => now(),
      monotonic_ms: (): number => monotonic(),
    },
  };
  const { instance } = await WebAssembly.instantiate(source, importObject);
  if (!isExports(instance.exports)) throw new CoreCallError("invalid", "not an om-wasm module");
  const ex: Exports = instance.exports;
  loaded.exports = ex;
  let dead = false;
  return {
    takeCallKeys(ctx, callSessionId) {
      if (dead) throw new CoreCallError("trapped", "the Core instance trapped earlier and must be re-opened from storage");
      if (typeof ex.om_call_keys !== "function") return null;
      const req = encoder.encode(JSON.stringify({ ctx, callSessionId }));
      try {
        const inPtr = ex.om_in_alloc(req.length);
        new Uint8Array(ex.memory.buffer, inPtr, req.length).set(req);
        const ptr = ex.om_call_keys(req.length);
        if (ptr === 0) return null;
        const shared = new Uint8Array(ex.memory.buffer, ptr, 64);
        try { return shared.slice(); } finally { shared.fill(0); }
      } catch (e) {
        dead = true;
        throw new CoreCallError("trapped", e instanceof Error ? e.message : "trap");
      }
    },
    call(command) {
      if (dead) throw new CoreCallError("trapped", "the Core instance trapped earlier and must be re-opened from storage");
      const req = encoder.encode(JSON.stringify(command));
      if (req.length > MAX_WASM_COMMAND_BYTES) throw new CoreCallError("size_limit", "command exceeds the Core ABI input limit");
      let text: string;
      try {
        const inPtr = ex.om_in_alloc(req.length);
        new Uint8Array(ex.memory.buffer, inPtr, req.length).set(req);
        const n = ex.om_call(req.length);
        text = decoder.decode(new Uint8Array(ex.memory.buffer, ex.om_out_ptr(), n).slice());
      } catch (e) {
        dead = true;
        throw new CoreCallError("trapped", e instanceof Error ? e.message : "trap");
      }
      const parsed = JSON.parse(text) as { ok?: unknown; err?: { code: string; message: string } };
      if (parsed.err) throw new CoreCallError(parsed.err.code, parsed.err.message);
      return parsed.ok;
    },
  };
}
