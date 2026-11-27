// SPDX-License-Identifier: MPL-2.0
import type { ProfileExecutionContext } from "../../om-platform-api/src";
import { createPluginEngines } from "../../om-core-wasm/src/scope-engines";
import type { PluginCapability, PluginEgressFacts, PluginHostConfig } from "../../om-core-wasm/src/scope-engines";
import { reserveInstance } from "./budget";
import { deadline, entry, keys, packet, receive, verdict, wire, workerRequest } from "../protocol";
import type { Call, EgressIntent, SendIntent, Verdict } from "../protocol";

type Core = Parameters<typeof createPluginEngines>[0];
type ErrorName = "host_policy_denied" | "malformed_request" | "resource_budget_exceeded";

export type Observation =
  | { type: "bootstrap"; origin: string; sameWindow: boolean; verdict: Verdict }
  | { type: "call"; kind: Call["kind"] | "account" | "malformed"; verdict: Verdict; memoryBytes?: number }
  | { type: "stop"; reason: "watchdog" | "worker_error" | "deadline" | "navigation"; verdict: Verdict }
  | { type: "done" };

export interface FrameOptions {
  readonly core: Core;
  readonly ctx: ProfileExecutionContext;
  readonly host: PluginHostConfig;
  readonly imports: readonly string[];
  readonly grants: readonly PluginCapability[];
  readonly entrypoint: string;
  readonly wallClockMs: number;
  readonly sessionMs: number;
  readonly egressFacts?: (intent: EgressIntent, signal: AbortSignal) => PluginEgressFacts | Promise<PluginEgressFacts>;
  readonly confirmSend?: (intent: SendIntent, signal: AbortSignal) => boolean | Promise<boolean>;
  readonly observe?: (event: Observation) => void;
}

const deniedEgress: PluginEgressFacts = {
  policyAllows: false,
  oneToOne: false,
  bothConsented: false,
  oneShotConfirmed: false,
  continuous: false,
  exportAuthorized: false,
};

/**
 * The plugin frame lives on a separate registrable domain, so `allow-same-origin` gives it only its own origin, never the
 * app's: the app can bind the port to the exact plugin origin while the frame stays cross-site isolated.
 */
export function mountSiteFrame(container: HTMLElement, origin: string, options: FrameOptions) {
  const frame = document.createElement("iframe");
  const url = new URL("/", origin);
  url.searchParams.set("app", location.origin);
  frame.src = url.href;
  frame.title = "OM plugin sandbox";
  frame.hidden = true;
  frame.referrerPolicy = "no-referrer";
  frame.setAttribute("sandbox", "allow-scripts allow-same-origin");
  frame.allow = `cross-origin-isolated ${origin}`;
  return bindFrame(container, frame, origin, options);
}

/** Internal exact-origin broker. The harness supplies its own iframe to replay the measured sandbox. */
export function bindFrame(container: HTMLElement, frame: HTMLIFrameElement, origin: string, options: FrameOptions) {
  if (
    !entry(options.entrypoint) || !deadline(options.wallClockMs) ||
    !deadline(options.sessionMs) || options.sessionMs <= options.wallClockMs
  ) {
    throw new Error("Invalid plugin frame execution limits or entrypoint");
  }
  const engines = createPluginEngines(options.core, options.ctx);
  const host = engines.host;
  const { errorCodes } = engines.registry();
  const binding = crypto.randomUUID();
  const controller = new AbortController();
  let port: MessagePort | undefined;
  let workerDone = false;
  let running = false;
  let pending = false;
  let loaded = false;
  let ready = false;
  let reportedMismatch = false;
  let nextIn = 1;
  let nextOut = 1;

  function error(refused: ErrorName): Verdict {
    const code = errorCodes[refused];
    if (code === undefined) throw new Error("Incomplete Core plugin error registry");
    return { refused, code };
  }

  const safe = (fn: () => unknown): Verdict => {
    try {
      const answer = fn();
      return verdict(answer) ? answer : error("host_policy_denied");
    } catch {
      return error("host_policy_denied");
    }
  };

  // Host budgets (ADR-0268 decision 4): a named refusal, never an eviction of a running instance.
  const releaseBudget = reserveInstance(Number(options.host.budgets.maxMemoryBytes));
  const opened = safe(() => host.open(binding, options.host));
  if ("refused" in opened) {
    releaseBudget();
    throw new Error(opened.refused);
  }

  function close() {
    if (controller.signal.aborted) return;
    controller.abort();
    clearTimeout(timer);
    window.removeEventListener("message", bootstrap);
    window.removeEventListener("pagehide", close);
    frame.removeEventListener("load", load);
    port?.close();
    frame.remove();
    safe(() => host.terminate(binding));
    safe(() => host.close(binding));
    releaseBudget();
  }

  function stop(reason: Extract<Observation, { type: "stop" }>["reason"]) {
    if (controller.signal.aborted) return;
    const verdict = error(reason === "watchdog" ? "resource_budget_exceeded" : "host_policy_denied");
    close();
    options.observe?.({ type: "stop", reason, verdict });
  }

  async function adjudicate(request: Call): Promise<Verdict> {
    const signal = controller.signal;
    switch (request.kind) {
      case "read":
        return safe(() => host.readContent(binding, request.capability));
      case "egress": {
        const intent = Object.freeze({ capability: request.capability, destination: request.destination });
        const facts = await options.egressFacts?.(intent, signal) ?? deniedEgress;
        return safe(() => signal.aborted ? null : host.egress(binding, intent.capability, facts));
      }
      case "send": {
        const intent = Object.freeze({ destination: request.destination, content: request.content });
        const confirmed = await options.confirmSend?.(intent, signal);
        return safe(() => signal.aborted ? null : host.sendMessage(binding, confirmed === true));
      }
      case "ui":
        return safe(() => host.update(binding, request.surface, request.nodes));
      case "host_state":
        return safe(() => host.reachHostState(binding));
    }
  }

  function transfer() {
    const target = frame.contentWindow;
    if (!loaded || !ready || port || controller.signal.aborted || !target) return;
    const channel = new MessageChannel();
    const endpoint = channel.port1;
    port = endpoint;
    endpoint.onmessageerror = () => stop("worker_error");
    endpoint.onmessage = (delivery) => {
      if (delivery.target !== endpoint || delivery.ports.length || controller.signal.aborted) return;
      if (!frame.isConnected || frame.contentWindow !== target) {
        stop("navigation");
        return;
      }
      void delivered(endpoint, delivery.data).catch(() => stop("worker_error"));
    };

    // The only Window-level transfer. The first port message confirms the same binding.
    target.postMessage(wire({ type: "bind", binding }), origin, [channel.port2]);
    endpoint.postMessage(packet(binding, nextOut++, { type: "authenticate" }));
  }

  async function delivered(endpoint: MessagePort, data: unknown) {
    const body = receive(data, binding, nextIn);
    if (body === "oversize") {
      // A raw over-limit packet has no authenticated sequence to reply to.
      close();
      options.observe?.({ type: "call", kind: "malformed", verdict: error("malformed_request") });
      return;
    }
    if (!body) return;

    if (!running && body.type === "bound" && keys(body, "type")) {
      nextIn++;
      const initialized = safe(() => host.instantiate(binding, options.imports, options.grants));
      const began = "done" in initialized ? safe(() => host.beginInvocation(binding)) : initialized;
      if ("refused" in began) {
        close();
        options.observe?.({ type: "stop", reason: "worker_error", verdict: began });
        return;
      }
      running = true;
      endpoint.postMessage(packet(binding, nextOut++, {
        type: "run",
        entrypoint: options.entrypoint,
        wallClockMs: options.wallClockMs,
      }));
      return;
    }
    if (!running) return;
    const message = workerRequest(body) ? body : undefined;
    if (message?.type === "done" && !pending) {
      close();
      options.observe?.({ type: "done" });
      return;
    }
    if (
      body.type === "stop" && keys(body, "type", "reason") &&
      (body.reason === "watchdog" || body.reason === "worker_error")
    ) {
      stop(body.reason);
      return;
    }
    if (message?.type === "fault") {
      stop("worker_error");
      return;
    }
    const malformed = body.type === "malformed" && keys(body, "type");
    if (pending || workerDone || (!malformed && message?.type !== "call" && message?.type !== "account")) return;

    const seq = nextIn++;
    pending = true;
    let kind: Extract<Observation, { type: "call" }>["kind"] = "malformed";
    let answer: Verdict;
    if (message?.type === "call") {
      kind = message.request.kind;
      answer = await adjudicate(message.request).catch(() => error("host_policy_denied"));
    } else if (message?.type === "account") {
      kind = "account";
      // The trusted Worker adapter meters fixture linear memory. Browsers supply no fuel count.
      answer = safe(() => host.account(binding, message.memoryBytes, 0));
      workerDone = "refused" in answer;
    } else {
      answer = error("malformed_request");
    }
    if (controller.signal.aborted) return;
    pending = false;
    endpoint.postMessage(packet(binding, nextOut++, { type: "verdict", replyTo: seq, verdict: answer }));
    options.observe?.({
      type: "call",
      kind,
      verdict: answer,
      ...(message?.type === "account" ? { memoryBytes: message.memoryBytes } : {}),
    });
  }

  function bootstrap(event: MessageEvent<unknown>) {
    if (port || event.data !== wire({ type: "ready" })) return;
    const sameWindow = event.isTrusted && event.source === frame.contentWindow && event.ports.length === 0;
    const exactOrigin = event.origin === origin;
    if (!sameWindow || !exactOrigin) {
      if (reportedMismatch) return;
      reportedMismatch = true;
    }
    const answer = safe(() => host.miniAppMessage(binding, exactOrigin, sameWindow, false));
    options.observe?.({ type: "bootstrap", origin: event.origin, sameWindow, verdict: answer });
    if (!exactOrigin || !sameWindow || "refused" in answer) return;
    ready = true;
    transfer();
  }

  function load() {
    if (loaded) {
      stop("navigation");
      return;
    }
    loaded = true;
    transfer();
  }

  const timer = setTimeout(() => stop("deadline"), options.sessionMs);
  frame.addEventListener("load", load);
  window.addEventListener("message", bootstrap);
  window.addEventListener("pagehide", close, { once: true });
  container.append(frame);
  return { frame, close };
}
