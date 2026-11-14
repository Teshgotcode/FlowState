import React from "react";

interface TextareaProps
  extends React.TextareaHTMLAttributes<HTMLTextAreaElement> {
  variant?: "default" | "compact";
}

export const Textarea: React.FC<TextareaProps> = ({
  className = "",
  variant = "default",
  ...props
}) => {
  const baseClasses =
    "text-[13px] font-[450] bg-surface border border-line-strong rounded-[var(--flow-radius-control)] text-start leading-[19px] transition-[background-color,border-color,box-shadow] duration-[var(--flow-dur-quick)] ease-[var(--flow-ease-spring)] placeholder:text-ink-3/80 hover:border-ink-4 focus:outline-none focus:border-focus focus:ring-[3px] focus:ring-logo-primary/15 resize-y";

  const variantClasses = {
    default: "px-3 py-2 min-h-[100px]",
    compact: "px-2.5 py-[6px] min-h-[80px]",
  };

  return (
    <textarea
      className={`${baseClasses} ${variantClasses[variant]} ${className}`}
      {...props}
    />
  );
};
