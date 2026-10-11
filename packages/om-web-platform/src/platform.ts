import type {
  BackgroundPush, Capability, CapabilityEntry, CapabilityReport, ClipboardWriteResult, FamilyApis,
  FamilyName, LocalContact, LocalContactsMode, LocalDiscoveryMode, PermissionState, PickedFile,
  PlatformAdapter,
} from "@om/platform-api";
import { FAMILY_NAMES, available, partial, unavailable } from "@om/platform-api";
import type { FileInputLike, FileLike, WebEnv } from "./env";
import { browserEnv } from "./env";

const isFn = (v: unknown): boolean => typeof v === "function";
const perm = (p: "granted" | "denied" | "default"): PermissionState => (p === "default" ? "prompt" : p);

const toPicked = (f: FileLike): PickedFile => ({
  name: f.name,
  size: f.size,
  mimeType: f.type,
  bytes: async () => new Uint8Array(await f.arrayBuffer()),
});

function pickViaInput(env: WebEnv, accept: string, multiple: boolean): Promise<readonly PickedFile[]> {
  const input = env.document?.createElement?.("input");
  if (input === undefined) return Promise.resolve([]);
  return new Promise((resolve) => {
    const finish = (el: FileInputLike): void => {
      resolve(Array.from(el.files ?? []).map(toPicked));
    };
    input.type = "file";
    input.multiple = multiple;
    input.accept = accept;
    input.addEventListener("change", () => { finish(input); });
    input.addEventListener("cancel", () => { resolve([]); });
    input.click();
  });
}

async function mediaAccess(env: WebEnv, kind: "audio" | "video"): Promise<"granted" | "denied"> {
  const devices = env.navigator?.mediaDevices;
  if (devices?.getUserMedia === undefined) return "denied";
  try {
    const stream = await devices.getUserMedia(kind === "audio" ? { audio: true } : { video: true });
    for (const t of stream.getTracks()) t.stop();
    return "granted";
  } catch {
    return "denied";
  }
}

type Builders = { [K in FamilyName]: (env: WebEnv) => Capability<FamilyApis[K]> };

const builders: Builders = {
  secureStorage: (env) =>
    env.indexedDB !== undefined && env.crypto?.subtle !== undefined
      ? partial({ protection: "devicevault_wrapped" }, "not_at_rest_sealed_by_os")
      : unavailable("no_indexeddb_or_webcrypto"),

  biometrics: (env) => {
    const check = env.PublicKeyCredential?.isUserVerifyingPlatformAuthenticatorAvailable;
    return check === undefined
      ? unavailable("no_webauthn")
      : partial({ userVerifyingAvailable: () => env.PublicKeyCredential?.isUserVerifyingPlatformAuthenticatorAvailable?.() ?? Promise.resolve(false) });
  },

  push: (env) =>
    env.navigator?.serviceWorker !== undefined && env.PushManager !== undefined && env.Notification !== undefined
      ? available({
          subscribe: async (applicationServerKey) => {
            if (env.Notification?.requestPermission === undefined) return "denied";
            if ((await env.Notification.requestPermission()) !== "granted") return "denied";
            const reg = await env.navigator?.serviceWorker?.ready;
            const sub = await reg?.pushManager?.subscribe({ userVisibleOnly: true, applicationServerKey });
            const p256dh = sub?.getKey("p256dh");
            const auth = sub?.getKey("auth");
            if (sub === undefined || !p256dh || !auth) return "denied";
            return { endpoint: sub.endpoint, p256dh: new Uint8Array(p256dh), auth: new Uint8Array(auth) };
          },
          unsubscribe: async () => {
            const reg = await env.navigator?.serviceWorker?.ready;
            const sub = await reg?.pushManager?.getSubscription();
            return (await sub?.unsubscribe()) ?? false;
          },
        })
      : unavailable("no_serviceworker_pushmanager_or_notification"),

  notifications: (env) =>
    env.Notification === undefined
      ? unavailable("no_notification_api")
      : available({
          permission: () => perm(env.Notification?.permission ?? "denied"),
          requestPermission: async () => perm((await env.Notification?.requestPermission?.()) ?? "denied"),
        }),

  backgroundExecution: (env) =>
    env.navigator?.serviceWorker === undefined
      ? unavailable("no_serviceworker")
      : partial({ mechanism: "service_worker" }, "browser_lifecycle_only"),

  share: (env) => {
    const share = env.navigator?.share;
    return share === undefined
      ? unavailable("no_navigator_share")
      : available({
          share: async (content) => {
            try {
              await env.navigator?.share?.(content);
              return "shared";
            } catch (e) {
              return e instanceof Error && e.name === "AbortError" ? "cancelled" : "failed";
            }
          },
        });
  },

  openExternal: (env) =>
    env.open === undefined
      ? unavailable("no_window_open")
      : available({ open: (url) => { env.open?.(url, "_blank", "noopener,noreferrer"); } }),

  files: (env) => {
    const api = {
      pick: async (o?: { accept?: readonly string[]; multiple?: boolean }): Promise<readonly PickedFile[]> => {
        const multiple = o?.multiple ?? false;
        if (env.showOpenFilePicker !== undefined) {
          try {
            const handles = await env.showOpenFilePicker({ multiple });
            return await Promise.all(handles.map(async (h) => toPicked(await h.getFile())));
          } catch {
            return [];
          }
        }
        return pickViaInput(env, (o?.accept ?? []).join(","), multiple);
      },
    };
    if (env.showOpenFilePicker !== undefined) return available(api);
    return env.document?.createElement === undefined ? unavailable("no_file_input") : partial(api, "input_element_fallback");
  },

  camera: (env) =>
    env.navigator?.mediaDevices?.getUserMedia === undefined
      ? unavailable("no_media_devices")
      : available({ requestAccess: () => mediaAccess(env, "video") }),

  microphone: (env) =>
    env.navigator?.mediaDevices?.getUserMedia === undefined
      ? unavailable("no_media_devices")
      : available({ requestAccess: () => mediaAccess(env, "audio") }),

  contactsLocal: (env) =>
    env.navigator?.contacts?.select === undefined
      ? unavailable("no_contact_picker")
      : partial({
          pick: async (): Promise<readonly LocalContact[]> => {
            const rows = (await env.navigator?.contacts?.select?.(["name", "tel"], { multiple: true })) ?? [];
            return rows.map((r) => (r.name?.[0] === undefined ? { tel: r.tel ?? [] } : { name: r.name[0], tel: r.tel ?? [] }));
          },
        }, "user_picked_only"),

  clipboard: (env) =>
    env.navigator?.clipboard?.writeText === undefined
      ? unavailable("no_async_clipboard")
      : partial({
          // Fail closed: a sensitive item is never written to a clipboard we cannot mark.
          write: async (text, options): Promise<ClipboardWriteResult> => {
            if (options.sensitive) return "sensitive_unsupported";
            try {
              await env.navigator?.clipboard?.writeText?.(text);
              return "written";
            } catch {
              return "failed";
            }
          },
        }, "sensitive_unsupported"),

  secureSurface: () => unavailable("web_cannot_block_capture"),
  backupExclusion: () => unavailable("web_cannot_exclude_backup"),

  inputPrivacy: () =>
    partial({
      fieldAttributes: () => ({ spellcheck: "false", autocomplete: "off", autocorrect: "off", autocapitalize: "off" }),
    }, "hints_only"),

  notificationRemoval: (env) =>
    env.ServiceWorkerRegistration !== undefined &&
    isFn((env.ServiceWorkerRegistration.prototype as { getNotifications?: unknown }).getNotifications) &&
    env.navigator?.serviceWorker !== undefined
      ? partial({
          remove: async (tag) => {
            const reg = await env.navigator?.serviceWorker?.ready;
            const list = (await reg?.getNotifications?.(tag === undefined ? undefined : { tag })) ?? [];
            for (const n of list) n.close();
            return list.length;
          },
        }, "os_history_not_cleared")
      : unavailable("no_get_notifications"),

  localDiscovery: () => partial({ mode: "manual_known_endpoint" }, "no_automatic_mdns"),
  callSystemIntegration: () => unavailable("no_native_call_ui"),

  networkHints: (env) =>
    typeof env.navigator?.onLine === "boolean"
      ? available({ isOnline: () => env.navigator?.onLine ?? false })
      : unavailable("no_online_hint"),

  windowSystem: (env) =>
    env.document?.visibilityState === undefined
      ? unavailable("no_visibility_state")
      : available({ visibility: () => (env.document?.visibilityState === "visible" ? "visible" : "hidden") }),

  haptics: (env) =>
    env.navigator?.vibrate === undefined
      ? unavailable("no_vibrate")
      : available({ vibrate: (p) => env.navigator?.vibrate?.(p) ?? false }),

  externalTransport: () => unavailable("web_has_no_sms_transport"),
};

/** Capabilities are decided by feature probes on `env`; never by user agent. Probing is side-effect free. */
export function createWebPlatform(env: WebEnv = browserEnv()): PlatformAdapter {
  const get = <K extends FamilyName>(k: K): Capability<FamilyApis[K]> => builders[k](env);
  const entry = (k: FamilyName): CapabilityEntry => {
    const c = get(k);
    return c.detail === undefined ? { state: c.state } : { state: c.state, detail: c.detail };
  };
  const capabilities = (): CapabilityReport => {
    const families = Object.fromEntries(FAMILY_NAMES.map((k) => [k, entry(k)])) as Record<FamilyName, CapabilityEntry>;
    const backgroundPush: BackgroundPush = families.push.state === "available" ? "webpush" : "none";
    const localDiscovery: LocalDiscoveryMode = get("localDiscovery").state === "unavailable" ? "unavailable" : "manual_known_endpoint";
    const localContacts: LocalContactsMode = families.contactsLocal.state === "unavailable" ? "unavailable" : "read_optional";
    return { families, backgroundPush, localDiscovery, localContacts, callSystemIntegration: families.callSystemIntegration.state !== "unavailable" };
  };
  return {
    capabilities,
    secureStorage: () => get("secureStorage"),
    biometrics: () => get("biometrics"),
    push: () => get("push"),
    notifications: () => get("notifications"),
    backgroundExecution: () => get("backgroundExecution"),
    share: () => get("share"),
    openExternal: () => get("openExternal"),
    files: () => get("files"),
    camera: () => get("camera"),
    microphone: () => get("microphone"),
    contactsLocal: () => get("contactsLocal"),
    clipboard: () => get("clipboard"),
    secureSurface: () => get("secureSurface"),
    backupExclusion: () => get("backupExclusion"),
    inputPrivacy: () => get("inputPrivacy"),
    notificationRemoval: () => get("notificationRemoval"),
    localDiscovery: () => get("localDiscovery"),
    callSystemIntegration: () => get("callSystemIntegration"),
    networkHints: () => get("networkHints"),
    windowSystem: () => get("windowSystem"),
    haptics: () => get("haptics"),
    externalTransport: () => get("externalTransport"),
  };
}
