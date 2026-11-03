// SPDX-License-Identifier: AGPL-3.0-or-later
import type { PqRootPin, RootKeyDescriptor } from "../../../apps/web/build/verify-release.mjs";
export declare const MANIFEST_FORMAT: string;
export declare const MANIFEST_FILE: string;
export declare const LOCK_FILE: string;
export declare const SUMS_FILE: string;
export declare const WORKER_FILE: string;
export declare function compareVersions(a: string, b: string): number | null;
export declare function verifyRelay(
  dir: string,
  opts?: { roots?: Record<string, RootKeyDescriptor>; threshold?: number; pq?: PqRootPin | null; now?: number; minVersion?: string | null; env?: Record<string, string | undefined> },
): { ok: boolean; errors: string[]; info: { version?: string; commit?: string; keyId?: string; delegationVersion?: number; signatureValid?: boolean; provenance?: boolean; workerSha256?: string; admissionSeen?: boolean } };
