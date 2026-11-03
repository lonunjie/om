// A fixed owner admission secret for the first-run setup e2e: the page is given this as its pending secret (as if made earlier on the device),
// and the local locked Home (port 8789, serve.ts) is started holding only its hash, exactly as a person's own deployment would be.
import { createHash } from "node:crypto";

export const E2E_ADMISSION_SECRET = Buffer.alloc(32, 0x5a);
export const E2E_ADMISSION_HASH = createHash("sha256").update("OM-RELAY-ADMISSION-v1").update(E2E_ADMISSION_SECRET).digest("hex");
export const LOCKED_RELAY = "http://127.0.0.1:8789";
