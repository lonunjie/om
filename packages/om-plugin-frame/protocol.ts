// SPDX-License-Identifier: MPL-2.0
// Local, engineering-only channel. This is not a codec for the frozen W4 CBOR objects.
import type { PluginCapability, PluginRefusal, PluginUiNode } from "../om-core-wasm/src/scope-engines";

const MAX_BYTES = 65_536;
const encoder = new TextEncoder();

export type Verdict = { readonly done: true } | PluginRefusal;
export type SendIntent = { readonly destination: string; readonly content: string };
export type EgressIntent = { readonly capability: PluginCapability; readonly destination: string };
export type Call =
  | { kind: "read"; capability: PluginCapability }
  | ({ kind: "egress" } & EgressIntent)
  | ({ kind: "send" } & SendIntent)
  | { kind: "ui"; surface: string; nodes: PluginUiNode[] }
  | { kind: "host_state" };

export type WorkerRequest =
  | { type: "call"; request: Call }
  | { type: "account"; memoryBytes: number }
  | { type: "done" }
  | { type: "fault" };

export type ToHost =
  | WorkerRequest
  | { type: "bound" }
  | { type: "malformed" }
  | { type: "stop"; reason: "watchdog" | "worker_error" };
export type ToFrame =
  | { type: "authenticate" }
  | { type: "run"; entrypoint: string; wallClockMs: number }
  | { type: "verdict"; replyTo: number; verdict: Verdict };

type Obj = Record<string, unknown>;
export const object = (value: unknown): value is Obj =>
  typeof value === "object" && value !== null && Object.getPrototypeOf(value) === Object.prototype;
export const keys = (value: Obj, ...expected: string[]) =>
  Object.keys(value).length === expected.length && expected.every((key) => Object.hasOwn(value, key));
const bounded = (value: unknown, bytes: number): value is string =>
  typeof value === "string" && value.length <= bytes && encoder.encode(value).byteLength <= bytes;
export const uint = (value: unknown): value is number =>
  typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
export const deadline = (value: unknown): value is number => uint(value) && value > 0 && value <= 2_147_483_647;
export const entry = (value: unknown): value is string =>
  typeof value === "string" && value.length <= 64 && /^[a-z][a-z0-9_-]*$/.test(value);
export const token = (value: unknown): value is string => typeof value === "string" && /^[a-f0-9-]{36}$/.test(value);
export const wire = (value: object): string => JSON.stringify(value);

export function decode(value: unknown): Obj | "oversize" | undefined {
  // Never traverse a structured-clone object or parse JSON above the transport ceiling.
  if (typeof value !== "string") return;
  if (!bounded(value, MAX_BYTES)) return "oversize";
  try {
    const result: unknown = JSON.parse(value);
    if (object(result)) return result;
  } catch {
    return;
  }
}

export function packet(binding: string, seq: number, body: ToHost | ToFrame): string {
  return wire({ binding, seq, body });
}

export function receive(data: unknown, binding: string, seq: number): Obj | "oversize" | undefined {
  const outer = decode(data);
  if (outer === "oversize") return outer;
  if (!outer || !keys(outer, "binding", "seq", "body") || outer.binding !== binding || outer.seq !== seq) return;
  if (object(outer.body)) return outer.body;
}

function capability(value: unknown): value is PluginCapability {
  return (uint(value) && value <= 255) || bounded(value, 128);
}

function uiNode(value: unknown): value is PluginUiNode {
  if (!object(value) || !bounded(value.id, 64) || !uint(value.kind) || value.kind > 255) return false;
  return Object.keys(value).every((key) =>
    key === "id" || key === "kind" ||
    (key === "parent" && bounded(value.parent, 64)) ||
    (key === "text" && bounded(value.text, 2_048)) ||
    (key === "action" && bounded(value.action, 256))
  );
}

export function call(value: unknown): value is Call {
  if (!object(value)) return false;
  switch (value.kind) {
    case "read":
      return keys(value, "kind", "capability") && capability(value.capability);
    case "egress":
      return keys(value, "kind", "capability", "destination") &&
        capability(value.capability) && bounded(value.destination, 512);
    case "send":
      return keys(value, "kind", "destination", "content") &&
        bounded(value.destination, 512) && bounded(value.content, 8_192);
    case "ui":
      // Permit one over-limit node so the Core can issue its own InvalidUiTree verdict.
      return keys(value, "kind", "surface", "nodes") && bounded(value.surface, 256) &&
        Array.isArray(value.nodes) && value.nodes.length <= 129 && value.nodes.every(uiNode);
    case "host_state":
      return keys(value, "kind");
    default:
      return false;
  }
}

export function workerRequest(value: Obj): value is Obj & WorkerRequest {
  switch (value.type) {
    case "call":
      return keys(value, "type", "request") && call(value.request);
    case "account":
      return keys(value, "type", "memoryBytes") && uint(value.memoryBytes);
    case "done":
    case "fault":
      return keys(value, "type");
    default:
      return false;
  }
}

export function verdict(value: unknown): value is Verdict {
  if (!object(value)) return false;
  if (keys(value, "done")) return value.done === true;
  return (keys(value, "refused", "code") || keys(value, "refused", "code", "reason")) &&
    bounded(value.refused, 128) && uint(value.code) && value.code >= 1 && value.code <= 12 &&
    (!Object.hasOwn(value, "reason") || bounded(value.reason, 256));
}
