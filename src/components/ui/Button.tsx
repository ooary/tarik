import type { ButtonHTMLAttributes } from "react";

export type ButtonTone = "primary" | "secondary" | "quiet";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  tone?: ButtonTone;
}

export function Button({ tone = "secondary", className = "", ...props }: ButtonProps) {
  return <button className={`ui-button ui-button-${tone} ${className}`.trim()} {...props} />;
}
