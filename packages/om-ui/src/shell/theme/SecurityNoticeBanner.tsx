import type { ReactNode } from "react";
import "./security-notice.css";

export type SecurityNoticeTone = "notice" | "danger" | "verified";

export interface SecurityNoticeBannerProps {
  /** Machine reason code from Core/Platform (e.g. `identity_changed`); never shown as copy. */
  readonly reasonCode: string;
  /** Localized label, supplied by the caller. This component contains no warning copy. */
  readonly label: ReactNode;
  readonly tone?: SecurityNoticeTone;
  readonly children?: ReactNode;
}

// Reserved surface (UI-04): colors/shape come only from reserved tokens that themes cannot override.
// State is conveyed by a fixed glyph and border as well as color.
export const SecurityNoticeBanner = ({ reasonCode, label, tone = "notice", children }: SecurityNoticeBannerProps) => (
  <div className="om-security-notice" data-tone={tone} data-reason-code={reasonCode} role={tone === "danger" ? "alert" : "status"}>
    <span className="om-security-notice__mark" aria-hidden="true" />
    <span className="om-security-notice__label">{label}</span>
    {children}
  </div>
);
