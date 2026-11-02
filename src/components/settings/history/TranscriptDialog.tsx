import React, { useEffect, useMemo, useRef, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { Check, Copy, FileDown, Search, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands, type HistoryEntry } from "@/bindings";
import { formatDateTime } from "@/utils/dateFormat";
import { Button } from "../../ui/Button";
import { Dialog } from "../../ui/Dialog";
import { Input } from "../../ui/Input";
import { copyToClipboard } from "./clipboard";
import { MeetingBadge } from "./MeetingBadge";
import {
  baseName,
  buildTranscriptMarkdown,
  formatSegmentTimestamp,
  suggestedExportFileName,
  transcriptText,
} from "./transcriptExport";

interface TranscriptDialogProps {
  entry: HistoryEntry;
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

/** The transcript window: every timed segment of one history entry with its
 * `mm:ss` timestamp, a search box over the text, a copy button for the whole
 * transcript and a Markdown export the user saves to a file they pick. */
export const TranscriptDialog: React.FC<TranscriptDialogProps> = ({
  entry,
  open,
  onOpenChange,
}) => {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [copied, setCopied] = useState(false);
  const [exporting, setExporting] = useState(false);
  const searchRef = useRef<HTMLInputElement>(null);

  const segments = useMemo(
    () => entry.metadata?.segments ?? [],
    [entry.metadata?.segments],
  );
  const durationSeconds = entry.metadata?.duration_secs ?? null;
  const isMeeting = entry.source === "meeting";
  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  // A fresh search and a rested copy button each time the window opens.
  useEffect(() => {
    if (open) {
      setQuery("");
      setCopied(false);
    }
  }, [open]);

  const needle = query.trim().toLowerCase();
  const visibleSegments = useMemo(
    () =>
      needle
        ? segments.filter((segment) =>
            segment.text.toLowerCase().includes(needle),
          )
        : segments,
    [segments, needle],
  );

  const handleCopy = async () => {
    const copiedOk = await copyToClipboard(transcriptText(segments));
    if (!copiedOk) {
      toast.error(t("settings.history.copyError"));
      return;
    }
    setCopied(true);
    setTimeout(() => setCopied(false), 2000);
  };

  const handleExport = async () => {
    setExporting(true);
    try {
      // The user picks the destination; nothing is uploaded anywhere.
      const path = await save({
        defaultPath: suggestedExportFileName(entry),
        filters: [
          {
            name: t("settings.history.markdownFile"),
            extensions: ["md"],
          },
        ],
      });
      if (!path) return;

      const markdown = buildTranscriptMarkdown(segments, {
        title: entry.title || formattedDate,
        date: formattedDate,
        dateLabel: t("settings.history.date"),
        durationLabel: t("settings.history.duration"),
        durationSeconds,
      });

      const result = await commands.exportTextFile(path, markdown);
      if (result.status === "ok") {
        toast.success(
          t("settings.history.exportSaved", { name: baseName(path) }),
        );
      } else {
        toast.error(t("settings.history.exportFailed"));
      }
    } catch (error) {
      console.error("Failed to export the transcript:", error);
      toast.error(t("settings.history.exportFailed"));
    } finally {
      setExporting(false);
    }
  };

  return (
    <Dialog
      open={open}
      onOpenChange={onOpenChange}
      title={t("settings.history.transcript")}
      description={
        <span className="flex flex-wrap items-center gap-x-3 gap-y-1">
          <span>{formattedDate}</span>
          {isMeeting && <MeetingBadge />}
          {durationSeconds !== null && (
            <span className="tabular-nums">
              {t("settings.history.duration")}{" "}
              {formatSegmentTimestamp(durationSeconds * 1000)}
            </span>
          )}
        </span>
      }
      closeLabel={t("common.close")}
      className="max-w-2xl!"
      initialFocusRef={searchRef}
      footer={
        <>
          <Button
            variant="secondary"
            size="sm"
            onClick={handleCopy}
            disabled={segments.length === 0}
            className="flex items-center gap-2"
          >
            {copied ? (
              <Check className="h-4 w-4" aria-hidden="true" />
            ) : (
              <Copy className="h-4 w-4" aria-hidden="true" />
            )}
            {t("settings.history.copyTranscript")}
          </Button>
          <Button
            variant="primary"
            size="sm"
            onClick={handleExport}
            disabled={exporting || segments.length === 0}
            className="flex items-center gap-2"
          >
            <FileDown className="h-4 w-4" aria-hidden="true" />
            {t("settings.history.exportMarkdown")}
          </Button>
        </>
      }
    >
      <div className="relative">
        <Search
          className="pointer-events-none absolute top-1/2 left-2.5 h-3.5 w-3.5 -translate-y-1/2 text-ink-3"
          aria-hidden="true"
        />
        <Input
          ref={searchRef}
          variant="compact"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          placeholder={t("settings.history.transcriptSearchPlaceholder")}
          aria-label={t("settings.history.transcriptSearchPlaceholder")}
          className="w-full ps-8 pe-7"
        />
        {query && (
          <button
            type="button"
            onClick={() => setQuery("")}
            aria-label={t("common.clear")}
            className="absolute top-1/2 right-1.5 -translate-y-1/2 cursor-pointer rounded p-0.5 text-ink-3 transition-colors hover:text-text"
          >
            <X className="h-3.5 w-3.5" aria-hidden="true" />
          </button>
        )}
      </div>

      <div className="mt-3">
        {visibleSegments.length === 0 ? (
          <p className="py-8 text-center text-sm text-ink-3">
            {needle
              ? t("settings.history.transcriptNoMatches", {
                  query: query.trim(),
                })
              : t("settings.history.transcriptEmpty")}
          </p>
        ) : (
          <ol className="divide-y divide-line">
            {visibleSegments.map((segment, index) => (
              <li
                key={`${segment.t0_ms}-${index}`}
                className="flex gap-3 py-2 first:pt-1 last:pb-1"
              >
                <span className="w-12 shrink-0 pt-0.5 text-xs font-semibold tabular-nums text-ink-3 select-none">
                  {formatSegmentTimestamp(segment.t0_ms)}
                </span>
                <p className="min-w-0 text-sm break-words whitespace-pre-wrap text-text select-text">
                  {segment.text}
                </p>
              </li>
            ))}
          </ol>
        )}
      </div>
    </Dialog>
  );
};
