// SPDX-License-Identifier: MPL-2.0
import { useId } from "react";
import { Button } from "../shell/primitives/Button";
import type { CommunitiesAction, CommunitiesIntent } from "./communities";
import type { CommunitiesCopy } from "./copy";

export interface ActionProps {
  readonly copy: CommunitiesCopy;
  readonly unavailable: string | null;
  readonly onChange: (intent: CommunitiesIntent) => void;
}

export function CommunityAction({
  action, copy, unavailable, onChange, consequenceId,
}: ActionProps & {
  readonly action: CommunitiesAction;
  readonly consequenceId?: string;
}) {
  const reasonId = useId();
  const kind = action.availability === "ready" ? action.intent.kind : action.kind;
  const reason = action.availability === "blocked" ? copy.rejected[action.reason] : unavailable;
  const description = [consequenceId, reason === null ? undefined : reasonId].filter(Boolean).join(" ");
  return (
    <div className="om-community-stack">
      <div>
        <Button
          variant="primary"
          disabled={reason !== null}
          aria-describedby={description || undefined}
          onClick={() => {
            if (reason === null && action.availability === "ready") onChange(action.intent);
          }}
        >
          {copy.action[kind]}
        </Button>
      </div>
      {reason === null ? null : <p id={reasonId}>{reason}</p>}
    </div>
  );
}
