import type { HTMLAttributes } from "react";
import "./primitives.css";

type TextTag = "p" | "span" | "div" | "h1" | "h2" | "h3" | "h4";

export interface TextProps extends HTMLAttributes<HTMLElement> {
  readonly as?: TextTag;
  readonly size?: "xs" | "sm" | "md" | "lg" | "xl" | "2xl";
  readonly tone?: "primary" | "secondary" | "muted";
  readonly weight?: "regular" | "medium" | "bold";
}

export const Text = ({ as: Tag = "p", size = "md", tone = "primary", weight = "regular", className, ...rest }: TextProps) => (
  <Tag {...rest} data-size={size} data-tone={tone} data-weight={weight} className={className ? `om-text ${className}` : "om-text"} />
);
