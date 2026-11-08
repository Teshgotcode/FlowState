import React, { useEffect, useRef, useState } from "react";
import { AlertTriangle, ArrowUp, X } from "lucide-react";
import { useTranslation } from "react-i18next";
import {
  commands,
  type HistoryEntry,
  type MeetingAnswer,
  type MeetingAskFailure,
} from "@/bindings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Spinner } from "../../ui/Spinner";
import { formatSegmentTimestamp } from "./transcriptExport";

/** The message each failure reason shows. The three "no local LLM" reasons
 * share one message: the fix is the same. */
export const FAILURE_MESSAGE_KEYS: Record<MeetingAskFailure, string> = {
  not_a_meeting: "settings.history.meetingAsk.failedNotMeeting",
  no_transcript: "settings.history.meetingAsk.failedNoTranscript",
  no_provider: "settings.history.meetingAsk.failedNoLocalLlm",
  no_model: "settings.history.meetingAsk.failedNoLocalLlm",
  no_local_llm: "settings.history.meetingAsk.failedNoLocalLlm",
  empty_question: "settings.history.meetingAsk.failedEmptyQuestion",
  already_asking: "settings.history.meetingAsk.failed",
  llm_unreachable: "settings.history.meetingAsk.failedUnreachable",
  invalid_response: "settings.history.meetingAsk.failedInvalidResponse",
  cancelled: "settings.history.meetingAsk.cancelled",
};

/** One question and what came of it: the answer with its quotes, or the
 * reason there is none. Never saved anywhere — the transcript stands
 * untouched, and the conversation lives only in this window. */
interface AskTurn {
  question: string;
  answer: MeetingAnswer | null;
  failure: MeetingAskFailure | null;
  pending: boolean;
}

interface MeetingAskPanelProps {
  entry: HistoryEntry;
  /** Scroll the transcript to the segment starting at this timestamp. */
  onJumpToSegment: (t0Ms: number) => void;
}

/** The "Ask" tab of the meeting note: ask the local LLM a question about this
 * meeting, answered from the meeting's own transcript. Every answer is
 * marked as generated and must be checked against the transcript; the quotes
 * under it are the exact transcript parts it builds on, and clicking one
 * jumps to that timestamp. Nothing is sent anywhere but the configured local
 * endpoint, and nothing is written back to the meeting. */
export const MeetingAskPanel: React.FC<MeetingAskPanelProps> = ({
  entry,
  onJumpToSegment,
}) => {
  const { t } = useTranslation();
  const [turns, setTurns] = useState<AskTurn[]>([]);
  const [question, setQuestion] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);
  const asking = turns.some((turn) => turn.pending);

  // The newest turn comes into view as it appears, like any chat.
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth", block: "nearest" });
  }, [turns]);

  const patchTurn = (index: number, patch: Partial<AskTurn>) => {
    setTurns((previous) =>
      previous.map((turn, position) =>
        position === index ? { ...turn, ...patch } : turn,
      ),
    );
  };

  const handleAsk = async () => {
    const trimmed = question.trim();
    if (!trimmed || asking) return;
    const index = turns.length;
    setQuestion("");
    patchTurn(index, {
      question: trimmed,
      answer: null,
      failure: null,
      pending: true,
    });
    try {
      const result = await commands.askMeetingTranscript(entry.id, trimmed);
      if (result.status === "ok" && result.data.outcome === "answered") {
        patchTurn(index, { pending: false, answer: result.data.answer });
      } else if (result.status === "ok" && result.data.outcome === "failed") {
        // The detail is for the log, never for the user: it can quote the
        // endpoint's raw error text.
        if (result.data.detail) {
          console.warn("Asking about the meeting failed:", result.data.detail);
        }
        patchTurn(index, { pending: false, failure: result.data.reason });
      } else {
        patchTurn(index, { pending: false, failure: "llm_unreachable" });
      }
    } catch (error) {
      console.error("Failed to ask about the meeting:", error);
      patchTurn(index, { pending: false, failure: "llm_unreachable" });
    }
  };

  const handleCancel = async () => {
    try {
      await commands.cancelMeetingAsk(entry.id);
    } catch (error) {
      console.error("Failed to cancel the question:", error);
    }
  };

  const handleKeyDown = (event: React.KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      void handleAsk();
    }
  };

  return (
    <div className="space-y-3">
      {turns.length === 0 && (
        <p className="text-sm text-ink-3">
          {t("settings.history.meetingAsk.description")}
        </p>
      )}

      {turns.map((turn, index) => (
        <div key={index} className="space-y-2">
          <p className="ms-auto w-fit max-w-[85%] rounded-lg bg-line px-2.5 py-1.5 text-sm break-words whitespace-pre-wrap text-text select-text">
            {turn.question}
          </p>

          {turn.pending ? (
            <div className="flex items-center gap-2">
              <Spinner />
              <span className="text-sm text-ink-3">
                {t("settings.history.meetingAsk.searching")}
              </span>
              <Button variant="secondary" size="sm" onClick={handleCancel}>
                <X className="h-4 w-4" aria-hidden="true" />
                {t("settings.history.meetingNote.cancel")}
              </Button>
            </div>
          ) : turn.answer ? (
            <div className="space-y-2">
              <p className="flex items-center gap-1.5 text-xs text-warning">
                <AlertTriangle
                  className="h-3.5 w-3.5 shrink-0"
                  aria-hidden="true"
                />
                {t("settings.history.meetingAsk.generatedLabel", {
                  model: turn.answer.model,
                })}
              </p>
              {turn.answer.found ? (
                <p className="text-sm break-words whitespace-pre-wrap text-text select-text">
                  {turn.answer.answer}
                </p>
              ) : (
                <p className="text-sm text-ink-3">
                  {turn.answer.answer.trim() ||
                    t("settings.history.meetingAsk.notFound")}
                </p>
              )}
              {turn.answer.quotes.length > 0 && (
                <ul className="space-y-1">
                  {turn.answer.quotes.map((quote, quoteIndex) =>
                    quote.t0_ms === null ? (
                      <li
                        key={quoteIndex}
                        className="border-s-2 border-line ps-2 text-xs text-ink-3"
                      >
                        {quote.text}
                      </li>
                    ) : (
                      <li key={quoteIndex}>
                        <button
                          type="button"
                          onClick={() => onJumpToSegment(quote.t0_ms!)}
                          title={t(
                            "settings.history.meetingAsk.showInTranscript",
                          )}
                          className="w-full cursor-pointer border-s-2 border-logo-primary ps-2 text-start text-xs text-ink-3 transition-colors hover:border-logo-primary hover:text-text"
                        >
                          <span className="me-1 font-semibold tabular-nums">
                            {formatSegmentTimestamp(quote.t0_ms)}
                          </span>
                          {quote.text}
                        </button>
                      </li>
                    ),
                  )}
                </ul>
              )}
            </div>
          ) : (
            <p className="text-sm text-error">
              {t(FAILURE_MESSAGE_KEYS[turn.failure ?? "llm_unreachable"])}
            </p>
          )}
        </div>
      ))}

      <div ref={bottomRef} />

      <div className="flex items-center gap-2">
        <Input
          variant="compact"
          value={question}
          onChange={(event) => setQuestion(event.target.value)}
          onKeyDown={handleKeyDown}
          placeholder={t("settings.history.meetingAsk.placeholder")}
          aria-label={t("settings.history.meetingAsk.placeholder")}
          disabled={asking}
          className="min-w-0 flex-1"
        />
        <Button
          variant="primary"
          size="sm"
          onClick={handleAsk}
          disabled={asking || !question.trim()}
          aria-label={t("settings.history.meetingAsk.ask")}
          className="shrink-0"
        >
          <ArrowUp className="h-4 w-4" aria-hidden="true" />
          {t("settings.history.meetingAsk.ask")}
        </Button>
      </div>
    </div>
  );
};
