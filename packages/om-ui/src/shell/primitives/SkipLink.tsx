import type { MouseEvent, ReactNode } from "react";
import "./primitives.css";

export interface SkipLinkProps {
  /** id of the element to move focus to (it receives tabindex=-1 if needed). */
  readonly targetId: string;
  readonly children: ReactNode;
}

export const SkipLink = ({ targetId, children }: SkipLinkProps) => {
  const onClick = (e: MouseEvent<HTMLAnchorElement>): void => {
    const target = document.getElementById(targetId);
    if (!target) return;
    e.preventDefault();
    if (!target.hasAttribute("tabindex")) target.setAttribute("tabindex", "-1");
    target.focus();
  };
  return <a className="om-skip-link" href={`#${targetId}`} onClick={onClick}>{children}</a>;
};
