import type { DeliveryState, OutcomeClass, RejectReason } from "@om/platform-api";
import type { ConversationCopy } from "./copy";
import "./conversations.css";

export type DeliveryDisplay =
  | { readonly kind: "pending" | "pending_network" | "pending_reconciliation" | "sent" | "delivered" }
  | { readonly kind: "rejected"; readonly reason: RejectReason };

/** Reasons for which retrying the same logical message makes sense. Everything else is shown without a retry control. */
export const RETRYABLE_REASONS: ReadonlySet<RejectReason> = new Set<RejectReason>(["unavailable", "rate_limited", "quota"]);

/** A live non-terminal operation outcome wins over the item's stored state (restart reconstruction, UI-13). A timeout is never terminal (UI-12). */
export function deliveryOf(state: DeliveryState, op?: OutcomeClass): DeliveryDisplay {
  switch (op?.kind) {
    case "PendingNetwork": return { kind: "pending_network" };
    case "PendingReconciliation": return { kind: "pending_reconciliation" };
    case "PendingAuthorization":
    case "NeedsUserAction": return { kind: "pending" };
    default: break;
  }
  switch (state.state) {
    case "pending_network": return { kind: "pending_network" };
    case "pending_reconciliation": return { kind: "pending_reconciliation" };
    case "rejected": return { kind: "rejected", reason: state.reason };
    case "sent": return { kind: "sent" };
    case "delivered": return { kind: "delivered" };
  }
}

const GLYPH: Readonly<Record<DeliveryDisplay["kind"], string>> = {
  pending: "…", pending_network: "…", pending_reconciliation: "…", sent: "✓", delivered: "✓✓", rejected: "!",
};

export function deliveryText(d: DeliveryDisplay, copy: ConversationCopy["delivery"]): string {
  switch (d.kind) {
    case "pending": return copy.pending;
    case "pending_network": return copy.pendingNetwork;
    case "pending_reconciliation": return copy.pendingReconciliation;
    case "sent": return copy.sent;
    case "delivered": return copy.delivered;
    case "rejected": return copy.rejected[d.reason];
  }
}

export const DeliveryIndicator = ({ delivery, copy }: { readonly delivery: DeliveryDisplay; readonly copy: ConversationCopy["delivery"] }) => (
  <span className="om-delivery" data-state={delivery.kind} data-reason={delivery.kind === "rejected" ? delivery.reason : undefined}>
    <span aria-hidden="true" className="om-delivery__glyph">{GLYPH[delivery.kind]}</span>
    <span className="om-delivery__text">{deliveryText(delivery, copy)}</span>
  </span>
);
