// SPDX-License-Identifier: MPL-2.0
import { useId, useState } from "react";
import { Sheet } from "../shell/layout/Sheet";
import { Button } from "../shell/primitives/Button";
import { CommunityAction, type ActionProps } from "./CommunityAction";
import type { CommunityDetails, RoomDetails } from "./communities";

export function Admission({
  details, copy, unavailable, onChange,
}: ActionProps & { readonly details: CommunityDetails | RoomDetails }) {
  const { admission } = details;
  const [review, setReview] = useState<typeof admission | null>(null);
  const consequenceId = useId();
  const boundary = details.kind === "community" ? copy.admission.boundary : copy.admission.roomBoundary;
  const { action, offer, evidence } = admission;

  return (
    <div className="om-community-stack">
      {admission.membership === null ? null : <p>{copy.admission.membership[details.kind][admission.membership]}</p>}
      <p>{boundary}</p>
      {evidence === undefined ? null : (
        <p>{copy.admission.evidence[evidence.kind](
          <bdi>{evidence.community}</bdi>,
          evidence.kind === "community_role" ? <bdi>{evidence.role}</bdi> : undefined,
        )}</p>
      )}
      {action === null ? null : (
        action.availability === "blocked" && offer === undefined ? (
          <CommunityAction action={action} copy={copy} unavailable={unavailable} onChange={onChange} />
        ) : (
          <div><Button onClick={() => { setReview(admission); }}>{copy.admission.review}</Button></div>
        )
      )}
      {review === admission && action !== null ? (
        <Sheet title={copy.admission.reviewTitle} closeLabel={copy.close} onClose={() => { setReview(null); }}>
          <div className="om-community-stack">
            <h3 tabIndex={-1} data-sheet-initial>
              {copy.place[details.kind]}{" · "}<bdi>{details.name}</bdi>
            </h3>
            <p id={consequenceId}>{boundary}</p>
            {offer === undefined ? null : (
              <>
                {offer.source === "host_observed" ? <p>{copy.admission.observed}</p> : null}
                <p>{offer.approvalRequired ? copy.admission.approval : copy.admission.noApproval}</p>
                <dl className="om-community-facts">
                  {offer.uses === undefined ? null : (
                    <><dt>{copy.admission.useLabel}</dt><dd>{copy.admission.uses[offer.uses]}</dd></>
                  )}
                  {offer.expiresAtMs === undefined ? null : (
                    <><dt>{copy.admission.expires}</dt><dd>{copy.formatTime(offer.expiresAtMs)}</dd></>
                  )}
                  {offer.grants === undefined ? null : (
                    <>
                      <dt>{copy.admission.grants}</dt>
                      <dd>{offer.grants.length === 0 ? copy.admission.noGrants : (
                        <ul>{offer.grants.map(({ id, label }) => <li key={id}><bdi>{label}</bdi></li>)}</ul>
                      )}</dd>
                    </>
                  )}
                </dl>
              </>
            )}
            <CommunityAction
              action={action}
              copy={copy}
              consequenceId={consequenceId}
              unavailable={unavailable}
              onChange={(intent) => {
                setReview(null);
                onChange(intent);
              }}
            />
          </div>
        </Sheet>
      ) : null}
    </div>
  );
}
