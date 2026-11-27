// SPDX-License-Identifier: MPL-2.0
import { isTerminalOutcome, type OutcomeClass } from "@om/platform-api";
import { useEffect, useId, useRef, useState } from "react";
import { Button } from "../shell/primitives/Button";
import { Text } from "../shell/primitives/Text";
import { Admission } from "./Admission";
import { GovernanceNotice, MemberPermissions, PermissionFacts } from "./Authority";
import type { ActionProps } from "./CommunityAction";
import { PublicationFeed } from "./PublicationFeed";
import type {
  CommunitiesDetails, CommunitiesHref, CommunitiesIntent, CommunitiesPlace,
  CommunitiesService, CommunitiesView, CommunityLinkState,
} from "./communities";
import type { CommunitiesCopy } from "./copy";
import "./communities.css";

export interface CommunitiesProps {
  readonly service?: CommunitiesService;
  readonly place: CommunitiesPlace;
  readonly hrefFor: CommunitiesHref;
  readonly copy: CommunitiesCopy;
}

export function Communities({ service, place, hrefFor, copy }: CommunitiesProps) {
  if (service === undefined) return null;
  const key = JSON.stringify([place.kind, place.id, place.kind === "topic" ? place.roomId : null]);
  return <CommunitiesState key={key} service={service} place={place} hrefFor={hrefFor} copy={copy} />;
}

function outcomeText(outcome: OutcomeClass, copy: CommunitiesCopy): string {
  switch (outcome.kind) {
    case "Rejected": return copy.rejected[outcome.reason];
    case "NeedsUserAction": return copy.needsAction[outcome.action];
    default: return copy.outcome[outcome.kind];
  }
}

type RequestKind = "read" | "change";

function CommunitiesState({ service, place, hrefFor, copy }: Required<CommunitiesProps>) {
  const headingId = useId();
  const heading = useRef<HTMLHeadingElement>(null);
  const request = useRef(0);
  const inFlight = useRef(false);
  const moveFocus = useRef(false);
  const [shown, setShown] = useState<{ service: CommunitiesService; view: CommunitiesView } | null>(null);
  const [busy, setBusy] = useState<RequestKind | null>("read");
  const [error, setError] = useState<RequestKind | null>(null);

  async function load(operation: RequestKind, work: () => Promise<CommunitiesView>) {
    const current = ++request.current;
    inFlight.current = true;
    setBusy(operation);
    setError(null);
    try {
      const view = await work();
      if (current === request.current) setShown({ service, view });
    } catch {
      if (current === request.current) setError(operation);
    } finally {
      if (current === request.current) {
        inFlight.current = false;
        setBusy(null);
      }
    }
  }

  useEffect(() => {
    // The parent keys this state by place. A new service means a new profile-bound read.
    void load("read", () => service.state(place));
    return () => { request.current += 1; };
  }, [service]);

  useEffect(() => {
    if (moveFocus.current) {
      moveFocus.current = false;
      heading.current?.focus();
    }
  });

  const view = shown?.service === service ? shown.view : null;
  const details = view?.details ?? null;
  const outcome = view?.outcome ?? null;
  const pending = outcome !== null && !isTerminalOutcome(outcome);
  const message = outcome === null ? null : outcomeText(outcome, copy);
  const status = busy === null ? message : busy === "read" ? copy.loading : copy.changing;
  const errorText = error === null ? null : error === "read" ? copy.loadFailed : copy.changeUnknown;
  const unavailable = errorText ?? (busy !== null || pending ? status : null);
  const selectedTopic = details?.kind === "room" ? details.selectedTopic : undefined;

  function change(intent: CommunitiesIntent) {
    if (inFlight.current || unavailable !== null) return;
    moveFocus.current = true;
    void load("change", () => service.change(place, intent));
  }

  return (
    <section className="om-community-stack" aria-labelledby={headingId}>
      <h2 id={headingId} ref={heading} tabIndex={-1}>
        <bdi>{selectedTopic?.title ?? details?.name ?? copy.place[place.kind]}</bdi>
      </h2>
      {details?.nameSource === "host_observed" ? <p>{copy.directoryObservation}</p> : null}
      <Text as="p" role="status">{status}</Text>
      {errorText === null ? null : <Text as="p" role="alert">{errorText}</Text>}
      {errorText !== null || pending || outcome?.kind === "Rejected" ? (
        <div>
          <Button
            disabled={busy !== null}
            onClick={() => {
              if (inFlight.current) return;
              moveFocus.current = true;
              void load("read", () => service.state(place));
            }}
          >
            {copy.refresh}
          </Button>
        </div>
      ) : null}
      {details === null ? null : (
        <Content details={details} copy={copy} hrefFor={hrefFor} unavailable={unavailable} onChange={change} />
      )}
    </section>
  );
}

function ChildrenList<Item extends { readonly id: string; readonly name: string; readonly link: CommunityLinkState }>({
  title, items, empty, destination, hrefFor, copy,
}: {
  readonly title: string;
  readonly items: readonly Item[];
  readonly destination: (item: Item) => CommunitiesPlace;
  readonly empty?: string;
  readonly hrefFor: CommunitiesHref;
  readonly copy: CommunitiesCopy["community"];
}) {
  return (
    <section className="om-community-stack">
      <h3>{title}</h3>
      {items.length === 0 ? <p>{empty}</p> : (
        <ul className="om-community-list" role="list">
          {items.map((item) => {
            const place = destination(item);
            return <li className="om-community-stack" key={item.id}>
              <div>
                <a className="om-community-link" href={hrefFor(place)}>
                  <bdi>{item.name}</bdi>
                </a>
              </div>
              <p>{copy.affiliation[item.link.official ? "linked" : "reference"]}</p>
              {place.kind === "room" && item.link.bound ? <p>{copy.delegated}</p> : null}
            </li>;
          })}
        </ul>
      )}
    </section>
  );
}

function Content({ details, hrefFor, ...actionProps }: ActionProps & {
  readonly details: CommunitiesDetails;
  readonly hrefFor: CommunitiesHref;
}) {
  const { copy } = actionProps;
  if (details.kind === "publication") {
    return <PublicationFeed publication={details} hrefFor={hrefFor} {...actionProps} />;
  }

  return (
    <>
      {details.kind === "room" && details.selectedTopic !== undefined ? (
        <div>
          <a className="om-community-link" href={hrefFor({ kind: "room", id: details.id })}>
            <span>{copy.room.backToRoom(<bdi>{details.name}</bdi>)}</span>
          </a>
        </div>
      ) : null}
      {details.kind === "room" && details.community !== undefined ? (
        <div>
          <a className="om-community-link" href={hrefFor({ kind: "community", id: details.community.id })}>
            <bdi>{details.community.name}</bdi>
          </a>
        </div>
      ) : null}
      {details.description !== undefined ? <p dir="auto">{details.description}</p> : null}
      {details.governance === undefined ? null : <GovernanceNotice governance={details.governance} hrefFor={hrefFor} copy={copy.governance} />}
      <Admission details={details} {...actionProps} />
      {details.kind === "community" ? (
        <>
          <ChildrenList title={copy.community.rooms} items={details.rooms} destination={({ id }) => ({ kind: "room", id })} empty={copy.community.noRooms} hrefFor={hrefFor} copy={copy.community} />
          <ChildrenList title={copy.community.publications} items={details.publications} destination={({ id }) => ({ kind: "publication", id })} empty={copy.community.noPublications} hrefFor={hrefFor} copy={copy.community} />
          {details.communities.length === 0 ? null : (
            <ChildrenList title={copy.community.communities} items={details.communities} destination={({ id }) => ({ kind: "community", id })} hrefFor={hrefFor} copy={copy.community} />
          )}
        </>
      ) : (
        <>
          {details.mode === undefined ? null : <p>{copy.room.mode[details.mode]}</p>}
          {details.mode === undefined || details.mode === "chat" ? null : <p>{copy.room[details.mode]}</p>}
          {details.selectedTopic === undefined ? null : <p>{copy.room.topicStatus[details.selectedTopic.status]}</p>}
          {details.participation.length === 0 ? null : (
            <section className="om-community-stack">
              <h3>{copy.room.participation}</h3>
              <PermissionFacts permissions={details.participation} copy={copy.authority} />
            </section>
          )}
          {details.selectedTopic !== undefined || (details.mode !== "forum" && details.topics.length === 0) ? null : (
            <section className="om-community-stack">
              <h3>{copy.room.topics}</h3>
              {details.topics.length === 0 ? <p>{copy.room.noTopics}</p> : (
                <ul className="om-community-list" role="list">
                  {details.topics.map((topic) => (
                    <li className="om-community-stack" key={topic.id}>
                      <div>
                        <a className="om-community-link" href={hrefFor({ kind: "topic", roomId: details.id, id: topic.id })}>
                          <bdi>{topic.title}</bdi>
                        </a>
                      </div>
                      <p>{copy.room.topicStatus[topic.status]}</p>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          )}
        </>
      )}
      <MemberPermissions members={details.members} copy={copy} />
    </>
  );
}
