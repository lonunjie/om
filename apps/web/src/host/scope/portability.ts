// SPDX-License-Identifier: MPL-2.0
import { CoreCallError, fromHex, toHex } from "@om/core-wasm";
import { captureProfileContext, type ProfileExecutionContext } from "@om/platform-api";
import type {
  ExportContent, ExportFormat, ExportIntent, ExportChoices, HistoryTarget, PortabilityDetail,
  PortabilityFailure, PortabilityReport, PortabilityService, PortabilityView,
} from "../../../../../packages/om-ui/src/portability/portability";
import type {
  PortabilityControlVersion, PortabilityEngine, PortabilityExportInput, PortabilityImportView,
  PortabilityReadableConversation, PortabilityRefusal, PortabilityVerdict,
} from "../../../../../packages/om-core-wasm/src/portability";
import { MAX_WASM_COMMAND_BYTES } from "../../../../../packages/om-core-wasm/src/wasm";

interface FileEntry {
  readonly name: string;
  readonly bytes: Uint8Array;
}

export interface FileHost {
  pick(ctx: ProfileExecutionContext): Promise<FileEntry | null>;
  save(ctx: ProfileExecutionContext, format: ExportFormat, files: readonly FileEntry[]): Promise<"saved" | "cancelled">;
}

interface HistorySelection {
  readonly targets: readonly HistoryTarget[];
  readonly from?: number;
  readonly to?: number;
  readonly contents: readonly ExportContent[];
}

interface ReadableCandidate {
  readonly target: HistoryTarget;
  readonly title: string;
  readonly messages: readonly {
    readonly timestamp: number;
    readonly message: PortabilityReadableConversation["messages"][number];
  }[];
}

interface ReadOnlyHistory {
  /** The verifier's verdicts and live control facts already held by the captured profile. */
  facts(ctx: ProfileExecutionContext): Promise<{
    readonly liveControl: readonly PortabilityControlVersion[];
    readonly verdicts: readonly PortabilityVerdict[];
  }>;
  /**
   * Atomically stage eligible items in the archive-only view. The source bytes are transient.
   * Enforce original expiry and drop ineligible items. Repeating the same handle must be harmless.
   */
  stage(ctx: ProfileExecutionContext, archive: {
    readonly handle: string;
    readonly archiveHex: string;
    readonly view: PortabilityImportView;
  }): Promise<void>;
}

export interface HistorySource {
  choices(ctx: ProfileExecutionContext): Promise<PortabilityView["formats"]>;
  /** Supply candidates only for the selected content categories. Core applies the archive's scope and retention rules. */
  archive?(ctx: ProfileExecutionContext, selection: HistorySelection): Promise<Pick<PortabilityExportInput, "candidates" | "meta">>;
  readable?(ctx: ProfileExecutionContext, selection: HistorySelection): Promise<readonly ReadableCandidate[]>;
  readonly readOnly?: ReadOnlyHistory;
}

export const emptyHistory: HistorySource = Object.freeze({
  choices: (): Promise<PortabilityView["formats"]> => Promise.resolve({
    archive: "device_unavailable", readable: "device_unavailable",
  }),
});

export interface PortabilityServiceOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engine: PortabilityEngine;
  readonly files: FileHost;
  readonly history?: HistorySource;
}

const cancelled = { status: "cancelled" } as const;
const rejected = (reason: PortabilityFailure) => ({ status: "rejected", reason } as const);
const failure = ({ refused }: PortabilityRefusal) => rejected(refused === "malformed_request" ? "invalid_scope" : refused);
const commandFailure = (error: unknown, fallback: PortabilityFailure) =>
  rejected(error instanceof CoreCallError && error.code === "size_limit" ? "too_large" : fallback);

function sameTarget(left: HistoryTarget, right: HistoryTarget): boolean {
  return left.kind === right.kind && left.id === right.id;
}

function localDay(value: string | null, end: boolean): number | undefined {
  if (value === null) return undefined;
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(value);
  if (parts === null) return NaN;
  const [year, month, day] = parts.slice(1).map(Number) as [number, number, number];
  const date = new Date(0);
  date.setHours(0, 0, 0, 0);
  date.setFullYear(year, month - 1, day);
  if (date.getFullYear() !== year || date.getMonth() !== month - 1 || date.getDate() !== day) return NaN;
  if (!end) return date.getTime();
  date.setHours(24, 0, 0, 0);
  return date.getTime() - 1;
}

function selectionFor(intent: ExportIntent, choices: ExportChoices): HistorySelection | PortabilityFailure {
  const targets = intent.scope.history === "all"
    ? choices.history
    : intent.scope.history;
  if (targets.length === 0 || targets.some((target) => !choices.history.some((choice) => sameTarget(choice, target)))) {
    return "invalid_scope";
  }
  for (const content of intent.scope.contents) {
    if (!Object.hasOwn(choices.contents, content)) return "invalid_scope";
    const availability = choices.contents[content];
    if (availability !== "available") return availability;
  }
  const from = localDay(intent.scope.from, false);
  const to = localDay(intent.scope.to, true);
  if ((from !== undefined && (!Number.isFinite(from) || from < 0)) ||
      (to !== undefined && (!Number.isFinite(to) || to < (from ?? 0)))) return "invalid_scope";
  return {
    targets,
    contents: intent.scope.contents,
    ...(from === undefined ? {} : { from }),
    ...(to === undefined ? {} : { to }),
  };
}

function details(code: PortabilityDetail["code"], count: number): PortabilityDetail[] {
  return count === 0 ? [] : [{ code, count }];
}

export function createPortabilityService({
  ctx: inputCtx, engine, files, history = emptyHistory,
}: PortabilityServiceOptions): PortabilityService {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);
  let selected: { readonly handle: string; readonly archiveHex: string } | undefined;
  let selecting = 0;

  async function formats(): Promise<PortabilityView["formats"]> {
    const choices = await history.choices(ctx);
    return {
      archive: typeof choices.archive === "string" || history.archive !== undefined ? choices.archive : "device_unavailable",
      readable: typeof choices.readable === "string" || history.readable !== undefined ? choices.readable : "device_unavailable",
    };
  }

  return {
    async state() {
      return { formats: await formats(), import: history.readOnly === undefined ? "device_unavailable" : "available" };
    },
    async exportHistory(inputIntent) {
      const intent = {
        ...inputIntent,
        scope: {
          ...inputIntent.scope,
          history: inputIntent.scope.history === "all" ? "all" : inputIntent.scope.history.map((target) => ({ ...target })),
          contents: [...inputIntent.scope.contents],
        },
      } satisfies ExportIntent;
      const choices = (await formats())[intent.format];
      if (typeof choices === "string") return rejected(choices);
      const selection = selectionFor(intent, choices);
      if (typeof selection === "string") return rejected(selection);
      let output: readonly FileEntry[];
      let report: PortabilityReport["details"];
      try {
        if (intent.format === "archive") {
          if (history.archive === undefined) return rejected("device_unavailable");
          const input = await history.archive(ctx, selection);
          const result = engine.export({
            ...input,
            selection: {
              conversations: [...new Set(selection.targets.map(({ id }) => id))],
              ...(selection.from === undefined ? {} : { from: selection.from }),
              ...(selection.to === undefined ? {} : { to: selection.to }),
              includeBlobs: selection.contents.includes("media"),
            },
          });
          if ("refused" in result) return failure(result);
          output = [{ name: "om-history.zip", bytes: fromHex(result.archiveHex) }];
          const { objects, blobs, records, omittedBlobs, excluded } = result.report;
          report = {
            preserved: [...details("canonical_evidence", objects), ...details("media", blobs - omittedBlobs.length),
              ...details("history", records)],
            lossy: [
              ...details("private_material", excluded.secret_type ?? 0), ...details("view_once", excluded.view_once ?? 0),
              ...details("disappearing", excluded.disappearing ?? 0), ...details("outside_scope", excluded.not_selected ?? 0),
            ],
            missingMedia: details("missing_media", omittedBlobs.length),
          };
        } else {
          if (history.readable === undefined) return rejected("device_unavailable");
          const candidates = await history.readable(ctx, selection);
          const conversations = candidates
            .filter(({ target }) => selection.targets.some((chosen) => sameTarget(chosen, target)))
            .map(({ title, messages }) => ({
              title,
              messages: messages.filter(({ timestamp }) =>
                (selection.from === undefined || timestamp >= selection.from) &&
                (selection.to === undefined || timestamp <= selection.to)).map(({ message }) => message),
            }));
          const result = engine.readableExport(conversations, intent.includeDisappearing);
          if ("refused" in result) return failure(result);
          output = result.files.map(({ path, bytesHex }) => ({ name: path, bytes: fromHex(bytesHex) }));
          report = { lossy: [
            ...details("view_once", result.excludedViewOnce), ...details("disappearing", result.excludedDisappearing),
          ] };
        }
      } catch (error) {
        return commandFailure(error, "read_failed");
      }
      try {
        return await files.save(ctx, intent.format, output) === "cancelled" ? cancelled
          : { status: "saved", report: { source: "local_history", destination: intent.format, details: report } };
      } catch {
        return rejected("write_failed");
      }
    },
    async chooseImportSource() {
      const request = ++selecting;
      selected = undefined;
      try {
        const file = await files.pick(ctx);
        if (file === null || request !== selecting) return cancelled;
        if (file.bytes.byteLength * 2 >= MAX_WASM_COMMAND_BYTES) return rejected("too_large");
        const archiveHex = toHex(file.bytes);
        const result = engine.inspect(archiveHex);
        if ("refused" in result) return failure(result);
        const handle = globalThis.crypto.randomUUID();
        selected = { handle, archiveHex };
        return {
          status: "selected",
          source: {
            handle, name: file.name,
            modes: { read_only: history.readOnly === undefined ? "device_unavailable" : result.modes.read_only, same_account: "unsupported" },
          },
        };
      } catch (error) {
        return commandFailure(error, "read_failed");
      }
    },
    async importHistory({ handle, mode }) {
      const source = selected;
      if (source === undefined || source.handle !== handle) return rejected("read_failed");
      if (mode !== "read_only") return rejected("unsupported");
      const readOnly = history.readOnly;
      if (readOnly === undefined) return rejected("device_unavailable");
      let imported: PortabilityImportView;
      try {
        const { liveControl, verdicts } = await readOnly.facts(ctx);
        const result = engine.importReadonly(source.archiveHex, liveControl, verdicts);
        if ("refused" in result) return failure(result);
        imported = result;
      } catch (error) {
        return commandFailure(error, "read_failed");
      }
      if (selected !== source) return cancelled;
      try {
        await readOnly.stage(ctx, { ...source, view: imported });
      } catch {
        return rejected("write_failed");
      }
      if (selected === source) selected = undefined;
      const count = (kind: PortabilityImportView["objects"][number]["status"]["kind"]) =>
        imported.objects.filter(({ status }) => status.kind === kind).length;
      return {
        status: "imported",
        mode,
        report: {
          source: "archive", destination: mode,
          details: {
            preserved: details("canonical_evidence", count("verified_history")),
            lossy: [
              ...details("unverified_objects", count("unverified")), ...details("stale_control", count("stale_control")),
              ...details("control_needs_live_validation", count("control_needs_live_validation")),
              ...details("quarantined_records", imported.quarantined.length),
            ],
            unverifiableAuthorship: details("unverified_authorship", imported.records.filter(({ downgraded }) => downgraded).length),
            missingMedia: details("missing_media", imported.missingBlobs.length),
          },
        },
      };
    },
  };
}
