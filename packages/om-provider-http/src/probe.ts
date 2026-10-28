// SPDX-License-Identifier: MPL-2.0
// Asking a Home who it is before the app uses it: one public GET of its capabilities document (no capability, no
// identity, no cookie), classified honestly. The answer is only what the Home claims about itself; it is a reason to
// refuse an obviously unsuitable address, never proof that the Home is trustworthy or who operates it.
import { API_MAJOR } from "@om/home-wire";
import { exchange } from "./http";

export type HomeHealth = "healthy" | "degraded" | "unhealthy" | "unknown";

export type HomeProbe =
  /** A Home that offers mailboxes at an API version this app speaks. `providerId` and `health` are self-reported. */
  | { readonly kind: "ok"; readonly baseUrl: string; readonly providerId: string; readonly health: HomeHealth }
  /** No answer: offline, refused, timed out, bad certificate, redirect, or blocked by the browser. */
  | { readonly kind: "unreachable" }
  /** The address answered with a server error (5xx). */
  | { readonly kind: "failing"; readonly status: number }
  /** The address answered, but not with an OM provider capabilities document. */
  | { readonly kind: "not_a_home" }
  /** A Home, but none of the API versions it offers is one this app speaks. */
  | { readonly kind: "wrong_version"; readonly offered: readonly string[] }
  /** A Home that does not offer mailboxes, which messaging needs. */
  | { readonly kind: "no_mailbox" };

export interface ProbeOptions {
  readonly fetch?: typeof fetch;
  readonly timeoutMs?: number;
}

const MAX_DOC_BYTES = 64 * 1024;
const HEALTH: readonly string[] = ["healthy", "degraded", "unhealthy", "unknown"];

/** A self-reported id is shown to the user, so it is bounded and stripped of anything that is not plain printable text. */
function cleanId(v: unknown): string | undefined {
  if (typeof v !== "string") return undefined;
  const s = v.trim();
  return s.length >= 1 && s.length <= 64 && /^[\x20-\x7e]+$/.test(s) ? s : undefined;
}

export async function probeHome(baseUrl: string, opts: ProbeOptions = {}): Promise<HomeProbe> {
  const r = await exchange(
    { fetch: opts.fetch ?? ((...a) => fetch(...a)), timeoutMs: opts.timeoutMs ?? 8000 },
    `${baseUrl}/.well-known/om-provider-capabilities`,
    { method: "GET" },
  );
  if (r.kind === "network") return { kind: "unreachable" };
  if (r.kind === "gateway") return r.status >= 500 ? { kind: "failing", status: r.status } : { kind: "not_a_home" };
  if (r.kind === "home") return { kind: "not_a_home" };
  if (r.body.length > MAX_DOC_BYTES) return { kind: "not_a_home" };
  let doc: unknown;
  try { doc = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(r.body)); } catch { return { kind: "not_a_home" }; }
  if (typeof doc !== "object" || doc === null || Array.isArray(doc)) return { kind: "not_a_home" };
  const d = doc as Record<string, unknown>;
  const providerId = cleanId(d["provider_id"]);
  const kinds = d["provider_kinds"];
  const versions = d["api_versions"];
  if (d["schema_version"] !== 1 || providerId === undefined || !Array.isArray(kinds) || !Array.isArray(versions)) return { kind: "not_a_home" };
  const offered = versions.filter((v): v is string => typeof v === "string").slice(0, 16).map((v) => v.slice(0, 16));
  if (!offered.includes(String(API_MAJOR))) return { kind: "wrong_version", offered };
  if (!kinds.includes("relay")) return { kind: "no_mailbox" };
  const h = d["health"];
  const health: HomeHealth = typeof h === "string" && HEALTH.includes(h) ? (h as HomeHealth) : "unknown";
  return { kind: "ok", baseUrl, providerId, health };
}
