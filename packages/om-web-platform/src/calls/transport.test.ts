import { describe, expect, it, vi } from "vitest";
import { CallTransport, type CallTransportEnvironment } from "./transport";

class Track {
  enabled = true;
  stop = vi.fn();
  constructor(readonly kind: "audio" | "video") {}
}

function stream(tracks: Track[] = []): MediaStream {
  return {
    getTracks: () => tracks,
    addTrack: (track: Track) => { tracks.push(track); },
  } as unknown as MediaStream;
}

const encode = (sdp: string) => [...new TextEncoder().encode(sdp)];

function transceiver(track: Track | null, kind = track?.kind ?? "audio") {
  const t = {
    sender: {
      track,
      transform: null,
      replaceTrack: vi.fn((next: Track) => { t.sender.track = next; return Promise.resolve(); }),
    },
    receiver: { track: new Track(kind), transform: null },
    direction: "recvonly",
  };
  return t;
}

class FakeConnection extends EventTarget {
  iceGatheringState = "new";
  connectionState = "new";
  localDescription: RTCSessionDescriptionInit | undefined;
  transceivers: ReturnType<typeof transceiver>[] = [];
  close = vi.fn();
  createOffer = vi.fn((opts?: RTCOfferOptions) => Promise.resolve({ type: "offer", sdp: opts?.iceRestart ? "restart" : "offer" } satisfies RTCSessionDescriptionInit));
  createAnswer = vi.fn(() => Promise.resolve({ type: "answer", sdp: "answer" } satisfies RTCSessionDescriptionInit));
  setLocalDescription = vi.fn((desc: RTCSessionDescriptionInit) => {
    this.iceGatheringState = "gathering";
    this.localDescription = desc;
    return Promise.resolve();
  });
  setRemoteDescription = vi.fn((desc: RTCSessionDescriptionInit) => {
    if (desc.type === "offer" && this.transceivers.length === 0) this.transceivers.push(transceiver(null), transceiver(null, "video"));
    return Promise.resolve();
  });
  addTransceiver = vi.fn((track: Track) => {
    const t = transceiver(track);
    this.transceivers.push(t);
    return t;
  });
  getTransceivers = () => this.transceivers;
  getSenders = () => this.transceivers.map(({ sender }) => sender);

  gathered(): void {
    this.localDescription = { ...this.localDescription!, sdp: `${this.localDescription!.sdp} candidate` };
    this.iceGatheringState = "complete";
    this.dispatchEvent(new Event("icegatheringstatechange"));
  }

  connected(value: string): void {
    this.connectionState = value;
    this.dispatchEvent(new Event("connectionstatechange"));
  }
}

class FakeWorker extends EventTarget {
  postMessage = vi.fn();
  terminate = vi.fn();
  send(data: string): void { this.dispatchEvent(new MessageEvent("message", { data })); }
}

function fixture(overrides: Partial<CallTransportEnvironment> = {}, supported = true) {
  const connection = new FakeConnection();
  const worker = new FakeWorker();
  const mic = new Track("audio");
  const camera = new Track("video");
  const replacement = new Track("audio");
  const getUserMedia = vi.fn((constraints: MediaStreamConstraints) => Promise.resolve(stream(
    typeof constraints.audio === "object" ? [replacement] : constraints.video ? [mic, camera] : [mic],
  )));
  const createPeer = vi.fn(() => connection as unknown as RTCPeerConnection);
  const state = vi.fn();
  const transport = new CallTransport({
    peer: createPeer,
    worker: () => worker as unknown as Worker,
    ...(supported ? { transform: (_worker: Worker, direction: "send" | "receive") => ({ direction }) as unknown as RTCRtpScriptTransform } : {}),
    media: { getUserMedia, enumerateDevices: () => Promise.resolve([]) },
    stream: () => stream(),
    ...overrides,
  }, { state, stream: vi.fn() });
  return { connection, worker, mic, camera, replacement, getUserMedia, createPeer, state, transport };
}

describe("CallTransport", () => {
  it("refuses unsupported capture before creating a peer, worker or permission prompt", async () => {
    const { getUserMedia, createPeer, transport } = fixture({}, false);
    await expect(transport.offer("audio")).rejects.toMatchObject({ reason: "unsupported" });
    expect(createPeer).not.toHaveBeenCalled();
    expect(getUserMedia).not.toHaveBeenCalled();
  });

  it("gathers without ICE servers, transforms both directions, and only reports active after decrypted media", async () => {
    const f = fixture();
    const offer = f.transport.offer("video");
    await vi.waitFor(() => { expect(f.connection.setLocalDescription).toHaveBeenCalled(); });
    expect(f.createPeer).toHaveBeenCalledWith({ iceServers: [], iceCandidatePoolSize: 0 });
    for (const t of f.connection.transceivers) {
      expect(t.sender.transform).toEqual({ direction: "send" });
      expect(t.receiver.transform).toEqual({ direction: "receive" });
    }
    f.connection.gathered();
    expect(await offer).toEqual(encode("offer candidate"));
    await f.transport.remoteAnswer(encode("answer"));
    f.worker.send("ready");
    const send = new Uint8Array(32).fill(1);
    const receive = new Uint8Array(32).fill(2);
    await f.transport.install({ generation: 0, send, receive });
    expect(f.worker.postMessage).toHaveBeenCalledOnce();
    expect(send).toEqual(new Uint8Array(32));
    expect(receive).toEqual(new Uint8Array(32));
    f.connection.connected("connected");
    f.worker.send("keyed");
    expect(f.state).not.toHaveBeenCalled();
    f.worker.send("received");
    expect(f.state).toHaveBeenCalledWith("active");
    f.transport.close();
    expect(f.mic.stop).toHaveBeenCalledOnce();
    expect(f.camera.stop).toHaveBeenCalledOnce();
    expect(f.worker.terminate).toHaveBeenCalledOnce();
    expect(f.connection.close).toHaveBeenCalledOnce();
  });

  it("answers video with audio only and switches input without enabling a muted microphone", async () => {
    const f = fixture();
    const answer = f.transport.answer(encode("offer"), "audio");
    await vi.waitFor(() => { expect(f.connection.setLocalDescription).toHaveBeenCalled(); });
    f.connection.gathered();
    expect(await answer).toEqual(encode("answer candidate"));
    expect(f.getUserMedia).toHaveBeenCalledWith({ audio: true, video: false });
    expect(f.connection.transceivers[1]?.sender.track).toBeNull();
    f.transport.setEnabled("audio", false);
    await f.transport.selectInput("audio", "new-mic");
    expect(f.replacement.enabled).toBe(false);
    expect(f.mic.stop).toHaveBeenCalledOnce();
    expect(f.connection.transceivers[0]?.sender.track).toBe(f.replacement);
    f.transport.close();
    expect(f.replacement.stop).toHaveBeenCalledOnce();
    expect(f.camera.stop).not.toHaveBeenCalled();
  });

  it("uses the same transport and key installation across ICE restart", async () => {
    const f = fixture();
    const offer = f.transport.offer("audio");
    await vi.waitFor(() => { expect(f.connection.setLocalDescription).toHaveBeenCalled(); });
    f.connection.gathered();
    await offer;
    await f.transport.remoteAnswer(encode("answer"));
    f.worker.send("ready");
    await f.transport.install({ generation: 0, send: new Uint8Array(32), receive: new Uint8Array(32) });
    f.worker.send("keyed");
    f.worker.send("received");
    f.connection.connected("connected");
    const restart = f.transport.restartOffer();
    await vi.waitFor(() => { expect(f.connection.setLocalDescription).toHaveBeenCalledTimes(2); });
    f.connection.gathered();
    expect(await restart).toEqual(encode("restart candidate"));
    expect(f.connection.createOffer).toHaveBeenLastCalledWith({ iceRestart: true });
    await f.transport.restartComplete(encode("new answer"));
    expect(f.state.mock.calls).toEqual([["active"], ["interrupted"], ["active"]]);
    expect(f.worker.postMessage).toHaveBeenCalledOnce();
    expect(f.createPeer).toHaveBeenCalledOnce();
    f.transport.close();
  });

  it("stops late permission results after cancellation", async () => {
    const mic = new Track("audio");
    let grant!: (value: MediaStream) => void;
    const capture = new Promise<MediaStream>((resolve) => { grant = resolve; });
    const f = fixture({ media: { getUserMedia: () => capture, enumerateDevices: () => Promise.resolve([]) } });
    const offered = f.transport.offer("audio");
    f.transport.close();
    grant(stream([mic]));
    await expect(offered).rejects.toMatchObject({ reason: "connection" });
    expect(mic.stop).toHaveBeenCalledOnce();
    expect(f.connection.createOffer).not.toHaveBeenCalled();
  });

  it("reports permission denial and releases the worker and connection on teardown", async () => {
    const f = fixture({ media: {
      getUserMedia: () => Promise.reject(new DOMException("Denied", "NotAllowedError")),
      enumerateDevices: () => Promise.resolve([]),
    } });
    await expect(f.transport.offer("audio")).rejects.toMatchObject({ reason: "permission" });
    f.transport.close();
    expect(f.worker.terminate).toHaveBeenCalledOnce();
    expect(f.connection.close).toHaveBeenCalledOnce();
  });
});
