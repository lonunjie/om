import { useEffect, useRef } from "react";

/** Each step renders fresh, so focusing on mount moves focus to the new step's heading. */
export function StepHeading({ children, id, focus = true }: { readonly children: string; readonly id?: string; readonly focus?: boolean }) {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => { if (focus) ref.current?.focus(); }, [focus]);
  return <h2 id={id} ref={ref} tabIndex={-1} className="om-onb__heading">{children}</h2>;
}
