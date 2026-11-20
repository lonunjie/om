import { afterEach, expect, it, vi } from "vitest";
import { DecoderMismatch, openFrameDecoder } from "./native";

const bytes = new Uint8Array(26);
const still = { width: 1, height: 1, frames: 1, durationMs: null, encodedSize: bytes.byteLength, totalPixels: 1 };
const animated = { ...still, frames: 2, durationMs: 68, totalPixels: 2 };
const rgba = Uint8ClampedArray.of(1, 2, 3, 255);

afterEach(() => { vi.unstubAllGlobals(); });

it("reads a native bitmap once and closes the native surface", async () => {
  const image = { width: 1, height: 1, close: vi.fn() };
  const createImageBitmap = vi.fn(() => Promise.resolve(image));
  const drawImage = vi.fn();
  vi.stubGlobal("ImageDecoder", undefined);
  vi.stubGlobal("createImageBitmap", createImageBitmap);
  vi.stubGlobal("OffscreenCanvas", class {
    getContext() { return { drawImage, clearRect: vi.fn(), getImageData: () => ({ data: rgba }) }; }
  });

  const decoder = await openFrameDecoder(bytes, animated);
  if (decoder === undefined) throw new Error("expected bitmap decoder");
  expect(await decoder.read(0)).toEqual({
    frame: { rgba, durationUs: null },
    facts: { width: 1, height: 1, frameCount: null },
  });
  expect(createImageBitmap).toHaveBeenCalledOnce();
  expect(drawImage).toHaveBeenCalledExactlyOnceWith(image, 0, 0);
  expect(image.close).toHaveBeenCalledOnce();
  decoder.close();
});

it("closes an animated decoder when its track count disagrees with Core", async () => {
  const close = vi.fn();
  const decode = vi.fn();
  vi.stubGlobal("OffscreenCanvas", vi.fn());
  vi.stubGlobal("ImageDecoder", class {
    static isTypeSupported() { return Promise.resolve(true); }
    tracks = { ready: Promise.resolve(), selectedTrack: { animated: true, frameCount: 3 } };
    close = close;
    decode = decode;
  });

  await expect(openFrameDecoder(bytes, animated)).rejects.toBeInstanceOf(DecoderMismatch);
  expect(close).toHaveBeenCalledOnce();
  expect(decode).not.toHaveBeenCalled();
});

it("rejects a native dimension mismatch before allocating a canvas", async () => {
  const image = { width: 2, height: 1, close: vi.fn() };
  const getContext = vi.fn();
  vi.stubGlobal("createImageBitmap", () => Promise.resolve(image));
  vi.stubGlobal("OffscreenCanvas", class { getContext = getContext; });

  const decoder = await openFrameDecoder(bytes, still);
  if (decoder === undefined) throw new Error("expected bitmap decoder");
  await expect(decoder.read(0)).rejects.toBeInstanceOf(DecoderMismatch);
  expect(getContext).not.toHaveBeenCalled();
  expect(image.close).toHaveBeenCalledOnce();
});
