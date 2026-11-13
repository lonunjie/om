// SPDX-License-Identifier: AGPL-3.0-or-later
import type { DescriptorStore } from "./descriptors";
import type { IntroGate } from "./intro";
import type { KeyPool } from "./keys";
import type { BlobSpool } from "./blobs";
import type { MailboxQueue } from "./queue";
import type { UsageMeter } from "./usage";

/** Bindings. No secrets: a Home holds only hashes of the capabilities it was handed. */
export interface Env {
  readonly QUEUES: DurableObjectNamespace<MailboxQueue>;
  readonly KEYS: DurableObjectNamespace<KeyPool>;
  readonly DESCRIPTORS: DurableObjectNamespace<DescriptorStore>;
  readonly INTRO: DurableObjectNamespace<IntroGate>;
  /** The blob spool (om.blob.spool.v1). Absent in a deployment whose configuration predates it: the feature is then neither advertised nor served. */
  readonly BLOBS?: DurableObjectNamespace<BlobSpool>;
  /** The self-kept usage counter of the personal profile (one object for the whole Home). Absent: the usage read is neither advertised nor served and nothing is shed. */
  readonly USAGE?: DurableObjectNamespace<UsageMeter>;
  /** `personal` selects the personal limits profile (smaller limits, usage counter, deposits shed at `hard`). Unset or `default`: the ordinary limits. */
  readonly OM_PROFILE?: string;
  /** Positive integers overriding the usage ceilings of USAGE_CONFIG (requests per UTC day). */
  readonly OM_USAGE_CEILING_WORKER?: string;
  readonly OM_USAGE_CEILING_DO?: string;
  /** Public name in ProviderCapabilities.provider_id (default "om-reference-home"). */
  readonly OM_PROVIDER_ID?: string;
  /** Free-plan guard: `1` declares that exhausting a limit is a hard stop (reported in quota details). */
  readonly OM_HARD_FREE_TIER?: string;
  /** `0` turns the optional wake channel (om.relay.wake.v1) off: not advertised, not served. Anything else leaves it on. */
  readonly OM_WAKE?: string;
  /** `0` turns the blob spool off even where it is bound and admission is configured. */
  readonly OM_BLOB_SPOOL?: string;
  /** Owner admission (om.admission.v1): comma-separated lowercase-hex `admissionHash` values. Public hashes, not secrets. When set, only a request carrying a matching admission secret may create resources. */
  readonly OM_ADMISSION?: string;
  /** `1` (set by the per-user template): creation stays closed while no valid admission hash is configured, even if `OM_ADMISSION` is empty or missing. */
  readonly OM_ADMISSION_REQUIRED?: string;
}
