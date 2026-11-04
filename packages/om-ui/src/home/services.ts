// What the Home settings screen needs from its host. The screen never fetches, stores or validates anything itself: it
// shows what the host reports and sends the user's choice back. (The web app adapts @om/provider-http to this.)

export type HomeUrlProblem = "malformed" | "not_https" | "credentials" | "query_or_fragment";
export type HomeHealth = "healthy" | "degraded" | "unhealthy" | "unknown";

/** The outcome of asking an address who it is. `providerId` and `health` are the Home's own claims. */
export type HomeProbeResult =
  | { readonly kind: "invalid_url"; readonly reason: HomeUrlProblem }
  | { readonly kind: "ok"; readonly baseUrl: string; readonly providerId: string; readonly health: HomeHealth; /** The Home asks for the owner's admission secret (om.admission.v1). Unknown to a host that does not check. */ readonly admission?: boolean }
  | { readonly kind: "unreachable" }
  | { readonly kind: "failing"; readonly status: number }
  | { readonly kind: "not_a_home" }
  | { readonly kind: "wrong_version"; readonly offered: readonly string[] }
  | { readonly kind: "no_mailbox" };

/** What a delivery service is to this person: the one OM mainly uses, a backup it falls back to, or one the person is moving away from. */
export type DeliveryRole = "main" | "backup" | "leaving";
export interface DeliveryServiceItem { readonly baseUrl: string; readonly role: DeliveryRole }

/** `added`: stored with its setup secret; `already`: this address is already in the list (nothing changed); `full`: the list is at its limit; `failed`: nothing was stored. */
export type AddBackupResult = "added" | "already" | "full" | "failed";

/**
 * Moving away from a service. `moved`: Core published a new set of services to every chat that has another; `stuck` is how many chats
 * had no other service and so stay on this one for now (a plain refusal for them, 0 when none). `last`: there is no other service to move to.
 */
export type MoveResult = { readonly kind: "moved"; readonly stuck: number } | { readonly kind: "last" } | { readonly kind: "failed" };

/** More than one delivery service (Advanced): list, add a backup, move away. The screen never stores or validates; the host does. */
export interface MoreHomesServices {
  /** Every service of this person, the main one first. */
  readonly services: readonly DeliveryServiceItem[];
  /** Where the one-click setup lives and the setup value for the next service (the same pieces as first run). */
  readonly setup: {
    readonly deployUrl: string | undefined;
    /** The setup value for a new service (public: a hash of a secret that stays on this device). Same value until a service is added. */
    readonly prepare: () => Promise<string>;
  };
  /** Stores a checked address and its matching setup secret as a backup. Takes effect on reconnect. */
  readonly add: (baseUrl: string) => Promise<AddBackupResult>;
  /** Stops using this service for new routes and moves every chat that has another service off it. May be repeated for the same service. */
  readonly move: (baseUrl: string) => Promise<MoveResult>;
}

export interface HomeSettingsServices {
  /** The Home the app is running against right now. */
  readonly current: {
    readonly baseUrl: string | undefined;
    /** `chosen` = the user's own choice; `default` = baked into this build of the app; `none` = no Home. */
    readonly source: "chosen" | "default" | "none";
    /** Whether this build has a default to go back to. */
    readonly hasDefault: boolean;
  };
  /** Validates the text and, if it is an acceptable address, asks it who it is. Never stores anything. */
  readonly probe: (input: string) => Promise<HomeProbeResult>;
  /** Stores the user's choice for the next connection. `false`: it was refused and nothing changed. */
  readonly save: (baseUrl: string) => Promise<boolean>;
  /** Forgets the user's choice (back to the build default, or to none). */
  readonly clear: () => Promise<boolean>;
  /** Mailboxes at this Home that another party already holds, as opaque hex ids (in memory; changes while the app runs). */
  readonly lostMailboxes: () => readonly string[];
  /** Reconnects with the saved choice. The host decides how (it restarts the connection and asks to unlock again). */
  readonly reconnect: () => void;
  /** Present when the host can hold more than one delivery service (the Advanced part of the page). */
  readonly more?: MoreHomesServices;
}
