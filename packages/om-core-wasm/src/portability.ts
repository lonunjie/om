// SPDX-License-Identifier: MPL-2.0
// Typed commands for the engineering-only scope module. The host owns live verification, time and file handling.
import type { WasmCore } from "./wasm";

export type PortabilityRefusalClass =
  | "malformed_request" | "not_an_archive" | "encrypted_unsupported"
  | "invalid_archive" | "integrity_failure" | "too_large" | "invalid_scope"
  | "unauthorized_device" | "wrong_account";

export type PortabilityErrorCodes = Readonly<Record<PortabilityRefusalClass, number>>;
export interface PortabilityRefusal {
  readonly refused: PortabilityRefusalClass;
  /** Stable code supplied by the wasm registry. */
  readonly code: number;
  /** Stable reader class, when the archive reader refused the bytes. */
  readonly reason?: string;
}
export type PortabilityAnswer<T> = T | PortabilityRefusal;
export type PortabilityUint = number | bigint;

export interface PortabilityInspection {
  readonly manifest: {
    readonly formatVersion: "om-archive-v1";
    readonly archiveId: string;
    readonly encryptionProfile: null;
    readonly objects: number;
    readonly blobs: number;
    readonly records: number;
    readonly missingBlobs: readonly string[];
  };
  readonly modes: {
    readonly read_only: "available";
    readonly same_account: "requires_live_validation";
  };
}

export interface PortabilityControlVersion {
  readonly stream: string;
  readonly version: PortabilityUint;
}

/** Host verdict for these exact bytes and declared type. Missing verdicts are unverified.
 * A host without expiry enforcement must leave disappearing objects unverified.
 */
export interface PortabilityVerdict {
  readonly objectType: string;
  readonly sha256: string;
  readonly verified: {
    readonly control?: PortabilityControlVersion;
    readonly evidence?: "foreign_identity_link" | "bridge_attestation";
  } | null;
}

export type PortabilityObjectStatus =
  | { readonly kind: "verified_history" | "unverified" }
  | { readonly kind: "stale_control"; readonly archived: string; readonly live: string }
  | { readonly kind: "control_needs_live_validation"; readonly archived: string };

export type PortabilityProvenance =
  | "foreign_export_label" | "bridge_attested" | "cryptographically_linked" | "user_annotation";

export interface PortabilityImportView {
  readonly objects: readonly { readonly path: string; readonly status: PortabilityObjectStatus }[];
  readonly records: readonly {
    readonly importedRecordId: string;
    readonly sourceNetworkNamespace: string;
    readonly claimedLevel: PortabilityProvenance;
    readonly effectiveLevel: PortabilityProvenance;
    readonly downgraded: boolean;
  }[];
  readonly quarantined: readonly {
    readonly path: string;
    readonly index: number;
    readonly reason:
      | "not_a_map" | "native_authorship_claim" | "unknown_field"
      | "missing_field" | "unknown_provenance" | "malformed";
  }[];
  readonly missingBlobs: readonly string[];
}

export type PortabilityItem =
  | { readonly kind: "object"; readonly objectType: string; readonly bytesHex: string }
  | {
    readonly kind: "blob";
    readonly blobId: string;
    readonly mediaType: string;
  } & (
    | { readonly bytesHex: string }
    | { readonly bytesHex?: null; readonly declared: { readonly size: PortabilityUint; readonly sha256: string } }
  )
  | {
    readonly kind: "record";
    readonly recordType: string;
    readonly stem: string;
    readonly bytesHex: string;
  };

export interface PortabilityCandidate {
  readonly item: PortabilityItem;
  readonly conversation?: string;
  readonly time?: PortabilityUint;
  readonly disappearing: boolean;
  readonly viewOnce: boolean;
}

export interface PortabilitySelection {
  readonly conversations?: readonly string[];
  readonly from?: PortabilityUint;
  readonly to?: PortabilityUint;
  readonly includeBlobs: boolean;
}

export interface PortabilityManifestMeta {
  readonly archiveId: string;
  /** Canonical CBOR map. */
  readonly exportScopeHex: string;
  readonly sourceAccountHint?: string | null;
  readonly includedConversations?: readonly string[];
  readonly generatedAtHint: PortabilityUint;
  readonly exporterImplementation?: string;
  readonly provenanceProfiles: readonly string[];
}

export interface PortabilityExportInput {
  readonly candidates: readonly PortabilityCandidate[];
  readonly selection: PortabilitySelection;
  readonly meta: PortabilityManifestMeta;
}

export type PortabilityExclusion = "secret_type" | "view_once" | "disappearing" | "not_selected";

export interface PortabilityReport {
  readonly objects: number;
  readonly blobs: number;
  readonly records: number;
  readonly omittedBlobs: readonly string[];
  readonly excluded: Readonly<Partial<Record<PortabilityExclusion, number>>>;
}

export interface PortabilityExportPlan {
  readonly report: PortabilityReport;
}

export interface PortabilityExport extends PortabilityExportPlan {
  readonly archiveHex: string;
}

export interface PortabilityReadableConversation {
  readonly title: string;
  readonly messages: readonly {
    readonly time: string;
    readonly author: string;
    readonly text: string;
    readonly disappearing: boolean;
    readonly viewOnce: boolean;
  }[];
}

export interface PortabilityReadableExport {
  readonly files: readonly { readonly path: string; readonly bytesHex: string }[];
  readonly disappearingChoice: "excluded" | "included_explicitly";
  readonly excludedDisappearing: number;
  readonly excludedViewOnce: number;
  readonly isMigrationFormat: false;
  readonly warning: string;
}

/** A history item classified by the host verifier. */
export interface PortabilityRehydrateItem {
  readonly id: string;
  readonly viewOnce: boolean;
  readonly expiry?: PortabilityUint;
  readonly device?: string;
}

/** The host derives the authorization and account match from its live protocol state. */
export interface PortabilityRehydrateContext {
  readonly authorizedDevice: boolean;
  readonly accountMatches: boolean;
  readonly now: PortabilityUint;
  readonly canEnforceExpiry: boolean;
  readonly liveControl: readonly PortabilityControlVersion[];
  readonly revokedDevices: readonly string[];
}

export interface PortabilityRehydratePlan {
  readonly import: readonly string[];
  readonly scheduleDeletion: readonly (readonly [id: string, expiry: string])[];
  readonly droppedExpired: readonly string[];
  readonly refusedViewOnce: readonly string[];
  readonly skippedUnenforceable: readonly string[];
  readonly historyOnlyRevokedDevice: readonly string[];
  readonly needsLiveValidation: readonly string[];
  readonly staleControl: readonly string[];
  readonly freshSessionsRequired: true;
}

export interface PortabilityEngine {
  registry(): { readonly errorCodes: PortabilityErrorCodes };
  inspect(archiveHex: string): PortabilityAnswer<PortabilityInspection>;
  importReadonly(archiveHex: string, liveControl: readonly PortabilityControlVersion[],
    verdicts: readonly PortabilityVerdict[]): PortabilityAnswer<PortabilityImportView>;
  /** Preview selection and exclusions. Export validates the final archive. */
  exportPlan(input: PortabilityExportInput): PortabilityAnswer<PortabilityExportPlan>;
  export(input: PortabilityExportInput): PortabilityAnswer<PortabilityExport>;
  /** Pass true only after the person explicitly chose to keep disappearing items in plaintext. */
  readableExport(conversations: readonly PortabilityReadableConversation[],
    includeDisappearing?: boolean): PortabilityAnswer<PortabilityReadableExport>;
  rehydratePlan(view: PortabilityImportView, items: readonly PortabilityRehydrateItem[],
    context: PortabilityRehydrateContext): PortabilityAnswer<PortabilityRehydratePlan>;
}

/** JSON decimal strings preserve the full u64 range. Unsafe JS numbers are refused by wasm. */
function uint(n: PortabilityUint): number | string {
  if (typeof n === "bigint") return n.toString();
  return Number.isSafeInteger(n) ? n : "";
}

function optionalUint(n: PortabilityUint | undefined): number | string | undefined {
  return n === undefined ? undefined : uint(n);
}

function controlVersion({ stream, version }: PortabilityControlVersion) {
  return { stream, version: uint(version) };
}

function exportInput({ candidates, selection, meta }: PortabilityExportInput) {
  return {
    candidates: candidates.map(({ item, ...candidate }) => ({
      ...candidate,
      time: optionalUint(candidate.time),
      item: item.kind === "blob" && "declared" in item
        ? { ...item, declared: { ...item.declared, size: uint(item.declared.size) } }
        : item,
    })),
    selection: { ...selection, from: optionalUint(selection.from), to: optionalUint(selection.to) },
    meta: { ...meta, generatedAtHint: uint(meta.generatedAtHint) },
  };
}

export function createPortabilityEngine(core: Pick<WasmCore, "call">): PortabilityEngine {
  const q = <T>(request: Record<string, unknown>): T => core.call({ cmd: "portability", request }) as T;
  return {
    registry: () => q({ op: "registry" }),
    inspect: (archiveHex) => q({ op: "inspect", archiveHex }),
    importReadonly: (archiveHex, liveControl, verdicts) => q({
      op: "import_readonly", archiveHex, liveControl: liveControl.map(controlVersion),
      verdicts: verdicts.map(({ verified, ...verdict }) => ({
        ...verdict,
        verified: verified === null ? null : {
          evidence: verified.evidence,
          control: verified.control && controlVersion(verified.control),
        },
      })),
    }),
    exportPlan: (input) => q({ op: "export_plan", ...exportInput(input) }),
    export: (input) => q({ op: "export", ...exportInput(input) }),
    readableExport: (conversations, includeDisappearing) => q({
      op: "readable_export", conversations, includeDisappearing,
    }),
    rehydratePlan: (view, items, context) => q({
      op: "rehydrate_plan",
      view,
      items: items.map((item) => ({ ...item, expiry: optionalUint(item.expiry) })),
      context: { ...context, now: uint(context.now), liveControl: context.liveControl.map(controlVersion) },
    }),
  };
}
