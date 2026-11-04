import type {
  AccountControlStateCode, CoreIntentKind, DeviceCautionCode, DeviceRole, DeviceStateCode, LastSeenClass, LinkOfferState, LinkRequestState,
  RejectReason, UserActionCode,
} from "@om/platform-api";

/** Wording for Core outcome classes. A timeout is never terminal, so there is no "failed" for pending outcomes. */
export interface OutcomeCopy {
  readonly working: string;
  readonly pendingAuthorization: string;
  readonly pendingNetwork: string;
  readonly pendingReconciliation: string;
  readonly needsAction: Readonly<Record<UserActionCode, string>>;
  readonly rejected: Readonly<Record<RejectReason, string>>;
  /** The request could not be handed to Core at all (nothing was sent). */
  readonly failed: string;
}

export type FrozenControlState = Exclude<AccountControlStateCode, "normal">;
export type DevicePendingKind = Extract<CoreIntentKind, "OfferDeviceLink" | "ApproveDeviceLink" | "RejectDeviceLink" | "SuspendDevice" | "RevokeDevice">;
export type LinkParseError = "empty" | "too_long" | "malformed";

/**
 * Existing-device side of linking (deviation D24, docs/reports/2026-10-28-relay-status.md): this device shows a link and a QR
 * code for the new device, shows the same check code the new device shows, and a person approves or declines here.
 * The design documents assumed the opposite direction (the new device offers, the existing one scans); the operator's
 * D24 ruling of 2026-11-04 reversed it.
 */
export interface LinkCopy {
  /** The button in the device list. */
  readonly start: string;
  /** Shown in place of the button on a device that cannot add devices (a messaging-only device). */
  readonly notControl: string;
  readonly heading: string;
  readonly body: string;
  readonly show: string;
  readonly cancel: string;
  readonly qrLabel: string;
  /** A visible tag on the code's frame, so a device-link code never looks like an invite code. */
  readonly qrBadge: string;
  /** Said under the code: this adds a device to your account, it is not an invitation, and it is for your own device only. */
  readonly qrNotInvite: string;
  readonly linkLabel: string;
  readonly expires: (epochMs: number) => string;
  readonly nextStep: string;
  readonly offerState: Readonly<Record<LinkOfferState, string>>;
  readonly tryAgain: string;
  readonly reviewHeading: string;
  readonly who: (deviceLabel: string) => string;
  readonly codeLabel: string;
  readonly codeHint: string;
  /** Always shown before approval: only link a device you hold; every other device is told (R-166). */
  readonly warning: string;
  readonly announce: string;
  readonly approve: string;
  readonly reject: string;
  readonly requestState: Readonly<Record<LinkRequestState, string>>;
  readonly linked: string;
  readonly rejectedDone: string;
  readonly done: string;
}

/** New-device side (D24): type or scan the link from the other device, then show the same check code and wait for its decision. */
export interface JoinCopy {
  readonly heading: string;
  readonly body: string;
  readonly scan: string;
  readonly scanNothing: string;
  readonly pasteLabel: string;
  readonly pasteHint: string;
  readonly open: string;
  readonly parseError: Readonly<Record<LinkParseError, string>>;
  readonly cancel: string;
  readonly codeLabel: string;
  readonly codeHint: string;
  /** Per state of the request: this device never approves itself, it waits for the other device. */
  readonly state: Readonly<Record<LinkRequestState, string>>;
  readonly joined: string;
  readonly tryAgain: string;
  readonly outcome: OutcomeCopy;
}

export interface DeviceActionCopy {
  readonly action: string;
  readonly heading: (deviceLabel: string) => string;
  /** Explicit consequences, shown before confirming. */
  readonly consequences: readonly string[];
  readonly confirm: string;
  readonly cancel: string;
  readonly done: string;
}

export interface DevicesCopy {
  readonly heading: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly controlState: Readonly<Record<FrozenControlState, string>>;
  readonly deviceLinkedNotice: (deviceLabel: string, epochMs: number) => string;
  /** Accessible name of the strip that carries the enrollment notices. */
  readonly noticesLabel: string;
  readonly linkRequestsHeading: string;
  readonly linkRequestRow: (deviceLabel: string) => string;
  readonly currentHeading: string;
  readonly otherHeading: string;
  readonly noOtherDevices: string;
  /** Shown in place of the add button while Core cannot link a device yet (`linking="later"`). */
  readonly linkLater: string;
  readonly removedHeading: string;
  readonly role: Readonly<Record<DeviceRole, string>>;
  readonly state: Readonly<Record<DeviceStateCode, string>>;
  readonly lastSeen: Readonly<Record<LastSeenClass, string>>;
  readonly cautions: Readonly<Record<DeviceCautionCode, string>>;
  readonly enrolled: (epochMs: number) => string;
  readonly thisDevice: string;
  readonly back: string;
  readonly detailsHeading: (deviceLabel: string) => string;
  readonly noActions: string;
  readonly frozenNoActions: string;
  readonly suspend: DeviceActionCopy;
  readonly revoke: DeviceActionCopy;
  readonly pendingHeading: string;
  readonly pendingKind: Readonly<Record<DevicePendingKind, string>>;
  readonly link: LinkCopy;
  readonly outcome: OutcomeCopy;
}
