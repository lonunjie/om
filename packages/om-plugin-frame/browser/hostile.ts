// SPDX-License-Identifier: MPL-2.0
// Synthetic, build-installed glue. This file is absent from the static Pages build.
import { servePlugin } from "../frame/worker";
import type { PluginUiNode } from "../../om-core-wasm/src/scope-engines";

// (module (func (export "spin") (loop br 0)))
const loopModule = Uint8Array.of(
  0,
  97,
  115,
  109,
  1,
  0,
  0,
  0,
  1,
  4,
  1,
  96,
  0,
  0,
  3,
  2,
  1,
  0,
  7,
  8,
  1,
  4,
  115,
  112,
  105,
  110,
  0,
  0,
  10,
  9,
  1,
  7,
  0,
  3,
  64,
  12,
  0,
  11,
  11,
);

// (module (import "fixture" "memory" (memory 1 2))
//   (func (export "grow") (result i32) i32.const 1 memory.grow))
const growthModule = Uint8Array.of(
  0,
  97,
  115,
  109,
  1,
  0,
  0,
  0,
  1,
  5,
  1,
  96,
  0,
  1,
  127,
  2,
  20,
  1,
  7,
  102,
  105,
  120,
  116,
  117,
  114,
  101,
  6,
  109,
  101,
  109,
  111,
  114,
  121,
  2,
  1,
  1,
  2,
  3,
  2,
  1,
  0,
  7,
  8,
  1,
  4,
  103,
  114,
  111,
  119,
  0,
  0,
  10,
  8,
  1,
  6,
  0,
  65,
  1,
  64,
  0,
  11,
);

servePlugin(async (fixture, { call, sampleMemory }) => {
  switch (fixture) {
    case "tainted":
    case "bilateral":
      await call({ kind: "read", capability: "content.read_selected" });
      await call({ kind: "egress", capability: "net.fetch_origin_scoped", destination: "https://example.invalid" });
      return;
    case "send":
      await call({ kind: "send", destination: "synthetic-room", content: "Synthetic exact outgoing content" });
      return;
    case "ui": {
      const id = (i: number) => i.toString(16).padStart(32, "0");
      const nodes: PluginUiNode[] = Array.from(
        { length: 129 },
        (_, i) => i === 0 ? { id: id(i), kind: 1 } : { id: id(i), kind: 1, parent: id(0) },
      );
      await call({ kind: "ui", surface: "panel", nodes });
      return;
    }
    case "oversized":
      await call({ kind: "send", destination: "synthetic-room", content: "x".repeat(65_536) });
      return;
    case "unknown":
      self.postMessage(JSON.stringify({ type: "run_host_command", command: "read_vault" }));
      await call({ kind: "read", capability: "made_up" });
      return;
    case "ungranted":
      await call({ kind: "read", capability: "content.read_attachment_selected" });
      return;
    case "host_state":
      if ("document" in globalThis || "localStorage" in globalThis || "omHostSentinel" in globalThis) {
        throw new Error("Unexpected ambient host state in Worker");
      }
      await call({ kind: "host_state" });
      return;
    case "spin": {
      const { instance } = await WebAssembly.instantiate(loopModule, {});
      const spin = instance.exports.spin;
      if (typeof spin !== "function") throw new Error("Missing fixture export");
      // The completed round trip proves the Worker was alive before it began the guest call.
      await call({ kind: "host_state" });
      spin();
      throw new Error("Loop returned");
    }
    case "memory": {
      const memory = new WebAssembly.Memory({ initial: 1, maximum: 2 });
      const { instance } = await WebAssembly.instantiate(growthModule, { fixture: { memory } });
      const grow = instance.exports.grow;
      await sampleMemory(memory);
      if (typeof grow !== "function" || grow() !== 1 || grow() !== -1) {
        throw new Error("Unexpected Wasm memory growth");
      }
      await sampleMemory(memory);
      return;
    }
    default:
      throw new Error("Unknown synthetic fixture");
  }
});
