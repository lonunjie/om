// Writes a tiny Y4M clip of an OM invite QR code. Chromium plays it as the fake camera
// (--use-file-for-fake-video-capture), so the real scanner runs against a real decoded frame stream.
import { mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { qrModules } from "../../../packages/om-web-platform/src/qr/qr.ts";

export const SCAN_FIXTURE_LINK = "om:invite/ScanFixtureToken0123456789ab";

export function writeQrY4m(payload: string = SCAN_FIXTURE_LINK, scale = 8, quiet = 4): string {
  const modules = qrModules(payload);
  if (modules === null) throw new Error("payload cannot be encoded");
  const size = (modules.length + quiet * 2) * scale; // even, as Y4M 4:2:0 requires
  const luma = new Uint8Array(size * size).fill(255);
  modules.forEach((row, y) => {
    row.forEach((dark, x) => {
      if (!dark) return;
      for (let dy = 0; dy < scale; dy++) for (let dx = 0; dx < scale; dx++) luma[((y + quiet) * scale + dy) * size + (x + quiet) * scale + dx] = 0;
    });
  });
  const chroma = new Uint8Array((size / 2) * (size / 2)).fill(128);
  const frame = Buffer.concat([Buffer.from("FRAME\n"), luma, chroma, chroma]);
  const header = Buffer.from(`YUV4MPEG2 W${String(size)} H${String(size)} F5:1 Ip A1:1 C420jpeg\n`);
  const dir = join(tmpdir(), "om-e2e");
  mkdirSync(dir, { recursive: true });
  const file = join(dir, "invite-qr.y4m");
  writeFileSync(file, Buffer.concat([header, frame, frame, frame]));
  return file;
}
