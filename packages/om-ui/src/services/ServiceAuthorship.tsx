// SPDX-License-Identifier: MPL-2.0
import type { ServicesCopy } from "./copy";
import type { ServiceAuthorshipView } from "./services";
import "./services.css";

/** Use in place of an account byline for a service-authored event. */
export function ServiceAuthorship({
  view,
  copy,
}: {
  readonly view: ServiceAuthorshipView;
  readonly copy: ServicesCopy["author"];
}) {
  const service = <bdi>{view.principal.name}</bdi>;
  return (
    <span className="om-service-reserved om-service-authorship">
      {view.representation === null
        ? service
        : copy.via(<bdi>{view.representation.accountName}</bdi>, service)}
      {" · "}
      {copy.automation}
    </span>
  );
}
