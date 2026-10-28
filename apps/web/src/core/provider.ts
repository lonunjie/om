// Provider (transport) side of the real Core wiring. A Provider is the host capability Core's network outbox runs
// against: key material, introduction service and relay mailboxes. No production provider adapter exists yet (the
// reference Home is `provider_reference` scope), so the production build runs with `null` and says so honestly:
// every call is "unavailable", which Core keeps as PendingNetwork and retries; nothing is invented or faked.
import type { Provider } from "@om/core-wasm";

/** What the app was started with. `null` means no provider is configured. */
export type ConfiguredProvider = Provider | null;

/** Used when no provider is configured: definitely-not-performed, so Core keeps everything queued. */
export const unavailableProvider: Provider = {
  perform: () => Promise.resolve({ result: "err", error: { kind: "unavailable" } }),
  poll: () => Promise.resolve([]),
};
