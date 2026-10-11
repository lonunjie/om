import { contrastRatio, isHexColor } from "./contrast";
import {
  RESERVED_COLOR_ROLES, THEME_COLOR_ROLES, THEME_CONTRAST_PAIRS, type Theme, type ThemeColorRole,
} from "./tokens";

export type ThemeValidation = { readonly ok: true; readonly theme: Theme } | { readonly ok: false; readonly errors: readonly string[] };

const isRecord = (v: unknown): v is Record<string, unknown> => typeof v === "object" && v !== null && !Array.isArray(v);
const ROLES = new Set<string>(THEME_COLOR_ROLES);
const RESERVED = new Set<string>(RESERVED_COLOR_ROLES);

const checkPalette = (variant: string, raw: unknown, errors: string[]): Record<ThemeColorRole, string> | undefined => {
  if (!isRecord(raw)) { errors.push(`${variant}: palette missing`); return undefined; }
  const start = errors.length;
  for (const key of Object.keys(raw)) {
    if (RESERVED.has(key)) errors.push(`${variant}: reserved token "${key}" cannot be set by a theme`);
    else if (!ROLES.has(key)) errors.push(`${variant}: unknown token "${key}"`);
  }
  for (const role of THEME_COLOR_ROLES) {
    const v = raw[role];
    if (typeof v !== "string" || !isHexColor(v)) errors.push(`${variant}: "${role}" must be a #rrggbb color`);
  }
  if (errors.length > start) return undefined;
  const palette = raw as Record<ThemeColorRole, string>;
  for (const [fg, bg, min] of THEME_CONTRAST_PAIRS) {
    if (contrastRatio(palette[fg], palette[bg]) < min) errors.push(`${variant}: ${fg} on ${bg} is below ${String(min)}:1`);
  }
  return palette;
};

// Fails closed: any problem rejects the whole theme. Both light and dark variants are required.
export const validateTheme = (raw: unknown): ThemeValidation => {
  const errors: string[] = [];
  if (!isRecord(raw)) return { ok: false, errors: ["theme must be an object"] };
  const { id, name } = raw;
  if (typeof id !== "string" || !/^[a-z][a-z0-9-]{1,63}$/.test(id)) errors.push("id must be a lowercase slug");
  if (typeof name !== "string" || name.length === 0 || name.length > 80) errors.push("name must be 1-80 characters");
  const light = checkPalette("light", raw["light"], errors);
  const dark = checkPalette("dark", raw["dark"], errors);
  if (errors.length > 0 || !light || !dark) return { ok: false, errors };
  return { ok: true, theme: { id: id as string, name: name as string, light, dark } };
};
