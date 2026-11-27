// SPDX-License-Identifier: MPL-2.0
// Build the engineering variant with pnpm --filter @om/core-wasm build:wasm:scope.
import { createHash } from "node:crypto";
import { existsSync, readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { createPortabilityEngine } from "./scope-engines";
import type {
  PortabilityAnswer, PortabilityEngine, PortabilityExportInput, PortabilityRefusalClass,
} from "./scope-engines";
import { instantiateCore } from "./wasm";

const wasm = new URL("../dist/om_wasm_scope.wasm", import.meta.url);
const fixtures = new URL(
  "../../../verification/prototypes/v014-archive/vectors/om-archive-v1-vectors.json",
  import.meta.url,
);
const { vectors } = JSON.parse(readFileSync(fixtures, "utf8")) as {
  vectors: { name: string; archive_hex: string }[];
};

function fixture(name: string): string {
  const vector = vectors.find((v) => v.name === name);
  if (!vector) throw new Error(`Missing fixture ${name}`);
  return vector.archive_hex;
}

function accepted<T>(result: PortabilityAnswer<T>): T {
  if ("refused" in (result as object)) throw new Error(JSON.stringify(result));
  return result as T;
}

describe.skipIf(!existsSync(wasm))("portability scope engine", () => {
  let engine: PortabilityEngine;

  beforeAll(async () => {
    engine = createPortabilityEngine(await instantiateCore(readFileSync(wasm), {
      randomFill: () => { throw new Error("portability requested entropy"); },
      nowMs: () => { throw new Error("portability read the clock"); },
    }));
  });

  it("exports a plain archive and imports only the selected items", () => {
    const blobId = "07".repeat(32);
    const object = { kind: "object", objectType: "test.event", bytesHex: "0102" } as const;
    const input = {
      meta: { archiveId: "ab".repeat(16), exportScopeHex: "a0", generatedAtHint: 0n, provenanceProfiles: [] },
      selection: { includeBlobs: true, conversations: ["a"], from: 1n },
      candidates: [
        { item: object, conversation: "a", time: 1n, disappearing: false, viewOnce: false },
        { item: object, conversation: "b", time: 2n, disappearing: false, viewOnce: false },
        { item: { ...object, objectType: "om.device_vault", bytesHex: "ff" }, disappearing: false, viewOnce: false },
        { item: { ...object, bytesHex: "fe" }, disappearing: false, viewOnce: true },
        { item: { ...object, bytesHex: "fd" }, disappearing: true, viewOnce: false },
        { item: { kind: "blob", blobId, mediaType: "image/png", declared: { size: 10n, sha256: "04".repeat(32) } },
          disappearing: false, viewOnce: false },
      ],
    } satisfies PortabilityExportInput;
    const plan = accepted(engine.exportPlan(input));
    expect(plan.report).toEqual({
      objects: 1, blobs: 1, records: 0, omittedBlobs: [`blobs/${blobId}`],
      excluded: { secret_type: 1, view_once: 1, disappearing: 1, not_selected: 1 },
    });
    const output = accepted(engine.export(input));
    expect(output.report).toEqual(plan.report);
    expect(accepted(engine.inspect(output.archiveHex))).toMatchObject({
      manifest: { archiveId: input.meta.archiveId, objects: 1, blobs: 1, records: 0, encryptionProfile: null },
      modes: { read_only: "available", same_account: "requires_live_validation" },
    });
    const view = accepted(engine.importReadonly(output.archiveHex, [], []));
    const digest = createHash("sha256").update(Buffer.from(object.bytesHex, "hex")).digest("hex");
    expect(view).toEqual({
      objects: [{ path: `canonical/objects/${digest}`, status: { kind: "unverified" } }],
      records: [], quarantined: [], missingBlobs: plan.report.omittedBlobs,
    });
  });

  it("replays the public archive fixtures and returns the reader's stable refusal classes", () => {
    const classes = {
      malformed_request: true, not_an_archive: true, encrypted_unsupported: true, invalid_archive: true,
      integrity_failure: true, too_large: true, invalid_scope: true, unauthorized_device: true, wrong_account: true,
    } satisfies Record<PortabilityRefusalClass, true>;
    const { errorCodes } = engine.registry();
    expect(Object.keys(errorCodes).sort()).toEqual(Object.keys(classes).sort());
    expect(new Set(Object.values(errorCodes)).size).toBe(Object.keys(classes).length);

    for (const v of vectors) {
      const result = engine.inspect(v.archive_hex);
      if (v.name.startsWith("encrypted-")) {
        expect(result).toEqual({
          refused: "encrypted_unsupported", code: errorCodes.encrypted_unsupported, reason: "encrypted_unsupported",
        });
        expect(engine.importReadonly(v.archive_hex, [], [])).toEqual(result);
      } else {
        const inspected = accepted(result);
        expect(inspected.manifest).toMatchObject({ objects: 3, blobs: 1, records: 1 });
        expect(inspected.manifest.missingBlobs).toHaveLength(v.name === "partial-missing-blob" ? 1 : 0);
        expect(accepted(engine.importReadonly(v.archive_hex, [], [])).missingBlobs).toEqual(inspected.manifest.missingBlobs);
      }
    }

    const corrupted = Buffer.from(fixture("plain"), "hex");
    const formatData = 30 + corrupted.readUInt16LE(26);
    corrupted.writeUInt8(corrupted.readUInt8(formatData) ^ 1, formatData);
    const archiveHex = corrupted.toString("hex");
    const failure = { refused: "integrity_failure", code: errorCodes.integrity_failure, reason: "Zip(Crc)" };
    expect(engine.inspect(archiveHex)).toEqual(failure);
    expect(engine.importReadonly(archiveHex, [], [])).toEqual(failure);
  });

  it("uses only the current host's control facts and returns a local history plan", () => {
    const controlBytes = "0405";
    const exported = accepted(engine.export({
      candidates: [
        { item: { kind: "object", objectType: "test.room.control", bytesHex: controlBytes },
          disappearing: false, viewOnce: false },
      ],
      selection: { includeBlobs: false },
      meta: { archiveId: "00".repeat(16), exportScopeHex: "a0", generatedAtHint: 0, provenanceProfiles: [] },
    }));
    const sha256 = createHash("sha256").update(Buffer.from(controlBytes, "hex")).digest("hex");
    const path = `canonical/objects/${sha256}`;
    const archived = 18_446_744_073_709_551_614n;
    const verdicts = [{
      objectType: "test.room.control", sha256, verified: { control: { stream: "room1", version: archived } },
    }];
    const live = [{ stream: "room1", version: archived + 1n }];
    const read = () => accepted(engine.importReadonly(exported.archiveHex, live, verdicts));
    const stale = read();
    expect(stale.objects).toEqual([{
      path, status: { kind: "stale_control", archived: archived.toString(), live: (archived + 1n).toString() },
    }]);
    const view = accepted(engine.importReadonly(exported.archiveHex, [], verdicts));
    expect(view).toEqual({
      objects: [{ path, status: { kind: "control_needs_live_validation", archived: archived.toString() } }],
      records: [], quarantined: [], missingBlobs: [],
    });
    expect(read()).toEqual(stale);
    expect(accepted(engine.importReadonly(exported.archiveHex, [], [])).objects).toEqual([
      { path, status: { kind: "unverified" } },
    ]);

    const context = { authorizedDevice: true, accountMatches: true, now: 7n,
      canEnforceExpiry: true, liveControl: live, revokedDevices: ["revoked"] };
    const items = [
      { id: "history", viewOnce: false, device: "revoked" },
      { id: "gone", viewOnce: false, expiry: 7n },
      { id: "later", viewOnce: false, expiry: 9n },
      { id: "once", viewOnce: true },
    ];
    expect(engine.rehydratePlan(view, items, { ...context, authorizedDevice: false }))
      .toMatchObject({ refused: "unauthorized_device" });
    expect(engine.rehydratePlan(view, items, { ...context, accountMatches: false }))
      .toMatchObject({ refused: "wrong_account" });
    expect(accepted(engine.rehydratePlan(view, items, context))).toEqual({
      import: ["history", "later"], scheduleDeletion: [["later", "9"]], droppedExpired: ["gone"],
      refusedViewOnce: ["once"], skippedUnenforceable: [], historyOnlyRevokedDevice: ["history"],
      needsLiveValidation: [path], staleControl: [], freshSessionsRequired: true,
    });
  });

  it("preserves the plaintext warning, neutralization and explicit disappearing choice", () => {
    const message = { time: "1", author: "A", text: "a\u202Eb\u0000", disappearing: false, viewOnce: false };
    const conversations = [{ title: "../Room", messages: [
      message, { ...message, text: "disappearing", disappearing: true }, { ...message, text: "one-time", viewOnce: true },
    ] }];
    const excluded = accepted(engine.readableExport(conversations));
    expect(excluded).toMatchObject({
      disappearingChoice: "excluded", excludedDisappearing: 1, excludedViewOnce: 1, isMigrationFormat: false,
    });
    const [file] = excluded.files;
    expect(file?.path).toBe("Room-1.txt");
    const text = Buffer.from(file!.bytesHex, "hex").toString("utf8");
    expect(text.startsWith(excluded.warning)).toBe(true);
    expect(text).toContain("a\uFFFDb\uFFFD");
    expect(text).not.toContain("\n[1] A: disappearing");
    const explicit = accepted(engine.readableExport(conversations, true));
    expect(explicit.disappearingChoice).toBe("included_explicitly");
    const explicitText = Buffer.from(explicit.files[0]!.bytesHex, "hex").toString("utf8");
    expect(explicitText).toContain("\n[1] A: disappearing");
    expect(explicitText).not.toContain("one-time");
  });
});
