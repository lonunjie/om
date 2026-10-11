// Generates the static CSS custom-property files from the token data. `pnpm exec vitest run packages/om-design`
// fails if src/css/*.css drift from these; regenerate with OM_WRITE_CSS=1.
import {
  ELEVATION, FONT_STACK, FONT_WEIGHT, MIN_TARGET_PX, MOTION_DURATION, MOTION_EASING, OM_CALM, RADIUS, REDUCED_MOTION_DURATION,
  RESERVED_COLOR_ROLES, RESERVED_DARK, RESERVED_LIGHT, SPACE, THEME_COLOR_ROLES, TYPE_SCALE, Z_LAYER,
  type ReservedPalette, type Theme,
} from "./tokens";

const decl = (name: string, value: string | number): string => `  --om-${name}: ${String(value)};`;
const block = (sel: string, lines: readonly string[], indent = ""): string =>
  `${indent}${sel} {\n${lines.map((l) => indent + l).join("\n")}\n${indent}}`;

export const generateTokensCss = (): string => {
  const base = [
    decl("font-sans", FONT_STACK.sans), decl("font-mono", FONT_STACK.mono),
    ...Object.entries(FONT_WEIGHT).map(([k, v]) => decl(`weight-${k}`, v)),
    ...Object.entries(TYPE_SCALE).flatMap(([k, v]) => [decl(`text-${k}-size`, v.size), decl(`text-${k}-line`, v.line)]),
    ...Object.entries(SPACE).map(([k, v]) => decl(`space-${k}`, v)),
    ...Object.entries(RADIUS).map(([k, v]) => decl(`radius-${k}`, v)),
    ...Object.entries(ELEVATION).map(([k, v]) => decl(`elevation-${k}`, v)),
    ...Object.entries(MOTION_DURATION).map(([k, v]) => decl(`motion-${k}`, v)),
    ...Object.entries(MOTION_EASING).map(([k, v]) => decl(`ease-${k}`, v)),
    ...Object.entries(Z_LAYER).map(([k, v]) => decl(`z-${k}`, v)),
    decl("target-min", `${String(MIN_TARGET_PX)}px`),
  ];
  const reduced = Object.keys(MOTION_DURATION).filter((k) => k !== "instant").map((k) => decl(`motion-${k}`, REDUCED_MOTION_DURATION));
  return [
    "/* Generated from src/tokens.ts by src/css.ts. Do not edit by hand. */",
    block(":root", base),
    "@media (prefers-reduced-motion: reduce) {\n" + block(":root:not([data-motion=\"full\"])", reduced, "  ") + "\n}",
    block(":root[data-motion=\"reduced\"]", reduced),
  ].join("\n") + "\n";
};

const appearanceCss = (light: readonly string[], dark: readonly string[]): string => [
  block(":root,\n:root[data-appearance=\"light\"]", light),
  "@media (prefers-color-scheme: dark) {\n" + block(":root:not([data-appearance=\"light\"])", dark, "  ") + "\n}",
  block(":root[data-appearance=\"dark\"]", dark),
].join("\n");

const palette = (roles: readonly string[], p: Readonly<Record<string, string>>): string[] =>
  roles.map((r) => decl(`color-${r}`, p[r] ?? ""));

export const generateThemeCss = (theme: Theme): string =>
  `/* Generated from src/tokens.ts by src/css.ts (theme: ${theme.name}). Do not edit by hand. */\n` +
  appearanceCss(palette(THEME_COLOR_ROLES, theme.light), palette(THEME_COLOR_ROLES, theme.dark)) + "\n";

// Reserved security tokens are emitted unscoped by theme: no [data-theme] rule or theme data can reach them.
export const generateReservedCss = (light: ReservedPalette = RESERVED_LIGHT, dark: ReservedPalette = RESERVED_DARK): string =>
  "/* Generated from src/tokens.ts by src/css.ts. Reserved security tokens (UI-04): themes cannot override. */\n" +
  appearanceCss(palette(RESERVED_COLOR_ROLES, light), palette(RESERVED_COLOR_ROLES, dark)) + "\n";

export const CSS_FILES: Readonly<Record<string, () => string>> = {
  "tokens.css": generateTokensCss,
  "theme-om-calm.css": () => generateThemeCss(OM_CALM),
  "reserved.css": () => generateReservedCss(),
};
