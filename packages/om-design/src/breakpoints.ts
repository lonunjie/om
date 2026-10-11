// Single source of truth for layout breakpoints (px). shell.css repeats these literals (CSS media queries cannot read
// variables); breakpoints.test.ts asserts every min-width in the CSS matches this module.
export const BREAKPOINTS = { split: 600, expanded: 900 } as const;

export type LayoutMode = "compact" | "split" | "expanded";

export const minWidthQuery = (px: number): string => `(min-width: ${String(px)}px)`;

export const layoutModeForWidth = (width: number): LayoutMode =>
  width >= BREAKPOINTS.expanded ? "expanded" : width >= BREAKPOINTS.split ? "split" : "compact";
