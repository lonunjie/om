// SPDX-License-Identifier: MPL-2.0

import type {
  ExportContent,
  ExportFormat,
  HistoryKind,
  ImportMode,
  PortabilityDetailCode,
  PortabilityFailure,
  PortabilityReport,
  ReportSection,
} from "./portability";

export interface PortabilityCopy {
  readonly title: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly retry: string;
  readonly back: string;
  readonly reasons: Readonly<Record<PortabilityFailure, string>>;
  readonly format: Readonly<Record<ExportFormat, string>>;
  readonly mode: Readonly<Record<ImportMode, string>>;
  readonly export: {
    readonly title: string;
    readonly scope: string;
    readonly format: string;
    readonly formatHint: Readonly<Record<ExportFormat, string>>;
    readonly history: string;
    readonly historyKind: Readonly<Record<HistoryKind, string>>;
    readonly allHistory: string;
    readonly chooseHistory: string;
    readonly noHistory: string;
    readonly dates: string;
    readonly dateHint: string;
    readonly from: string;
    readonly to: string;
    readonly contents: string;
    readonly content: Readonly<Record<ExportContent, string>>;
    readonly noFormats: string;
    readonly chooseFormat: string;
    readonly chooseScope: string;
    readonly review: string;
    readonly plaintext: string;
    readonly archiveRetention: string;
    readonly readableRetention: string;
    readonly includeDisappearing: string;
    readonly disappearingWarning: string;
    readonly save: string;
    readonly saving: string;
    readonly cancelled: string;
    readonly failed: string;
    readonly saved: string;
    readonly again: string;
  };
  readonly import: {
    readonly title: string;
    readonly chooseFile: string;
    readonly changeFile: string;
    readonly choosing: string;
    readonly chooseFirst: string;
    readonly mode: string;
    readonly modeHint: Readonly<Record<ImportMode, string>>;
    readonly start: string;
    readonly importing: string;
    readonly fileCancelled: string;
    readonly cancelled: string;
    readonly chooseFailed: string;
    readonly failed: string;
    readonly imported: Readonly<Record<ImportMode, string>>;
  };
  readonly report: {
    readonly title: string;
    readonly source: string;
    readonly destination: string;
    readonly from: Readonly<Record<PortabilityReport["source"], string>>;
    readonly section: Readonly<Record<ReportSection, string>>;
    readonly detail: Readonly<Record<PortabilityDetailCode, string>>;
    readonly count: (label: string, count: number) => string;
    readonly empty: string;
  };
}
