import React from "react";
import { useTranslation } from "react-i18next";

/** The scroll container every hub page sits in. */
export const Page: React.FC<{
  children: React.ReactNode;
  /** Reading width in px; insights wants a wider column than dictations. */
  width?: number;
  className?: string;
}> = ({ children, width, className = "" }) => (
  // The shell's panel owns scroll and padding; a page only sets a max width.
  <div className={className} style={width ? { maxWidth: width } : undefined}>
    {children}
  </div>
);

/** Title block: a small overline, the screen's name, one line of guidance. */
export const PageHeader: React.FC<{
  overline?: string;
  title: string;
  subtitle?: React.ReactNode;
  action?: React.ReactNode;
}> = ({ overline, title, subtitle, action }) => (
  <header className="mb-[22px] flex items-start justify-between gap-[24px]">
    <div className="min-w-0">
      {overline && (
        <p className="mb-[6px] font-ui text-[10.5px] font-semibold uppercase tracking-[0.10em] text-ink-3">
          {overline}
        </p>
      )}
      <h1 className="font-display text-[27px] font-semibold leading-[34px] tracking-[-0.01em] text-text">
        {title}
      </h1>
      {subtitle && (
        <p className="mt-[5px] max-w-[56ch] text-[12.5px] leading-[19px] text-ink-2">
          {subtitle}
        </p>
      )}
    </div>
    {action && <div className="shrink-0 pt-1">{action}</div>}
  </header>
);

export const Card: React.FC<
  React.HTMLAttributes<HTMLDivElement> & { as?: "div" | "section" }
> = ({ children, className = "", as: Tag = "div", ...rest }) => (
  <Tag
    {...rest}
    className={`rounded-[12px] border border-line bg-surface ${className}`}
  >
    {children}
  </Tag>
);

export const SectionLabel: React.FC<{ children: React.ReactNode }> = ({
  children,
}) => (
  <p className="font-ui text-[12px] font-semibold uppercase tracking-[0.06em] text-ink-3">
    {children}
  </p>
);

/** One number worth reading at a glance. */
export const StatTile: React.FC<{
  label: string;
  value: React.ReactNode;
  hint?: string;
  icon?: React.ReactNode;
  /** marigold marks a figure worth celebrating (a streak). */
  tone?: "plain" | "streak";
}> = ({ label, value, hint, icon, tone = "plain" }) => (
  <div className="flex flex-col gap-[2px] border-line/80 px-[2px] py-[2px]">
    <div className="flex items-center gap-1.5 text-ink-2">
      {icon}
      <span className="truncate font-ui text-[10.5px] font-semibold uppercase tracking-[0.1em]">
        {label}
      </span>
    </div>
    <div className="flex items-baseline gap-2 overflow-hidden">
      <span
        className={`shrink-0 font-display text-[24px] font-semibold leading-none tracking-[-0.02em] tabular-nums ${
          tone === "streak" ? "text-logo-primary" : "text-text"
        }`}
      >
        {value}
      </span>
      {hint && <span className="truncate text-[11px] text-ink-2">{hint}</span>}
    </div>
  </div>
);

export const EmptyState: React.FC<{
  icon: React.ReactNode;
  title: string;
  body: string;
  action?: React.ReactNode;
}> = ({ icon, title, body, action }) => (
  <div className="flex flex-col items-center justify-center gap-3 rounded-[16px] border border-line bg-surface/60 px-6 py-16 text-center">
    <div className="text-ink-3">{icon}</div>
    <h2 className="font-display text-[19px] font-semibold tracking-[-0.02em] text-text">
      {title}
    </h2>
    <p className="max-w-[44ch] text-[13px] leading-relaxed text-ink-2">
      {body}
    </p>
    {action}
  </div>
);

/** Locale-aware thousands separator: 1 204, not 1,204, for bokmål readers. */
export const useNumberFormat = () => {
  const { i18n } = useTranslation();
  const format = new Intl.NumberFormat(i18n.language || "nb");
  return (value: number) => format.format(value);
};
