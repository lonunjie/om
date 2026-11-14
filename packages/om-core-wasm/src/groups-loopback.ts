// TEST AND DEV ONLY: an in-process `GroupCarriage` network between profiles, standing in for the carriage the design has not
// specified yet (groups plan Q2). It moves opaque wires between two or more drivers, with the faults a real transport has
// (down, duplicate delivery). It is not a protocol and carries nothing across a process boundary. Never import it from product code.
import type { ConversationId, GroupOutbound, GroupSender, GroupWire, ProfileId } from "@om/platform-api";
import type { CarriageResult, GroupCarriage } from "./groups-driver";
import type { WasmCoreClient } from "./runtime";

type Handler = (profileId: ProfileId, from: GroupSender, wire: GroupWire) => Promise<void>;

export interface GroupLoopback {
  /** Two profiles that share a 1:1: each side's own conversation id for it. */
  connect(a: { profileId: ProfileId; relationship: ConversationId }, b: { profileId: ProfileId; relationship: ConversationId }): void;
  /** A device (hex DeviceID) that belongs to a profile. */
  device(profileId: ProfileId, deviceId: string): void;
  carriage(profileId: ProfileId): GroupCarriage;
  /** While down, every send reports `unavailable`. */
  setDown(profileId: ProfileId, down: boolean): void;
  /** The next delivery to this profile is handed over twice. */
  duplicateNext(profileId: ProfileId): void;
  /** Every wire taken so far, in order (for confinement assertions: a wire to a device never travelled over a relationship). */
  readonly log: readonly { readonly from: ProfileId; readonly to: ProfileId; readonly via: "relationship" | "device"; readonly wire: GroupWire }[];
}

export function createGroupLoopback(): GroupLoopback {
  const peers = new Map<string, { profile: ProfileId; theirRel: ConversationId }>(); // `${profile}|${myRel}` -> the other side
  const devices = new Map<string, ProfileId>();
  const deviceOf = new Map<ProfileId, string>();
  const handlers = new Map<ProfileId, Handler>();
  const down = new Set<ProfileId>();
  const dup = new Set<ProfileId>();
  const log: { from: ProfileId; to: ProfileId; via: "relationship" | "device"; wire: GroupWire }[] = [];

  async function deliver(to: ProfileId, from: GroupSender, wire: GroupWire): Promise<void> {
    const h = handlers.get(to);
    if (!h) return;
    await h(to, from, wire);
    if (dup.delete(to)) await h(to, from, wire);
  }

  return {
    connect(a, b) {
      peers.set(`${a.profileId}|${a.relationship}`, { profile: b.profileId, theirRel: b.relationship });
      peers.set(`${b.profileId}|${b.relationship}`, { profile: a.profileId, theirRel: a.relationship });
    },
    device(profileId, deviceId) { devices.set(deviceId, profileId); deviceOf.set(profileId, deviceId); },
    setDown(p, d) { if (d) down.add(p); else down.delete(p); },
    duplicateNext(p) { dup.add(p); },
    log,
    carriage(me): GroupCarriage {
      return {
        available: () => true,
        async send(profileId: ProfileId, out: GroupOutbound): Promise<CarriageResult> {
          if (down.has(profileId)) return "unavailable";
          if ("relationship" in out.to) {
            const peer = peers.get(`${profileId}|${out.to.relationship}`);
            if (!peer) return "undeliverable";
            if (down.has(peer.profile)) return "unavailable";
            log.push({ from: profileId, to: peer.profile, via: "relationship", wire: out.wire });
            await deliver(peer.profile, { relationship: peer.theirRel }, out.wire);
            return "sent";
          }
          const target = devices.get(out.to.device);
          const mine = deviceOf.get(profileId);
          if (target === undefined || mine === undefined) return "undeliverable";
          if (down.has(target)) return "unavailable";
          log.push({ from: profileId, to: target, via: "device", wire: out.wire });
          await deliver(target, { device: mine }, out.wire);
          return "sent";
        },
        onReceive(handler) {
          handlers.set(me, handler);
          return () => { handlers.delete(me); };
        },
      };
    },
  };
}

/**
 * Browser end-to-end only: a carriage between the profiles open in ONE Core runtime (two identities in one browser), found
 * through the client itself. It assumes exactly the two-identity setup of the e2e: the peer of a relationship is the other
 * open profile, whose only conversation is that 1:1. Anything else is `undeliverable`. Nothing leaves the page.
 */
export function createClientPairCarriage(client: () => Pick<WasmCoreClient, "query" | "contextFor">): GroupCarriage {
  let handler: Handler | undefined;
  const profiles = async (): Promise<readonly ProfileId[]> => (await client().query({ kind: "GetProfileList" })).map((p) => p.id);
  const deviceOf = async (id: ProfileId): Promise<string | undefined> => {
    const v = await client().query({ kind: "GetDevices", ctx: client().contextFor(id) });
    const me = v.devices.find((d) => d.isCurrent);
    return me === undefined ? undefined : String(me.id);
  };
  const relOf = async (id: ProfileId): Promise<ConversationId | undefined> => {
    const l = await client().query({ kind: "GetConversationList", ctx: client().contextFor(id) });
    return l.length === 1 ? l[0]?.id : undefined;
  };
  return {
    available: () => true,
    async send(me, out): Promise<CarriageResult> {
      try {
        const all = await profiles();
        if ("relationship" in out.to) {
          const peers = all.filter((p) => p !== me);
          const peer = peers.length === 1 ? peers[0] : undefined;
          const rel = peer === undefined ? undefined : await relOf(peer);
          if (peer === undefined || rel === undefined || handler === undefined) return "undeliverable";
          await handler(peer, { relationship: rel }, out.wire);
          return "sent";
        }
        let target: ProfileId | undefined;
        for (const p of all) if (p !== me && (await deviceOf(p)) === out.to.device) target = p;
        const mine = await deviceOf(me);
        if (target === undefined || mine === undefined || handler === undefined) return "undeliverable";
        await handler(target, { device: mine }, out.wire);
        return "sent";
      } catch { return "unavailable"; }
    },
    onReceive(h) { handler = h; return () => { handler = undefined; }; },
  };
}
