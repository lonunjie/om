// SPDX-License-Identifier: MPL-2.0
import { useEffect, useId, useRef, useState } from "react";
import type { ReactNode } from "react";
import { Sheet } from "../shell/layout/Sheet";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import { PluginUiTree } from "./PluginUiTree";
import type { PluginsCopy } from "./copy";
import type {
  PluginAction, PluginActionKind, PluginCarryView, PluginIntent, PluginOutcome, PluginPermissionsView,
  PluginReleaseView, PluginSettingsView, PluginTaintView, PluginsService,
} from "./plugins";
import "./plugins.css";

export interface PluginSettingsProps {
  readonly service?: PluginsService;
  readonly copy: PluginsCopy;
}

export function PluginSettings({ service, copy }: PluginSettingsProps) {
  return service === undefined ? null : <SettingsState service={service} copy={copy} />;
}

function outcomeText(outcome: PluginOutcome, copy: PluginsCopy): string {
  switch (outcome.kind) {
    case "Rejected": return copy.errors[outcome.code];
    case "NeedsUserAction": return copy.needsAction[outcome.action];
    default: return copy.outcome[outcome.kind];
  }
}

function TaintNotice({ taint, copy }: { readonly taint: PluginTaintView; readonly copy: PluginsCopy }) {
  return <SecurityNoticeBanner reasonCode={taint.egress} label={
    <span className="om-plugin-stack">
      <strong>{copy.tainted}</strong>
      <span>{copy.egress[taint.egress]}</span>
      <span>{copy.conversationPolicy[taint.conversation]}</span>
    </span>
  } />;
}

function Permissions({ release, access, carry, copy }: {
  readonly release: PluginReleaseView;
  readonly access: PluginPermissionsView;
  readonly carry?: PluginCarryView | undefined;
  readonly copy: PluginsCopy;
}) {
  const sensitive = release.requested.some((capability) => capability.startsWith("content.") ||
    capability === "net.fetch_origin_scoped" || capability === "service.invoke");
  return <div className="om-plugin-stack om-plugin-reserved">
    <dl className="om-plugin-facts">
      <dt>{copy.version}</dt><dd><bdi>{release.label}</bdi></dd>
      <dt>{copy.requested}</dt>
      <dd>{release.requested.length === 0 ? copy.noCapabilities : <ul className="om-plugin-stack">
        {release.requested.map((capability) => {
          const permission = access.permissions[capability];
          const granted = permission?.kind === "granted";
          const code = permission?.kind === "disabled" ? permission.reason : "approval_required";
          const reason = code !== "approval_required" ? copy.errors[code] :
            carry?.disabledUntilGranted.includes(capability) ? copy.beyondCeiling : copy.approvalRequired;
          return <li key={capability}>
            {copy.capability[capability]} <strong>{granted ? copy.granted : copy.disabled}</strong>
            {granted ? null : <p>{reason}</p>}
          </li>;
        })}
      </ul>}</dd>
      {access.requestedOrigins.length === 0 ? null : <>
        <dt>{copy.requestedOrigins}</dt>
        <dd><ul>{access.requestedOrigins.map((origin) => <li key={origin}><bdi>{origin}</bdi></li>)}</ul></dd>
      </>}
    </dl>
    {sensitive ? <SecurityNoticeBanner reasonCode="plugin_read_and_egress" label={copy.readDoesNotAllowEgress} /> : null}
  </div>;
}

function actionKind<Kind extends PluginActionKind>(action: PluginAction<Kind>): Kind {
  return action.availability === "ready" ? action.intent.kind : action.kind;
}

function Action({ action, reason, copy, onAction }: {
  readonly action: PluginAction;
  readonly reason: string | null;
  readonly copy: PluginsCopy;
  readonly onAction: () => void;
}) {
  const reasonId = useId();
  const kind = actionKind(action);
  const unavailable = action.availability === "blocked" ? copy.errors[action.reason] : reason;
  const variant = kind === "remove" || kind === "revoke" ? "danger" : kind === "cancel_send" ? "secondary" : "primary";
  return <div className="om-plugin-stack">
    <div><Button variant={variant} disabled={unavailable !== null}
      aria-describedby={unavailable === null ? undefined : reasonId} onClick={onAction}>
      {copy.action[kind]}
    </Button></div>
    {unavailable === null ? null : <p id={reasonId}>{unavailable}</p>}
  </div>;
}

type Review = { readonly kind: "release" | "send" | "grant" | "revoke" | "remove"; readonly id: string };
type RequestKind = "read" | "change";

function SettingsState({ service, copy }: Required<PluginSettingsProps>) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const request = useRef(0);
  const inFlight = useRef(false);
  const focusAfterReview = useRef(false);
  const [shown, setShown] = useState<{ readonly service: PluginsService; readonly view: PluginSettingsView } | null>(null);
  const [busy, setBusy] = useState<RequestKind | null>("read");
  const [error, setError] = useState<RequestKind | null>(null);
  const [review, setReview] = useState<Review | null>(null);

  function load(kind: RequestKind, fetch: () => Promise<PluginSettingsView>) {
    const id = ++request.current;
    inFlight.current = true;
    setBusy(kind);
    setError(null);
    void Promise.resolve().then(fetch).then(
      (view) => {
        if (id !== request.current) return;
        setShown({ service, view });
        setBusy(null);
        inFlight.current = false;
      },
      () => {
        if (id !== request.current) return;
        setError(kind);
        setBusy(null);
        inFlight.current = false;
      },
    );
  }

  useEffect(() => {
    setReview(null);
    setShown(null);
    load("read", () => service.state());
    return () => { request.current += 1; };
  }, [service]);

  useEffect(() => {
    if (review === null && focusAfterReview.current) {
      focusAfterReview.current = false;
      heading.current?.focus();
    }
  }, [review]);

  const view = shown?.service === service ? shown.view : null;
  const outcome = view?.outcome ?? null;
  const pending = outcome !== null && outcome.kind !== "Committed" && outcome.kind !== "Rejected";
  const status = busy === "read" ? copy.loading : busy === "change" ? copy.changing :
    outcome === null || outcome.kind === "Rejected" ? null : outcomeText(outcome, copy);
  const errorText = error === "read" ? copy.loadFailed : error === "change" ? copy.changeUnknown : null;
  const unavailable = errorText ?? (busy !== null || pending ? status : null);
  const installation = view?.installation ?? null;
  const releaseReview = view?.releaseReview ?? null;
  const send = view?.send ?? null;
  const runtime = installation?.runtime;
  const tree = runtime?.kind === "active" ? runtime.settings : null;

  function change(intent: PluginIntent) {
    if (unavailable !== null || inFlight.current) return;
    if (review !== null) {
      focusAfterReview.current = true;
      setReview(null);
    }
    load("change", () => service.change(intent));
  }

  function confirm(action: PluginAction) {
    if (action.availability === "ready") change(action.intent);
  }

  const close = () => { setReview(null); };
  const release = review?.kind === "release" && releaseReview?.release.id === review.id ? releaseReview : null;
  const sendReview = review?.kind === "send" && send?.id === review.id ? send : null;
  const grant = review?.kind === "grant" ? installation?.grantReviews.find(({ id }) => id === review.id) : undefined;
  const reduction = review?.kind === "remove" || review?.kind === "revoke" ? installation?.actions.find((action) =>
    actionKind(action) === review.kind && action.availability === "ready" && action.intent.target === review.id) : undefined;
  const reductionKind = reduction === undefined ? null : actionKind(reduction);
  const reviewing = release !== null || sendReview !== null || grant !== undefined || reduction !== undefined;

  return <section className="om-plugin-stack" aria-labelledby={headingId}>
    <div className="om-plugin-stack" inert={reviewing}>
      <h2 id={headingId} ref={heading} tabIndex={-1}>{copy.heading}</h2>
      <Text role="status">{status}</Text>
      {errorText === null ? null : <Text role="alert">{errorText}</Text>}
      {outcome?.kind === "Rejected" ? <SecurityNoticeBanner
        reasonCode={String(outcome.code)} label={copy.errors[outcome.code]} tone="danger" /> : null}
      {errorText !== null || pending ? <div><Button disabled={busy !== null} onClick={() => {
        if (inFlight.current) return;
        heading.current?.focus();
        load("read", () => service.state());
      }}>{copy.refresh}</Button></div> : null}
      {view === null ? null : <>
        <h3 className="om-plugin-reserved"><bdi>{view.name}</bdi></h3>
        <dl className="om-plugin-facts om-plugin-reserved">
          <dt>{copy.publisher}</dt><dd><bdi>{view.publisher}</bdi></dd>
        </dl>
        {installation === null && releaseReview === null ? <Text>{copy.empty}</Text> : null}
        {runtime === undefined ? null : runtime.kind === "active" ? <Text>{copy.runtime.active}</Text> :
          <SecurityNoticeBanner reasonCode={runtime.kind} label={copy.runtime[runtime.kind]} />}
        {runtime?.kind === "active" && runtime.taint !== null ? <TaintNotice taint={runtime.taint} copy={copy} /> : null}
        {installation === null ? null : <Permissions release={installation.release} access={installation} carry={installation.carry} copy={copy} />}
        {releaseReview === null ? null : <div><Button onClick={() => {
          setReview({ kind: "release", id: releaseReview.release.id });
        }}>{copy.review[releaseReview.kind]}</Button></div>}
        {send === null ? null : <div><Button onClick={() => {
          setReview({ kind: "send", id: send.id });
        }}>{copy.review.confirm_send}</Button></div>}
        {installation?.grantReviews.map((item) => <div key={item.id}><Button onClick={() => {
          setReview({ kind: "grant", id: item.id });
        }}>{copy.reviewGrant(copy.capability[item.capability])}</Button></div>)}
        {installation?.actions.map((action) => <Action key={actionKind(action)} action={action} reason={unavailable} copy={copy} onAction={() => {
          if (action.availability === "ready") setReview({ kind: action.intent.kind, id: action.intent.target });
        }} />)}
        {tree === null ? null : <PluginUiTree key={tree.id} tree={tree} copy={copy} unavailable={unavailable} onIntent={change} />}
      </>}
    </div>
    {release === null ? null : <PluginReviewSheet title={copy.title[release.kind]} name={view?.name} copy={copy} onClose={close}>
      <dl className="om-plugin-facts"><dt>{copy.publisher}</dt><dd><bdi>{view?.publisher}</bdi></dd></dl>
      <Permissions release={release.release} access={release} carry={release.kind === "update" ? release.carry : undefined} copy={copy} />
      <p>{copy.consequence[release.kind]}</p>
      <Action action={release.action} reason={unavailable} copy={copy} onAction={() => { confirm(release.action); }} />
    </PluginReviewSheet>}
    {sendReview === null ? null : <PluginReviewSheet title={copy.title.confirm_send} name={view?.name} copy={copy} onClose={close}>
      <p>{copy.consequence.confirm_send}</p>
      <dl className="om-plugin-facts">
        <dt>{copy.destination}</dt><dd className="om-plugin-exact"><bdi>{sendReview.destination}</bdi></dd>
        <dt>{copy.message}</dt><dd className="om-plugin-exact"><bdi>{sendReview.content}</bdi></dd>
      </dl>
      <div className="om-plugin-actions">
        <Action action={sendReview.cancel} reason={unavailable} copy={copy} onAction={() => { confirm(sendReview.cancel); }} />
        <Action action={sendReview.confirm} reason={unavailable} copy={copy} onAction={() => { confirm(sendReview.confirm); }} />
      </div>
    </PluginReviewSheet>}
    {grant === undefined ? null : <PluginReviewSheet title={copy.title.approve} name={view?.name} copy={copy} onClose={close}>
      <p>{copy.capability[grant.capability]}</p>
      <dl className="om-plugin-facts"><dt>{copy.grantScope}</dt><dd><bdi>{grant.scope}</bdi></dd></dl>
      <Action action={grant.action} reason={unavailable} copy={copy} onAction={() => { confirm(grant.action); }} />
    </PluginReviewSheet>}
    {reduction === undefined || reductionKind === null ? null : <PluginReviewSheet title={copy.title[reductionKind]} name={view?.name} copy={copy} onClose={close}>
      <p>{copy.consequence[reductionKind]}</p>
      <Action action={reduction} reason={unavailable} copy={copy} onAction={() => { confirm(reduction); }} />
    </PluginReviewSheet>}
  </section>;
}

function PluginReviewSheet({ title, name, copy, onClose, children }: {
  readonly title: string;
  readonly name: string | undefined;
  readonly copy: PluginsCopy;
  readonly onClose: () => void;
  readonly children: ReactNode;
}) {
  return <div className="om-plugin-sheet om-plugin-reserved">
    <Sheet title={title} closeLabel={copy.close} onClose={onClose}>
      <div className="om-plugin-stack">
        <h3 data-sheet-initial tabIndex={-1}><bdi>{name}</bdi></h3>
        {children}
      </div>
    </Sheet>
  </div>;
}
