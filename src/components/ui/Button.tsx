import React from "react";

interface ButtonProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  variant?:
    | "primary"
    | "primary-soft"
    | "secondary"
    | "outline"
    | "warning"
    | "danger"
    | "danger-ghost"
    | "ghost";
  size?: "sm" | "md" | "lg";
}

export const Button: React.FC<ButtonProps> = ({
  children,
  className = "",
  variant = "primary",
  size = "md",
  ...props
}) => {
  const baseClasses =
    "font-[560] rounded-[var(--flow-radius-control)] border transition-all duration-[var(--flow-dur-quick)] ease-[var(--flow-ease-spring)] active:scale-[0.985] focus:outline-none focus-visible:ring-2 focus-visible:ring-focus disabled:opacity-45 disabled:cursor-not-allowed disabled:active:scale-100 cursor-pointer";

  const variantClasses = {
    primary:
      "text-on-accent bg-background-ui border-logo-primary/25 shadow-[var(--flow-shadow-control)] hover:brightness-[0.97] hover:border-logo-primary/40",
    "primary-soft":
      "text-text bg-logo-primary/14 border-transparent hover:bg-logo-primary/20",
    secondary:
      "bg-surface border-line-strong text-text shadow-[var(--flow-shadow-control)] hover:bg-wash",
    // For screens where secondary's grey fill reads as disabled (onboarding)
    outline:
      "text-text bg-background border-line-strong hover:border-plum focus-visible:ring-1 focus-visible:ring-logo-primary",
    // Secondary's neutral resting look, but hover/focus use the semantic
    // --color-warning token (theme.css) instead of the pink accent — for
    // buttons sitting on warning surfaces like SecureInputWarning
    warning:
      "text-text bg-wash border-line hover:bg-warning-wash hover:border-warning focus:ring-1 focus:ring-warning",
    danger:
      "text-background bg-error border-error hover:bg-error/80 hover:border-error/80 focus:ring-1 focus:ring-error",
    "danger-ghost":
      "text-error border-transparent hover:bg-error-wash focus:bg-error-wash",
    ghost:
      "text-current border-transparent hover:bg-wash hover:border-transparent focus:bg-wash",
  };

  const sizeClasses = {
    sm: "px-2.5 py-[5px] text-[12.5px]",
    md: "px-3.5 py-[7px] text-[13px]",
    lg: "px-4 py-2 text-[14px]",
  };

  return (
    <button
      className={`${baseClasses} ${variantClasses[variant]} ${sizeClasses[size]} ${className}`}
      {...props}
    >
      {children}
    </button>
  );
};
