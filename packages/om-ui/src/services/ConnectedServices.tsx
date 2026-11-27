// SPDX-License-Identifier: MPL-2.0
import { isTerminalOutcome } from "@om/platform-api";
import type { ConversationId, OutcomeClass } from "@om/platform-api";
import { useEffect, useId, useRef, useState } from "react";
import { Sheet } from "../shell/layout/Sheet";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { ServicesCopy } from "./copy";
import type {
  ConnectedServicesService,
  ConnectedServicesView,
  ServiceAction,
  ServiceEntryView,
  ServiceIntent,
  ServiceInvocationView,
  ServiceReadAccess,
} from "./services";
import "./services.css";

export interface ConnectedServicesProps {
  readonly service?: ConnectedServicesService;
  /** Omit for Settings. Pass the Room to show its service recipients. */
  readonly conversationId?: ConversationId;
  readonly copy: ServicesCopy;
}

export function ConnectedServices({
  service,
  conversationId,
  copy,
}: ConnectedServicesProps) {
  if (service === undefined) return null;
  return (
    <ServiceState
      key={conversationId}
      service={service}
      conversationId={conversationId}
      copy={copy}
    />
  );
}

function outcomeText(outcome: OutcomeClass, copy: ServicesCopy): string {
  switch (outcome.kind) {
    case "Rejected":
      return copy.rejected[outcome.reason];
    case "NeedsUserAction":
      return copy.needsAction[outcome.action];
    default:
      return copy.outcome[outcome.kind];
  }
}

function ReadAccess({
  access,
  copy,
}: {
  readonly access: ServiceReadAccess;
  readonly copy: ServicesCopy;
}) {
  switch (access.kind) {
    case "none":
      return copy.noRead;
    case "selected_context":
      return copy.modeConsequence.selected_context;
    case "full_room_member":
      return (
        <div className="om-service-stack">
          <SecurityNoticeBanner
            reasonCode="full_room_member"
            label={copy.modeConsequence.full_room_member}
          />
          {access.roster.filterClasses === null ? null : (
            <p>{copy.triggerFilter}</p>
          )}
        </div>
      );
    case "local_personal":
      return access.scopes.length === 0 ? (
        copy.noLocalRead
      ) : (
        <div>
          {copy.localRead}
          <ul>
            {access.scopes.map((scope, index) => (
              <li key={index}>
                <bdi>{scope}</bdi>
              </li>
            ))}
          </ul>
        </div>
      );
  }
}

function EntryFacts({
  entry,
  copy,
}: {
  readonly entry: ServiceEntryView;
  readonly copy: ServicesCopy;
}) {
  return (
    <dl className="om-service-facts">
      <dt>
        {entry.status === "offered"
          ? copy.requestedScopeLabel
          : copy.scopeLabel}
      </dt>
      <dd>
        {entry.scope.kind === "personal" ? (
          copy.scope.personal
        ) : (
          <>
            {copy.scope[entry.scope.kind]}
            {" · "}
            <bdi>{entry.scope.name}</bdi>
            {entry.scope.kind === "space" ? <p>{copy.spaceBoundary}</p> : null}
          </>
        )}
      </dd>
      <dt>{copy.modeLabel}</dt>
      <dd>
        {copy.mode[entry.mode]}
        {entry.status === "offered" ? (
          <SecurityNoticeBanner
            reasonCode={entry.mode}
            label={copy.modeConsequence[entry.mode]}
          />
        ) : null}
      </dd>
      <dt>{entry.status === "offered" ? copy.requested : copy.granted}</dt>
      <dd>
        {entry.capabilities.length === 0 ? (
          copy.noCapabilities
        ) : (
          <ul>
            {entry.capabilities.map((capability) => (
              <li key={capability.id}>
                <bdi>{capability.label}</bdi>
              </li>
            ))}
          </ul>
        )}
      </dd>
      {entry.status === "offered" && entry.triggerClasses !== undefined ? (
        <>
          <dt>{copy.triggerClasses}</dt>
          <dd>
            {entry.triggerClasses === null ? copy.noTriggerFilter : (
              <ul>
                {entry.triggerClasses.map((event) => <li key={event}>{copy.eventClass[event]}</li>)}
              </ul>
            )}
            <p>{copy.triggerFilter}</p>
          </dd>
        </>
      ) : null}
      {entry.status === "offered" ? null : (
        <>
          <dt>{copy.access}</dt>
          <dd>
            <ReadAccess access={entry.readAccess} copy={copy} />
          </dd>
        </>
      )}
      {entry.installedBy === undefined ? null : (
        <>
          <dt>{copy.addedBy}</dt>
          <dd>
            <bdi>{entry.installedBy.name}</bdi>
          </dd>
          <dt>{copy.addedFrom}</dt>
          <dd>
            <bdi>{entry.installedBy.device}</bdi>
          </dd>
          <dt>{copy.addedAt}</dt>
          <dd>{copy.formatTime(entry.installedBy.atMs)}</dd>
        </>
      )}
    </dl>
  );
}

function InvocationFacts({
  invocation,
  copy,
}: {
  readonly invocation: ServiceInvocationView;
  readonly copy: ServicesCopy;
}) {
  const { disclosure, flow } = invocation;
  return (
    <>
      <h4>{copy.disclosure}</h4>
      {disclosure.kind === "local" ? (
        <p>{copy.localDisclosure}</p>
      ) : (
        <>
          <dl className="om-service-facts">
            <dt>{copy.destination}</dt>
            <dd>
              <bdi>{disclosure.destination}</bdi>
            </dd>
            <dt>{copy.disclosureExpires}</dt>
            <dd>{copy.formatTime(disclosure.expiresAtMs)}</dd>
          </dl>
          <ol>
            {disclosure.items.map((item) => (
              <li key={item.id}>
                <strong>{copy.item[item.kind]}</strong>
                <div className="om-service-disclosure-value">
                  <bdi>{item.value}</bdi>
                </div>
              </li>
            ))}
          </ol>
          <p>{copy.discloseOnce}</p>
        </>
      )}
      <p>{flow.tainted ? copy.tainted : copy.untainted}</p>
      <SecurityNoticeBanner
        reasonCode={flow.egress}
        label={copy.egress[flow.egress]}
      />
    </>
  );
}

function actionKind(action: ServiceAction): ServiceIntent["kind"] {
  return action.availability === "ready" ? action.intent.kind : action.kind;
}

function ServiceConfirm({
  action,
  unavailable,
  label,
  copy,
  onConfirm,
}: {
  readonly action: ServiceAction;
  readonly unavailable: string | null;
  readonly label?: string | undefined;
  readonly copy: ServicesCopy;
  readonly onConfirm: (intent: ServiceIntent) => void;
}) {
  const reasonId = useId();
  const kind = actionKind(action);
  const reason =
    action.availability === "blocked"
      ? copy.rejected[action.reason]
      : unavailable;
  const reducing = kind === "remove" || kind === "revoke";
  const consequenceId = reducing ? `${reasonId}-consequence` : undefined;
  const description = [consequenceId, reason === null ? undefined : reasonId]
    .filter(Boolean)
    .join(" ");
  return (
    <div className="om-service-stack">
      {reducing ? (
        <p id={consequenceId}>
          {copy.consequence[kind]} {copy.previouslyShared}
        </p>
      ) : null}
      <div>
        <Button
          variant={reducing ? "danger" : "primary"}
          disabled={reason !== null}
          aria-describedby={description || undefined}
          onClick={() => {
            if (reason === null && action.availability === "ready")
              onConfirm(action.intent);
          }}
        >
          {label ?? copy.action[kind]}
        </Button>
      </div>
      {reason === null ? null : <p id={reasonId}>{reason}</p>}
    </div>
  );
}

type Review = { readonly kind: "entry" | "invocation"; readonly id: string };
type RequestKind = "read" | "change";

function ServiceState({
  service,
  conversationId,
  copy,
}: {
  readonly service: ConnectedServicesService;
  readonly conversationId: ConversationId | undefined;
  readonly copy: ServicesCopy;
}) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const request = useRef(0);
  const focusAfterConfirm = useRef(false);
  const [shown, setShown] = useState<{
    readonly service: ConnectedServicesService;
    readonly view: ConnectedServicesView;
  } | null>(null);
  const [review, setReview] = useState<Review | null>(null);
  const [busy, setBusy] = useState<RequestKind | null>("read");
  const [error, setError] = useState<RequestKind | null>(null);

  function load(kind: RequestKind, result: Promise<ConnectedServicesView>) {
    const id = ++request.current;
    setBusy(kind);
    setError(null);
    void result.then(
      (view) => {
        if (id !== request.current) return;
        setShown({ service, view });
        setBusy(null);
      },
      () => {
        if (id !== request.current) return;
        setError(kind);
        setBusy(null);
      },
    );
  }

  useEffect(() => {
    setReview(null);
    load("read", service.state(conversationId));
    return () => {
      request.current += 1;
    };
  }, [service, conversationId]);

  useEffect(() => {
    if (review === null && focusAfterConfirm.current) {
      focusAfterConfirm.current = false;
      heading.current?.focus();
    }
  }, [review]);

  const view = shown?.service === service ? shown.view : null;
  const outcome = view?.outcome ?? null;
  const pending = outcome !== null && !isTerminalOutcome(outcome);
  const message = outcome === null ? null : outcomeText(outcome, copy);
  const status =
    busy === null
      ? message
      : { read: copy.loading, change: copy.changing }[busy];
  const errorText =
    error === null
      ? null
      : { read: copy.loadFailed, change: copy.changeUnknown }[error];
  const unavailable = errorText ?? (busy !== null || pending ? status : null);
  const selectedEntry =
    review?.kind === "entry"
      ? view?.entries.find((entry) => entry.id === review.id)
      : undefined;
  const invocation = view?.invocation ?? null;
  const selectedInvocation =
    review?.kind === "invocation" && invocation?.id === review.id
      ? invocation
      : null;

  function change(intent: ServiceIntent) {
    if (review === null) {
      heading.current?.focus();
    } else {
      focusAfterConfirm.current = true;
      setReview(null);
    }
    load("change", service.change(conversationId, intent));
  }

  return (
    <section className="om-service-stack" aria-labelledby={headingId}>
      <h2 id={headingId} ref={heading} tabIndex={-1}>
        {copy.heading}
      </h2>
      <Text role="status">{status}</Text>
      {errorText === null ? null : <Text role="alert">{errorText}</Text>}
      {errorText !== null || pending ? (
        <div>
          <Button
            disabled={busy !== null}
            onClick={() => {
              heading.current?.focus();
              load("read", service.state(conversationId));
            }}
          >
            {copy.refresh}
          </Button>
        </div>
      ) : null}
      {view?.entries.length === 0 && invocation === null ? (
        <Text>{copy.empty}</Text>
      ) : null}
      {view?.entries.map((entry) => (
        <article
          key={entry.id}
          aria-label={entry.principal.name}
          className="om-service-stack om-service-panel om-service-reserved"
        >
          <h3>
            <bdi>{entry.principal.name}</bdi>
            {" · "}
            {copy.author.automation}
          </h3>
          <p>{copy.status[entry.status]}</p>
          {entry.readAccess.kind === "full_room_member" ? (
            <ReadAccess access={entry.readAccess} copy={copy} />
          ) : null}
          <div>
            <Button
              aria-label={copy.reviewName(entry.principal.name)}
              onClick={() => {
                setReview({ kind: "entry", id: entry.id });
              }}
            >
              {copy.review}
            </Button>
          </div>
          {entry.actions
            .filter((action) => actionKind(action) === "revoke")
            .map((action) => (
              <ServiceConfirm
                key={actionKind(action)}
                action={action}
                unavailable={unavailable}
                copy={copy}
                onConfirm={change}
              />
            ))}
        </article>
      ))}
      {invocation === null ? null : (
        <div>
          <Button
            onClick={() => {
              setReview({ kind: "invocation", id: invocation.id });
            }}
          >
            {copy.invocationReview(invocation.principal.name)}
          </Button>
        </div>
      )}
      {selectedEntry === undefined ? null : (
        <Sheet
          title={copy.reviewTitle}
          closeLabel={copy.close}
          onClose={() => {
            setReview(null);
          }}
        >
          <div className="om-service-stack om-service-reserved">
            <h3 tabIndex={-1} data-sheet-initial>
              <bdi>{selectedEntry.principal.name}</bdi>
            </h3>
            <EntryFacts entry={selectedEntry} copy={copy} />
            {selectedEntry.actions
              .filter((action) => actionKind(action) !== "revoke")
              .map((action) => (
                <ServiceConfirm
                  key={actionKind(action)}
                  action={action}
                  unavailable={unavailable}
                  copy={copy}
                  onConfirm={change}
                />
              ))}
          </div>
        </Sheet>
      )}
      {selectedInvocation === null ? null : (
        <Sheet
          title={copy.invocationTitle}
          closeLabel={copy.close}
          onClose={() => {
            setReview(null);
          }}
        >
          <div className="om-service-stack om-service-reserved">
            <h3 tabIndex={-1} data-sheet-initial>
              <bdi>{selectedInvocation.principal.name}</bdi>
              {" · "}
              <bdi>{selectedInvocation.command}</bdi>
            </h3>
            <InvocationFacts invocation={selectedInvocation} copy={copy} />
            <ServiceConfirm
              action={selectedInvocation.action}
              label={
                selectedInvocation.disclosure.kind === "selected_context"
                  ? copy.shareAndRun
                  : undefined
              }
              unavailable={unavailable}
              copy={copy}
              onConfirm={change}
            />
          </div>
        </Sheet>
      )}
    </section>
  );
}
