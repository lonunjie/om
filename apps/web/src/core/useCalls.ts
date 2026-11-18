import { useCallback, useEffect, useRef, useState } from "react";
import type { CallNotice, CallView, ConversationSummary, CoreIntent, QueryResult } from "@om/platform-api";
import type { CallDriver } from "@om/core-wasm";
import type { CallMediaKind, CallTransport, CallTransportFailure } from "@om/web-platform/calls";
import { firstOutcome, resolveIntent, type ConversationServices } from "@om/ui";

type Failure = CallTransportFailure | "privacy";
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
  iceSequence: number;
  restarting: boolean;
  initialAnswer: boolean;
}

const actions: Record<Intent["kind"], string> = {
  StartCall: "start-call", AcceptCall: "accept-call", DeclineCall: "decline-call", EndCall: "end-call",
  UpdateCallIce: "update-call-ice", SetCallMedia: "set-call-media",
};

const terminal = (view: CallView | null) => view === null || view.state === "ended" || view.state === "failed";

function failureOf(error: unknown): Failure {
  const reason = (error as { reason?: unknown } | undefined)?.reason;
  if (reason === "unsupported" || reason === "permission" || reason === "device" || reason === "privacy") return reason;
  return "connection";
}

export function useCalls(services: ConversationServices, driver?: CallDriver) {
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
  const allowed = useRef(false);
  const recover = useRef<() => void>(() => undefined);
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
    call.transport.close();
    if (life.current) {
      setRemote(null);
      setDevices([]);
    }
  }, []);

  useEffect(() => {
    if (!driver) return;
    life.current = true;
    const noteFailure = (call: LiveCall) => {
      if (selected.current !== call) return;
      release(call);
      setFailure("connection");
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
          if (selected.current !== call || !allowed.current) return;
          const keys = await driver.takeMediaKeys(ctx, id);
          if (!keys) throw new Error("Missing media keys");
          await call.transport.install({
            generation: keys.generation,
            send: keys.send as Uint8Array<ArrayBuffer>,
            receive: keys.receive as Uint8Array<ArrayBuffer>,
          });
        })().catch(() => noteFailure(call));
      }
      if (!call.keys) return;
      const ice = driver.description(ctx, id, true);
      if (!ice || ice.seq <= call.iceSequence || (call.outgoing && !call.restarting)) return;
      call.iceSequence = ice.seq;
      void (async () => {
        if (call.outgoing) {
          await call.transport.restartComplete(ice.sdp);
          call.restarting = false;
        } else {
          const sdp = await call.transport.restartAnswer(ice.sdp);
          if (selected.current !== call || !allowed.current) return;
          await intent({ kind: "UpdateCallIce", callSessionId: id, sdp });
          await call.transport.restartComplete();
        }
      })().catch(() => noteFailure(call));
    };
    const onPrivacy = (level: string) => {
      if (!life.current) return;
      const direct = level === "recommended";
      allowed.current = direct;
      setPrivacyAllowsDirect(direct);
      const call = selected.current;
      if (!direct && call) {
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
    const restart = () => {
      const call = selected.current;
      const id = call?.id;
      if (!call?.outgoing || !id || !allowed.current || call.restarting || !call.keys ||
          (current.current?.state !== "active" && current.current?.state !== "reconnecting")) return;
      call.restarting = true;
      void (async () => {
        const sdp = await call.transport.restartOffer();
        if (selected.current === call && allowed.current) await intent({ kind: "UpdateCallIce", callSessionId: id, sdp });
      })().catch(() => noteFailure(call));
    };
    const depart = () => {
      const call = selected.current;
      if (call) {
        release(call);
        if (call.id) void intent({ kind: "EndCall", callSessionId: call.id }).catch(() => undefined);
      }
    };
    recover.current = restart;
    addEventListener("online", restart);
    addEventListener("pagehide", depart);
    return () => {
      life.current = false;
      allowed.current = false;
      offView();
      offDescription();
      reconcile.current = () => undefined;
      offNotices();
      offPrivacy();
      removeEventListener("online", restart);
      removeEventListener("pagehide", depart);
      recover.current = () => undefined;
      depart();
    };
  }, [driver, client, ctx, intent, release]);

  const busy = useRef(false);
  async function prepare(kind: CallMediaKind, target: ConversationSummary | null): Promise<void> {
    if (!driver || busy.current || selected.current || !life.current) return;
    const incoming = target === null ? current.current : null;
    if (target ? (!terminal(current.current) || target.allowCalls === undefined) : incoming?.state !== "incoming") return;
    if (!allowed.current) { setFailure("privacy"); return; }
    busy.current = true;
    setPending(true);
    setFailure(null);
    let call: LiveCall | undefined;
    try {
      const { createCallTransport } = await import("./callTransport");
      if (!life.current || !allowed.current || (incoming && current.current?.callSessionId !== incoming.callSessionId)) return;
      const transport = createCallTransport((fact) => {
        if (!call || !call.id || selected.current !== call || !allowed.current) return;
        const id = call.id;
        if (fact === "failed") { release(call); setFailure("connection"); }
        void driver.mediaState(ctx, id, fact).catch(() => undefined);
        if (fact === "active") void driver.routeFacts(ctx, id, "direct").catch(() => undefined);
        if (fact === "interrupted" && navigator.onLine) recover.current();
      }, (stream) => { if (call && selected.current === call) setRemote(stream); });
      call = { transport, outgoing: incoming === null, video: kind === "video", id: incoming?.callSessionId ?? null, keys: false, initialAnswer: false, iceSequence: -1, restarting: false };
      selected.current = call;
      if (incoming) {
        const offer = driver.description(ctx, incoming.callSessionId);
        if (offer?.kind !== "offer") throw new Error("Call offer expired");
        const sdp = await transport.answer(offer.sdp, kind);
        if (selected.current !== call || !allowed.current) return;
        if (current.current?.callSessionId !== incoming.callSessionId || current.current.state !== "incoming") throw new Error("Call offer expired");
        await intent({ kind: "AcceptCall", callSessionId: incoming.callSessionId, sdp, mediaFlags: kind === "video" ? 3 : 1 });
      } else if (target) {
        const sdp = await transport.offer(kind);
        if (selected.current !== call || !allowed.current) return;
        if (!terminal(current.current)) { release(call); return; }
        await intent({ kind: "StartCall", target: target.personId, media: kind, sdp });
        const next = await client.query({ kind: "GetCallView", ctx });
        if (!next?.outgoing || next.conversationId !== target.id) throw new Error("Call unavailable");
        if (selected.current !== call || !allowed.current) {
          if (!terminal(next)) await intent({ kind: "EndCall", callSessionId: next.callSessionId });
          return;
        }
        call.id = next.callSessionId;
        reconcile.current(next);
      }
      const available = await transport.devices().catch(() => []);
      if (life.current && selected.current === call) setDevices(available);
    } catch (error) {
      if (call) release(call);
      if (life.current && allowed.current && (!call || !terminal(current.current) || !call.id)) setFailure(failureOf(error));
    } finally {
      busy.current = false;
      if (life.current) setPending(false);
    }
  }

  async function end(decline = false): Promise<void> {
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
      if (!enabled && selected.current === call && allowed.current) call.transport.setEnabled(kind, true);
    } catch {
      if (life.current) setFailure("connection");
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
    view, notices, failure, pending, privacyAllowsDirect, remote, devices, cameraAvailable: selected.current?.video === true,
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
