import { useRef, type KeyboardEvent, type ReactNode } from "react";
import { useEdgeSwipeBack } from "./useEdgeSwipeBack";
import { BREAKPOINTS, minWidthQuery, type LayoutMode } from "@om/design";
import { useMediaQuery } from "../theme/useMediaQuery";
import { SkipLink } from "../primitives/SkipLink";
import "../primitives/primitives.css";
import "./shell.css";

export interface ShellDestination {
  readonly id: string;
  readonly label: string;
  readonly icon?: ReactNode;
}

export interface ShellLabels {
  readonly navigation: string;
  readonly list: string;
  readonly detail: string;
  readonly skipToContent: string;
}

export interface AppShellProps {
  /** Exposed top-level destinations (Chats first). One destination => no tab bar on compact. */
  readonly destinations: readonly ShellDestination[];
  readonly activeDestination: string;
  readonly onDestinationChange: (id: string) => void;
  /** Compact only: true while a conversation is open (hides the tab bar, shows detail + composer). */
  readonly conversationOpen: boolean;
  readonly labels: ShellLabels;
  readonly list: ReactNode;
  readonly detail: ReactNode;
  readonly composerZone?: ReactNode;
  /** The Omnibar's entry: in the lower reach zone on a compact list, in the top bar from 600 px. In a compact conversation it belongs inside the composer (Composer's `trailing` slot), so it is not drawn twice. */
  readonly omnibarTrigger?: ReactNode;
  /** The open Omnibar (see `Omnibar`). While it is shown the composer and the lower reach zone are hidden on a compact screen: only one text entry at a time. */
  readonly omnibar?: ReactNode;
  readonly profileControl?: ReactNode;
  /** Goes back from the open pane (compact). With it, a touch drag from the leading edge follows the finger and goes back when released far enough. */
  readonly onBack?: () => void;
  /** Mirrors the trailing reach zone to the leading side (user preference; not inferred). */
  readonly mirrored?: boolean;
}

export const useLayoutMode = (): LayoutMode => {
  const expanded = useMediaQuery(minWidthQuery(BREAKPOINTS.expanded));
  const split = useMediaQuery(minWidthQuery(BREAKPOINTS.split));
  return expanded ? "expanded" : split ? "split" : "compact";
};

const TabBar = ({ destinations, active, onChange, label }: {
  destinations: readonly ShellDestination[]; active: string; onChange: (id: string) => void; label: string;
}) => {
  const refs = useRef(new Map<string, HTMLButtonElement>());
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>): void => {
    const i = destinations.findIndex((d) => d.id === active);
    const rtl = getComputedStyle(e.currentTarget).direction === "rtl";
    const step = e.key === "ArrowRight" ? (rtl ? -1 : 1) : e.key === "ArrowLeft" ? (rtl ? 1 : -1) : 0;
    const n = destinations.length;
    const next = e.key === "Home" ? 0 : e.key === "End" ? n - 1 : step === 0 ? undefined : (i + step + n) % n;
    const dest = next === undefined ? undefined : destinations[next];
    if (!dest) return;
    e.preventDefault();
    onChange(dest.id);
    refs.current.get(dest.id)?.focus();
  };
  return (
    <div className="om-shell__tabbar" role="tablist" aria-label={label} onKeyDown={onKeyDown}>
      {destinations.map((d) => (
        <button
          key={d.id}
          type="button"
          role="tab"
          aria-selected={d.id === active}
          tabIndex={d.id === active ? 0 : -1}
          className="om-shell__dest"
          ref={(el) => { if (el) refs.current.set(d.id, el); else refs.current.delete(d.id); }}
          onClick={() => { onChange(d.id); }}
        >
          {d.icon ? <span className="om-shell__dest-icon" aria-hidden="true">{d.icon}</span> : null}
          <span>{d.label}</span>
        </button>
      ))}
    </div>
  );
};

const Rail = ({ destinations, active, onChange, label, footer }: {
  destinations: readonly ShellDestination[]; active: string; onChange: (id: string) => void; label: string; footer: ReactNode;
}) => (
  <div className="om-shell__rail">
    <nav aria-label={label}>
      {destinations.map((d) => (
        <button
          key={d.id}
          type="button"
          className="om-shell__dest"
          aria-current={d.id === active ? "page" : undefined}
          onClick={() => { onChange(d.id); }}
        >
          {d.icon ? <span className="om-shell__dest-icon" aria-hidden="true">{d.icon}</span> : null}
          <span>{d.label}</span>
        </button>
      ))}
    </nav>
    {footer ? <div className="om-shell__rail-footer">{footer}</div> : null}
  </div>
);

// Adaptive shell: slots only. Compact root is Chats; wide shows list + detail panes under a top bar (Omnibar, identity). A left rail
// exists only once a second destination is exposed: a rail with one item adds chrome, not orientation (UI_UX_SYSTEM.md, Default compact shell).
// DOM order (= focus order): list -> detail -> composer.
export const AppShell = (p: AppShellProps) => {
  const mode = useLayoutMode();
  const compact = mode === "compact";
  const detailPane = useRef<HTMLElement>(null);
  const rail = mode === "expanded" && p.destinations.length >= 2;
  const showTabBar = p.destinations.length >= 2 && (mode === "split" || (compact && !p.conversationOpen));
  const showList = !compact || !p.conversationOpen;
  const showDetail = !compact || p.conversationOpen;
  useEdgeSwipeBack(detailPane, compact && p.conversationOpen, p.onBack);
  // A compact page with no composer (a Settings page) still needs a way to reach the Omnibar.
  const triggerInComposer = compact && p.conversationOpen && !p.composerZone;
  const tabs = { destinations: p.destinations, active: p.activeDestination, onChange: p.onDestinationChange, label: p.labels.navigation };
  return (
    <div className="om-shell" data-layout={mode} data-conversation-open={p.conversationOpen} data-mirrored={p.mirrored === true} data-omnibar={p.omnibar ? "open" : "closed"}>
      <SkipLink targetId="om-main">{p.labels.skipToContent}</SkipLink>
      {rail ? <Rail {...tabs} footer={p.profileControl} /> : null}
      {!compact && (p.omnibarTrigger || (!rail && p.profileControl)) ? (
        <div className="om-shell__topbar">
          {p.omnibarTrigger}
          {!rail ? p.profileControl : null}
        </div>
      ) : null}
      <main id="om-main" className="om-shell__panes" tabIndex={-1}>
        {showList ? <section className="om-shell__list" aria-label={p.labels.list}>{p.list}</section> : null}
        {showDetail ? <section ref={detailPane} className="om-shell__detail" aria-label={p.labels.detail}>{p.detail}</section> : null}
        {p.composerZone || triggerInComposer ? (
          <div className="om-shell__composer">
            {p.composerZone}
            {triggerInComposer ? p.omnibarTrigger : null}
          </div>
        ) : null}
      </main>
      {compact && !p.conversationOpen && (p.omnibarTrigger || p.profileControl) ? (
        <div className="om-shell__reach">
          {p.omnibarTrigger}
          {p.profileControl}
        </div>
      ) : null}
      {showTabBar ? <TabBar {...tabs} /> : null}
      {p.omnibar}
    </div>
  );
};
