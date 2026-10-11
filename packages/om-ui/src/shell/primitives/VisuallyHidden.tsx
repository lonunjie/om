import type { HTMLAttributes } from "react";
import "./primitives.css";

export const VisuallyHidden = ({ className, ...rest }: HTMLAttributes<HTMLSpanElement>) => (
  <span {...rest} className={className ? `om-visually-hidden ${className}` : "om-visually-hidden"} />
);
