// SPDX-License-Identifier: AGPL-3.0-or-later
// Runtime bindings for the shared queue's WebSocket code. Injected only in the Node build.
import { WAKE_AUTH_SECONDS, WAKE_CLOSE, WAKE_FRAME_MAX, WAKE_SUBPROTOCOL } from "@om/home-wire";
import { AsyncLocalStorage } from "node:async_hooks";
import type { IncomingMessage } from "node:http";
import type { Duplex } from "node:stream";
import { WebSocketServer } from "ws";

type WireSocket = Parameters<Parameters<WebSocketServer["handleUpgrade"]>[3]>[0];

export const wakeEffects = new AsyncLocalStorage<(() => void)[]>();

function afterCommit(effect: () => void): void {
  const pending = wakeEffects.getStore();
  if (pending) pending.push(effect);
  else effect();
}

export interface WakeQueue {
  readonly sockets: Set<WakeSocket>;
  message(socket: WakeSocket, data: string | ArrayBuffer): Promise<void>;
  release(): void;
}

export class WakeSocket {
  private attachment: unknown;
  private socket?: WireSocket;
  private queue?: WakeQueue;
  private auto?: WebSocketRequestResponsePair;
  private timer?: ReturnType<typeof setTimeout>;

  accept(queue: WakeQueue, auto: WebSocketRequestResponsePair | undefined): void {
    this.queue = queue;
    if (auto) this.auto = auto;
    queue.sockets.add(this);
    this.timer = setTimeout(() => { this.close(WAKE_CLOSE.authTimeout, "too late"); }, WAKE_AUTH_SECONDS * 1000);
    this.timer.unref();
  }

  attach(socket: WireSocket): void {
    this.socket = socket;
    if (!this.queue) { socket.terminate(); return; }
    socket.on("close", () => { this.release(); });
    socket.on("error", () => { this.release(); socket.terminate(); });
    let busy = false;
    socket.on("message", (raw, binary) => {
      if (socket.readyState !== 1) return;
      const text = binary ? undefined : raw.toString();
      if (text !== undefined && text === this.auto?.request) {
        this.send(this.auto.response);
        return;
      }
      if (busy) { this.close(WAKE_CLOSE.badFrame, "bad frame"); return; }
      busy = true;
      const payload = text ?? Uint8Array.from(raw).buffer;
      void this.queue?.message(this, payload)
        .catch(() => { this.close(1011, "error"); })
        .finally(() => { busy = false; });
    });
  }

  serializeAttachment(value: unknown): void {
    this.attachment = value;
    if ((value as { authed?: boolean } | undefined)?.authed === true) clearTimeout(this.timer);
  }

  deserializeAttachment(): unknown { return this.attachment; }

  send(data: string | Uint8Array): void {
    afterCommit(() => {
      if (this.socket?.readyState !== 1 || this.socket.bufferedAmount > 4096) {
        this.terminate();
        return;
      }
      this.socket.send(data);
    });
  }

  close(code: number, reason: string): void {
    afterCommit(() => {
      this.release();
      this.socket?.close(code, reason);
    });
  }

  terminate(): void {
    this.release();
    this.socket?.terminate();
  }

  private release(): void {
    clearTimeout(this.timer);
    const queue = this.queue;
    queue?.sockets.delete(this);
    queue?.release();
    delete this.queue;
  }
}

class WakeClient {
  constructor(readonly peer: WakeSocket) {}
}

export class WebSocketPair {
  readonly 0: WakeClient;
  readonly 1 = new WakeSocket();

  constructor() { this[0] = new WakeClient(this[1]); }
}

export class WebSocketRequestResponsePair {
  constructor(readonly request: string, readonly response: string) {}
}

export const Response = class extends globalThis.Response {
  readonly upgrade: WakeSocket | undefined;

  constructor(body?: BodyInit | null, init?: ResponseInit & { webSocket?: WakeClient }) {
    super(body, init?.status === 101 ? { ...init, status: 200 } : init);
    this.upgrade = init?.webSocket?.peer;
  }

  override get status(): number { return this.upgrade ? 101 : super.status; }
};

export class WakeTransport {
  private readonly server = new WebSocketServer({
    noServer: true,
    // A slightly larger ingress bound lets the Home issue its typed bad-frame close.
    maxPayload: 4 * WAKE_FRAME_MAX,
    perMessageDeflate: false,
    handleProtocols: () => WAKE_SUBPROTOCOL,
  });

  constructor() {
    this.server.on("headers", (headers) => {
      headers.push("Cache-Control: no-store", "Referrer-Policy: no-referrer");
    });
  }

  upgrade(request: IncomingMessage, socket: Duplex, head: Buffer, wake: WakeSocket): void {
    // The HTTP request timeout is for uploads, not an authenticated idle wake socket.
    request.socket.setTimeout(0);
    socket.once("close", () => { wake.terminate(); });
    this.server.handleUpgrade(request, socket, head, (ws) => { wake.attach(ws); });
  }

  close(): void {
    for (const socket of this.server.clients) socket.terminate();
    this.server.close();
  }
}

export function upgradePeer(response: globalThis.Response): WakeSocket | undefined {
  return response instanceof Response ? response.upgrade : undefined;
}
