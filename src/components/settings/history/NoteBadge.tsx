import React from "react";
import { StickyNote } from "lucide-react";
import { useTranslation } from "react-i18next";

/** The pill that marks a history entry as a Scratchpad take, from its
 * `source` field, so quick notes stand out from hotkey dictations wherever
 * the entry is shown. */
export const NoteBadge: React.FC<{ className?: string }> = ({
  className = "",
}) => {
  const { t } = useTranslation();
  return (
    <span
      className={`inline-flex shrink-0 items-center gap-1 rounded-full border border-line bg-wash px-2 py-0.5 text-xs font-medium text-ink-2 ${className}`}
    >
      <StickyNote className="h-3 w-3" aria-hidden="true" />
      {t("settings.history.note")}
    </span>
  );
};
