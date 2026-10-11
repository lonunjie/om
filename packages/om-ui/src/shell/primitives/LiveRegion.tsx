import type { ReactNode } from "react";
import { VisuallyHidden } from "./VisuallyHidden";

export interface LiveRegionProps {
  readonly politeness?: "polite" | "assertive";
  readonly children?: ReactNode;
}

// Keep mounted and change `children` to announce. Visually hidden; pair with visible UI for sighted users.
export const LiveRegion = ({ politeness = "polite", children }: LiveRegionProps) => (
  <VisuallyHidden role={politeness === "assertive" ? "alert" : "status"} aria-live={politeness} aria-atomic="true">
    {children}
  </VisuallyHidden>
);
