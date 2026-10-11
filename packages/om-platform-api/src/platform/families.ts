import type { LocalDiscoveryMode } from "./capability";
// Capability families and their deliberately tiny method surfaces. No DOM types.
export type PermissionState = "granted" | "denied" | "prompt";

export interface SecureStorageApi { readonly protection: "os_keystore" | "devicevault_wrapped" }
export interface BiometricsApi { userVerifyingAvailable(): Promise<boolean> }
export interface PushSubscriptionInfo {
  readonly endpoint: string;
  readonly p256dh: Uint8Array;
  readonly auth: Uint8Array;
}
export interface PushApi {
  subscribe(applicationServerKey: Uint8Array): Promise<PushSubscriptionInfo | "denied">;
  unsubscribe(): Promise<boolean>;
}
export interface NotificationsApi {
  permission(): PermissionState;
  requestPermission(): Promise<PermissionState>;
}
export interface BackgroundExecutionApi { readonly mechanism: "service_worker" | "os_task" }
export interface ShareContent { readonly title?: string; readonly text?: string; readonly url?: string }
export interface ShareApi { share(content: ShareContent): Promise<"shared" | "cancelled" | "failed"> }
export interface OpenExternalApi { open(url: string): void }
export interface PickFilesOptions { readonly accept?: readonly string[]; readonly multiple?: boolean }
export interface PickedFile {
  readonly name: string;
  readonly size: number;
  readonly mimeType: string;
  bytes(): Promise<Uint8Array>;
}
export interface FilesApi { pick(options?: PickFilesOptions): Promise<readonly PickedFile[]> }
export interface MediaAccessApi { requestAccess(): Promise<"granted" | "denied"> }
export interface LocalContact { readonly name?: string; readonly tel: readonly string[] }
export interface ContactsLocalApi { pick(): Promise<readonly LocalContact[]> }
export type ClipboardWriteResult = "written" | "sensitive_unsupported" | "failed";
export interface ClipboardApi { write(text: string, options: { readonly sensitive: boolean }): Promise<ClipboardWriteResult> }
export interface SecureSurfaceApi { setProtected(on: boolean): void }
export interface BackupExclusionApi { exclude(path: string): Promise<void> }
export interface InputPrivacyApi { fieldAttributes(): Readonly<Record<string, string>> }
export interface NotificationRemovalApi { remove(tag?: string): Promise<number> }
export interface LocalDiscoveryApi { readonly mode: Exclude<LocalDiscoveryMode, "unavailable"> }
export interface CallSystemIntegrationApi { reportIncoming(callId: string): Promise<void> }
export interface NetworkHintsApi { isOnline(): boolean }
export interface WindowSystemApi { visibility(): "visible" | "hidden" }
export interface HapticsApi { vibrate(pattern: readonly number[]): boolean }
export interface ExternalTransportApi { sendText(to: string, text: string): Promise<void> }

/** Family name -> API type. Keys are the PlatformAdapter accessor names. */
export interface FamilyApis {
  secureStorage: SecureStorageApi;
  biometrics: BiometricsApi;
  push: PushApi;
  notifications: NotificationsApi;
  backgroundExecution: BackgroundExecutionApi;
  share: ShareApi;
  openExternal: OpenExternalApi;
  files: FilesApi;
  camera: MediaAccessApi;
  microphone: MediaAccessApi;
  contactsLocal: ContactsLocalApi;
  clipboard: ClipboardApi;
  secureSurface: SecureSurfaceApi;
  backupExclusion: BackupExclusionApi;
  inputPrivacy: InputPrivacyApi;
  notificationRemoval: NotificationRemovalApi;
  localDiscovery: LocalDiscoveryApi;
  callSystemIntegration: CallSystemIntegrationApi;
  networkHints: NetworkHintsApi;
  windowSystem: WindowSystemApi;
  haptics: HapticsApi;
  externalTransport: ExternalTransportApi;
}

export type FamilyName = keyof FamilyApis;

export const FAMILY_NAMES = [
  "secureStorage", "biometrics", "push", "notifications", "backgroundExecution", "share",
  "openExternal", "files", "camera", "microphone", "contactsLocal", "clipboard",
  "secureSurface", "backupExclusion", "inputPrivacy", "notificationRemoval", "localDiscovery",
  "callSystemIntegration", "networkHints", "windowSystem", "haptics", "externalTransport",
] as const satisfies readonly FamilyName[];
