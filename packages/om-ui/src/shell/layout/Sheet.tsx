import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { Icon } from "../primitives/icons";
import { IconButton } from "../primitives/IconButton";
import { Text } from "../primitives/Text";
import "./sheet.css";

export interface SheetProps {
  /** Accessible name and visible heading. */
  readonly title: string;
  readonly closeLabel: string;
  readonly onClose: () => void;
  readonly children: ReactNode;
}

const FOCUSABLE = "a[href], button:not(:disabled), input:not(:disabled), select:not(:disabled), textarea:not(:disabled), summary, [tabindex]:not([tabindex='-1'])";

/**
 * A sheet: a contextual surface for acting within the current place (COMPONENT_BEHAVIOR_AND_STATES.md s8). It keeps the place visible
 * underneath, returns focus to what opened it, closes with Escape or its close control, and holds Tab inside while it is open.
 */
export function Sheet({ title, closeLabel, onClose, children }: SheetProps) {
  const panel = useRef<HTMLDivElement>(null);
  const opener = useRef<Element | null>(typeof document === "undefined" ? null : document.activeElement);
  useEffect(() => {
    const from = opener.current;
    panel.current?.querySelector<HTMLElement>("[data-sheet-initial]")?.focus({ preventScroll: true });
    if (!panel.current?.contains(document.activeElement)) panel.current?.focus({ preventScroll: true });
    return () => { if (from instanceof HTMLElement && from.isConnected) from.focus({ preventScroll: true }); };
  }, []);
  // On the document, not the panel: a control that disables itself while it works drops focus to the page, and Escape and Tab must still be the sheet's.
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    const onKey = (e: globalThis.KeyboardEvent): void => {
      if (e.key === "Escape") { e.preventDefault(); close.current(); return; }
      if (e.key !== "Tab" || panel.current === null) return;
      const items = [...panel.current.querySelectorAll<HTMLElement>(FOCUSABLE)].filter((el) => el.offsetParent !== null);
      const first = items[0];
      const last = items[items.length - 1];
      const at = document.activeElement;
      if (first === undefined || last === undefined) { e.preventDefault(); panel.current.focus(); return; }
      const inside = at !== null && panel.current.contains(at);
      if (!inside) { e.preventDefault(); (e.shiftKey ? last : first).focus(); }
      else if (e.shiftKey && (at === first || at === panel.current)) { e.preventDefault(); last.focus(); }
      else if (!e.shiftKey && at === last) { e.preventDefault(); first.focus(); }
    };
    document.addEventListener("keydown", onKey);
    return () => { document.removeEventListener("keydown", onKey); };
  }, []);
  return (
    <div className="om-sheet">
      <div className="om-sheet__scrim" aria-hidden="true" onPointerDown={onClose} />
      <div ref={panel} className="om-sheet__panel" role="dialog" aria-modal="true" aria-label={title} tabIndex={-1}>
        <header className="om-sheet__head">
          <Text as="h2" size="lg" weight="bold">{title}</Text>
          <IconButton label={closeLabel} icon={<Icon name="close" />} onClick={onClose} />
        </header>
        <div className="om-sheet__body">{children}</div>
      </div>
    </div>
  );
}
