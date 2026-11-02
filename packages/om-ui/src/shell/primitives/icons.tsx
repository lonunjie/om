// A few line icons the shell needs (stroke follows the text colour, so every theme and state works). Decorative: the control
// that holds one always carries its own accessible name.
import type { ReactNode } from "react";

export type IconName = "back" | "send" | "settings" | "chevron" | "plus";

const PATHS: Readonly<Record<IconName, ReactNode>> = {
  back: <path d="M15 5l-7 7 7 7" />,
  send: <path d="M12 19V5M5 12l7-7 7 7" />,
  chevron: <path d="M9 5l7 7-7 7" />,
  plus: <path d="M12 5v14M5 12h14" />,
  settings: <><circle cx="12" cy="12" r="3" /><path d="M12 2.5v3M12 18.5v3M2.5 12h3M18.5 12h3M5.3 5.3l2.1 2.1M16.6 16.6l2.1 2.1M18.7 5.3l-2.1 2.1M7.4 16.6l-2.1 2.1" /></>,
};

export const Icon = ({ name, size = 22 }: { readonly name: IconName; readonly size?: number }) => (
  <svg className="om-icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {PATHS[name]}
  </svg>
);
