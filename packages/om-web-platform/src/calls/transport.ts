import type { SFrameInstall, SFrameMediaProbe, SFrameWorkerFact } from "./sframe-worker";

export type CallMediaKind = "audio" | "video";
export type CallMediaFact = "active" | "interrupted" | "failed";
export type CallTransportFailure = "unsupported" | "permission" | "device" | "connection";

export class CallTransportError extends Error {
  constructor(readonly reason: CallTransportFailure) { super(reason); }
}

export interface CallTransportEnvironment {
  readonly peer?: (config: RTCConfiguration) => RTCPeerConnection;
  readonly media?: Pick<MediaDevices, "getUserMedia" | "enumerateDevices">;
  readonly worker?: () => Worker;
  readonly transform?: (worker: Worker, direction: "send" | "receive") => RTCRtpScriptTransform;
  readonly stream: () => MediaStream;
}

export function browserCallEnvironment(worker: () => Worker): CallTransportEnvironment {
  const media = (navigator as { readonly mediaDevices?: MediaDevices }).mediaDevices;
  const webCrypto = (globalThis as { readonly crypto?: Partial<Crypto> }).crypto;
  const supported = globalThis.isSecureContext && typeof RTCPeerConnection === "function" &&
    typeof RTCRtpSender === "function" && typeof RTCRtpReceiver === "function" &&
    typeof RTCRtpScriptTransform === "function" && "transform" in RTCRtpSender.prototype &&
    "transform" in RTCRtpReceiver.prototype && typeof media?.getUserMedia === "function" &&
    typeof webCrypto?.subtle?.encrypt === "function";
  return {
    stream: () => new MediaStream(),
    ...(supported ? {
      peer: (config: RTCConfiguration) => new RTCPeerConnection(config),
      media,
      worker,
      transform: (script: Worker, direction: "send" | "receive") => new RTCRtpScriptTransform(script, { direction }),
    } : {}),
  };
}

const GATHER_MS = 3000;
const CONNECT_MS = 20_000;
const MAX_SDP_BYTES = 16_384;
const encode = new TextEncoder();
const decode = new TextDecoder("utf-8", { fatal: true });

function description(type: "offer" | "answer", bytes: readonly number[]): RTCSessionDescriptionInit {
  return { type, sdp: decode.decode(Uint8Array.from(bytes)) };
}

interface Facts {
  readonly state: (fact: CallMediaFact) => void;
  readonly stream: (remote: MediaStream) => void;
}

/** One transport per Core call. The host carries the returned descriptions only through Core intents. */
export class CallTransport {
  readonly #env: CallTransportEnvironment;
  readonly #facts: Facts;
  readonly #stop = new AbortController();
  #connection: RTCPeerConnection | undefined;
  #worker: Worker | undefined;
  #workerReady: Promise<void> | undefined;
  #remote: MediaStream | undefined;
  #capture: MediaStreamTrack[] = [];
  #transformed = new Set<RTCRtpTransceiver>();
  #timeout: ReturnType<typeof setTimeout> | undefined;
  #fact: CallMediaFact | undefined;
  #received = false;
  #probe = 0;
  #keyed = false;
  #recovering = false;
  #inputs: Partial<Record<CallMediaKind, string>> = {};

  constructor(env: CallTransportEnvironment, facts: Facts) {
    this.#env = env;
    this.#facts = facts;
  }

  async #captureMedia(kind: CallMediaKind, device?: string): Promise<MediaStreamTrack[]> {
    const media = this.#env.media;
    if (!media) throw new CallTransportError("unsupported");
    const constraints = device === undefined ? true : { deviceId: { exact: device } };
    let stream: MediaStream;
    try {
      stream = await media.getUserMedia(device === undefined
        ? { audio: true, video: kind === "video" }
        : { audio: kind === "audio" && constraints, video: kind === "video" && constraints });
    } catch (error) {
      const denied = error instanceof DOMException && (error.name === "NotAllowedError" || error.name === "SecurityError");
      throw new CallTransportError(denied ? "permission" : "device");
    }
    const tracks = stream.getTracks();
    if (this.#stop.signal.aborted) {
      tracks.forEach((track) => { track.stop(); });
      throw new CallTransportError("connection");
    }
    this.#capture.push(...tracks);
    return tracks;
  }

  #open(): RTCPeerConnection {
    const { peer, media, worker, transform } = this.#env;
    if (!peer || !media || !worker || !transform) throw new CallTransportError("unsupported");
    if (this.#connection || this.#stop.signal.aborted) throw new CallTransportError("connection");
    const script = worker();
    this.#worker = script;
    this.#workerReady = new Promise((resolve, reject) => {
      const abort = () => { reject(new CallTransportError("connection")); };
      this.#stop.signal.addEventListener("abort", abort, { once: true });
      script.addEventListener("message", (event: MessageEvent<SFrameWorkerFact>) => {
        if (event.data === "ready") {
          this.#stop.signal.removeEventListener("abort", abort);
          resolve();
        }
        if (event.data === "keyed") this.#keyed = true;
        if (typeof event.data === "object" && event.data.received === this.#probe) this.#received = true;
        if (event.data === "failed") this.#report("failed");
        else this.#updateState();
      });
      script.addEventListener("error", () => { this.#report("failed"); });
      script.addEventListener("messageerror", () => { this.#report("failed"); });
    });
    // Observe an abort even if the caller is still waiting for media permission.
    void this.#workerReady.catch(() => undefined);
    // With no advertised Home media feature, accepted contacts get direct candidates only.
    const connection = peer({ iceServers: [], iceCandidatePoolSize: 0 });
    this.#connection = connection;
    const remote = this.#env.stream();
    this.#remote = remote;
    connection.addEventListener("connectionstatechange", () => { this.#updateState(); });
    connection.addEventListener("track", ({ track, transceiver }) => {
      if (this.#stop.signal.aborted) return;
      try {
        this.#attach(transceiver);
      } catch {
        this.#report("failed");
        return;
      }
      remote.addTrack(track);
      this.#facts.stream(remote);
    });
    return connection;
  }

  #attach(transceiver: RTCRtpTransceiver): void {
    if (this.#transformed.has(transceiver)) return;
    const { transform } = this.#env;
    const worker = this.#worker;
    if (!transform || !worker) throw new CallTransportError("connection");
    transceiver.sender.transform = transform(worker, "send");
    transceiver.receiver.transform = transform(worker, "receive");
    this.#transformed.add(transceiver);
  }

  #peer(): RTCPeerConnection {
    const connection = this.#connection;
    if (!connection) throw new CallTransportError("connection");
    return connection;
  }

  async offer(kind: CallMediaKind): Promise<readonly number[]> {
    const connection = this.#open();
    const tracks = await this.#captureMedia(kind);
    for (const track of tracks) this.#attach(connection.addTransceiver(track, { direction: "sendrecv" }));
    return this.#gather(await connection.createOffer());
  }

  async answer(offer: readonly number[], kind: CallMediaKind): Promise<readonly number[]> {
    const connection = this.#open();
    const tracks = await this.#captureMedia(kind);
    await connection.setRemoteDescription(description("offer", offer));
    for (const transceiver of connection.getTransceivers()) {
      this.#attach(transceiver);
      const track = tracks.find((value) => value.kind === transceiver.receiver.track.kind);
      if (track) {
        transceiver.direction = "sendrecv";
        await transceiver.sender.replaceTrack(track);
      }
    }
    return this.#gather(await connection.createAnswer());
  }

  async remoteAnswer(answer: readonly number[]): Promise<void> {
    await this.#peer().setRemoteDescription(description("answer", answer));
    this.#watchConnection();
  }

  async restartOffer(): Promise<readonly number[]> {
    const connection = this.#peer();
    this.#recovering = true;
    this.#report("interrupted");
    this.#watchConnection();
    return this.#gather(await connection.createOffer({ iceRestart: true }));
  }

  async restartAnswer(offer: readonly number[]): Promise<readonly number[]> {
    const connection = this.#peer();
    this.#recovering = true;
    this.#report("interrupted");
    await connection.setRemoteDescription(description("offer", offer));
    for (const transceiver of connection.getTransceivers()) this.#attach(transceiver);
    this.#watchConnection();
    return this.#gather(await connection.createAnswer());
  }

  async restartComplete(answer?: readonly number[]): Promise<void> {
    const connection = this.#peer();
    if (answer !== undefined) await connection.setRemoteDescription(description("answer", answer));
    this.#probeMedia();
    this.#recovering = false;
    this.#updateState();
  }

  async #gather(local: RTCSessionDescriptionInit): Promise<readonly number[]> {
    const connection = this.#peer();
    const signal = this.#stop.signal;
    await connection.setLocalDescription(local);
    if (connection.iceGatheringState !== "complete" && !signal.aborted) {
      await new Promise<void>((resolve) => {
        const finish = () => {
          clearTimeout(timer);
          signal.removeEventListener("abort", finish);
          connection.removeEventListener("icegatheringstatechange", changed);
          resolve();
        };
        const changed = () => { if (connection.iceGatheringState === "complete") finish(); };
        const timer = setTimeout(finish, GATHER_MS);
        connection.addEventListener("icegatheringstatechange", changed);
        signal.addEventListener("abort", finish, { once: true });
      });
    }
    const sdp = connection.localDescription?.sdp;
    if (signal.aborted || !sdp) throw new CallTransportError("connection");
    const bytes = encode.encode(sdp);
    if (bytes.length > MAX_SDP_BYTES) throw new CallTransportError("connection");
    return Array.from(bytes);
  }

  async install(keys: SFrameInstall): Promise<void> {
    try {
      const ready = this.#workerReady;
      if (!ready || this.#stop.signal.aborted) throw new CallTransportError("connection");
      this.#watchConnection();
      await ready;
      if (!this.#worker) throw new CallTransportError("connection");
      // Transfer ownership once. The worker clears the buffers after importing into WebCrypto.
      const buffers = [...new Set([keys.send.buffer, keys.receive.buffer])];
      this.#worker.postMessage(keys, buffers);
    } finally {
      if (keys.send.byteLength > 0) keys.send.fill(0);
      if (keys.receive.byteLength > 0) keys.receive.fill(0);
    }
  }

  #watchConnection(): void {
    if (this.#stop.signal.aborted) return;
    clearTimeout(this.#timeout);
    this.#timeout = setTimeout(() => { this.#report("failed"); }, CONNECT_MS);
  }

  #probeMedia(): void {
    this.#received = false;
    this.#worker?.postMessage({ probe: ++this.#probe } satisfies SFrameMediaProbe);
  }

  #updateState(): void {
    const state = this.#connection?.connectionState;
    if (state === "connected" && !this.#recovering && this.#keyed && this.#received) {
      clearTimeout(this.#timeout);
      this.#report("active");
    } else if (state === "disconnected" || state === "failed") {
      if (this.#fact === "active") {
        this.#watchConnection();
        this.#probeMedia();
      }
      this.#report("interrupted");
    }
  }

  #report(fact: CallMediaFact): void {
    if (this.#stop.signal.aborted || fact === this.#fact) return;
    this.#fact = fact;
    if (fact === "failed") this.close();
    this.#facts.state(fact);
  }

  setEnabled(kind: CallMediaKind, enabled: boolean): void {
    for (const track of this.#capture) if (track.kind === kind) track.enabled = enabled;
  }

  async devices(): Promise<readonly MediaDeviceInfo[]> {
    return this.#env.media?.enumerateDevices() ?? [];
  }

  async selectInput(kind: CallMediaKind, deviceId: string): Promise<void> {
    const sender = this.#connection?.getSenders().find((entry) => entry.track?.kind === kind);
    const old = sender?.track;
    if (!sender || !old || this.#inputs[kind] === deviceId) return;
    const [track] = await this.#captureMedia(kind, deviceId);
    if (!track) throw new CallTransportError("device");
    track.enabled = old.enabled;
    try {
      await sender.replaceTrack(track);
    } catch (error) {
      track.stop();
      this.#capture = this.#capture.filter((item) => item !== track);
      throw error;
    }
    old.stop();
    this.#capture = this.#capture.filter((item) => item !== old);
    this.#inputs[kind] = deviceId;
  }

  close(): void {
    if (this.#stop.signal.aborted) return;
    this.#stop.abort();
    clearTimeout(this.#timeout);
    this.#connection?.close();
    this.#worker?.terminate();
    this.#capture.forEach((track) => { track.stop(); });
    this.#remote?.getTracks().forEach((track) => { track.stop(); });
    this.#capture = [];
    this.#transformed.clear();
    this.#connection = undefined;
    this.#worker = undefined;
    this.#workerReady = undefined;
    this.#remote = undefined;
  }
}
