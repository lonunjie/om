// OM design tokens as data. CSS custom properties are generated from these (css.ts) into src/css/*.css.
// Themes are declarative: they may supply only THEME_COLOR_ROLES. RESERVED_COLOR_ROLES belong to OM (UI-04).

export const THEME_COLOR_ROLES = [
  "bg-canvas", "bg-surface", "bg-raised", "bg-sunken", "bg-overlay",
  "text-primary", "text-secondary", "text-muted",
  "accent", "accent-hover", "accent-pressed", "accent-text", "accent-soft",
  "border", "border-subtle",
  "selection-bg", "selection-text",
  "success-fg", "success-bg", "warning-fg", "warning-bg", "danger-fg", "danger-bg", "info-fg", "info-bg",
  "message-incoming", "message-outgoing", "message-failed",
] as const;
export type ThemeColorRole = (typeof THEME_COLOR_ROLES)[number];

export const RESERVED_COLOR_ROLES = [
  "security-notice-bg", "security-notice-fg", "security-notice-border",
  "security-danger-bg", "security-danger-fg", "security-danger-border",
  "security-verified-bg", "security-verified-fg", "security-verified-border",
  "focus-ring", "focus-halo",
] as const;
export type ReservedColorRole = (typeof RESERVED_COLOR_ROLES)[number];

export type ThemePalette = Readonly<Record<ThemeColorRole, string>>;
export type ReservedPalette = Readonly<Record<ReservedColorRole, string>>;

export interface Theme {
  readonly id: string;
  readonly name: string;
  readonly light: ThemePalette;
  readonly dark: ThemePalette;
}

// Contrast requirements (WCAG 2.x AA): text 4.5:1, non-text 3:1. [foreground, background, minimum]
export type ContrastPair = readonly [ThemeColorRole, ThemeColorRole, number];
const TEXT_ON: readonly ThemeColorRole[] = ["bg-canvas", "bg-surface", "bg-raised", "bg-sunken"];
export const THEME_CONTRAST_PAIRS: readonly ContrastPair[] = [
  ...(["text-primary", "text-secondary", "text-muted", "accent"] as const).flatMap((fg) =>
    TEXT_ON.map((bg): ContrastPair => [fg, bg, 4.5])),
  ["accent-text", "accent", 4.5],
  ["selection-text", "selection-bg", 4.5],
  ["success-fg", "success-bg", 4.5], ["warning-fg", "warning-bg", 4.5],
  ["danger-fg", "danger-bg", 4.5], ["info-fg", "info-bg", 4.5],
  ["text-primary", "message-incoming", 4.5], ["text-primary", "message-outgoing", 4.5], ["danger-fg", "message-failed", 4.5],
  ["accent", "message-outgoing", 4.5],
  // The audited border.strong (DEFAULT_THEME_AND_COLOR_SEMANTICS.md s3) is about 2.3:1 on the canvas in light: below the 3:1 a
  // control boundary needs on its own. Every bordered control also has a text or fill cue; the shortfall is reported to the
  // operator (docs/reports/2026-11-03-ui-conformance-review.md, Applied). This floor documents today's value so it cannot fall further.
  ["border", "bg-canvas", 2.2], ["border", "bg-surface", 2.2],
];

export const RESERVED_CONTRAST_PAIRS: readonly (readonly [ReservedColorRole, ReservedColorRole, number])[] = [
  ["security-notice-fg", "security-notice-bg", 4.5], ["security-notice-border", "security-notice-bg", 3],
  ["security-danger-fg", "security-danger-bg", 4.5], ["security-danger-border", "security-danger-bg", 3],
  ["security-verified-fg", "security-verified-bg", 4.5], ["security-verified-border", "security-verified-bg", 3],
];

/**
 * A status tint: the status colour mixed into a surface. The audited palette (docs/ux-design/tokens/om-theme-clear-v1.json)
 * gives the status colours and the failed surface but no success or warning backing, so these two are derived, never typed by hand.
 */
const tint = (status: string, surface: string, weight: number): string => {
  const ch = (hex: string, i: number): number => parseInt(hex.slice(1 + 2 * i, 3 + 2 * i), 16);
  return "#" + [0, 1, 2].map((i) => Math.round(ch(status, i) * weight + ch(surface, i) * (1 - weight)).toString(16).padStart(2, "0")).join("").toUpperCase();
};

/**
 * OM Calm: the audited palette, role for role (docs/ux-design/tokens/om-theme-clear-v1.json; DEFAULT_THEME_AND_COLOR_SEMANTICS.md s3-4).
 * Mapping: surface.base is bg-canvas; surface.raised is bg-raised (and bg-surface in light); surface.subtle is bg-sunken (and bg-surface in dark);
 * text.tertiary is text-muted; border.strong is border; state.selected is selection-bg; the info tint is accent.soft.
 */
export const OM_CALM: Theme = {
  id: "om-calm",
  name: "OM Calm",
  light: {
    "bg-canvas": "#F7F9FC", "bg-surface": "#FFFFFF", "bg-raised": "#FFFFFF", "bg-sunken": "#EFF3F8", "bg-overlay": "#FFFFFF",
    "text-primary": "#172033", "text-secondary": "#566176", "text-muted": "#627085",
    accent: "#0067C5", "accent-hover": "#0057AA", "accent-pressed": "#004A90", "accent-text": "#FFFFFF", "accent-soft": "#DDEEFF",
    border: "#9BA8B9", "border-subtle": "#D7DEE8",
    "selection-bg": "#DCEBFF", "selection-text": "#172033",
    "success-fg": "#18794E", "success-bg": tint("#18794E", "#FFFFFF", 0.1), "warning-fg": "#8A5A00", "warning-bg": tint("#8A5A00", "#FFFFFF", 0.1),
    "danger-fg": "#C4333B", "danger-bg": "#FFF0F0", "info-fg": "#0067C5", "info-bg": "#DDEEFF",
    "message-incoming": "#FFFFFF", "message-outgoing": "#E7F2FF", "message-failed": "#FFF0F0",
  },
  dark: {
    "bg-canvas": "#11151C", "bg-surface": "#171D26", "bg-raised": "#1D2530", "bg-sunken": "#171D26", "bg-overlay": "#25303D",
    "text-primary": "#F2F5F8", "text-secondary": "#B9C2CE", "text-muted": "#8F9BAA",
    accent: "#66B2FF", "accent-hover": "#8AC4FF", "accent-pressed": "#4AA4FF", "accent-text": "#07182B", "accent-soft": "#183A5B",
    border: "#667485", "border-subtle": "#34404F",
    "selection-bg": "#223D5E", "selection-text": "#F2F5F8",
    "success-fg": "#5DD6A1", "success-bg": tint("#5DD6A1", "#1D2530", 0.18), "warning-fg": "#F6C65B", "warning-bg": tint("#F6C65B", "#1D2530", 0.18),
    "danger-fg": "#FF7B7B", "danger-bg": "#451D25", "info-fg": "#66B2FF", "info-bg": "#183A5B",
    "message-incoming": "#1D2530", "message-outgoing": "#183A5B", "message-failed": "#451D25",
  },
};

// Reserved security surfaces (UI-04): a status colour is the border, the matching tint is the backing, the primary text colour is the words.
// Focus is the audited violet (state.focus), separate from the action blue (DEFAULT_THEME_AND_COLOR_SEMANTICS.md s2, COMPONENT_BEHAVIOR_AND_STATES.md s7).
export const RESERVED_LIGHT: ReservedPalette = {
  "security-notice-bg": OM_CALM.light["warning-bg"], "security-notice-fg": "#172033", "security-notice-border": "#8A5A00",
  "security-danger-bg": "#FFF0F0", "security-danger-fg": "#172033", "security-danger-border": "#C4333B",
  "security-verified-bg": OM_CALM.light["success-bg"], "security-verified-fg": "#172033", "security-verified-border": "#18794E",
  "focus-ring": "#6536D9", "focus-halo": "#FFFFFF",
};
export const RESERVED_DARK: ReservedPalette = {
  "security-notice-bg": OM_CALM.dark["warning-bg"], "security-notice-fg": "#F2F5F8", "security-notice-border": "#F6C65B",
  "security-danger-bg": "#451D25", "security-danger-fg": "#F2F5F8", "security-danger-border": "#FF7B7B",
  "security-verified-bg": OM_CALM.dark["success-bg"], "security-verified-fg": "#F2F5F8", "security-verified-border": "#5DD6A1",
  "focus-ring": "#B69CFF", "focus-halo": "#11151C",
};

export const BUILT_IN_THEMES: readonly Theme[] = [OM_CALM];

export const FONT_STACK = {
  sans: 'system-ui, -apple-system, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif',
  mono: 'ui-monospace, SFMono-Regular, Menlo, Consolas, monospace',
} as const;

export const TYPE_SCALE = {
  xs: { size: "0.75rem", line: "1.0rem" }, sm: { size: "0.875rem", line: "1.25rem" },
  md: { size: "1rem", line: "1.5rem" }, lg: { size: "1.125rem", line: "1.625rem" },
  xl: { size: "1.375rem", line: "1.875rem" }, "2xl": { size: "1.75rem", line: "2.25rem" },
} as const;
export const FONT_WEIGHT = { regular: "400", medium: "500", bold: "700" } as const;

export const SPACE = { 0: "0", 1: "0.25rem", 2: "0.5rem", 3: "0.75rem", 4: "1rem", 5: "1.5rem", 6: "2rem", 7: "3rem" } as const;
export const RADIUS = { sm: "0.25rem", md: "0.5rem", lg: "1rem", pill: "999px" } as const;
export const ELEVATION = {
  0: "none",
  1: "0 1px 2px rgb(0 0 0 / 0.12)",
  2: "0 2px 8px rgb(0 0 0 / 0.16)",
  3: "0 8px 24px rgb(0 0 0 / 0.22)",
} as const;

// MOTION_AND_PERFORMANCE.md semantic tokens. Reduced motion zeroes everything except `instant` (state acknowledgement).
export const MOTION_DURATION = { instant: "80ms", quick: "150ms", standard: "220ms", slow: "350ms" } as const;
export const MOTION_EASING = {
  enter: "cubic-bezier(0, 0, 0.2, 1)", exit: "cubic-bezier(0.4, 0, 1, 1)", move: "cubic-bezier(0.4, 0, 0.2, 1)",
} as const;
export const REDUCED_MOTION_DURATION = "0.01ms";

export const Z_LAYER = { base: 0, raised: 10, nav: 20, sticky: 30, overlay: 40, modal: 50, toast: 60, security: 70 } as const;

export const MIN_TARGET_PX = 44;
