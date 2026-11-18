import { useId, useState } from "react";
import type { SyntheticEvent } from "react";
import type { GroupPollView, PollVisibility } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { VisuallyHidden } from "../shell/primitives/VisuallyHidden";
import type { GroupsCopy } from "./copy";

const VISIBILITIES: readonly PollVisibility[] = ["immediate", "after_vote", "after_close"];
export const MAX_POLL_OPTIONS = 12;

/** The form that makes a poll (ADR-0223). It says plainly that votes are not secret. */
export function PollForm({ copy, busy, onCreate, onCancel }: {
  readonly copy: GroupsCopy["poll"];
  readonly busy: boolean;
  readonly onCreate: (poll: { readonly question: string; readonly options: readonly string[]; readonly multiple: boolean; readonly visibility: PollVisibility }) => void;
  readonly onCancel: () => void;
}) {
  const [question, setQuestion] = useState("");
  const [options, setOptions] = useState<readonly string[]>(["", ""]);
  const [multiple, setMultiple] = useState(false);
  const [visibility, setVisibility] = useState<PollVisibility>("immediate");
  const filled = options.map((o) => o.trim()).filter((o) => o !== "");
  const distinct = new Set(filled).size === filled.length;
  const valid = question.trim() !== "" && filled.length >= 2 && distinct;
  const submit = (e: SyntheticEvent): void => { e.preventDefault(); if (valid && !busy) onCreate({ question: question.trim(), options: filled, multiple, visibility }); };
  return (
    <form className="om-poll-form" onSubmit={submit}>
      <label className="om-poll-form__field">{copy.question}
        <input data-sheet-initial type="text" maxLength={256} value={question} placeholder={copy.questionPlaceholder} onChange={(e) => { setQuestion(e.target.value); }} />
      </label>
      {options.map((o, i) => (
        <div key={i} className="om-poll-form__row">
          <label className="om-poll-form__field">{copy.option(i + 1)}
            <input type="text" maxLength={80} value={o} onChange={(e) => { setOptions(options.map((x, j) => (j === i ? e.target.value : x))); }} />
          </label>
          {options.length > 2 ? <Button variant="quiet" onClick={() => { setOptions(options.filter((_, j) => j !== i)); }}>{copy.removeOption(i + 1)}</Button> : null}
        </div>
      ))}
      {options.length < MAX_POLL_OPTIONS ? <Button variant="quiet" onClick={() => { setOptions([...options, ""]); }}>{copy.addOption}</Button> : null}
      {!distinct ? <p role="alert" className="om-poll-form__note">{copy.duplicate}</p> : null}
      <label className="om-poll-form__check"><input type="checkbox" checked={multiple} onChange={(e) => { setMultiple(e.target.checked); }} /> {copy.multiple}</label>
      <fieldset className="om-poll-form__group">
        <legend>{copy.visibilityLabel}</legend>
        {VISIBILITIES.map((v) => (
          <label key={v} className="om-poll-form__check"><input type="radio" name="poll-visibility" checked={visibility === v} onChange={() => { setVisibility(v); }} /> {copy.visibility[v]}</label>
        ))}
      </fieldset>
      <p className="om-poll-form__note">{copy.notSecret}</p>
      <div className="om-poll-form__actions">
        <Button variant="primary" type="submit" disabled={!valid || busy}>{copy.submit}</Button>
        <Button variant="quiet" onClick={onCancel}>{copy.cancel}</Button>
      </div>
    </form>
  );
}

/** A poll in the timeline: choose, change or take back a vote, and (for its author or a moderator) close it. Counts appear when the poll's own rule allows. */
export function PollCard({ question, poll, copy, busy, onVote, onClose }: {
  readonly question: string;
  readonly poll: GroupPollView;
  readonly copy: GroupsCopy["poll"];
  readonly busy: boolean;
  readonly onVote: (optionIds: readonly number[]) => void;
  readonly onClose: () => void;
}) {
  const id = useId();
  const [picked, setPicked] = useState<readonly number[] | null>(null);
  const [confirming, setConfirming] = useState(false);
  const choice = picked ?? poll.mine;
  const limit = poll.multiple ? (poll.maxSelections ?? poll.options.length) : 1;
  const toggle = (o: number): void => {
    if (!poll.multiple) { setPicked([o]); return; }
    setPicked(choice.includes(o) ? choice.filter((x) => x !== o) : choice.length < limit ? [...choice, o] : choice);
  };
  const changed = picked !== null && (picked.length !== poll.mine.length || picked.some((x) => !poll.mine.includes(x)));
  const counts = poll.options.some((o) => o.count !== undefined);
  const hidden = !counts ? (poll.visibility === "after_close" ? copy.hiddenUntilClose : copy.hiddenUntilVote) : null;
  return (
    <section className="om-poll" role="group" aria-labelledby={`${id}-q`} data-closed={poll.closed ? "true" : undefined}>
      <p id={`${id}-q`} className="om-poll__question">{question}</p>
      <p className="om-poll__hint">{poll.closed ? copy.closed : poll.multiple ? copy.chooseMany(limit) : copy.chooseOne}</p>
      <ul className="om-poll__options">
        {poll.options.map((o) => (
          <li key={o.id} className="om-poll__option">
            <label>
              <input type={poll.multiple ? "checkbox" : "radio"} name={id} checked={choice.includes(o.id)} disabled={poll.closed || busy} onChange={() => { toggle(o.id); }} />
              {" "}<bdi>{o.label}</bdi>
              {poll.mine.includes(o.id) ? <VisuallyHidden> {copy.yourChoice}</VisuallyHidden> : null}
            </label>
            {o.count === undefined ? null : <span className="om-poll__count">{copy.votes(o.count)}</span>}
          </li>
        ))}
      </ul>
      {hidden === null ? (poll.voters === undefined ? null : <p className="om-poll__hint" role="status">{copy.voters(poll.voters)}</p>) : <p className="om-poll__hint">{hidden}</p>}
      {poll.closed ? null : (
        <div className="om-poll__actions">
          <Button variant="primary" disabled={busy || !changed || choice.length === 0} onClick={() => { onVote(choice); setPicked(null); }}>{poll.mine.length > 0 ? copy.changeVote : copy.vote}</Button>
          {poll.mine.length > 0 ? <Button variant="quiet" disabled={busy} onClick={() => { onVote([]); setPicked(null); }}>{copy.takeBack}</Button> : null}
          {poll.canClose && !confirming ? <Button variant="quiet" disabled={busy} onClick={() => { setConfirming(true); }}>{copy.close}</Button> : null}
        </div>
      )}
      {confirming && !poll.closed ? (
        <div className="om-poll__confirm" role="group" aria-label={copy.close}>
          <p>{copy.closeConfirm}</p>
          <Button variant="primary" disabled={busy} onClick={() => { setConfirming(false); onClose(); }}>{copy.closeNow}</Button>
          <Button variant="quiet" onClick={() => { setConfirming(false); }}>{copy.keepOpen}</Button>
        </div>
      ) : null}
    </section>
  );
}
