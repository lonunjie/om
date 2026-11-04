import type { PlatformAdapter } from "@om/platform-api";
import type { ComposerCapture } from "@om/ui";

/**
 * What this device offers the composer. Each control exists only where the browser really supports it:
 * a picker (a file input counts), a microphone with a recorder (without MediaRecorder the family is "partial" and recording would fail),
 * and one-shot position. Nothing is requested here; the browser's own permission prompt appears only when the person taps a control.
 */
export function composerCapture(platform: Pick<PlatformAdapter, "files" | "microphone" | "location">): ComposerCapture {
  const files = platform.files();
  const mic = platform.microphone();
  const where = platform.location();
  return {
    ...(files.state === "unavailable" ? {} : { files: files.api }),
    ...(mic.state === "available" ? { microphone: mic.api } : {}),
    ...(where.state === "available" ? { location: where.api } : {}),
  };
}
