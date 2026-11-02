import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { ArrowDown } from "lucide-react";
import { useMeetingStore } from "../../stores/meetingStore";
import { formatSegmentTimestamp } from "./history/transcriptExport";

/**
 * The meeting's live transcript: every segment the backend's running decode
 * has finished, in the order it was spoken, arriving while the meeting still
 * records. The list follows the newest segment while the reader sits at the
 * bottom; scrolling up pauses the following (a line can be read while the
 * meeting goes on) and the "jump to bottom" button resumes it.
 *
 * The panel is plain rendering over store state: the recording and the audio
 * upload live in the app's own threads and never wait on it.
 */
export const MeetingLiveTranscript: React.FC = () => {
  const { t } = useTranslation();
  const segments = useMeetingStore((s) => s.segments);
  const scrollRef = useRef<HTMLDivElement>(null);
  // Whether the panel keeps following the newest segment. Derived from the
  // scroll position itself, so only a manual scroll up pauses it.
  const [following, setFollowing] = useState(true);

  const onScroll = () => {
    const el = scrollRef.current;
    if (!el) return;
    const atBottom = el.scrollHeight - el.scrollTop - el.clientHeight < 24;
    setFollowing(atBottom);
  };

  const jumpToBottom = () => {
    const el = scrollRef.current;
    if (!el) return;
    el.scrollTop = el.scrollHeight;
    setFollowing(true);
  };

  // Follow the newest segment while reading at the bottom. Set directly, not
  // smooth: the newest line must already sit at the bottom by the next paint,
  // not animate in behind the ones after it.
  useEffect(() => {
    if (!following) return;
    const el = scrollRef.current;
    if (el) el.scrollTop = el.scrollHeight;
  }, [segments, following]);

  return (
    <div className="px-4 pb-3">
      <div
        ref={scrollRef}
        onScroll={onScroll}
        role="log"
        aria-live="polite"
        aria-label={t("settings.meeting.liveTranscript")}
        className="relative h-48 overflow-y-auto rounded-lg border border-line bg-wash px-3 py-2"
      >
        {segments.length === 0 ? (
          <p className="py-6 text-center text-xs text-ink-3">
            {t("settings.meeting.liveEmpty")}
          </p>
        ) : (
          <ol className="divide-y divide-line">
            {segments.map((segment) => (
              <li
                key={segment.index}
                className="flex gap-3 py-1.5 first:pt-1 last:pb-1"
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
      {!following && segments.length > 0 && (
        <button
          type="button"
          onClick={jumpToBottom}
          className="mt-2 flex w-full cursor-pointer items-center justify-center gap-1.5 rounded-md border border-line bg-background px-2 py-1 text-xs font-medium text-ink-2 transition-colors hover:text-text"
        >
          <ArrowDown className="h-3.5 w-3.5" aria-hidden="true" />
          {t("settings.meeting.jumpToBottom")}
        </button>
      )}
    </div>
  );
};
