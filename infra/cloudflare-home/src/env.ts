// SPDX-License-Identifier: MPL-2.0
import type { DescriptorStore } from "./descriptors";
import type { IntroGate } from "./intro";
import type { KeyPool } from "./keys";
import type { MailboxQueue } from "./queue";

/** Bindings. No secrets: a Home holds only hashes of the capabilities it was handed. */
export interface Env {
  readonly QUEUES: DurableObjectNamespace<MailboxQueue>;
  readonly KEYS: DurableObjectNamespace<KeyPool>;
  readonly DESCRIPTORS: DurableObjectNamespace<DescriptorStore>;
  readonly INTRO: DurableObjectNamespace<IntroGate>;
  /** Public name in ProviderCapabilities.provider_id (default "om-reference-home"). */
  readonly OM_PROVIDER_ID?: string;
  /** Free-plan guard: `1` declares that exhausting a limit is a hard stop (reported in quota details). */
  readonly OM_HARD_FREE_TIER?: string;
}
