import { useEffect, useId, useRef, useState } from "react";
import { isTerminalOutcome } from "@om/platform-api";
import type { ConversationId, OutcomeClass } from "@om/platform-api";
import { Sheet } from "../shell/layout/Sheet";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type {
  BridgeEntryView,
  BridgeIntent,
  BridgeMappingView,
  BridgeRoomView,
  BridgeService,
} from "./bridges";
import type { BridgeCopy } from "./copy";
import "./bridges.css";

export interface BridgeRoomProps {
  readonly service?: BridgeService;
  readonly conversationId: ConversationId;
  readonly copy: BridgeCopy;
}

/** Mount in the Room wherever members can see its plaintext boundary. */
export function BridgeRoom({ service, conversationId, copy }: BridgeRoomProps) {
  return service === undefined ? null : (
    <BridgeRoomState
      key={conversationId}
      service={service}
      conversationId={conversationId}
      copy={copy}
    />
  );
}

function outcomeText(outcome: OutcomeClass, copy: BridgeCopy): string {
  switch (outcome.kind) {
    case "Rejected":
      return copy.rejected[outcome.reason];
    case "NeedsUserAction":
      return copy.needsAction[outcome.action];
    default:
      return copy.outcome[outcome.kind];
  }
}

function MappingFacts({
  mapping,
  copy,
}: {
  readonly mapping: BridgeMappingView;
  readonly copy: BridgeCopy;
}) {
  return (
    <div className="om-bridge-stack">
      <dl className="om-bridge-facts">
        <dt>{copy.network}</dt>
        <dd>
          <bdi>{mapping.network}</bdi>
        </dd>
        <dt>{copy.scope}</dt>
        <dd>
          <bdi>{mapping.scope}</bdi>
        </dd>
        <dt>{copy.directionLabel}</dt>
        <dd>{copy.direction[mapping.direction]}</dd>
        <dt>{copy.exports}</dt>
        <dd>
          {mapping.exportedClasses.length === 0 ? (
            copy.noExports
          ) : (
            <ul>
              {mapping.exportedClasses.map((kind) => (
                <li key={kind}>{copy.eventClass[kind]}</li>
              ))}
            </ul>
          )}
        </dd>
      </dl>
      <p>{copy.externalSeparate}</p>
      {mapping.externalClaim === null ? (
        <p>{copy.noExternalClaim}</p>
      ) : (
        <dl className="om-bridge-facts">
          <dt>{copy.externalClaim}</dt>
          <dd>
            <bdi>{mapping.externalClaim}</bdi>
          </dd>
        </dl>
      )}
    </div>
  );
}

function BridgeFacts({
  bridge,
  copy,
}: {
  readonly bridge: BridgeEntryView;
  readonly copy: BridgeCopy;
}) {
  return (
    <div className="om-bridge-stack">
      <dl className="om-bridge-facts">
        <dt>{copy.principal}</dt>
        <dd>
          <bdi>{bridge.principal.id}</bdi>
        </dd>
      </dl>
      {bridge.membership === "member" ? (
        <>
          <p>{copy.consequence.admit}</p>
          <p>{copy.encryptionTo(<bdi>{bridge.principal.name}</bdi>)}</p>
          {bridge.mappings.map((mapping) => (
            <div className="om-bridge-stack" key={mapping.id}>
              <p>
                <strong>{copy.mapping[mapping.state]}</strong>
              </p>
              <MappingFacts mapping={mapping} copy={copy} />
            </div>
          ))}
        </>
      ) : bridge.membership === "proposed" ? (
        bridge.mappings.map((mapping) => (
          <MappingFacts key={mapping.id} mapping={mapping} copy={copy} />
        ))
      ) : null}
    </div>
  );
}

function BridgeRoomState({
  service,
  conversationId,
  copy,
}: Required<BridgeRoomProps>) {
  const headingId = useId();
  const request = useRef(0);
  const heading = useRef<HTMLHeadingElement>(null);
  const focusAfterConfirm = useRef(false);
  const [shown, setShown] = useState<{
    readonly service: BridgeService;
    readonly view: BridgeRoomView;
  } | null>(null);
  const [reviewId, setReviewId] = useState<string | null>(null);
  const [busy, setBusy] = useState<"read" | "change" | null>("read");
  const [error, setError] = useState<"read" | "change" | null>(null);

  function load(kind: "read" | "change", result: Promise<BridgeRoomView>) {
    const id = ++request.current;
    setBusy(kind);
    setError(null);
    void result.then(
      (next) => {
        if (id !== request.current) return;
        setShown({ service, view: next });
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
    setReviewId(null);
    load("read", service.state(conversationId));
    return () => {
      request.current += 1;
    };
  }, [conversationId, service]);

  useEffect(() => {
    if (reviewId === null && focusAfterConfirm.current) {
      focusAfterConfirm.current = false;
      heading.current?.focus();
    }
  }, [reviewId]);

  const view = shown?.service === service ? shown.view : null;
  const review = view?.bridges.find(
    (bridge) => bridge.principal.id === reviewId,
  );
  const action = review?.action;
  const outcome = view?.outcome ?? null;
  const status =
    busy === null
      ? outcome && outcomeText(outcome, copy)
      : { read: copy.loading, change: copy.changing }[busy];
  const errorText =
    error && { read: copy.loadFailed, change: copy.changeUnknown }[error];
  const isPending = outcome !== null && !isTerminalOutcome(outcome);
  const disabledReason = busy === null ? errorText : status;

  function change(intent: BridgeIntent) {
    // The old Review control may disappear when Core returns a different roster.
    focusAfterConfirm.current = true;
    setReviewId(null);
    load("change", service.change(conversationId, intent));
  }

  return (
    <section aria-labelledby={headingId} className="om-bridge-stack">
      <h2 id={headingId} ref={heading} tabIndex={-1}>
        {copy.heading}
      </h2>
      <Text as="p" role="status">
        {status}
      </Text>
      {errorText === null ? null : (
        <Text as="p" role="alert">
          {errorText}
        </Text>
      )}
      {errorText !== null || isPending ? (
        <Button
          disabled={busy !== null}
          onClick={() => {
            heading.current?.focus();
            load("read", service.state(conversationId));
          }}
        >
          {copy.refresh}
        </Button>
      ) : null}
      {view?.bridges.length === 0 ? <Text>{copy.empty}</Text> : null}
      {view?.bridges.map((bridge) => (
        <article
          className="om-bridge-security om-bridge-stack"
          key={bridge.principal.id}
        >
          <h3>
            <bdi>{bridge.principal.name}</bdi>
          </h3>
          <SecurityNoticeBanner
            reasonCode={bridge.membership}
            label={copy.membership[bridge.membership]}
          />
          <BridgeFacts bridge={bridge} copy={copy} />
          {bridge.membership === "removed" ? null : (
            <div>
              <Button
                aria-label={copy.reviewName(bridge.principal.name)}
                onClick={() => {
                  setReviewId(bridge.principal.id);
                }}
              >
                {copy.review}
              </Button>
            </div>
          )}
        </article>
      ))}
      {review === undefined ? null : (
        <Sheet
          title={copy.reviewTitle}
          closeLabel={copy.close}
          onClose={() => {
            setReviewId(null);
          }}
        >
          <div className="om-bridge-security om-bridge-stack">
            <h3 tabIndex={-1} data-sheet-initial>
              <bdi>{review.principal.name}</bdi>
            </h3>
            {action == null ? null : (
              <p>
                {
                  copy.consequence[
                    action.availability === "ready"
                      ? action.intent.kind
                      : action.kind
                  ]
                }
              </p>
            )}
            {review.membership === "proposed" ? (
              <p>{copy.admissionBoundary}</p>
            ) : null}
            <BridgeFacts bridge={review} copy={copy} />
            {action == null ? null : (
              <BridgeConfirm
                action={action}
                disabledReason={disabledReason}
                copy={copy}
                onConfirm={change}
              />
            )}
          </div>
        </Sheet>
      )}
    </section>
  );
}

function BridgeConfirm({
  action,
  disabledReason,
  copy,
  onConfirm,
}: {
  readonly action: NonNullable<BridgeEntryView["action"]>;
  readonly disabledReason: string | null;
  readonly copy: BridgeCopy;
  readonly onConfirm: (intent: BridgeIntent) => void;
}) {
  const id = useId();
  const kind =
    action.availability === "ready" ? action.intent.kind : action.kind;
  const reason =
    action.availability === "blocked"
      ? copy.rejected[action.reason]
      : disabledReason;
  return (
    <div className="om-bridge-stack">
      <div>
        <Button
          variant={kind === "remove" ? "danger" : "primary"}
          disabled={reason !== null}
          aria-describedby={reason === null ? undefined : id}
          onClick={() => {
            if (action.availability === "ready") onConfirm(action.intent);
          }}
        >
          {copy.action[kind]}
        </Button>
      </div>
      {reason === null ? null : (
        <p id={id} role="status">
          {reason}
        </p>
      )}
    </div>
  );
}
