import { createContext, useContext, useEffect, useMemo, type ReactNode } from "react";
import { BUILT_IN_THEMES, OM_CALM, THEME_COLOR_ROLES, validateTheme, type Theme } from "@om/design";
import "@om/design/css/tokens.css";
import "@om/design/css/theme-om-calm.css";
import "@om/design/css/reserved.css";
import "@om/design/css/base.css";
import { useMediaQuery } from "./useMediaQuery";

export type Appearance = "auto" | "light" | "dark";
export type MotionPreference = "system" | "reduced" | "full";

interface ThemeState {
  readonly theme: Theme;
  readonly appearance: Appearance;
  readonly resolvedAppearance: "light" | "dark";
  readonly motion: MotionPreference;
}

const DEFAULT_STATE: ThemeState = { theme: OM_CALM, appearance: "auto", resolvedAppearance: "light", motion: "system" };
export const ThemeContext = createContext<ThemeState>(DEFAULT_STATE);
export const useTheme = (): ThemeState => useContext(ThemeContext);

export interface ThemeProviderProps {
  readonly theme?: Theme;
  readonly appearance?: Appearance;
  readonly motion?: MotionPreference;
  readonly children: ReactNode;
}

const isBuiltIn = (t: Theme): boolean => BUILT_IN_THEMES.some((b) => b === t);

// Applies a theme by toggling root attributes (built-in themes ship static CSS) and, for validated custom themes,
// CSSOM custom properties on the root. Never injects style elements. Reserved security tokens are never written here.
export const ThemeProvider = ({ theme = OM_CALM, appearance = "auto", motion = "system", children }: ThemeProviderProps) => {
  const systemDark = useMediaQuery("(prefers-color-scheme: dark)");
  const resolvedAppearance = appearance === "auto" ? (systemDark ? "dark" : "light") : appearance;
  const active = useMemo<Theme>(() => {
    if (isBuiltIn(theme)) return theme;
    const v = validateTheme(theme);
    return v.ok ? v.theme : OM_CALM; // fail closed to the built-in theme
  }, [theme]);

  useEffect(() => {
    const root = document.documentElement;
    root.setAttribute("data-theme", active.id);
    root.setAttribute("data-appearance", appearance);
    if (motion === "system") root.removeAttribute("data-motion");
    else root.setAttribute("data-motion", motion);
    const custom = !isBuiltIn(active);
    if (custom) {
      const palette = active[resolvedAppearance];
      for (const role of THEME_COLOR_ROLES) root.style.setProperty(`--om-color-${role}`, palette[role]);
    }
    return () => {
      if (custom) for (const role of THEME_COLOR_ROLES) root.style.removeProperty(`--om-color-${role}`);
    };
  }, [active, appearance, resolvedAppearance, motion]);

  const value = useMemo<ThemeState>(
    () => ({ theme: active, appearance, resolvedAppearance, motion }),
    [active, appearance, resolvedAppearance, motion],
  );
  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
};
