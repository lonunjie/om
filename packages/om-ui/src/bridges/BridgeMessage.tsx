import type { BridgeMessageView } from "./bridges";
import type { BridgeCopy } from "./copy";
import "./bridges.css";

/** Message-level attribution and presentation evidence, supplied by Core even after a bridge leaves. */
export function BridgeMessage({
  view,
  copy,
}: {
  readonly view: BridgeMessageView;
  readonly copy: BridgeCopy["message"];
}) {
  return (
    <div className="om-bridge-security om-bridge-stack">
      <dl className="om-bridge-facts">
        <dt>{copy.author}</dt>
        <dd>
          <bdi>{view.principal.name}</bdi>
        </dd>
        <dt>{copy.foreignActor}</dt>
        <dd>
          <bdi>{view.actor ?? copy.unknownActor}</bdi>
          {" · "}
          <bdi>{view.network}</bdi>
        </dd>
        <dt>{copy.linkedIdentity}</dt>
        <dd>
          {view.identityLink.kind === "linked" ? (
            <bdi>{view.identityLink.accountName}</bdi>
          ) : (
            copy.noIdentityLink
          )}
        </dd>
      </dl>
      <ul aria-label={copy.fidelity}>
        {view.flags.map((flag) => (
          <li key={flag}>{copy.flag[flag]}</li>
        ))}
        {view.carry === undefined ? null : <li>{copy.carry[view.carry]}</li>}
      </ul>
    </div>
  );
}
