import type { SecurityNotice, SecurityNoticeCode } from "@om/platform-api";
import { SecurityNoticeBanner, type SecurityNoticeTone } from "../shell/theme/SecurityNoticeBanner";
import type { ConversationCopy } from "./copy";

const TONE: Readonly<Record<SecurityNoticeCode, SecurityNoticeTone>> = {
  safety_number_changed: "danger", integrity_failure: "danger", session_reset: "notice", device_added: "notice", unverified_contact: "notice",
};

/** Renders Core security notices on the reserved banner surface, keyed by machine reason code. Copy comes from props. */
export const SecurityNotices = ({ notices, copy }: { readonly notices: readonly SecurityNotice[]; readonly copy: ConversationCopy["notices"] }) => (
  <>
    {notices.map((n) => (
      <SecurityNoticeBanner key={`${n.code}:${n.subject ?? ""}`} reasonCode={n.code} label={copy[n.code]} tone={TONE[n.code]} />
    ))}
  </>
);
