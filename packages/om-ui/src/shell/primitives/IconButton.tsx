import type { ButtonHTMLAttributes, ReactNode } from "react";
import "./primitives.css";

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children" | "aria-label"> {
  /** Accessible name (required, localized by the caller). */
  readonly label: string;
  readonly icon: ReactNode;
}

export const IconButton = ({ label, icon, className, type = "button", ...rest }: IconButtonProps) => (
  <button {...rest} type={type} aria-label={label} className={className ? `om-icon-button ${className}` : "om-icon-button"}>
    <span aria-hidden="true">{icon}</span>
  </button>
);
