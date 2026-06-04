import type { ButtonHTMLAttributes, ReactNode, Ref } from "react";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  label: string;
  children: ReactNode;
  buttonRef?: Ref<HTMLButtonElement>;
}

export function IconButton({ label, children, className = "", buttonRef, ...props }: IconButtonProps) {
  return (
    <button ref={buttonRef} className={`icon-button ${className}`} aria-label={label} title={label} {...props}>
      {children}
    </button>
  );
}
