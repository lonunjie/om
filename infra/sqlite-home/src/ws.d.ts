// SPDX-License-Identifier: AGPL-3.0-or-later
// The interface of ws used by this transport.
declare module "ws" {
  import type { IncomingMessage } from "node:http";
  import type { Duplex } from "node:stream";
  export class WebSocketServer {
    constructor(options: {
      noServer: boolean;
      maxPayload: number;
      perMessageDeflate: boolean;
      handleProtocols: () => string;
    });
    readonly clients: Set<WebSocket>;
    on(event: "headers", callback: (headers: string[]) => void): void;
    handleUpgrade(request: IncomingMessage, socket: Duplex, head: Buffer, callback: (socket: WebSocket) => void): void;
    close(): void;
  }
  interface WebSocket {
    on(event: "message", callback: (data: Buffer, binary: boolean) => void): void;
    on(event: "close" | "error", callback: () => void): void;
    send(data: string | Uint8Array): void;
    close(code: number, reason: string): void;
    terminate(): void;
    readonly readyState: number;
    readonly bufferedAmount: number;
  }
}
