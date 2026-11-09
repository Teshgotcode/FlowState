import React, { useState } from "react";
import { AlertTriangle } from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  commands,
  type HistoryEntry,
  type MeetingActionItem,
  type MeetingNoteFailure,
} from "@/bindings";
import { Button } from "../../ui/Button";
import { Spinner } from "../../ui/Spinner";
import { ExternalSendNotice } from "../../meeting/ExternalSendNotice";
import { isExternalHost } from "../settings-utils";
import { MeetingAskPanel } from "./MeetingAskPanel";
import { useSettings } from "../../../hooks/useSettings";

/** The message each failure reason shows. The three "no local LLM"
 * reasons share one message: the fix is the same. */
const FAILURE_MESSAGE_KEYS: Record<MeetingNoteFailure, string> = {
  not_a_meeting: "settings.history.meetingNote.failedNotMeeting",
  no_transcript: "settings.history.meetingNote.failedNoTranscript",
  no_provider: "settings.history.meetingNote.failedNoLocalLlm",
  no_model: "settings.history.meetingNote.failedNoLocalLlm",
  no_local_llm: "settings.history.meetingNote.failedNoLocalLlm",
  already_generating: "settings.history.meetingNote.failed",
  llm_unreachable: "settings.history.meetingNote.failedUnreachable",
  invalid_response: "settings.history.meetingNote.failedInvalidResponse",
  cancelled: "settings.history.meetingNote.cancelled",
};

/** Acknowledged external hosts: the blocking notice shows once per host. */
const EXTERNAL_ACK_KEY = "flowstate.meetingExternalAck";

/** The host an acknowledgement is keyed on: the URL's host, or the raw
 * string when there is nothing to parse. */
const hostOf = (baseUrl: string): string => {
  try {
    return new URL(baseUrl).host || baseUrl;
  } catch {
    return baseUrl;
  }
};

const readAckedHosts = (): string[] => {
  try {
    const stored: unknown = JSON.parse(
      localStorage.getItem(EXTERNAL_ACK_KEY) ?? "[]",
    );
    return Array.isArray(stored)
      ? stored.filter((host): host is string => typeof host === "string")
      : [];
  } catch {
    return [];
  }
};

const acknowledgeHost = (host: string) => {
  try {
    const acked = readAckedHosts();
    if (!acked.includes(host)) {
      localStorage.setItem(EXTERNAL_ACK_KEY, JSON.stringify([...acked, host]));
    }
  } catch {
    // Storage may be unavailable; the notice just shows again next time.
  }
};

interface MeetingNoteSectionProps {
  entry: HistoryEntry;
  /** Scroll the transcript to the segment starting at this timestamp; the
   * "Ask" tab's quotes click through to their place in the transcript. */
  onJumpToSegment: (t0Ms: number) => void;
}

/** The meeting note: a draft a local LLM writes over this meeting's
 * transcript, only when the user clicks for it, and the "Ask" tab, where the
 * same local LLM answers questions about the meeting with quotes back into
 * the transcript. The note sits above the transcript in the transcript
 * window, marked as a draft the user must check; it is never sent, uploaded
 * or shared anywhere by the app. */
export const MeetingNoteSection: React.FC<MeetingNoteSectionProps> = ({
  entry,
  onJumpToSegment,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const [tab, setTab] = useState<"note" | "ask">("note");
  const [generating, setGenerating] = useState(false);
  const [pendingNotice, setPendingNotice] = useState<{
    providerName: string;
    host: string;
  } | null>(null);

  const note = entry.metadata?.meeting_note ?? null;
  const hasTranscript =
    (entry.metadata?.segments?.length ?? 0) > 0 ||
    entry.transcription_text.trim().length > 0;

  const showFailure = (reason: MeetingNoteFailure, detail: string | null) => {
    if (detail) {
      // The detail is for the log, never for the user: it can quote the
      // endpoint's raw error text.
      console.warn("Meeting note generation failed:", detail);
    }
    if (reason === "cancelled") {
      toast(t("settings.history.meetingNote.cancelled"));
      return;
    }
    if (reason === "already_generating") {
      // The button is disabled while this window generates; this only happens
      // when another window started a run for the same entry.
      console.warn("A meeting note generation is already running");
      return;
    }
    toast.error(t(FAILURE_MESSAGE_KEYS[reason]));
  };

  const runGenerate = async () => {
    setGenerating(true);
    try {
      const result = await commands.generateMeetingNote(entry.id);
      if (result.status === "ok" && result.data.outcome === "failed") {
        showFailure(result.data.reason, result.data.detail);
      }
      // A generated note arrives on the entry itself, through the history
      // update the backend emits; the transcript stays exactly as it was.
    } catch (error) {
      console.error("Failed to generate the meeting note:", error);
      toast.error(t("settings.history.meetingNote.failed"));
    } finally {
      setGenerating(false);
    }
  };

  /** The generate button, gated: an off-machine provider must be
   * acknowledged for its host before the meeting text is sent anywhere. */
  const handleGenerate = async () => {
    if (generating) return;
    const providers = settings?.post_process_providers ?? [];
    const providerId =
      settings?.post_process_provider_id || providers[0]?.id || "openai";
    const provider =
      providers.find((candidate) => candidate.id === providerId) ||
      providers[0];
    const baseUrl = provider?.base_url || "";
    if (isExternalHost(baseUrl)) {
      const host = hostOf(baseUrl);
      if (!readAckedHosts().includes(host)) {
        setPendingNotice({
          providerName: provider?.label || providerId,
          host,
        });
        return;
      }
    }
    await runGenerate();
  };

  const handleNoticeOk = (remember: boolean) => {
    const host = pendingNotice?.host;
    setPendingNotice(null);
    if (remember && host !== undefined) {
      acknowledgeHost(host);
    }
    void runGenerate();
  };

  const handleCancel = async () => {
    try {
      await commands.cancelMeetingNote(entry.id);
    } catch (error) {
      console.error("Failed to cancel the meeting note generation:", error);
    }
  };

  if (!hasTranscript) return null;

  return (
    <section
      aria-label={t("settings.history.meetingNote.title")}
      className="mb-4 rounded-lg border border-line bg-wash p-3 space-y-3"
    >
      <div className="flex items-center justify-between gap-2">
        <div
          role="tablist"
          aria-label={t("settings.history.meetingNote.title")}
          className="flex items-center gap-0.5 rounded-lg border border-line-strong bg-wash p-0.5"
        >
          {(
            [
              ["note", "settings.history.meetingNote.tab"],
              ["ask", "settings.history.meetingAsk.tab"],
            ] as const
          ).map(([id, labelKey]) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={tab === id}
              onClick={() => setTab(id)}
              className={`cursor-pointer rounded-md px-2 py-1 text-xs font-semibold transition-colors ${
                tab === id
                  ? "bg-logo-primary/20 text-text"
                  : "text-ink-3 hover:bg-line"
              }`}
            >
              {t(labelKey)}
            </button>
          ))}
        </div>
        {tab === "note" &&
          (generating ? (
            <div className="flex items-center gap-2">
              <Spinner />
              <span className="text-sm text-ink-3">
                {t("settings.history.meetingNote.generating")}
              </span>
              <Button variant="secondary" size="sm" onClick={handleCancel}>
                {t("settings.history.meetingNote.cancel")}
              </Button>
            </div>
          ) : (
            <Button
              variant={note ? "secondary" : "primary"}
              size="sm"
              onClick={handleGenerate}
              title={t("settings.history.meetingNote.description")}
            >
              {note
                ? t("settings.history.meetingNote.regenerate")
                : t("settings.history.meetingNote.generate")}
            </Button>
          ))}
      </div>

      {tab === "ask" ? (
        <MeetingAskPanel entry={entry} onJumpToSegment={onJumpToSegment} />
      ) : note ? (
        <div className="space-y-3">
          <p className="flex items-center gap-1.5 text-xs text-warning">
            <AlertTriangle
              className="h-3.5 w-3.5 shrink-0"
              aria-hidden="true"
            />
            {t("settings.history.meetingNote.draftLabel", {
              model: note.model,
            })}
          </p>

          <div>
            <h4 className="text-sm font-semibold">
              {t("settings.history.meetingNote.summary")}
            </h4>
            <p className="mt-0.5 text-sm break-words whitespace-pre-wrap text-text select-text">
              {note.summary}
            </p>
          </div>

          {/* Topic sections when the note has them; the fixed buckets are
              what notes written before sections existed fall back to. */}
          {(note.sections ?? []).map((section) => (
            <div key={section.heading}>
              <h4 className="text-sm font-semibold">{section.heading}</h4>
              <ul className="mt-0.5 list-disc space-y-0.5 ps-5 text-sm text-text select-text">
                {(section.bullets ?? []).map((bullet) => (
                  <li key={bullet} className="break-words">
                    {bullet}
                  </li>
                ))}
              </ul>
            </div>
          ))}
          {(note.sections ?? []).length === 0 && (
            <>
              <NoteList
                label={t("settings.history.meetingNote.decisions")}
                items={note.decisions}
              />
              <ActionItemList items={note.action_items} />
              <NoteList
                label={t("settings.history.meetingNote.openQuestions")}
                items={note.open_questions}
              />
            </>
          )}
        </div>
      ) : (
        !generating && (
          <p className="text-sm text-ink-3">
            {t("settings.history.meetingNote.description")}
          </p>
        )
      )}

      {pendingNotice && (
        <ExternalSendNotice
          providerName={pendingNotice.providerName}
          host={pendingNotice.host}
          onOk={handleNoticeOk}
          onCancel={() => setPendingNotice(null)}
        />
      )}
    </section>
  );
};

const noneKey = "settings.history.meetingNote.none";

/** One of the note's plain list fields; an empty list says so, plainly. */
const NoteList: React.FC<{ label: string; items: string[] }> = ({
  label,
  items,
}) => {
  const { t } = useTranslation();
  return (
    <div>
      <h4 className="text-sm font-semibold">{label}</h4>
      {items.length === 0 ? (
        <p className="mt-0.5 text-sm text-ink-3">{t(noneKey)}</p>
      ) : (
        <ul className="mt-0.5 space-y-1">
          {items.map((item, index) => (
            <li
              key={index}
              className="text-sm break-words text-text select-text"
            >
              {item}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};

/** The action items, each with its owner on the line under the task when the
 * transcript named one. */
const ActionItemList: React.FC<{ items: MeetingActionItem[] }> = ({
  items,
}) => {
  const { t } = useTranslation();
  return (
    <div>
      <h4 className="text-sm font-semibold">
        {t("settings.history.meetingNote.actionItems")}
      </h4>
      {items.length === 0 ? (
        <p className="mt-0.5 text-sm text-ink-3">{t(noneKey)}</p>
      ) : (
        <ul className="mt-0.5 space-y-1.5">
          {items.map((item, index) => (
            <li key={index}>
              <p className="text-sm break-words text-text select-text">
                {item.task}
              </p>
              {item.owner?.trim() ? (
                <p className="text-xs text-ink-3">
                  {t("settings.history.meetingNote.responsible", {
                    owner: item.owner,
                  })}
                </p>
              ) : null}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
};
