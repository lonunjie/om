// SPDX-License-Identifier: MPL-2.0
import { SecurityNoticeBanner } from "../shell/theme/SecurityNoticeBanner";
import type { CommunitiesHref, GovernanceView, MemberPermissionsView, PermissionView } from "./communities";
import type { CommunitiesCopy } from "./copy";

export function PermissionFacts({ permissions, copy }: {
  readonly permissions: readonly PermissionView[];
  readonly copy: CommunitiesCopy["authority"];
}) {
  return (
    <dl className="om-community-facts">
      {permissions.map(({ id, label, answer }) => (
        <div key={id}>
          <dt><bdi>{label}</bdi></dt>
          <dd>
            {copy.answer[answer.kind]}
            {answer.kind === "allowed" && answer.sources.length > 0 ? (
              <ul>
                {answer.sources.map((source, index) => (
                  <li key={index}>{copy.source[source.kind]("name" in source ? <bdi>{source.name}</bdi> : undefined)}</li>
                ))}
              </ul>
            ) : null}
          </dd>
        </div>
      ))}
    </dl>
  );
}

export function MemberPermissions({ members, copy }: {
  readonly members: readonly MemberPermissionsView[];
  readonly copy: CommunitiesCopy;
}) {
  if (members.length === 0) return null;
  const { authority } = copy;
  return (
    <section className="om-community-stack">
      <h3>{authority.heading}</h3>
      {members.map((member) => (
        <details key={member.id}>
          <summary><bdi>{member.name}</bdi></summary>
          <div className="om-community-stack">
            <h4>{authority.roles}</h4>
            {member.roles.length === 0 ? (
              <p>{authority.noRoles}</p>
            ) : (
              <ul>{member.roles.map(({ id, name }) => <li key={id}><bdi>{name}</bdi></li>)}</ul>
            )}
            {member.permissions.length === 0 ? null : (
              <>
                <h4>{authority.permissions}</h4>
                <PermissionFacts permissions={member.permissions} copy={authority} />
              </>
            )}
            {member.restrictions.length === 0 ? null : (
              <>
                <h4>{authority.restrictions}</h4>
                <ul>
                  {member.restrictions.map((restriction) => (
                    <li className="om-community-stack" key={restriction.id}>
                      <strong><bdi>{restriction.capability.label}</bdi></strong>
                      <p>{authority.restriction[restriction.state]}</p>
                      {restriction.reason === undefined ? null : <p dir="auto">{restriction.reason}</p>}
                      {restriction.intendedEndAtMs === undefined ? null : (
                        <dl className="om-community-facts">
                          <dt>{authority.intendedEnd}</dt>
                          <dd>{copy.formatTime(restriction.intendedEndAtMs)}</dd>
                        </dl>
                      )}
                    </li>
                  ))}
                </ul>
              </>
            )}
          </div>
        </details>
      ))}
    </section>
  );
}

export function GovernanceNotice({ governance, copy, hrefFor }: {
  readonly governance: GovernanceView;
  readonly copy: CommunitiesCopy["governance"];
  readonly hrefFor: CommunitiesHref;
}) {
  const { state, successor } = governance;
  if (state === "active") return null;
  const successorPlace = successor?.kind === "community"
    ? { kind: successor.kind, id: successor.community.id }
    : successor === undefined ? undefined : { kind: successor.kind, id: successor.room.id };
  const successorName = successor?.kind === "community" ? successor.community.name : successor?.room.name;
  return (
    <div className="om-community-stack">
      <SecurityNoticeBanner reasonCode={state} label={copy.notice[state]} />
      <p>{copy.recovery}</p>
      <p>{copy.fork}</p>
      {successorPlace === undefined ? null : (
        <div>
          <a className="om-community-link" href={hrefFor(successorPlace)}>
            <span>{copy.openSuccessor(<bdi>{successorName}</bdi>)}</span>
          </a>
        </div>
      )}
    </div>
  );
}
