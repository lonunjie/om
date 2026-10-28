import type { PqRootPin } from "./pq";

// ============================================================================================================
// PQ ROOT PIN (ADR-0182, deploy/KEY-CEREMONY-PQ.md). Kept in its own file so the Ed25519 root pin in release-key.ts and
// the PQ pin can be committed independently.
//
// PQ_ROOT is null until the operator has generated the ML-DSA-65 root key and committed its PUBLIC descriptor
// (`root-ceremony.mjs show-pq-root` prints it). While it is null no PQ signature is checked and a delegation that
// requires one (`pqRequired`) is refused (`pq_root_unset`, fail closed). Set the SAME value in build/verify-release.mjs
// (PINNED_PQ_ROOT); pq-cosign.test.ts checks both agree.
//
// PQ_REQUIRED_BY_BUILD flips to true in a later client release once the PQ root has signed the delegation chain: it makes
// this build refuse any delegation without a valid PQ signature even for a browser that has never seen a `pqRequired`
// delegation (the persisted floor, floor.ts, does that for browsers that have).
// ============================================================================================================
export const PQ_ROOT: PqRootPin | null = null;
export const PQ_REQUIRED_BY_BUILD = false;
