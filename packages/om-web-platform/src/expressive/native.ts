import type { ExpressiveDecodeFacts, ExpressiveFrame, ValidatedExpressiveAsset } from "@om/platform-api";

const MEDIA_TYPE = "image/webp";

export class DecoderMismatch extends Error {}

export interface FrameDecoder {
  readonly frameCount: number | null;
  read(index: number): Promise<{ readonly frame: ExpressiveFrame; readonly facts: ExpressiveDecodeFacts }>;
  close(): void;
}

export async function openFrameDecoder(
  bytes: Uint8Array<ArrayBuffer>, expected: ValidatedExpressiveAsset["facts"],
): Promise<FrameDecoder | undefined> {
  if (typeof OffscreenCanvas !== "function") return undefined;

  let canvas: OffscreenCanvasRenderingContext2D | undefined;
  function readPixels(image: ImageBitmap | VideoFrame, width: number, height: number, durationUs: number | null): ExpressiveFrame {
    if (width !== expected.width || height !== expected.height) throw new DecoderMismatch();
    if (canvas === undefined) {
      const context = new OffscreenCanvas(width, height).getContext("2d");
      if (context === null) throw new Error("No 2D canvas");
      canvas = context;
    }
    canvas.clearRect(0, 0, width, height);
    canvas.drawImage(image, 0, 0);
    return { rgba: canvas.getImageData(0, 0, width, height).data, durationUs };
  }

  if (expected.frames > 1 && typeof ImageDecoder === "function" && await ImageDecoder.isTypeSupported(MEDIA_TYPE)) {
    const decoder = new ImageDecoder({ type: MEDIA_TYPE, data: bytes, preferAnimation: true });
    try {
      await decoder.tracks.ready;
      const track = decoder.tracks.selectedTrack;
      if (track === null || !track.animated || track.frameCount !== expected.frames) throw new DecoderMismatch();
      const frameCount = track.frameCount;
      return {
        frameCount,
        async read(index) {
          const { image, complete } = await decoder.decode({ frameIndex: index, completeFramesOnly: true });
          try {
            const { displayWidth: width, displayHeight: height, duration } = image;
            if (!complete || image.codedWidth !== expected.width || image.codedHeight !== expected.height
              || duration === null || !Number.isSafeInteger(duration) || duration < 0) throw new DecoderMismatch();
            return { frame: readPixels(image, width, height, duration), facts: { width, height, frameCount } };
          } finally {
            image.close();
          }
        },
        close: () => { decoder.close(); canvas = undefined; },
      };
    } catch (error) {
      decoder.close();
      throw error;
    }
  }

  if (typeof createImageBitmap !== "function") return undefined;
  return {
    frameCount: null,
    async read() {
      // A bitmap from an animated blob uses the format's default image or first frame.
      const image = await createImageBitmap(new Blob([bytes], { type: MEDIA_TYPE }));
      try {
        const { width, height } = image;
        return { frame: readPixels(image, width, height, null), facts: { width, height, frameCount: null } };
      } finally {
        image.close();
      }
    },
    close: () => { canvas = undefined; },
  };
}
