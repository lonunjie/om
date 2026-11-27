// SPDX-License-Identifier: MPL-2.0
import { CoreCallError, fromHex, toHex } from "@om/core-wasm";
import { encode, map, type CborValue } from "@om/home-wire";
import {
  asConversationId, captureProfileContext, type ConversationTimelineItem, type CoreClient,
  type GetConversationList, type GetConversationView, type GetDevices, type ProfileExecutionContext,
} from "@om/platform-api";
import type {
  ExportContent, ExportFormat, ExportIntent, ExportChoices, HistoryTarget, PortabilityDetail,
  PortabilityFailure, PortabilityReport, PortabilityService, PortabilityView,
} from "../../../../../packages/om-ui/src/portability/portability";
import type {
  PortabilityCandidate, PortabilityControlVersion, PortabilityEngine, PortabilityExportInput, PortabilityImportView,
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
  readonly projectionOnly?: true;
  choices(ctx: ProfileExecutionContext): Promise<PortabilityView["formats"]>;
  /** Supply candidates only for the selected content categories. Core applies the archive's scope and retention rules. */
  archive?(ctx: ProfileExecutionContext, selection: HistorySelection): Promise<HistoryArchive>;
  readable?(ctx: ProfileExecutionContext, selection: HistorySelection): Promise<readonly ReadableCandidate[]>;
  readonly readOnly?: ReadOnlyHistory;
}

interface HistoryArchive extends Pick<PortabilityExportInput, "candidates" | "meta"> {
  /** Attachments for which the projection has no plaintext or plaintext digest to declare. */
  readonly missingMedia?: number;
}

const unavailableFormats = Object.freeze({
  archive: "device_unavailable", readable: "device_unavailable",
} as const);

export const emptyHistory: HistorySource = Object.freeze({
  choices: (): Promise<PortabilityView["formats"]> => Promise.resolve({ ...unavailableFormats }),
});

const MAX_HISTORY_CONVERSATIONS = 128;
const MAX_HISTORY_MESSAGES = 4096;
const LOCAL_PROJECTION = "om.local_projection";

class HistorySourceError extends Error {
  constructor(readonly reason: PortabilityFailure) {
    super(reason);
  }
}

type HistoryQuery = GetDevices | GetConversationList | GetConversationView;
type ClassifiedMessage = ConversationTimelineItem & { readonly disappearing: boolean; readonly viewOnce: boolean };

function hasRetentionFlags(message: ConversationTimelineItem): message is ClassifiedMessage {
  return "disappearing" in message && typeof message.disappearing === "boolean" &&
    "viewOnce" in message && typeof message.viewOnce === "boolean";
}

function inDateRange(timestamp: number, { from, to }: HistorySelection): boolean {
  return (from === undefined || timestamp >= from) && (to === undefined || timestamp <= to);
}

export function createCoreHistorySource({ ctx: inputCtx, client }: {
  readonly ctx: ProfileExecutionContext;
  readonly client: Pick<CoreClient, "query">;
}): HistorySource {
  const ctx = captureProfileContext(inputCtx.profileId, inputCtx.generation);

  function requireContext(caller: ProfileExecutionContext): void {
    if (caller.profileId !== ctx.profileId || caller.generation !== ctx.generation) {
      throw new HistorySourceError("device_unavailable");
    }
  }

  async function query<Q extends HistoryQuery>(request: Q) {
    try {
      const result = await client.query(request);
      if ("refused" in result) throw new HistorySourceError("device_unavailable");
      return result;
    } catch {
      throw new HistorySourceError("device_unavailable");
    }
  }

  async function readHistory(caller: ProfileExecutionContext, { targets }: HistorySelection) {
    requireContext(caller);
    if (targets.some(({ kind }) => kind !== "chat")) throw new HistorySourceError("invalid_scope");
    const ids = new Set(targets.map(({ id }) => asConversationId(id)));
    if (ids.size > MAX_HISTORY_CONVERSATIONS) throw new HistorySourceError("too_large");
    let messageCount = 0;
    const history = [];
    for (const id of ids) {
      // The public query returns a whole timeline. Refuse oversized exports before mapping
      // the results instead of silently saving a truncated history.
      const { summary, timeline } = await query({ kind: "GetConversationView", ctx, conversationId: id });
      if (summary.id !== id) throw new HistorySourceError("device_unavailable");
      messageCount += timeline.length;
      if (messageCount > MAX_HISTORY_MESSAGES) throw new HistorySourceError("too_large");
      if (!timeline.every(hasRetentionFlags)) throw new HistorySourceError("device_unavailable");
      history.push({ id, title: summary.nickname ?? summary.displayName, timeline });
    }
    return history;
  }

  const author = (direction: ClassifiedMessage["direction"], title: string) => direction === "outgoing" ? "You" : title;

  return {
    projectionOnly: true,
    async choices(caller) {
      try {
        requireContext(caller);
        const { devices } = await query({ kind: "GetDevices", ctx });
        if (!devices.some(({ isCurrent }) => isCurrent)) return unavailableFormats;
        const conversations = await query({ kind: "GetConversationList", ctx });
        if (conversations.length === 0) return unavailableFormats;
        const archive: ExportChoices = {
          history: conversations.map(({ id, displayName, nickname }) => ({ kind: "chat", id, name: nickname ?? displayName })),
          contents: {
            media: "available", contacts: "unsupported", aliases: "unsupported",
            provenance: "unsupported", local_preferences: "unsupported",
          },
        };
        return { archive, readable: { ...archive, contents: { ...archive.contents, media: "unsupported" } } };
      } catch {
        return unavailableFormats;
      }
    },
    async archive(caller, selection) {
      const history = await readHistory(caller, selection);
      const archiveId = toHex(globalThis.crypto.getRandomValues(new Uint8Array(16)));
      const includeMedia = selection.contents.includes("media");
      const candidates: PortabilityCandidate[] = [];
      const blobs = new Map<string, PortabilityCandidate>();
      const missingBlobs = new Set<string>();
      for (const { id, title, timeline } of history) {
        for (const message of timeline) {
          const { messageId, sentAt, disappearing, viewOnce } = message;
          const scope = { conversation: id, time: sentAt, disappearing, viewOnce };
          const attachments: CborValue[] = [];
          if (!viewOnce && !disappearing && includeMedia && inDateRange(sentAt, selection)) {
            for (const attachment of message.attachments ?? []) {
              const { blobId, mediaType, size } = attachment;
              if (!/^[0-9a-f]{64}$/.test(blobId)) throw new HistorySourceError("device_unavailable");
              attachments.push(map([["blob_id", blobId]]));
              if (blobs.has(blobId)) continue;
              // Only plaintext already supplied by the projection is eligible. Opening an
              // attachment through the client can fetch it and change its delivery state.
              if ("bytes" in attachment && attachment.bytes instanceof Uint8Array && attachment.bytes.byteLength === size) {
                blobs.set(blobId, { ...scope, item: { kind: "blob", blobId, mediaType, bytesHex: toHex(attachment.bytes) } });
                missingBlobs.delete(blobId);
              } else {
                missingBlobs.add(blobId);
              }
            }
          }
          // Retain exclusion metadata for the writer's report without copying protected text.
          const records = viewOnce || disappearing ? [] : [map([
            ["imported_record_id", JSON.stringify([id, messageId])],
            ["source_network_namespace", LOCAL_PROJECTION],
            ["source_export_fingerprint", archiveId],
            ["source_conversation_ref", id],
            ["source_time_hint", sentAt],
            ["display_metadata", map([["conversation", title], ["author", author(message.direction, title)]])],
            ["content", map([["text", message.text]])],
            ["attachments", attachments],
            ["provenance_level", "foreign_export_label"],
          ])];
          candidates.push({ ...scope, item: {
            kind: "record", recordType: "imported_history", stem: `message-${String(candidates.length)}`, bytesHex: toHex(encode(records)),
          } });
        }
      }
      return {
        candidates: [...candidates, ...blobs.values()],
        meta: {
          archiveId,
          exportScopeHex: toHex(encode(map([["from", selection.from], ["to", selection.to], ["include_media", includeMedia]]))),
          includedConversations: history.map(({ id }) => id),
          generatedAtHint: Date.now(),
          provenanceProfiles: ["foreign_export_label"],
        },
        missingMedia: missingBlobs.size,
      };
    },
    async readable(caller, selection) {
      return (await readHistory(caller, selection)).map(({ id, title, timeline }) => ({
        target: { kind: "chat", id },
        title,
        messages: timeline.map(({ sentAt, direction, text, viewOnce, disappearing }) => ({
          timestamp: sentAt,
          message: {
            time: new Date(sentAt).toISOString(), author: author(direction, title),
            text: viewOnce ? "" : text, disappearing, viewOnce,
          },
        })),
      }));
    },
  };
}

export interface PortabilityServiceOptions {
  readonly ctx: ProfileExecutionContext;
  readonly engine: PortabilityEngine;
  readonly files: FileHost;
  readonly history?: HistorySource;
}

const cancelled = { status: "cancelled" } as const;
const rejected = (reason: PortabilityFailure) => ({ status: "rejected", reason } as const);
const failure = ({ refused }: PortabilityRefusal) => rejected(refused === "malformed_request" ? "invalid_scope" : refused);
function commandFailure(error: unknown, fallback: PortabilityFailure) {
  if (error instanceof HistorySourceError) return rejected(error.reason);
  return rejected(error instanceof CoreCallError && error.code === "size_limit" ? "too_large" : fallback);
}

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
      const projectionEvidence: PortabilityDetail[] = history.projectionOnly ? [{ code: "canonical_evidence", count: 0 }] : [];
      let output: readonly FileEntry[];
      let report: PortabilityReport["details"];
      try {
        if (intent.format === "archive") {
          if (history.archive === undefined) return rejected("device_unavailable");
          const { missingMedia = 0, ...input } = await history.archive(ctx, selection);
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
            preserved: [...(history.projectionOnly ? projectionEvidence
              : details("canonical_evidence", objects)), ...details("media", blobs - omittedBlobs.length),
              ...details("history", records)],
            lossy: [
              ...details("private_material", excluded.secret_type ?? 0), ...details("view_once", excluded.view_once ?? 0),
              ...details("disappearing", excluded.disappearing ?? 0), ...details("outside_scope", excluded.not_selected ?? 0),
            ],
            missingMedia: details("missing_media", omittedBlobs.length + missingMedia),
          };
        } else {
          if (history.readable === undefined) return rejected("device_unavailable");
          const candidates = await history.readable(ctx, selection);
          const conversations = candidates
            .filter(({ target }) => selection.targets.some((chosen) => sameTarget(chosen, target)))
            .map(({ title, messages }) => ({
              title,
              messages: messages.filter(({ timestamp }) => inDateRange(timestamp, selection)).map(({ message }) => message),
            }));
          const result = engine.readableExport(conversations, intent.includeDisappearing);
          if ("refused" in result) return failure(result);
          output = result.files.map(({ path, bytesHex }) => ({ name: path, bytes: fromHex(bytesHex) }));
          report = {
            ...(history.projectionOnly ? { preserved: projectionEvidence } : {}),
            lossy: [...details("view_once", result.excludedViewOnce), ...details("disappearing", result.excludedDisappearing)],
          };
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
