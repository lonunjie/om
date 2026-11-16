import type { ChosenPrivacyLevel } from "@om/platform-api";

/**
 * Settings > Protection: the person's own protection level (SECURITY_PRIVACY_PROFILES sections 3 and 4; the names are the existing ones).
 * Plain words, no protocol terms. Every level says what it changes today, and nothing it does not (the bundles are policy, never a different
 * encryption: section 1).
 */
export interface ProtectionCopy {
  readonly heading: string;
  /** What a level is, in one calm line: the same strong encryption in all three. */
  readonly intro: string;
  /** What the choice changes right now (only this; nothing is promised beyond it). */
  readonly scope: string;
  readonly loading: string;
  readonly loadFailed: string;
  readonly levelsLabel: string;
  readonly level: Readonly<Record<ChosenPrivacyLevel, { readonly name: string; readonly body: string }>>;
  /** Shown instead of the three when Core reports a level made of separate settings. */
  readonly custom: string;
  readonly inUse: string;
  /** The button on a level that is not in use: "Use Hardened". */
  readonly use: (name: string) => string;
  /** The cost of a tightening, said under its button before the tap. */
  readonly cost: Readonly<Record<"hardened" | "maximum", string>>;
  readonly loosen: {
    readonly title: (name: string) => string;
    /** What changes, in the person's terms. */
    readonly body: Readonly<Record<ChosenPrivacyLevel, string>>;
    readonly confirm: (name: string) => string;
    readonly keep: string;
  };
  readonly applied: (name: string) => string;
  /** Nothing changed. */
  readonly refused: string;
}
