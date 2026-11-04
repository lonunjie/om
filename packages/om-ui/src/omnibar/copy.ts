/** All user-visible strings of the Omnibar. Components contain no copy; the host supplies (localized) text. */
export interface OmnibarCopy {
  /** The control's accessible name never changes (OMNIBAR_BEHAVIOR / NAVIGATION_AND_OMNIBAR.md): "Omnibar". */
  readonly name: string;
  /** What the closed trigger on a list or top bar shows. */
  readonly triggerText: string;
  /** Accessible name of the compact trigger inside a conversation's composer. */
  readonly openAction: string;
  readonly placeholder: string;
  readonly close: string;
  /** The list of results, for assistive technology. */
  readonly results: string;
  /** Heading shown while nothing is typed. */
  readonly suggested: string;
  readonly kinds: {
    readonly conversation: string;
    readonly action: string;
    readonly setting: string;
  };
  readonly none: (query: string) => string;
  /** Said once when the results change, for assistive technology. */
  readonly count: (n: number) => string;
}
