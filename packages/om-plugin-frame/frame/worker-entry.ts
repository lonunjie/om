// SPDX-License-Identifier: MPL-2.0
import { servePlugin } from "./worker";

servePlugin(async (entrypoint) => {
  if (entrypoint !== "smoke") throw new Error("No plugin component installed for this entrypoint");
  // Empty Wasm module, confined to a fresh dedicated Worker. No guest-supplied JavaScript is loaded.
  await WebAssembly.instantiate(Uint8Array.of(0, 97, 115, 109, 1, 0, 0, 0), {});
});
