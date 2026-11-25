/**
 * The per-contact "find each other on the same Wi-Fi" choice (LAN_RENDEZVOUS_AND_OFFLINE_LOCAL_MODE section 2.1: off by default, per
 * contact, off under Hardened and Maximum Privacy). Core holds the choice and decides whether it may be on; this is only what the
 * UI needs to show it and to ask for a change. The host supplies it, behind a flag that is off in releases, so the row never
 * appears unless a host can actually carry it. Nothing here holds a key, a tag or a route.
 */
export interface NearbyView {
  /** The person turned it on for this contact and Core allows it now. */
  readonly enabled: boolean;
  /** Core would let it be turned on: the protection level allows it and the chat is active. */
  readonly available: boolean;
}

export interface NearbyService {
  /** What this device's browser or system can do (from the `localDelivery` capability row). `partial` is said, not hidden. */
  readonly platform: "available" | "partial" | "unavailable";
  state(conversationId: string): Promise<NearbyView>;
  /** Asks Core for the change and answers with what Core now says (never what was asked for). Rejects when Core could not be reached. */
  set(conversationId: string, on: boolean): Promise<NearbyView>;
}
