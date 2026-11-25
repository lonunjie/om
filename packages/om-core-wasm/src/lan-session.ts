// SPDX-License-Identifier: MPL-2.0
// One local-delivery connection (ADR-0158 sections 3 and 4): the offer and answer SDP of the `om-lan-v1` channel travel only as
// `LanSignalEnvelopeV1`, sealed and opened by Core under the pair seed (which never leaves Core). The envelopes are carried by a
// `Signaller` the host provides (QR, copy and paste, a shared screen): this file never touches a network and never holds a key.
// The peer is structural so any `LanPeer` of `@om/web-platform/lan` fits, and so tests can use a fake. A refusal or a mismatch
// ends the attempt with a reason; there is no weaker fallback (a channel is never opened from an unsealed or unmatched answer).
import type { ProfileExecutionContext } from "@om/platform-api";
import type { ScopeEngines } from "./scope-engines";

/** Carries sealed envelopes (hex) to and from the other person by a route the host owns. */
export interface Signaller {
  send(envelopeHex: string): Promise<void>;
  /** The next envelope from the other person; rejects if the attempt is abandoned. */
  receive(): Promise<string>;
}

export interface SessionPeer<Channel> {
  offer(): Promise<string>;
  accept(offerSdp: string): Promise<string>;
  finish(answerSdp: string): Promise<void>;
  opened(): Promise<Channel>;
  close(): void;
}

export type LanSessionFailure = "refused" | "wrong_kind" | "wrong_session";
export class LanSessionError extends Error {
  constructor(readonly reason: LanSessionFailure, readonly detail?: string) { super(detail === undefined ? reason : `${reason}: ${detail}`); }
}

export interface LanSessionOptions<Channel> {
  readonly lanSignal: ScopeEngines["lanSignal"];
  /** Captured by the caller at dispatch (ProfileExecutionContext): the session never reads a global active profile. */
  readonly ctx: ProfileExecutionContext;
  readonly conversationId: string;
  readonly peer: SessionPeer<Channel>;
  readonly signaller: Signaller;
}

/** Start the connection: seal an offer, wait for the sealed answer to the same session, then wait for the channel. */
export async function lanInitiate<Channel>(o: LanSessionOptions<Channel> & { readonly sessionId: string }): Promise<Channel> {
  try {
    const sealed = o.lanSignal.seal(o.ctx, o.conversationId, { kind: "offer", sessionId: o.sessionId, sdp: await o.peer.offer() });
    if ("refused" in sealed) throw new LanSessionError("refused", sealed.refused);
    await o.signaller.send(sealed.envelope);
    const back = o.lanSignal.open(o.ctx, o.conversationId, await o.signaller.receive());
    if ("refused" in back) throw new LanSessionError("refused", back.refused);
    if (back.kind !== "answer") throw new LanSessionError("wrong_kind", back.kind);
    if (back.sessionId !== o.sessionId) throw new LanSessionError("wrong_session");
    await o.peer.finish(back.sdp);
    return await o.peer.opened();
  } catch (e) {
    o.peer.close();
    throw e;
  }
}

/** Answer a connection: open the sealed offer, return the sealed answer under the same session id, then wait for the channel. */
export async function lanRespond<Channel>(o: LanSessionOptions<Channel>): Promise<Channel> {
  try {
    const got = o.lanSignal.open(o.ctx, o.conversationId, await o.signaller.receive());
    if ("refused" in got) throw new LanSessionError("refused", got.refused);
    if (got.kind !== "offer") throw new LanSessionError("wrong_kind", got.kind);
    const sealed = o.lanSignal.seal(o.ctx, o.conversationId, { kind: "answer", sessionId: got.sessionId, sdp: await o.peer.accept(got.sdp) });
    if ("refused" in sealed) throw new LanSessionError("refused", sealed.refused);
    await o.signaller.send(sealed.envelope);
    return await o.peer.opened();
  } catch (e) {
    o.peer.close();
    throw e;
  }
}
