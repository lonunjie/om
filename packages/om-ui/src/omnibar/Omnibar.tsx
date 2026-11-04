import { useEffect, useId, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { Icon } from "../shell/primitives/icons";
import { IconButton } from "../shell/primitives/IconButton";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { OmnibarCopy } from "./copy";
import { rankEntries, type OmniEntry } from "./match";
import "./omnibar.css";

export interface OmnibarTriggerProps {
  readonly copy: OmnibarCopy;
  readonly onOpen: () => void;
  /** `field` looks like the find-or-do field (list and top bar); `icon` is the small control beside Send in a composer. */
  readonly variant?: "field" | "icon";
  /** A keyboard shortcut to show on a keyboard device, for example "Ctrl K". */
  readonly shortcut?: string;
}

/** The closed Omnibar. It is always a real button with the same name, so a keyboard, a switch or a voice command can open it. */
export function OmnibarTrigger({ copy, onOpen, variant = "field", shortcut }: OmnibarTriggerProps) {
  if (variant === "icon") {
    return <IconButton className="om-omni-trigger om-omni-trigger--icon" label={copy.openAction} icon={<Icon name="omni" />} aria-haspopup="dialog" onClick={onOpen} />;
  }
  return (
    <button type="button" className="om-omni-trigger" aria-label={`${copy.name}: ${copy.triggerText}`} aria-haspopup="dialog" onClick={onOpen}>
      <Icon name="omni" size={20} />
      <span className="om-omni-trigger__text">{copy.triggerText}</span>
      {shortcut ? <kbd className="om-omni-trigger__key" aria-hidden="true">{shortcut}</kbd> : null}
    </button>
  );
}

export interface OmnibarProps {
  readonly entries: readonly OmniEntry[];
  /** Results for what was typed that are not in the fixed list, for example a pasted invitation link. Computed on the device; they come first because they are explicit intent. */
  readonly dynamic?: (query: string) => readonly OmniEntry[];
  readonly copy: OmnibarCopy;
  /** Called when the Omnibar should go away. The host stops rendering it. */
  readonly onClose: () => void;
}

/**
 * The universal find-or-do surface (NAVIGATION_AND_OMNIBAR.md): a conventional combobox over a listbox, not a focus trap. It is
 * local and deterministic: nothing typed here leaves the device, results are known before the first key, and the armed result never
 * moves under the person. On a compact screen it takes the lower zone from the composer; wide, it opens under the top bar.
 */
export function Omnibar({ entries, dynamic, copy, onClose }: OmnibarProps) {
  const base = useId();
  const [query, setQuery] = useState("");
  const [armedId, setArmedId] = useState<string | null>(null);
  const input = useRef<HTMLInputElement>(null);
  const opener = useRef<Element | null>(typeof document === "undefined" ? null : document.activeElement);
  const restore = useRef(false);

  const results = useMemo(() => [...(dynamic?.(query) ?? []), ...rankEntries(entries, query)], [entries, dynamic, query]);
  const armed = results.find((r) => r.id === armedId) ?? results[0];
  const optionId = (id: string): string => `${base}-${id}`;

  useEffect(() => {
    input.current?.focus();
    const from = opener.current;
    return () => { if (restore.current && from instanceof HTMLElement && from.isConnected) from.focus(); };
  }, []);

  const dismiss = (): void => { restore.current = true; onClose(); };
  const run = (entry: OmniEntry): void => { onClose(); entry.run(); };

  const move = (step: 1 | -1 | "first" | "last"): void => {
    if (results.length === 0) return;
    const i = armed === undefined ? 0 : results.indexOf(armed);
    const next = step === "first" ? 0 : step === "last" ? results.length - 1 : Math.max(0, Math.min(results.length - 1, i + step));
    setArmedId(results[next]?.id ?? null);
  };
  const onKeyDown = (e: KeyboardEvent<HTMLDivElement>): void => {
    if (e.nativeEvent.isComposing) return;
    switch (e.key) {
      case "Escape": e.preventDefault(); dismiss(); break;
      case "ArrowDown": e.preventDefault(); move(1); break;
      case "ArrowUp": e.preventDefault(); move(-1); break;
      case "Home": if (e.target === input.current && query === "") { e.preventDefault(); move("first"); } break;
      case "End": if (e.target === input.current && query === "") { e.preventDefault(); move("last"); } break;
      case "Enter": if (e.target === input.current && armed !== undefined) { e.preventDefault(); run(armed); } break;
      default: break;
    }
  };

  const listId = `${base}-list`;
  return (
    <div className="om-omni" role="dialog" aria-label={copy.name} onKeyDown={onKeyDown}>
      <div className="om-omni__scrim" aria-hidden="true" onPointerDown={dismiss} />
      <div className="om-omni__panel">
        <div className="om-omni__results">
          {query.trim() === "" && results.length > 0 ? <p className="om-omni__heading">{copy.suggested}</p> : null}
          {results.length === 0 ? <p className="om-omni__none">{copy.none(query.trim())}</p> : (
            <ul id={listId} role="listbox" aria-label={copy.results} className="om-omni__list">
              {results.map((r) => (
                <li
                  key={r.id} id={optionId(r.id)} role="option" aria-selected={r === armed} data-armed={r === armed ? "true" : undefined}
                  className="om-omni__option" onPointerDown={(e) => { e.preventDefault(); }} onClick={() => { run(r); }}
                >
                  <span className="om-omni__label">{r.label}</span>
                  {r.detail ? <span className="om-omni__detail">{r.detail}</span> : null}
                  <span className="om-omni__kind">{copy.kinds[r.kind]}</span>
                </li>
              ))}
            </ul>
          )}
          <VisuallyHidden><span role="status">{copy.count(results.length)}</span></VisuallyHidden>
        </div>
        <div className="om-omni__bar">
          <IconButton className="om-omni__close" label={copy.close} icon={<Icon name="close" />} onClick={dismiss} />
          <input
            ref={input} type="text" role="combobox" className="om-omni__input" aria-label={copy.name} placeholder={copy.placeholder}
            aria-expanded={results.length > 0} aria-controls={results.length > 0 ? listId : undefined} aria-autocomplete="list"
            aria-activedescendant={armed === undefined ? undefined : optionId(armed.id)} enterKeyHint="go"
            autoComplete="off" autoCapitalize="off" autoCorrect="off" spellCheck={false}
            value={query} onChange={(e) => { setQuery(e.target.value); setArmedId(null); }}
          />
        </div>
      </div>
    </div>
  );
}
