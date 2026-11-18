import { useCallback, useEffect, useId, useMemo, useRef, useState } from "react";
import type { ChangeEvent, KeyboardEvent, ReactNode, RefObject } from "react";
import { findSameNames } from "../groups/sameName";
import { MAX_SUGGESTIONS, activeMentionQuery, insertMention, matchCandidates, mentionRanges } from "./mentions";
import type { MentionCandidate, MentionPick, MentionRange } from "./mentions";

export interface MentionsCopy {
  /** Names the list of people for a screen reader. */
  readonly listLabel: string;
  /** Said (politely, once the list changes) how many people match. */
  readonly matches: (count: number) => string;
  readonly none: string;
  /** Two people with the same or lookalike names: the cue that does not depend on the name, as the people lists use. */
  readonly sameName: (name: string, index: number, of: number) => string;
  /** Read after a mention that names the reader. */
  readonly you: string;
}

export interface UseMentions {
  readonly onChange: (e: ChangeEvent<HTMLTextAreaElement>) => void;
  /** True when the key was used by the list (the caller then does nothing else with it). */
  readonly onKeyDown: (e: KeyboardEvent<HTMLTextAreaElement>) => boolean;
  readonly onSelect: () => void;
  readonly ranges: (text: string) => readonly MentionRange[];
  readonly reset: () => void;
  readonly picker: ReactNode;
  /** ARIA for the textarea. */
  readonly inputProps: Readonly<Record<string, string | boolean | undefined>>;
}

/** The mention list under the composer: `@` opens it, arrows move, Enter or Tab choose, Escape closes. People are never named by colour or position alone. */
export function useMentions(o: { readonly candidates: readonly MentionCandidate[]; readonly copy: MentionsCopy; readonly draft: string; readonly onDraftChange: (next: string) => void; readonly input: RefObject<HTMLTextAreaElement | null> }): UseMentions {
  const { candidates, copy, draft, onDraftChange, input } = o;
  const id = useId();
  const [caret, setCaret] = useState(0);
  const [active, setActive] = useState(0);
  const [closed, setClosed] = useState(false);
  const picks = useRef<MentionPick[]>([]);
  const pending = useRef<number | null>(null);
  const q = candidates.length === 0 ? null : activeMentionQuery(draft, Math.min(caret, draft.length));
  const shown = useMemo(() => (q === null || closed ? [] : matchCandidates(candidates, q.query)), [q, closed, candidates]);
  const same = useMemo(() => findSameNames(candidates), [candidates]);
  const open = q !== null && !closed;
  const index = Math.min(active, Math.max(0, shown.length - 1));
  useEffect(() => {
    if (pending.current !== null && input.current !== null) { input.current.setSelectionRange(pending.current, pending.current); pending.current = null; }
  }, [draft, input]);
  const choose = useCallback((c: MentionCandidate) => {
    if (q === null) return;
    const r = insertMention(draft, q.from, Math.min(caret, draft.length), c.name);
    picks.current = [...picks.current, { account: c.id, label: r.label }];
    pending.current = r.caret;
    setCaret(r.caret);
    setActive(0);
    onDraftChange(r.text);
  }, [q, draft, caret, onDraftChange]);
  const optionId = (i: number): string => `${id}-o${String(i)}`;
  const cue = (c: MentionCandidate): string => { const s = same.of(c.id); return s === null ? c.name : copy.sameName(c.name, s.index, s.of); };
  const picker = open ? (
    <div className="om-mentions">
      <ul id={`${id}-list`} className="om-mentions__list" role="listbox" aria-label={copy.listLabel}>
        {shown.map((c, i) => (
          <li key={c.id} id={optionId(i)} role="option" aria-selected={i === index} className="om-mentions__option" onMouseDown={(e) => { e.preventDefault(); choose(c); }}>
            <bdi>{cue(c)}</bdi>
          </li>
        ))}
      </ul>
      <p className="om-visually-hidden" role="status">{shown.length === 0 ? copy.none : copy.matches(Math.min(shown.length, MAX_SUGGESTIONS))}</p>
    </div>
  ) : null;
  return {
    onChange: (e) => { setCaret(e.target.selectionStart); setClosed(false); setActive(0); onDraftChange(e.target.value); },
    onKeyDown: (e) => {
      if (!open || shown.length === 0 || e.nativeEvent.isComposing) return false;
      if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); setActive((index + (e.key === "ArrowDown" ? 1 : shown.length - 1)) % shown.length); return true; }
      if (e.key === "Enter" || e.key === "Tab") { e.preventDefault(); const c = shown[index]; if (c !== undefined) choose(c); return true; }
      if (e.key === "Escape") { e.preventDefault(); setClosed(true); return true; }
      return false;
    },
    onSelect: () => { if (input.current !== null) setCaret(input.current.selectionStart); },
    ranges: (text) => mentionRanges(text, picks.current),
    reset: () => { picks.current = []; },
    picker,
    inputProps: open && shown.length > 0 ? { "aria-autocomplete": "list", "aria-controls": `${id}-list`, "aria-activedescendant": optionId(index) } : {},
  };
}
