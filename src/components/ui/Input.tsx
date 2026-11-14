import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

// Forwards refs so callers can focus the field on open (the transcript
// search does).
export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className = "", variant = "default", disabled, ...props }, ref) => {
    const baseClasses =
      "text-[13px] font-[450] bg-surface border border-line-strong rounded-[var(--flow-radius-control)] text-start transition-[background-color,border-color,box-shadow] duration-[var(--flow-dur-quick)] ease-[var(--flow-ease-spring)] placeholder:text-ink-3/80";

    const interactiveClasses = disabled
      ? "opacity-55 cursor-not-allowed bg-wash border-line"
      : "hover:border-ink-4 focus:outline-none focus:border-focus focus:ring-[3px] focus:ring-logo-primary/15";

    const variantClasses = {
      default: "px-3 py-[7px]",
      compact: "px-2.5 py-[5px]",
    } as const;

    return (
      <input
        ref={ref}
        className={`${baseClasses} ${variantClasses[variant]} ${interactiveClasses} ${className}`}
        disabled={disabled}
        {...props}
      />
    );
  },
);

Input.displayName = "Input";
