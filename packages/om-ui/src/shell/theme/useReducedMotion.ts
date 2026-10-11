import { useMediaQuery } from "./useMediaQuery";
import { useTheme } from "./ThemeProvider";

// True when non-essential motion must be removed: explicit OM preference wins over the platform setting.
// Callers use it to skip spatial transitions only; it must never delay or gate an action.
export const useReducedMotion = (): boolean => {
  const { motion } = useTheme();
  const system = useMediaQuery("(prefers-reduced-motion: reduce)");
  return motion === "reduced" || (motion === "system" && system);
};
