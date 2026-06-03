import { LoaderCircle } from "lucide-react";
import type { ButtonHTMLAttributes, ReactNode } from "react";

type Variant = "primary" | "secondary" | "danger" | "ghost";

interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  icon?: ReactNode;
  loading?: boolean;
}

export function Button({ variant = "secondary", icon, loading = false, children, className = "", disabled, ...props }: ButtonProps) {
  return (
    <button className={`button button-${variant} ${className}`} disabled={disabled || loading} {...props}>
      {loading ? <LoaderCircle size={16} className="spinner" /> : icon}
      {children}
    </button>
  );
}
