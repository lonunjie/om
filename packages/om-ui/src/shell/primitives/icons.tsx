// A few line icons the shell needs (stroke follows the text colour, so every theme and state works). Decorative: the control
// that holds one always carries its own accessible name.
import type { ReactNode } from "react";

export type IconName =
  | "back" | "send" | "settings" | "chevron" | "plus" | "chat" | "share" | "link" | "qr" | "check" | "search" | "lock" | "devices" | "shield" | "delivery" | "userPlus" | "close" | "scan" | "download" | "omni" | "attach" | "mic" | "pin" | "file" | "play";

const PATHS: Readonly<Record<IconName, ReactNode>> = {
  back: <path d="M19 12H5M11 5l-7 7 7 7" />,
  send: <path d="M12 19V5M5 12l7-7 7 7" />,
  chevron: <path d="M9 5l7 7-7 7" />,
  plus: <path d="M12 5v14M5 12h14" />,
  // A cog: eight teeth round a hub (not rays, which read as a sun).
  settings: <><path d="M10.17 4.62L10.36 2.14L13.64 2.14L13.83 4.62L15.92 5.49L17.81 3.86L20.14 6.19L18.51 8.08L19.38 10.17L21.86 10.36L21.86 13.64L19.38 13.83L18.51 15.92L20.14 17.81L17.81 20.14L15.92 18.51L13.83 19.38L13.64 21.86L10.36 21.86L10.17 19.38L8.08 18.51L6.19 20.14L3.86 17.81L5.49 15.92L4.62 13.83L2.14 13.64L2.14 10.36L4.62 10.17L5.49 8.08L3.86 6.19L6.19 3.86L8.08 5.49Z" /><circle cx="12" cy="12" r="3" /></>,
  chat: <path d="M5 4.5h14a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2h-7l-5 4v-4H5a2 2 0 0 1-2-2v-8a2 2 0 0 1 2-2z" />,
  share: <><circle cx="18" cy="5" r="2.8" /><circle cx="6" cy="12" r="2.8" /><circle cx="18" cy="19" r="2.8" /><path d="M8.5 10.7l7-4.2M8.5 13.3l7 4.2" /></>,
  link: <path d="M10 14a4 4 0 0 0 5.7 0l3-3a4 4 0 0 0-5.7-5.7l-1 1M14 10a4 4 0 0 0-5.7 0l-3 3a4 4 0 0 0 5.7 5.7l1-1" />,
  qr: <><rect x="3" y="3" width="7" height="7" rx="1" /><rect x="14" y="3" width="7" height="7" rx="1" /><rect x="3" y="14" width="7" height="7" rx="1" /><path d="M14 14h3v3M21 14v.01M14 21h3M21 17v4" /></>,
  scan: <path d="M4 8V6a2 2 0 0 1 2-2h2M16 4h2a2 2 0 0 1 2 2v2M20 16v2a2 2 0 0 1-2 2h-2M8 20H6a2 2 0 0 1-2-2v-2M4 12h16" />,
  check: <path d="M5 12.5l4.5 4.5L19 7.5" />,
  search: <><circle cx="11" cy="11" r="6.5" /><path d="M16 16l4.5 4.5" /></>,
  lock: <><rect x="5" y="11" width="14" height="10" rx="2" /><path d="M8 11V8a4 4 0 0 1 8 0v3" /></>,
  devices: <><rect x="3" y="5" width="13" height="10" rx="1.5" /><path d="M1.5 19h16" /><rect x="18" y="9" width="4" height="10" rx="1" /></>,
  shield: <><path d="M12 3l8 3v6c0 4.5-3.2 8-8 9-4.8-1-8-4.5-8-9V6z" /><path d="M9 12l2 2 4-4" /></>,
  delivery: <path d="M21 3L10 14M21 3l-7 18-4-7-7-4z" />,
  userPlus: <><circle cx="10" cy="8" r="4" /><path d="M3 21c0-4 3-6 7-6M18 14v6M15 17h6" /></>,
  close: <path d="M6 6l12 12M18 6L6 18" />,
  download: <path d="M12 4v11M7 11l5 5 5-5M5 20h14" />,
  attach: <path d="M20 11.5l-8 8a5 5 0 0 1-7-7l8.5-8.5a3.3 3.3 0 0 1 4.7 4.7L9.7 17.2a1.7 1.7 0 0 1-2.4-2.4L15 7" />,
  mic: <><rect x="9" y="3" width="6" height="11" rx="3" /><path d="M5 11a7 7 0 0 0 14 0M12 18v3" /></>,
  pin: <><path d="M12 21s-6.5-6-6.5-11a6.5 6.5 0 0 1 13 0c0 5-6.5 11-6.5 11z" /><circle cx="12" cy="10" r="2.4" /></>,
  file: <><path d="M6 3h8l4 4v14H6z" /><path d="M14 3v4h4" /></>,
  play: <path d="M8 5l11 7-11 7z" />,
  // The Omnibar: a ring with a dot, deliberately not a magnifier (it finds and does).
  omni: <><circle cx="12" cy="12" r="8" /><circle cx="12" cy="12" r="2.6" fill="currentColor" /></>,
};

export const Icon = ({ name, size = 22 }: { readonly name: IconName; readonly size?: number }) => (
  <svg className="om-icon" width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round" strokeLinejoin="round" aria-hidden="true" focusable="false">
    {PATHS[name]}
  </svg>
);
