import React from "react";

interface InputProps extends React.InputHTMLAttributes<HTMLInputElement> {
  variant?: "default" | "compact";
}

// Forwards refs so callers can focus the field on open (the transcript
// search does).
export const Input = React.forwardRef<HTMLInputElement, InputProps>(
  ({ className = "", variant = "default", disabled, ...props }, ref) => {
    const baseClasses =
      "px-2 py-1 text-sm font-semibold bg-wash border border-line-strong rounded-md text-start transition-all duration-150";

    const interactiveClasses = disabled
      ? "opacity-60 cursor-not-allowed bg-wash border-line"
      : "hover:bg-logo-primary/10 hover:border-logo-primary focus:outline-none focus:bg-logo-primary/20 focus:border-logo-primary";

    const variantClasses = {
      default: "px-3 py-2",
      compact: "px-2 py-1",
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
