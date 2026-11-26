// SPDX-License-Identifier: MPL-2.0
import { bindFrame, mountOpaqueFrame } from "../embedder/broker";
import type { FrameOptions, Observation } from "../embedder/broker";
import { mountPluginFrame } from "../embedder/index";
import { instantiateCore } from "../../om-core-wasm/src/wasm";
import { createPluginEngines } from "../../om-core-wasm/src/scope-engines";
import { packet, wire } from "../protocol";

type Fixture =
  | "opaque"
  | "foreign"
  | "wrong_origin"
  | "forged_ready"
  | "forged_port"
  | "tainted"
  | "bilateral"
  | "send"
  | "ui"
  | "oversized"
  | "host_state"
  | "spin"
  | "memory"
  | "unknown"
  | "ungranted";

declare const PLUGIN_ORIGIN: string;
declare const SINK_ORIGIN: string;

const wasm = fetch("/scope.wasm").then(async (response) => {
  if (!response.ok) throw new Error("Unable to load scope-engines WASM");
  return instantiateCore(await response.arrayBuffer());
});
const imports = ["content.read_selected", "net.fetch_origin_scoped", "conversation.send_message", "ui.panel"];

function frameUrl(origin: string, route = "/") {
  const url = new URL(route, origin);
  url.searchParams.set("app", location.origin);
  return url.href;
}

function measuredFrame(src: string) {
  const frame = document.createElement("iframe");
  frame.sandbox.add("allow-scripts", "allow-same-origin");
  frame.allow = `cross-origin-isolated ${PLUGIN_ORIGIN}`;
  frame.referrerPolicy = "no-referrer";
  frame.src = src;
  return frame;
}

function wrongBinding() {
  return new Promise<{ replies: unknown[]; sent: boolean }>((resolve) => {
    const frame = measuredFrame(frameUrl(PLUGIN_ORIGIN));
    const channel = new MessageChannel();
    const replies: unknown[] = [];
    const binding = crypto.randomUUID();
    let sent = false;
    const timer = setTimeout(() => {
      window.removeEventListener("message", bind);
      channel.port1.close();
      if (!sent) channel.port2.close();
      frame.remove();
      resolve({ replies, sent });
    }, 1_500);
    channel.port1.onmessage = (event) => replies.push(event.data);
    function bind(event: MessageEvent<unknown>) {
      const target = frame.contentWindow;
      if (
        !target || event.origin !== PLUGIN_ORIGIN || event.source !== target ||
        event.data !== wire({ type: "ready" }) || sent
      ) return;
      sent = true;
      target.postMessage(wire({ type: "bind", binding }), PLUGIN_ORIGIN, [channel.port2]);
      channel.port1.postMessage(packet(crypto.randomUUID(), 1, { type: "authenticate" }));
      channel.port1.postMessage(packet(binding, 2, { type: "run", entrypoint: "host_state", wallClockMs: 500 }));
    }
    window.addEventListener("message", bind);
    document.body.append(frame);
    window.addEventListener("pagehide", () => clearTimeout(timer), { once: true });
  });
}

async function run(fixture: Fixture) {
  const core = await wasm;
  const registry = createPluginEngines(core).registry();
  if (fixture === "forged_port") return { registry, forged: await wrongBinding() };

  const observations: Observation[] = [];
  const elapsed: number[] = [];
  const started = performance.now();
  const prompts: unknown[] = [];
  const foreign = fixture === "foreign" ? measuredFrame(frameUrl(SINK_ORIGIN, "/foreign")) : undefined;
  let finish: () => void;
  const ended = new Promise<void>((resolve) => {
    finish = resolve;
  });
  const options: FrameOptions = {
    core,
    // Synthetic grants exercise the real host policy. Loopback topology is not process-isolation evidence.
    host: { siteIsolation: fixture !== "opaque", budgets: { maxMemoryBytes: 65_536, maxFuel: 1_000_000 } },
    imports,
    grants: [5, 14, 11, 3],
    entrypoint: fixture === "foreign" ? "host_state" : fixture,
    wallClockMs: fixture === "spin" ? 1_500 : 2_500,
    sessionMs: 4_000,
    confirmSend: (intent) => {
      prompts.push(intent);
      return false;
    },
    ...(fixture === "bilateral"
      ? {
        egressFacts: () => ({
          policyAllows: true,
          oneToOne: true,
          bothConsented: false,
          oneShotConfirmed: false,
          continuous: false,
          exportAuthorized: false,
        }),
      }
      : {}),
    observe(event) {
      observations.push(event);
      elapsed.push(performance.now() - started);
      if (event.type === "stop" || event.type === "done") finish();
    },
  };
  // Use a quiet, wrong-origin document for the target when probing an unsolicited sibling or a forged event.
  const probe = fixture === "foreign" || fixture === "forged_ready";
  const src = fixture === "wrong_origin"
    ? frameUrl(SINK_ORIGIN, "/foreign")
    : probe
    ? frameUrl(SINK_ORIGIN, "/quiet")
    : frameUrl(PLUGIN_ORIGIN);
  const frame = fixture === "opaque" ? undefined : measuredFrame(src);
  const session = frame
    ? bindFrame(document.body, frame, PLUGIN_ORIGIN, options)
    : mountOpaqueFrame(document.body, PLUGIN_ORIGIN, options);
  if (foreign) document.body.append(foreign);
  if (fixture === "forged_ready") {
    window.dispatchEvent(
      new MessageEvent("message", {
        source: session.frame.contentWindow,
        origin: PLUGIN_ORIGIN,
        data: wire({ type: "ready" }),
      }),
    );
  }
  const defaultDisabled = mountPluginFrame(document.body, options) === null;
  const sandbox = session.frame.getAttribute("sandbox");
  try {
    await ended;
    return { registry, sandbox, observations, elapsed, prompts, defaultDisabled, isolated: crossOriginIsolated };
  } finally {
    session.close();
    foreign?.remove();
  }
}

Object.assign(window, { runPluginFixture: run, omHostSentinel: "synthetic-app-only" });
