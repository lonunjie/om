// SPDX-License-Identifier: Apache-2.0
// Feasibility prototype for V-001. Not product code. Not for production.
// Drives the wasm32-unknown-unknown build inside Node (V8). Usage:
//   node run_wasm.mjs WASM selftest|vectors|rng
//   node run_wasm.mjs WASM bench SETUP MSGS
//   node run_wasm.mjs WASM persist DIR     (fresh WebAssembly.Instance per step; one JSON line per step)
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { createHash, webcrypto } from 'node:crypto';
import { performance } from 'node:perf_hooks';

const [, , wasmPath, cmd, a = '0', b = '0'] = process.argv;
const module = new WebAssembly.Module(readFileSync(wasmPath));

function instance() {
  let mem;
  const inst = new WebAssembly.Instance(module, {
    om_host: {
      random_fill: (ptr, len) => {
        const view = new Uint8Array(mem.buffer, ptr, len);
        for (let o = 0; o < len; o += 65536) webcrypto.getRandomValues(view.subarray(o, Math.min(len, o + 65536)));
      },
      now_ns: () => performance.now() * 1e6,
    },
  });
  mem = inst.exports.memory;
  return inst;
}

function run(inst, c, x = 0, y = 0, input = null) {
  if (input) {
    const p = inst.exports.om_in_alloc(input.length);
    new Uint8Array(inst.exports.memory.buffer, p, input.length).set(input);
  }
  const n = inst.exports.om_run(c, x, y) >>> 0;
  const ptr = inst.exports.om_out_ptr();
  const len = n === 0xffffffff ? 0 : n;
  const bytes = new Uint8Array(inst.exports.memory.buffer, ptr, n === 0xffffffff ? 256 : len).slice();
  if (n === 0xffffffff) throw new Error(new TextDecoder().decode(bytes).replace(/\0.*$/s, ''));
  return bytes;
}

const txt = (u) => new TextDecoder().decode(u);
if (cmd === 'selftest') {
  const out = txt(run(instance(), 1));
  console.log(out);
  process.exit(JSON.parse(out).all_ok ? 0 : 1);
} else if (cmd === 'bench') {
  console.log(txt(run(instance(), 2, Number(a), Number(b))));
} else if (cmd === 'vectors') {
  console.log(txt(run(instance(), 3)));
} else if (cmd === 'rng') {
  console.log(txt(run(instance(), 5)));
} else if (cmd === 'persist') {
  mkdirSync(a, { recursive: true });
  const n = Number(txt(run(instance(), 6)));
  let world = null;
  for (let k = 0; k < n; k++) {
    // A fresh instance = fresh linear memory: nothing survives except the bytes on disk.
    const onDisk = k === 0 ? null : readFileSync(`${a}/world.cbor`);
    world = run(instance(), 4, k, 0, onDisk);
    writeFileSync(`${a}/world.cbor`, world);
    console.log(JSON.stringify({ step: k, world_sha256: createHash('sha256').update(world).digest('hex') }));
  }
} else {
  console.error('unknown command');
  process.exit(2);
}
