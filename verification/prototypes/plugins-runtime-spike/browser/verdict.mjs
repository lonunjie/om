// SPDX-License-Identifier: MPL-2.0
import { isDeepStrictEqual } from "node:util";
import { memoryPages } from "./fixture.mjs";

export const apis = ["fetch", "websocket", "xhr", "importScripts", "sendBeacon"];
const success = { fetch: "completed", websocket: "opened", xhr: "completed", importScripts: "executed", sendBeacon: "queued" };
const failures = new Set(["threw", "error-event", "rejected", "closed"]);

function blocked(control, confined, traffic, violations, scope) {
  if (!control || !confined) return "not-measured";
  const atSink = (probe) => traffic.some(({ path }) => path === new URL(probe.url).pathname);
  if (atSink(confined) || (confined.outcome === success[confined.api] && confined.api !== "sendBeacon")) return "failed";
  if (control.outcome === "unavailable" && confined.outcome === "unavailable") return "api-unavailable";
  if (!atSink(control) || control.outcome !== success[control.api] || (control.api === "xhr" && control.status !== 200)) {
    return "control-failed";
  }
  const target = new URL(confined.api === "websocket" ? confined.url.replace(/^http/, "ws") : confined.url);
  const directive = confined.api === "importScripts" ? "script-src" : "connect-src";
  const enforced = violations.some((event) => event.disposition === "enforce" &&
    event.effectiveDirective?.startsWith(directive) && (event.blockedURI === target.href ||
      (scope === "frame" && [target.origin, `${target.origin}/`].includes(event.blockedURI))));
  if (confined.outcome !== "unavailable" && (enforced || failures.has(confined.outcome))) return "blocked";
  return "inconclusive";
}

export function evaluate(out, native, samples) {
  const control = out.control?.channel?.result;
  const confined = out.confined?.channel?.result;
  const probe = (phase, api) => phase?.worker_network?.results?.find((result) => result.api === api);
  const network = Object.fromEntries(apis.map((api) => [api, blocked(probe(control, api), probe(confined, api),
    out.traffic, confined?.worker_network?.violations ?? [], "worker")]));
  network.frameSendBeacon = blocked(control?.frame_beacon, confined?.frame_beacon, out.traffic,
    confined?.frame_violations ?? [], "frame");
  const parity = Object.fromEntries(Object.entries(native.calls).map(([name, expected]) => [name,
    confined?.calls?.results?.[name] === undefined ? "not-measured" : expected.pass &&
      isDeepStrictEqual(confined.calls.results[name], expected.result) ? "passed" : "failed"]));
  const growth = confined?.calls?.results?.grow?.ok;
  const memory = !growth ? "not-measured" : isDeepStrictEqual(growth, [1, memoryPages - 1, memoryPages, 9 - memoryPages])
    ? "bounded-fixture-linear-memory" : "failed";
  const spin = confined?.spin;
  const watchdog = !spin ? "not-measured" : spin.reason === "watchdog-terminated" && spin.messages.length === 1 &&
    spin.messages[0].type === "spin-started" && confined.frame_alive_after_spin ? "passed" : "failed";
  const origin = out.confined?.wrong_origin;
  const channel = out.confined?.channel;
  const originBinding = !origin ? "not-measured" : origin.same_window && origin.actual_origin === out.origins.sink &&
    origin.claimed_origin === out.origins.plugin && origin.decision === "reject-origin" && origin.ports_transferred === 0 &&
    out.confined.plugin_origin?.actual_origin === out.origins.plugin && out.confined.ports_transferred === 1 &&
    channel?.received?.length === 2 && channel.received.every(({ valid_endpoint }) => valid_endpoint) && channel.result
    ? "passed" : "failed";
  const startup = !confined?.startup ? "not-measured" : confined.startup.length === samples &&
    confined.startup.every(({ type }) => type === "ready") ? "passed" : "failed";
  const pass = native.pass && startup === "passed" && Object.values(parity).every((value) => value === "passed") &&
    memory === "bounded-fixture-linear-memory" && watchdog === "passed" && originBinding === "passed" &&
    apis.filter((api) => api !== "sendBeacon").every((api) => network[api] === "blocked") &&
    network.frameSendBeacon === "blocked" && ["blocked", "api-unavailable"].includes(network.sendBeacon);
  return { pass, startup, parity, memory, watchdog, origin_binding: originBinding, network };
}
