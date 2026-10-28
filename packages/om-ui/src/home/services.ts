// What the Home settings screen needs from its host. The screen never fetches, stores or validates anything itself: it
// shows what the host reports and sends the user's choice back. (The web app adapts @om/provider-http to this.)

export type HomeUrlProblem = "malformed" | "not_https" | "credentials" | "query_or_fragment";
export type HomeHealth = "healthy" | "degraded" | "unhealthy" | "unknown";

/** The outcome of asking an address who it is. `providerId` and `health` are the Home's own claims. */
export type HomeProbeResult =
  | { readonly kind: "invalid_url"; readonly reason: HomeUrlProblem }
  | { readonly kind: "ok"; readonly baseUrl: string; readonly providerId: string; readonly health: HomeHealth }
  | { readonly kind: "unreachable" }
  | { readonly kind: "failing"; readonly status: number }
  | { readonly kind: "not_a_home" }
  | { readonly kind: "wrong_version"; readonly offered: readonly string[] }
  | { readonly kind: "no_mailbox" };

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
}
