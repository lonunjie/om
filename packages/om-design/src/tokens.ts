// OM design tokens as data. CSS custom properties are generated from these (css.ts) into src/css/*.css.
// Themes are declarative: they may supply only THEME_COLOR_ROLES. RESERVED_COLOR_ROLES belong to OM (UI-04).

export const THEME_COLOR_ROLES = [
  "bg-canvas", "bg-surface", "bg-raised", "bg-sunken",
  "text-primary", "text-secondary", "text-muted",
  "accent", "accent-text", "border",
  "selection-bg", "selection-text",
  "success-fg", "success-bg", "warning-fg", "warning-bg", "danger-fg", "danger-bg", "info-fg", "info-bg",
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
  ["border", "bg-canvas", 3], ["border", "bg-surface", 3],
];

export const RESERVED_CONTRAST_PAIRS: readonly (readonly [ReservedColorRole, ReservedColorRole, number])[] = [
  ["security-notice-fg", "security-notice-bg", 4.5], ["security-notice-border", "security-notice-bg", 3],
  ["security-danger-fg", "security-danger-bg", 4.5], ["security-danger-border", "security-danger-bg", 3],
  ["security-verified-fg", "security-verified-bg", 4.5], ["security-verified-border", "security-verified-bg", 3],
];

export const OM_CALM: Theme = {
  id: "om-calm",
  name: "OM Calm",
  light: {
    "bg-canvas": "#F7F8FA", "bg-surface": "#FFFFFF", "bg-raised": "#FFFFFF", "bg-sunken": "#EEF0F4",
    "text-primary": "#14181F", "text-secondary": "#4A5361", "text-muted": "#5B6472",
    accent: "#1F5FBF", "accent-text": "#FFFFFF", border: "#7C8696",
    "selection-bg": "#CFE0FB", "selection-text": "#14181F",
    "success-fg": "#0F6B3A", "success-bg": "#E6F5EC", "warning-fg": "#7A4B00", "warning-bg": "#FFF3D6",
    "danger-fg": "#A4161A", "danger-bg": "#FDECEC", "info-fg": "#0F4C8A", "info-bg": "#E7F0FB",
  },
  dark: {
    "bg-canvas": "#0F1217", "bg-surface": "#171B22", "bg-raised": "#1E232C", "bg-sunken": "#0B0D11",
    "text-primary": "#EEF1F5", "text-secondary": "#B6BECB", "text-muted": "#9AA3B2",
    accent: "#7FB0FF", "accent-text": "#0B1320", border: "#6B7587",
    "selection-bg": "#24406E", "selection-text": "#EEF1F5",
    "success-fg": "#8FDDB0", "success-bg": "#10301E", "warning-fg": "#F5CF84", "warning-bg": "#3A2A08",
    "danger-fg": "#FFB4B0", "danger-bg": "#3A1416", "info-fg": "#A9CCFF", "info-bg": "#11284A",
  },
};

export const RESERVED_LIGHT: ReservedPalette = {
  "security-notice-bg": "#FFF3D6", "security-notice-fg": "#4A2E00", "security-notice-border": "#7A4B00",
  "security-danger-bg": "#FDECEC", "security-danger-fg": "#7A0E12", "security-danger-border": "#A4161A",
  "security-verified-bg": "#E6F5EC", "security-verified-fg": "#0A4A28", "security-verified-border": "#0F6B3A",
  "focus-ring": "#0B57D0", "focus-halo": "#FFFFFF",
};
export const RESERVED_DARK: ReservedPalette = {
  "security-notice-bg": "#3A2A08", "security-notice-fg": "#FFE9B5", "security-notice-border": "#F5CF84",
  "security-danger-bg": "#3A1416", "security-danger-fg": "#FFD6D3", "security-danger-border": "#FFB4B0",
  "security-verified-bg": "#10301E", "security-verified-fg": "#C4F0D6", "security-verified-border": "#8FDDB0",
  "focus-ring": "#9CC2FF", "focus-halo": "#0F1217",
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
