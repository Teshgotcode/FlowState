import React from "react";
import { useTranslation } from "react-i18next";
import { useMeetingStore } from "@/stores/meetingStore";
import { useMeetingClock } from "@/hooks/useMeetingClock";
import { formatDurationClock } from "@/lib/utils/format";

/** The running meeting in the footer — a pulsing dot, its label and the
 * clock so far — so the recording stays visible whatever section the window
 * shows. Passive by design: its start, stop and cancel live in General. */
const MeetingIndicator: React.FC = () => {
  const { t } = useTranslation();
  const status = useMeetingStore((s) => s.status);
  const isRecording = status.state === "recording";
  const elapsed = useMeetingClock(
    isRecording ? status.started_at : 0,
    isRecording,
  );

  if (!isRecording) return null;

  return (
    <span className="flex items-center gap-1.5 text-ink-3 tabular-nums">
      <span
        className="h-1.5 w-1.5 rounded-full bg-error animate-pulse"
        aria-hidden="true"
      />
      {t("settings.meeting.recording")}
      <span>{formatDurationClock(elapsed)}</span>
    </span>
  );
};

export default MeetingIndicator;
