/// <reference types="node" />
import { readFileSync, writeFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { BREAKPOINTS, layoutModeForWidth } from "./breakpoints";
import { contrastRatio } from "./contrast";
import { CSS_FILES } from "./css";
import { validateTheme } from "./themeValidator";
import {
  BUILT_IN_THEMES, OM_CALM, RESERVED_COLOR_ROLES, RESERVED_CONTRAST_PAIRS, RESERVED_DARK, RESERVED_LIGHT,
  THEME_COLOR_ROLES, THEME_CONTRAST_PAIRS,
} from "./tokens";

const cssUrl = (name: string): URL => new URL(`./css/${name}`, import.meta.url);

describe("OM Calm contrast (WCAG AA)", () => {
  for (const variant of ["light", "dark"] as const) {
    it(`${variant}: every text/background and non-text pair meets its minimum`, () => {
      const p = OM_CALM[variant];
      const failures = THEME_CONTRAST_PAIRS.filter(([fg, bg, min]) => contrastRatio(p[fg], p[bg]) < min)
        .map(([fg, bg, min]) => `${fg}/${bg} ${contrastRatio(p[fg], p[bg]).toFixed(2)} < ${String(min)}`);
      expect(failures).toEqual([]);
    });
  }
  it("reserved security palettes meet AA in both variants", () => {
    for (const p of [RESERVED_LIGHT, RESERVED_DARK]) {
      const failures = RESERVED_CONTRAST_PAIRS.filter(([fg, bg, min]) => contrastRatio(p[fg], p[bg]) < min);
      expect(failures).toEqual([]);
      expect(contrastRatio(p["focus-ring"], p["focus-halo"])).toBeGreaterThanOrEqual(3);
    }
  });
  it("focus ring has 3:1 against every themable background in both variants", () => {
    for (const [theme, reserved] of [[OM_CALM.light, RESERVED_LIGHT], [OM_CALM.dark, RESERVED_DARK]] as const) {
      for (const bg of ["bg-canvas", "bg-surface", "bg-raised", "bg-sunken"] as const) {
        expect(contrastRatio(reserved["focus-ring"], theme[bg])).toBeGreaterThanOrEqual(3);
      }
    }
  });
});

describe("theme validator", () => {
  it("accepts built-in themes", () => {
    for (const t of BUILT_IN_THEMES) expect(validateTheme(t).ok).toBe(true);
  });
  it("rejects a theme that redefines a reserved token", () => {
    for (const role of RESERVED_COLOR_ROLES) {
      const r = validateTheme({ ...OM_CALM, light: { ...OM_CALM.light, [role]: "#000000" } });
      expect(r.ok).toBe(false);
      if (!r.ok) expect(r.errors.join("\n")).toContain(`reserved token "${role}"`);
    }
  });
  it("rejects unknown tokens, missing variants, bad colors and low contrast", () => {
    expect(validateTheme({ ...OM_CALM, dark: undefined }).ok).toBe(false);
    expect(validateTheme({ ...OM_CALM, light: { ...OM_CALM.light, bogus: "#000000" } }).ok).toBe(false);
    expect(validateTheme({ ...OM_CALM, light: { ...OM_CALM.light, accent: "url(x)" } }).ok).toBe(false);
    expect(validateTheme({ ...OM_CALM, light: { ...OM_CALM.light, "text-primary": "#F7F8FA" } }).ok).toBe(false);
    expect(validateTheme(null).ok).toBe(false);
  });
});

describe("generated CSS", () => {
  for (const [name, gen] of Object.entries(CSS_FILES)) {
    it(`${name} matches token data`, () => {
      if (process.env["OM_WRITE_CSS"]) writeFileSync(cssUrl(name), gen());
      expect(readFileSync(cssUrl(name), "utf8")).toBe(gen());
    });
  }
  it("defines every themable and reserved role", () => {
    const theme = readFileSync(cssUrl("theme-om-calm.css"), "utf8");
    const reserved = readFileSync(cssUrl("reserved.css"), "utf8");
    for (const r of THEME_COLOR_ROLES) expect(theme).toContain(`--om-color-${r}:`);
    for (const r of RESERVED_COLOR_ROLES) {
      expect(reserved).toContain(`--om-color-${r}:`);
      expect(theme).not.toContain(`--om-color-${r}:`);
    }
  });
  it("reduced motion keeps instant and zeroes the rest", () => {
    const css = readFileSync(cssUrl("tokens.css"), "utf8");
    expect(css).toContain("prefers-reduced-motion: reduce");
    expect(css).toMatch(/--om-motion-standard: 0\.01ms/);
    expect(css).not.toMatch(/--om-motion-instant: 0\.01ms/);
  });
});

describe("breakpoints", () => {
  it("maps widths to layout modes at 600 and 900", () => {
    expect([599, 600, 899, 900].map(layoutModeForWidth)).toEqual(["compact", "split", "split", "expanded"]);
    expect(BREAKPOINTS).toEqual({ split: 600, expanded: 900 });
  });
});
