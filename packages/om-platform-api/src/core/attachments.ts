import type { MessageId, ProfileExecutionContext } from "./ids";

/** What the sender knows about a file besides its bytes (untrusted display data on the receiving side). */
export interface AttachmentMeta {
  readonly mediaType: string;
  readonly filename?: string;
  readonly width?: number;
  readonly height?: number;
  readonly durationMs?: number;
}

/** What the person's delivery service advertises for files (`om.blob.spool.v1`); `null` from `limits()` means it offers none. */
export interface AttachmentLimits {
  /** Largest single object in bytes (the file plus a fixed 16 bytes). */
  readonly maxBlobBytes: number;
  readonly maxSpoolBytes?: number;
  readonly maxBlobs?: number;
}

/** Typed refusals before anything leaves the device. The UI owns the words. */
export type AttachmentRefusal =
  | { readonly error: "too_large"; /** Largest allowed file in bytes. */ readonly max: number }
  | { readonly error: "storage_full" | "no_blob_store" | "empty" | "invalid" | "not_found" | "integrity_failure" | "unsupported" | "unavailable" };

export type AttachmentPrepareResult =
  | { readonly ok: true; readonly blobId: string }
  | { readonly ok: false; readonly refusal: AttachmentRefusal };

export type AttachmentOpenResult =
  | { readonly ok: true; readonly bytes: Uint8Array; readonly mediaType: string; readonly filename?: string }
  | { readonly ok: false; readonly refusal: AttachmentRefusal };

/**
 * The file path of a CoreClient: seal on this device (`prepare`), send with the `SendAttachments` intent, fetch and open
 * on the receiving side (`open`; never automatic). All crypto is Core's; the host only moves ciphertext.
 */
export interface AttachmentApi {
  /** The advertised limits, or `null` when the delivery service holds no files (then every `prepare` is refused). */
  limits(): AttachmentLimits | null;
  prepare(ctx: ProfileExecutionContext, bytes: Uint8Array, meta: AttachmentMeta): Promise<AttachmentPrepareResult>;
  /**
   * Seal a file that is too big for the delivery service so it can be offered straight to the other person
   * (`SendAttachmentsDirect`). Nothing is stored anywhere; Core holds the sealed object while this app runs. Refused with a
   * typed reason when the file would fit the delivery service (use `prepare`) or cannot be sent at all.
   */
  prepareDirect?(ctx: ProfileExecutionContext, bytes: Uint8Array, meta: AttachmentMeta): Promise<AttachmentPrepareResult>;
  /** Fetch from the sender's store, check the digest, decrypt. Resolves with a typed refusal, never partial content. */
  open(ctx: ProfileExecutionContext, messageId: MessageId, index: number): Promise<AttachmentOpenResult>;
}
