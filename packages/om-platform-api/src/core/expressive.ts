// SPDX-License-Identifier: MPL-2.0
import type { AttachmentPrepareResult } from "./attachments";
import type { MessageId, ProfileExecutionContext } from "./ids";
import type { ExpressiveDecodeResult, ExpressiveMediaApi, ValidatedExpressiveAsset } from "../platform/expressive";

export const PACK_FILE_MEDIA_TYPE = "application/vnd.om.content-pack";
export const PACK_FILE_MAX_BYTES = 32 * 1024 * 1024;

export type ExpressiveSource =
  | { readonly kind: "attachment" | "inline"; readonly index: number }
  | { readonly kind: "reaction"; readonly key: string; readonly mine: boolean };

/** The existing Core SendExpressive target. The prepared item's kind distinguishes stickers from loops. */
export type ExpressiveTarget = "Attachment" | "Inline"
  /** Echo the prepared asset's canonical reaction key. Core verifies the match. */
  | { readonly Reaction: { readonly message_id: MessageId; readonly key: string } }
  | { readonly Edit: { readonly message_id: MessageId } };

export interface ExpressivePackItem {
  readonly packId: string;
  readonly releaseId: string;
  readonly itemId: string;
}

export type ExpressiveCachePolicy = "Full" | "Lazy" | "FavouritesOnly";

export interface ExpressivePackView {
  readonly releaseId: string;
  readonly title: string;
  readonly signedBy: readonly (readonly number[])[];
  readonly itemCount: number;
  readonly cachePolicy: ExpressiveCachePolicy;
  readonly favourites: readonly string[];
  readonly cachedBlobs: number;
}

export interface ExpressivePicker {
  readonly packId: string;
  readonly pack: ExpressivePackView;
  readonly recents: readonly string[];
  readonly items: readonly {
    readonly itemId: string;
    readonly blobId: string;
    readonly ciphertextDigest?: string;
    readonly kind: ValidatedExpressiveAsset["kind"];
    readonly label: string;
    readonly altText: string;
    readonly fallbackText: string | null;
    /** The canonical custom-reaction key, or null when the item cannot be sent as a reaction. */
    readonly reactionKey: string | null;
    readonly cached: boolean;
    readonly favourite: boolean;
  }[];
}

export interface ExpressivePackInput {
  readonly release: Uint8Array;
  readonly control?: Uint8Array;
}

/** Confirm exactly the preview shown to the person, including the controller head. */
export interface ExpressivePackConfirmation {
  readonly releaseId: string;
  readonly controlDigest: string | null;
}

export interface ExpressivePackPreview extends ExpressivePackConfirmation {
  readonly packId: string;
  readonly title: string;
  readonly sequence: number;
  readonly description: string | null;
  readonly creatorDisplay: string | null;
  /** Display metadata. These strings do not authorize retrieval. */
  readonly sourceUrls: readonly string[];
  readonly spdxId: string | null;
  readonly licenseUrl: string | null;
  readonly licenseTextHash: string | null;
  readonly itemCount: number;
  /** Total bytes of the distinct declared plaintext objects, in decimal. */
  readonly size: string;
  readonly controllerCount: number;
  readonly threshold: number;
  readonly signedBy: readonly (readonly number[])[];
}

export type ExpressivePackFileReview =
  | { readonly ticket: number; readonly preview: ExpressivePackPreview }
  | { readonly refused: string };

export type ExpressivePackIntent =
  | { readonly kind: "install_pack_file"; readonly ticket: number; readonly confirmation: ExpressivePackConfirmation }
  | { readonly kind: "install_pack"; readonly input: ExpressivePackInput; readonly confirmation: ExpressivePackConfirmation;
      readonly policy: ExpressiveCachePolicy; readonly favourites: readonly string[];
      /** Ciphertext already present in this profile’s durable cache. */
      readonly cached: readonly { readonly blobId: string; readonly ciphertext: Uint8Array }[] }
  | { readonly kind: "remove_pack"; readonly packId: string }
  | (ExpressivePackItem & { readonly kind: "update_picker"; readonly change: { readonly kind: "favourite"; readonly present: boolean } | { readonly kind: "recent" } });

export type ExpressivePackQuery =
  | { readonly kind: "preview_pack"; readonly input: ExpressivePackInput }
  | { readonly kind: "packs" }
  | { readonly kind: "picker"; readonly packId: string }
  | { readonly kind: "open"; readonly messageId: MessageId; readonly source: ExpressiveSource };

export type ExpressivePlaceholder = { readonly state: "placeholder"; readonly altText: string };
export type OpenedExpressive = ExpressivePlaceholder
  | { readonly state: "renderable"; readonly asset: ValidatedExpressiveAsset; readonly altText: string };

export interface ExpressiveQueryResult {
  preview_pack: { readonly preview: ExpressivePackPreview } | { readonly refused: string };
  packs: readonly (readonly [packId: string, pack: ExpressivePackView])[];
  picker: ExpressivePicker | null;
  open: OpenedExpressive | null;
}

export type DecodedExpressive = ExpressivePlaceholder
  | (Extract<ExpressiveDecodeResult, { readonly state: "decoded" }> & { readonly altText: string });

/** Core capability for the optional expressive-media host. Pack bytes are supplied by the local cache/import path. */
export type ExpressiveBlobPolicy = "fresh" | "shared_blob";

export interface ExpressiveBlobExposure {
  readonly kind: "shared_blob_linkage";
  readonly line: string;
}

export interface ExpressiveApi {
  available(): boolean;
  reviewPackFile(ctx: ProfileExecutionContext, bytes: Uint8Array): Promise<ExpressivePackFileReview>;
  discardPackFile(ctx: ProfileExecutionContext, ticket: number): Promise<void>;
  readCached(ctx: ProfileExecutionContext, digest: string): Promise<Uint8Array | null>;
  decodeCached(ctx: ProfileExecutionContext, selection: ExpressivePackItem, ciphertext: Uint8Array, decoder: ExpressiveMediaApi,
    options: { readonly reducedMotion: boolean }): Promise<DecodedExpressive | null>;
  do(ctx: ProfileExecutionContext, intent: ExpressivePackIntent): Promise<{ readonly accepted: true } | { readonly refused: string }>;
  query<Q extends ExpressivePackQuery>(ctx: ProfileExecutionContext, query: Q): Promise<ExpressiveQueryResult[Q["kind"]]>;
  blobExposure?(policy: ExpressiveBlobPolicy): ExpressiveBlobExposure | null;
  prepare(ctx: ProfileExecutionContext, selection: ExpressivePackItem, ciphertext: Uint8Array, policy?: ExpressiveBlobPolicy): Promise<AttachmentPrepareResult>;
  /** Feeds Core's validated bytes to the platform and returns frames only after Core accepts the observations. */
  decode(ctx: ProfileExecutionContext, messageId: MessageId, source: ExpressiveSource, decoder: ExpressiveMediaApi,
    options: { readonly reducedMotion: boolean }): Promise<DecodedExpressive | null>;
}
