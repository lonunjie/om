import { useCallback, useEffect, useRef, useState } from "react";
import type { CallNotice, CallView, ConversationSummary, CoreIntent, QueryResult } from "@om/platform-api";
import type { CallMediaProvider } from "@om/provider-http";
import type { CallDriver } from "@om/core-wasm";
import type { CallIceConfig, CallMediaKind, CallTransport, CallTransportFailure } from "@om/web-platform/calls";
import { firstOutcome, resolveIntent, type ConversationServices } from "@om/ui";

type Failure = CallTransportFailure | "privacy" | "privateUnavailable" | "relayConnection";
type OwnMedia = Awaited<ReturnType<CallMediaProvider["mediaCapabilities"]>>;

class CallPreparationError extends Error {
  constructor(readonly reason: Failure) { super(reason); }
}

function directConfig(home: OwnMedia): CallIceConfig {
  return { iceTransportPolicy: "all", iceServers: home?.stun_urls.map((url) => ({ urls: url })) ?? [] };
}

async function relayConfig(home: OwnMedia, provider: CallMediaProvider | undefined, strict: boolean): Promise<CallIceConfig | undefined> {
  if (home === undefined || provider === undefined) return undefined;
  const result = await provider.issueTurn();
  if (result.kind !== "issued") return undefined;
  const grant = result.lease;
  return {
    iceTransportPolicy: strict ? "relay" : "all",
    iceServers: [...(strict ? [] : directConfig(home).iceServers ?? []), {
      urls: [...grant.urls], username: grant.username, credential: grant.credential,
    }],
  };
}

const hasRelay = (bytes: readonly number[]): boolean =>
  /^a=candidate:.*\styp relay(?:\s|$)/m.test(new TextDecoder().decode(Uint8Array.from(bytes)));
const ableToRecover = (view: CallView | null): boolean =>
  view?.state === "active" || view?.state === "connecting" || view?.state === "reconnecting";
type CallIntent = Extract<CoreIntent, {
  kind: "StartCall" | "AcceptCall" | "DeclineCall" | "EndCall" | "UpdateCallIce" | "SetCallMedia";
}>;
type WithoutContext<T> = T extends CallIntent ? Omit<T, "ctx" | "operationId"> : never;
type Intent = WithoutContext<CallIntent>;

interface LiveCall {
  readonly transport: CallTransport;
  readonly outgoing: boolean;
  readonly video: boolean;
  id: string | null;
  keys: boolean;
  keysReady: boolean;
  iceSequence: number;
  restarting: boolean;
  restartSeen: number;
  readonly strict: boolean;
  usingTurn: boolean;
  triedTurn: boolean;
  turnTimer?: ReturnType<typeof setTimeout>;
  initialAnswer: boolean;
}

const actions: Record<Intent["kind"], string> = {
  StartCall: "start-call", AcceptCall: "accept-call", DeclineCall: "decline-call", EndCall: "end-call",
  UpdateCallIce: "update-call-ice", SetCallMedia: "set-call-media",
};

const terminal = (view: CallView | null) => view === null || view.state === "ended" || view.state === "failed";

function failureOf(error: unknown): Failure {
  const reason = (error as { reason?: unknown } | undefined)?.reason;
  if (reason === "unsupported" || reason === "permission" || reason === "device" || reason === "privacy" || reason === "privateUnavailable" || reason === "relayConnection") return reason;
  return "connection";
}

export function useCalls(services: ConversationServices, driver?: CallDriver, provider?: CallMediaProvider) {
  const { client, ctx } = services;
  const [view, setView] = useState<CallView | null>(null);
  const [notices, setNotices] = useState<readonly CallNotice[]>([]);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<Failure | null>(null);
  const [remote, setRemote] = useState<MediaStream | null>(null);
  const [devices, setDevices] = useState<readonly MediaDeviceInfo[]>([]);
  const life = useRef(false);
  const selected = useRef<LiveCall | null>(null);
  const current = useRef<CallView | null>(null);
  const reconcile = useRef<(next: CallView | null) => void>(() => undefined);
  const dismissed = useRef<string | null>(null);
  const privacy = useRef<"direct" | "private" | undefined>(undefined);
  const ownMedia = useRef<OwnMedia>(undefined);
  const [privateRoute, setPrivateRoute] = useState(false);
  const preparation = useRef<AbortController | null>(null);
  const recover = useRef<(tryTurn?: boolean) => void>(() => undefined);
  const [privacyAllowsDirect, setPrivacyAllowsDirect] = useState(false);
  const intent = useCallback(async (action: Intent): Promise<void> => {
    const { kind, ...params } = action;
    const request = resolveIntent(services, actions[kind], params);
    if (request.status !== "intent") throw new Error("Call action refused");
    const result = await client.dispatch(request.intent);
    if ((await firstOutcome(services, result.operationId)).kind === "Rejected") throw new Error("Call refused");
  }, [client, ctx, services]);

  const release = useCallback((call: LiveCall): void => {
    if (selected.current !== call) return;
    selected.current = null;
    clearTimeout(call.turnTimer);
    call.transport.close();
    if (life.current) {
      setRemote(null);
      setDevices([]);
    }
  }, []);

  useEffect(() => {
    if (!driver) return;
    life.current = true;
    const noteFailure = (call: LiveCall, error: unknown) => {
      if (selected.current !== call) return;
      release(call);
      const reason = failureOf(error);
      setFailure(reason === "connection" && call.usingTurn ? "relayConnection" : reason);
      if (call.id) void driver.mediaState(ctx, call.id, "failed").catch(() => undefined);
    };
    const onView = (next: CallView | null) => {
      if (!life.current) return;
      if (current.current !== next) {
        current.current = next;
        setView(next?.callSessionId === dismissed.current ? null : next);
      }
      const call = selected.current;
      if (!call) return;
      if (call.id === null) return; // Offer is preparing or awaiting the dispatch outcome.
      if (next === null || terminal(next) || next.callSessionId !== call.id) {
        release(call);
        return;
      }
      const id = call.id;
      if (next.state === "connecting" && !call.keys) {
        call.keys = true;
        void (async () => {
          if (call.outgoing && !call.initialAnswer) {
            const answer = driver.description(ctx, id);
            if (answer?.kind !== "answer") throw new Error("Missing call answer");
            call.initialAnswer = true;
            await call.transport.remoteAnswer(answer.sdp);
          }
          if (selected.current !== call) return;
          const keys = await driver.takeMediaKeys(ctx, id);
          if (!keys) throw new Error("Missing media keys");
          await call.transport.install({
            generation: keys.generation,
            send: keys.send as Uint8Array<ArrayBuffer>,
            receive: keys.receive as Uint8Array<ArrayBuffer>,
          });
          call.keysReady = true;
          if (selected.current !== call) return;
          onView(current.current);
          if (call.outgoing && !call.strict && ownMedia.current && current.current?.state === "connecting") {
            call.turnTimer = setTimeout(() => {
              if (current.current?.state === "connecting") recover.current(true);
            }, 7000);
          }
        })().catch((error: unknown) => { noteFailure(call, error); });
      }
      if (!call.keysReady) return;
      // Core owns restart progress: when it gives up on an exchange and allows another, it raises restartAttempt and the caller's host
      // starts a new restart (new description, new seq). Anything the old exchange left behind is abandoned (ADR-0228 s2).
      const retry = next.outgoing && next.restartAttempt > call.restartSeen;
      call.restartSeen = next.restartAttempt;
      if (retry) {
        call.restarting = false;
        recover.current(true);
      }
      const ice = driver.description(ctx, id, true);
      if (!ice || ice.seq <= call.iceSequence || call.outgoing !== call.restarting) return;
      call.iceSequence = ice.seq;
      void (async () => {
        if (call.outgoing) {
          await call.transport.restartComplete(ice.sdp);
          call.restarting = false;
        } else {
          call.restarting = true;
          call.transport.beginRestart();
          const config = call.strict || call.usingTurn || hasRelay(ice.sdp)
            ? await relayConfig(ownMedia.current, provider, call.strict) : undefined;
          if (selected.current !== call) return;
          if (call.strict && !config) throw new CallPreparationError("privateUnavailable");
          call.usingTurn ||= config !== undefined;
          const sdp = await call.transport.restartAnswer(ice.sdp, config);
          if (selected.current !== call) return;
          await intent({ kind: "UpdateCallIce", callSessionId: id, sdp });
          await call.transport.restartComplete();
          call.restarting = false;
          onView(current.current);
        }
      })().catch((error: unknown) => { noteFailure(call, error); });
    };
    const onPrivacy = (level: string) => {
      if (!life.current) return;
      const direct = level === "recommended";
      const policy = direct ? "direct" : "private";
      if (privacy.current === "direct" && !direct) preparation.current?.abort();
      privacy.current = policy;
      setPrivacyAllowsDirect(direct);
      const call = selected.current;
      if (!direct && call && !call.strict) {
        release(call);
        setFailure("privacy");
        if (call.id) void intent({ kind: "EndCall", callSessionId: call.id }).catch(() => undefined);
      }
    };
    const observe = <Q extends { kind: "GetCallView" | "GetCallNotices" | "GetPrivacyLevel"; ctx: typeof ctx }>(
      query: Q, accept: (value: QueryResult<Q>) => void,
    ) => {
      let changed = false;
      const stop = client.subscribe(query, (value) => { changed = true; accept(value); });
      void client.query(query).then((value) => { if (!changed && life.current) accept(value); }).catch(() => undefined);
      return () => { changed = true; stop(); };
    };
    reconcile.current = onView;
    const offView = observe({ kind: "GetCallView", ctx }, onView);
    const offDescription = driver.onChange(ctx, () => {
      // A restart updates an authenticated description even if the CallView compares equal.
      onView(current.current);
    });
    const offNotices = observe({ kind: "GetCallNotices", ctx }, setNotices);
    const offPrivacy = observe({ kind: "GetPrivacyLevel", ctx }, onPrivacy);
    const restart = (tryTurn = false) => {
      const call = selected.current;
      const id = call?.id;
      if (!call?.outgoing || !id || call.restarting || !call.keysReady || !ableToRecover(current.current)) return;
      const firstFallback = tryTurn && !call.usingTurn && !call.triedTurn && ownMedia.current !== undefined;
      if (current.current?.state === "connecting" && !firstFallback && !call.strict) return;
      const wasConnecting = current.current?.state === "connecting";
      call.restarting = true;
      call.triedTurn ||= firstFallback;
      void (async () => {
        const withTurn = call.strict || call.usingTurn || firstFallback;
        const config = withTurn ? await relayConfig(ownMedia.current, provider, call.strict) : undefined;
        if (selected.current !== call) return;
        if (firstFallback && wasConnecting && current.current?.state === "active") {
          call.restarting = false;
          return;
        }
        if (withTurn && !config) {
          if (call.strict || call.usingTurn) throw new CallPreparationError("privateUnavailable");
          if (current.current?.state === "connecting") { call.restarting = false; return; }
        }
        const sdp = await call.transport.restartOffer(config);
        call.usingTurn ||= config !== undefined;
        if (selected.current === call) await intent({ kind: "UpdateCallIce", callSessionId: id, sdp });
      })().catch((error: unknown) => { noteFailure(call, error); });
    };
    const depart = () => {
      preparation.current?.abort();
      const call = selected.current;
      if (call) {
        release(call);
        if (call.id) void intent({ kind: "EndCall", callSessionId: call.id }).catch(() => undefined);
      }
    };
    const online = () => { restart(); };
    const offline = () => { selected.current?.transport.interrupt(); };
    const network = (navigator as Navigator & { connection?: EventTarget }).connection;
    const networkChanged = () => {
      if (navigator.onLine) restart();
    };
    const mediaDevices = (navigator as { mediaDevices?: MediaDevices }).mediaDevices;
    const refreshDevices = () => {
      const call = selected.current;
      if (call) void call.transport.devices().then((list) => {
        if (selected.current === call) setDevices(list);
      }).catch(() => undefined);
    };
    recover.current = restart;
    addEventListener("online", online);
    addEventListener("offline", offline);
    network?.addEventListener("change", networkChanged);
    mediaDevices?.addEventListener("devicechange", refreshDevices);
    addEventListener("pagehide", depart);
    return () => {
      life.current = false;
      privacy.current = undefined;
      offView();
      offDescription();
      reconcile.current = () => undefined;
      offNotices();
      offPrivacy();
      removeEventListener("online", online);
      removeEventListener("offline", offline);
      network?.removeEventListener("change", networkChanged);
      mediaDevices?.removeEventListener("devicechange", refreshDevices);
      removeEventListener("pagehide", depart);
      recover.current = () => undefined;
      depart();
    };
  }, [driver, client, ctx, intent, release, provider]);

  useEffect(() => {
    if (!driver || !provider) return;
    let active = true;
    const refresh = () => {
      void provider.mediaCapabilities().then((media) => {
        if (!active) return;
        ownMedia.current = media;
        setPrivateRoute(media !== undefined);
      }).catch(() => undefined);
    };
    refresh();
    addEventListener("online", refresh);
    return () => {
      active = false;
      ownMedia.current = undefined;
      removeEventListener("online", refresh);
    };
  }, [driver, provider]);

  const busy = useRef(false);
  async function prepare(kind: CallMediaKind, target: ConversationSummary | null): Promise<void> {
    if (!driver || busy.current || selected.current || !life.current) return;
    const incoming = target === null ? current.current : null;
    if (target ? (!terminal(current.current) || target.allowCalls === undefined) : incoming?.state !== "incoming") return;
    const policy = privacy.current;
    if (!policy || (policy === "private" && !ownMedia.current)) { setFailure("privacy"); return; }
    const strict = policy === "private";
    const attempt = new AbortController();
    preparation.current = attempt;
    const canProceed = () => life.current && privacy.current !== undefined && (strict || privacy.current === "direct") && !attempt.signal.aborted;
    const owns = (call: LiveCall) => canProceed() && selected.current === call;
    busy.current = true;
    setPending(true);
    setFailure(null);
    if (target) {
      dismissed.current = current.current?.callSessionId ?? null;
      setView(null);
    }
    let call: LiveCall | undefined;
    try {
      const { createCallTransport } = await import("./callTransport");
      if (!canProceed() || (incoming && current.current?.callSessionId !== incoming.callSessionId)) return;
      const transport = createCallTransport({
        state: (fact) => {
          if (!call || !call.id || selected.current !== call) return;
          const id = call.id;
          if (fact === "active") clearTimeout(call.turnTimer);
          if (fact === "failed") { release(call); setFailure(call.usingTurn ? "relayConnection" : "connection"); }
          void driver.mediaState(ctx, id, fact).catch(() => undefined);
          if (fact === "interrupted" && navigator.onLine) recover.current(true);
        },
        stream: (stream) => { if (call && selected.current === call) setRemote(stream); },
        route: (route) => {
          if (call?.id && selected.current === call) void driver.routeFacts(ctx, call.id, route).catch(() => undefined);
        },
      });
      if (!transport.supported()) throw new CallPreparationError("unsupported");
      const config = strict ? await relayConfig(ownMedia.current, provider, true) : directConfig(ownMedia.current);
      if (!canProceed() || (incoming && current.current?.state !== "incoming")) return;
      if (!config) throw new CallPreparationError("privateUnavailable");
      call = { transport, outgoing: incoming === null, video: kind === "video", id: incoming?.callSessionId ?? null, keys: false, keysReady: false, initialAnswer: false, iceSequence: -1, restarting: false, restartSeen: 0, strict, usingTurn: strict, triedTurn: strict };
      selected.current = call;
      if (incoming) {
        const offer = driver.description(ctx, incoming.callSessionId);
        if (offer?.kind !== "offer") throw new Error("Call offer expired");
        const sdp = await transport.answer(offer.sdp, kind, config);
        if (!owns(call)) return;
        if (current.current?.callSessionId !== incoming.callSessionId || current.current.state !== "incoming") throw new Error("Call offer expired");
        await intent({ kind: "AcceptCall", callSessionId: incoming.callSessionId, sdp, mediaFlags: kind === "video" ? 3 : 1 });
      } else if (target) {
        const sdp = await transport.offer(kind, config);
        if (!owns(call)) return;
        if (!terminal(current.current)) { release(call); return; }
        await intent({ kind: "StartCall", target: target.personId, media: kind, sdp });
        const next = await client.query({ kind: "GetCallView", ctx });
        if (!next?.outgoing || next.conversationId !== target.id) throw new Error("Call unavailable");
        if (!owns(call)) {
          if (!terminal(next)) await intent({ kind: "EndCall", callSessionId: next.callSessionId });
          return;
        }
        call.id = next.callSessionId;
        reconcile.current(next);
      }
      const available = await transport.devices().catch(() => []);
      if (owns(call)) setDevices(available);
    } catch (error) {
      const showFailure = canProceed() && (!call || !terminal(current.current) || call.id === null);
      if (call) release(call);
      if (showFailure) setFailure(failureOf(error));
    } finally {
      preparation.current = null;
      busy.current = false;
      finishPreparation();
    }
  }

  function finishPreparation(): void {
    if (life.current) setPending(false);
  }

  async function end(decline = false): Promise<void> {
    preparation.current?.abort();
    const call = selected.current;
    if (call) release(call);
    setPending(false);
    const next = current.current;
    if (next && !terminal(next)) {
      try { await intent({ kind: decline ? "DeclineCall" : "EndCall", callSessionId: next.callSessionId }); }
      catch { if (life.current) setFailure("connection"); }
    }
  }

  async function toggle(kind: CallMediaKind): Promise<void> {
    const call = selected.current;
    const next = current.current;
    if (!call || !next || (next.state !== "active" && next.state !== "reconnecting") || busy.current) return;
    const bit = kind === "audio" ? 1 : 2;
    const enabled = (next.localMediaFlags & bit) !== 0;
    if (enabled) call.transport.setEnabled(kind, false);
    busy.current = true;
    try {
      await intent({ kind: "SetCallMedia", callSessionId: next.callSessionId, mediaFlags: next.localMediaFlags ^ bit });
      if (!enabled && selected.current === call) call.transport.setEnabled(kind, true);
    } catch {
      if (selected.current === call) {
        setFailure("connection");
        if (enabled && driver) {
          // The track is already disabled. If Core cannot commit that state, end the call without reopening capture.
          release(call);
          void driver.mediaState(ctx, next.callSessionId, "failed").catch(() => undefined);
        }
      }
    } finally {
      busy.current = false;
    }
  }

  async function selectInput(kind: CallMediaKind, deviceId: string): Promise<void> {
    if (busy.current) return;
    busy.current = true;
    try { await selected.current?.transport.selectInput(kind, deviceId); }
    catch (error) { if (life.current) setFailure(failureOf(error)); }
    finally { busy.current = false; }
  }

  if (!driver) return undefined;
  return {
    view, notices, failure, pending, canUseCallRoute: privacyAllowsDirect || privateRoute, remote, devices, cameraAvailable: selected.current?.video === true,
    start: (target: ConversationSummary, kind: CallMediaKind) => { void prepare(kind, target); },
    answer: (kind: CallMediaKind) => { void prepare(kind, null); },
    end: (decline?: boolean) => { void end(decline); },
    toggle: (kind: CallMediaKind) => { void toggle(kind); },
    selectInput: (kind: CallMediaKind, deviceId: string) => { void selectInput(kind, deviceId); },
    dismiss: () => {
      setFailure(null);
      if (terminal(current.current)) {
        dismissed.current = current.current?.callSessionId ?? null;
        setView(null);
      }
    },
  };
}
