// SPDX-License-Identifier: MPL-2.0

export type ExportFormat = "archive" | "readable";
export type ImportMode = "read_only" | "same_account";
export type HistoryKind = "chat" | "space" | "publication";

export const exportContent = [
  "media",
  "contacts",
  "aliases",
  "provenance",
  "local_preferences",
] as const;
export type ExportContent = (typeof exportContent)[number];

export interface HistoryTarget {
  readonly id: string;
  readonly kind: HistoryKind;
}

export interface HistoryChoice extends HistoryTarget {
  readonly name: string;
}

export type UnavailableReason =
  | "device_unavailable"
  | "unsupported"
  | "policy_denied"
  | "unauthorized_device"
  | "wrong_account";

export type Availability = "available" | UnavailableReason;

export interface ExportChoices {
  readonly history: readonly HistoryChoice[];
  readonly contents: Readonly<Record<ExportContent, Availability>>;
}

export interface PortabilityView {
  readonly formats: Readonly<
    Record<ExportFormat, ExportChoices | UnavailableReason>
  >;
  readonly import: Availability;
}

export interface ExportScope {
  readonly history: "all" | readonly HistoryTarget[];
  /** Local calendar dates as entered. The adapter owns conversion and Core validates the range. */
  readonly from: string | null;
  readonly to: string | null;
  readonly contents: readonly ExportContent[];
}

/** An explicit save action after the warning. The service revalidates the scope and retention rules. */
export type ExportIntent = { readonly scope: ExportScope } & (
  | { readonly format: "archive" }
  | { readonly format: "readable"; readonly includeDisappearing: boolean }
);

export type PortabilityFailure =
  | UnavailableReason
  | "invalid_scope"
  | "not_an_archive"
  | "encrypted_unsupported"
  | "invalid_archive"
  | "integrity_failure"
  | "too_large"
  | "read_failed"
  | "write_failed";

export type PortabilityDetailCode =
  | "history"
  | "media"
  | "contacts"
  | "aliases"
  | "provenance"
  | "local_preferences"
  | "canonical_evidence"
  | "canonical_evidence_absent"
  | "retention_unclassified"
  | "outside_scope"
  | "private_material"
  | "view_once"
  | "disappearing"
  | "expired"
  | "stale_control"
  | "control_needs_live_validation"
  | "unverified_objects"
  | "unverified_authorship"
  | "quarantined_records"
  | "missing_media"
  | "unverified_timestamps"
  | "reactions_flattened"
  | "edits_unavailable"
  | "topics_unavailable"
  | "deletion_history_unknown";

export interface PortabilityDetail {
  readonly code: PortabilityDetailCode;
  readonly count?: number;
}

export const reportSections = [
  "preserved",
  "lossy",
  "unverifiableAuthorship",
  "missingMedia",
  "timestampLimitations",
  "unsupportedFeatures",
] as const;
export type ReportSection = (typeof reportSections)[number];

export interface PortabilityReport {
  readonly source: "local_history" | "archive";
  readonly destination: ExportFormat | ImportMode;
  readonly details: Readonly<
    Partial<Record<ReportSection, readonly PortabilityDetail[]>>
  >;
}

type Rejected = {
  readonly status: "rejected";
  readonly reason: PortabilityFailure;
};
type Cancelled = { readonly status: "cancelled" };

export type ExportAnswer =
  | Rejected
  | Cancelled
  | { readonly status: "saved"; readonly report: PortabilityReport };

/** A platform-owned file reference and Core's inspection of the available modes for this file. */
export interface ImportSource {
  readonly handle: string;
  readonly name: string;
  readonly modes: Readonly<Record<ImportMode, Availability>>;
}

export type ImportSourceAnswer =
  | Rejected
  | Cancelled
  | { readonly status: "selected"; readonly source: ImportSource };

export interface ImportIntent {
  readonly handle: string;
  readonly mode: ImportMode;
}

export type ImportAnswer =
  | Rejected
  | Cancelled
  | {
      readonly status: "imported";
      readonly mode: ImportMode;
      readonly report: PortabilityReport;
    };

/**
 * One service per captured profile context, supplied only by a supporting host. Core owns
 * validation, staging, authority and fidelity. The host owns file picking and saving.
 * Mutation promises stay pending through reconciliation and reject only if no operation
 * started. A completed answer means the operation committed and the host finished its part.
 */
export interface PortabilityService {
  state(): Promise<PortabilityView>;
  exportHistory(intent: ExportIntent): Promise<ExportAnswer>;
  chooseImportSource(): Promise<ImportSourceAnswer>;
  importHistory(intent: ImportIntent): Promise<ImportAnswer>;
}
