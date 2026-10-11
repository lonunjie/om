import { useEffect, useRef, type ReactNode } from "react";
import "../shell/primitives/primitives.css";

/** Heading that takes focus when its view opens, so keyboard and screen-reader users land on the new step. */
export function FocusHeading({ id, children }: { readonly id: string; readonly children: ReactNode }) {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => { ref.current?.focus(); }, []);
  return (
    <h2 ref={ref} id={id} tabIndex={-1} className="om-text om-settings__heading" data-size="lg" data-tone="primary" data-weight="bold">{children}</h2>
  );
}
