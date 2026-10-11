import type { ButtonHTMLAttributes } from "react";
import "./primitives.css";

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  readonly variant?: "primary" | "secondary" | "quiet";
}

export const Button = ({ variant = "secondary", className, type = "button", ...rest }: ButtonProps) => (
  <button {...rest} type={type} data-variant={variant} className={className ? `om-button ${className}` : "om-button"} />
);
