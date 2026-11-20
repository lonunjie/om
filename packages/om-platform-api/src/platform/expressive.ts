/** Core hands off the same plaintext and header facts returned by its expressive asset validator.
 * The caller owns these bytes and must keep them immutable through decode(). */
export interface ValidatedExpressiveAsset {
  readonly mediaProfileId: string;
  readonly kind: "sticker" | "custom_emoji" | "reaction_asset" | "loop_animation";
  readonly bytes: Uint8Array<ArrayBuffer>;
  readonly facts: {
    readonly width: number;
    readonly height: number;
    readonly frames: number;
    readonly durationMs: number | null;
    readonly encodedSize: number;
    readonly totalPixels: number;
  };
}

/** Native decoder observations for Core. A bitmap-only path cannot observe the track's frame count. */
export interface ExpressiveDecodeFacts {
  readonly width: number;
  readonly height: number;
  readonly frameCount: number | null;
}

export interface ExpressiveFrame {
  readonly rgba: Uint8ClampedArray<ArrayBuffer>;
  readonly durationUs: number | null;
}

export interface ExpressiveDecodeFallback {
  readonly state: "fallback";
  readonly reason: "unsupported_profile" | "invalid_budget" | "unavailable" | "decode_failed"
    | "decode_timeout" | "decoder_mismatch";
}

export type ExpressiveFrameResult =
  | { readonly state: "frame"; readonly frame: ExpressiveFrame }
  | { readonly state: "paused" | "closed" }
  | ExpressiveDecodeFallback;

export interface ExpressivePlayback {
  play(): void;
  pause(): void;
  /** Pulls one frame. The consumer owns pacing and replaces its previous frame. */
  nextFrame(): Promise<ExpressiveFrameResult>;
  /** Releases decoder resources on unmount, message expiry or profile change. */
  close(): void;
}

export type ExpressiveDecodeResult =
  | { readonly state: "decoded"; readonly facts: ExpressiveDecodeFacts; readonly frame: ExpressiveFrame;
      readonly playback?: ExpressivePlayback }
  | ExpressiveDecodeFallback;

export interface ExpressiveMediaApi {
  decode(asset: ValidatedExpressiveAsset, options: { readonly reducedMotion: boolean }): Promise<ExpressiveDecodeResult>;
}
