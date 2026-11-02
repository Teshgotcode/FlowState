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

interface MeetingNoteSectionProps {
  entry: HistoryEntry;
}

/** The meeting note: a draft a local LLM writes over this meeting's
 * transcript, only when the user clicks for it. The note sits above the
 * transcript in the transcript window, marked as a draft the user must check;
 * it is never sent, uploaded or shared anywhere by the app. */
export const MeetingNoteSection: React.FC<MeetingNoteSectionProps> = ({
  entry,
}) => {
  const { t } = useTranslation();
  const [generating, setGenerating] = useState(false);

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

  const handleGenerate = async () => {
    if (generating) return;
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
        <h3 className="text-xs font-medium text-ink-3 uppercase tracking-wide">
          {t("settings.history.meetingNote.title")}
        </h3>
        {generating ? (
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
        )}
      </div>

      {note ? (
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

          <NoteList
            label={t("settings.history.meetingNote.decisions")}
            items={note.decisions}
          />
          <ActionItemList items={note.action_items} />
          <NoteList
            label={t("settings.history.meetingNote.openQuestions")}
            items={note.open_questions}
          />
        </div>
      ) : (
        !generating && (
          <p className="text-sm text-ink-3">
            {t("settings.history.meetingNote.description")}
          </p>
        )
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
