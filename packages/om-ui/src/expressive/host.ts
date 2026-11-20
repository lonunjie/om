// SPDX-License-Identifier: MPL-2.0
import type {
  FilesApi, ExpressiveMediaApi, ExpressivePackInput, ExpressiveSource, MessageId, ProfileExecutionContext,
} from "@om/platform-api";
import type { ExpressiveCopy } from "./copy";

export interface ExpressiveDescription {
  readonly altText: string;
  readonly canOpen: boolean;
  /** The host can open these bytes without a network request. */
  readonly local?: boolean;
}

export interface ExpressiveReview {
  readonly input: ExpressivePackInput;
  readonly cached?: readonly { readonly blobId: string; readonly ciphertext: Uint8Array }[];
}

/** Host I/O over the captured profile. A pack never supplies a renderer, route, or style. */
export interface ExpressiveHost {
  readonly files?: FilesApi;
  readonly decoder?: ExpressiveMediaApi;
  readonly describe: (ctx: ProfileExecutionContext, messageId: MessageId, source: ExpressiveSource) => Promise<ExpressiveDescription | null>;
  readonly readCached?: (ctx: ProfileExecutionContext, blobId: string) => Promise<Uint8Array | null>;
  readonly review?: ExpressiveReview;
}

export interface ExpressiveSurface extends ExpressiveHost {
  readonly copy: ExpressiveCopy;
}
