// Structural slice of the browser globals the probes look at. Every member is optional so a
// missing API is simply absent; no DOM lib types leak out of this package.
export interface NotificationLike { close(): void }
export interface PushSubscriptionLike {
  readonly endpoint: string;
  getKey(name: "p256dh" | "auth"): ArrayBuffer | null;
  unsubscribe(): Promise<boolean>;
}
export interface RegistrationLike {
  readonly pushManager?: {
    subscribe(o: { userVisibleOnly: boolean; applicationServerKey: Uint8Array }): Promise<PushSubscriptionLike>;
    getSubscription(): Promise<PushSubscriptionLike | null>;
  };
  getNotifications?(filter?: { tag?: string }): Promise<readonly NotificationLike[]>;
}
export interface FileLike {
  readonly name: string;
  readonly size: number;
  readonly type: string;
  arrayBuffer(): Promise<ArrayBuffer>;
}
export interface FileInputLike {
  type: string;
  multiple: boolean;
  accept: string;
  readonly files: ArrayLike<FileLike> | null;
  click(): void;
  addEventListener(event: "change" | "cancel", handler: () => void): void;
}
export interface MediaStreamLike { getTracks(): readonly { stop(): void }[] }
export type NotificationPermissionLike = "granted" | "denied" | "default";

export interface WebEnv {
  readonly navigator?: {
    readonly onLine?: boolean;
    readonly share?: (data: { title?: string; text?: string; url?: string }) => Promise<void>;
    readonly vibrate?: (pattern: readonly number[]) => boolean;
    readonly clipboard?: { readonly writeText?: (text: string) => Promise<void> };
    readonly mediaDevices?: {
      readonly getUserMedia?: (c: { audio?: boolean; video?: boolean }) => Promise<MediaStreamLike>;
    };
    readonly serviceWorker?: { readonly ready: Promise<RegistrationLike> };
    readonly contacts?: {
      readonly select?: (
        props: readonly string[],
        opts?: { multiple?: boolean },
      ) => Promise<readonly { name?: readonly string[]; tel?: readonly string[] }[]>;
    };
  };
  readonly Notification?: {
    readonly permission: NotificationPermissionLike;
    readonly requestPermission?: () => Promise<NotificationPermissionLike>;
  };
  readonly PushManager?: unknown;
  readonly ServiceWorkerRegistration?: { readonly prototype: object };
  readonly PublicKeyCredential?: {
    readonly isUserVerifyingPlatformAuthenticatorAvailable?: () => Promise<boolean>;
  };
  readonly indexedDB?: unknown;
  readonly crypto?: { readonly subtle?: unknown };
  readonly showOpenFilePicker?: (o?: {
    multiple?: boolean;
    types?: { accept: Record<string, string[]> }[];
  }) => Promise<readonly { getFile(): Promise<FileLike> }[]>;
  readonly document?: {
    readonly visibilityState?: string;
    readonly createElement?: (tag: "input") => FileInputLike;
  };
  readonly open?: (url: string, target: string, features: string) => unknown;
}

/** The live browser. Probes only read property presence; nothing here prompts. */
export function browserEnv(): WebEnv {
  return globalThis as unknown as WebEnv;
}
