import { useId, type ReactNode } from "react";
import type { CallNotice, CallReason, CallState, CallView as CoreCallView } from "@om/platform-api";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import "./calls.css";

export interface CallCopy {
  readonly audio: string;
  readonly video: string;
  readonly incoming: (name: string, media: string) => string;
  readonly withContact: (name: string) => string;
  readonly identity: (name: string) => string;
  readonly state: Readonly<Record<CallState, string>>;
  readonly reason: Readonly<Record<CallReason, string>>;
  readonly answerAudio: string;
  readonly answerVideo: string;
  readonly decline: string;
  readonly hangUp: string;
  readonly mute: string;
  readonly unmute: string;
  readonly cameraOn: string;
  readonly cameraOff: string;
  readonly dismiss: string;
  readonly cancel: string;
  readonly preparing: string;
  readonly noDirect: string;
  readonly missed: string;
  readonly missedSeveral: string;
  readonly time: (ms: number) => string;
}

interface CallViewProps {
  readonly copy: CallCopy;
  readonly call: CoreCallView | null;
  readonly name: string;
  readonly identity?: string | undefined;
  readonly pending: boolean;
  readonly privacyAllowsDirect: boolean;
  readonly error: string | null;
  readonly media?: ReactNode;
  readonly cameraAvailable: boolean;
  readonly onAnswer: (kind: "audio" | "video") => void;
  readonly onEnd: (decline?: boolean) => void;
  readonly onToggle: (kind: "audio" | "video") => void;
  readonly onDismiss: () => void;
}

export function CallView({ copy, call, name, identity, pending, privacyAllowsDirect, error, media, cameraAvailable, onAnswer, onEnd, onToggle, onDismiss }: CallViewProps) {
  const titleId = useId();
  if (call === null && !pending && error === null) return null;
  const incoming = call?.state === "incoming";
  const ended = call?.state === "ended" || call?.state === "failed";
  const controls = call?.state === "active" || call?.state === "reconnecting";
  const muted = call !== null && (call.localMediaFlags & 1) === 0;
  const camera = call !== null && (call.localMediaFlags & 2) !== 0;
  const title = incoming
    ? copy.incoming(name, call.media === "video" ? copy.video : copy.audio)
    : call ? copy.withContact(name) : copy.state.starting;
  return (
    <section className="om-call" aria-labelledby={titleId}>
      <div role="status" aria-live="polite" aria-atomic="true">
        <Text as="h2" id={titleId} weight="bold">{title}</Text>
        {identity === undefined ? null : <Text as="p" size="sm">{copy.identity(identity)}</Text>}
        {incoming || call === null ? null : <Text as="p">{ended && call.reason ? copy.reason[call.reason] : copy.state[call.state]}</Text>}
        {pending ? <Text as="p">{copy.preparing}</Text> : null}
        {incoming && !privacyAllowsDirect && error === null ? <Text as="p">{copy.noDirect}</Text> : null}
      </div>
      {error === null ? null : <p role="alert">{error}</p>}
      {media}
      <div className="om-call__controls">
        {incoming ? <>
          {privacyAllowsDirect ? <Button disabled={pending} onClick={() => { onAnswer("audio"); }}>{copy.answerAudio}</Button> : null}
          {privacyAllowsDirect && call.media === "video" ? <Button disabled={pending} onClick={() => { onAnswer("video"); }}>{copy.answerVideo}</Button> : null}
          <Button variant="quiet" onClick={() => { onEnd(true); }}>{copy.decline}</Button>
        </> : null}
        {controls && call ? <>
          <Button variant="quiet" onClick={() => { onToggle("audio"); }}>{muted ? copy.unmute : copy.mute}</Button>
          {call.media === "video" && cameraAvailable ? <Button variant="quiet" onClick={() => { onToggle("video"); }}>{camera ? copy.cameraOff : copy.cameraOn}</Button> : null}
        </> : null}
        {call !== null && !incoming && !ended ? <Button variant="quiet" onClick={() => { onEnd(); }}>{copy.hangUp}</Button> : null}
        {call === null && pending ? <Button variant="quiet" onClick={() => { onEnd(); }}>{copy.cancel}</Button> : null}
        {ended || (call === null && !pending) ? <Button variant="quiet" onClick={onDismiss}>{copy.dismiss}</Button> : null}
      </div>
    </section>
  );
}

export function CallTimelineNotice({ notice, copy }: { readonly notice: CallNotice; readonly copy: CallCopy }) {
  return (
    <li className="om-call__notice">
      <span>{notice.folded ? copy.missedSeveral : copy.missed}</span>
      <time dateTime={new Date(notice.atMs).toISOString()}>{copy.time(notice.atMs)}</time>
    </li>
  );
}
