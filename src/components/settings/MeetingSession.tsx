import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../ui/Button";
import { Dialog } from "../ui/Dialog";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { useMeetingStore } from "../../stores/meetingStore";
import { MeetingLiveTranscript } from "./MeetingLiveTranscript";
import { MeetingReminderSetting } from "./MeetingReminder";
import { SystemAudioSetting } from "./SystemAudio";
import { MeetingRetentionSelector } from "./MeetingRetention";
import { useMeetingClock } from "../../hooks/useMeetingClock";
import { formatDurationClock } from "../../lib/utils/format";

/**
 * The meeting session in the main window: a recording that starts here, runs
 * with a live clock until it is stopped (saved to History with its
 * transcript) or cancelled (deleted, nothing saved).
 */
export const MeetingSession: React.FC = () => {
  const { t } = useTranslation();
  const status = useMeetingStore((s) => s.status);
  const start = useMeetingStore((s) => s.start);
  const stop = useMeetingStore((s) => s.stop);
  const cancel = useMeetingStore((s) => s.cancel);
  const [confirmingCancel, setConfirmingCancel] = useState(false);
  // One command in flight at a time: a second click while the first runs
  // would only earn the backend's "already/not recording" refusal.
  const [busy, setBusy] = useState(false);

  const isRecording = status.state === "recording";
  const elapsed = useMeetingClock(
    isRecording ? status.started_at : 0,
    isRecording,
  );

  // The session can end elsewhere (a stop finishing, app logic); a dialog
  // asking to cancel a finished meeting would only confuse.
  useEffect(() => {
    if (!isRecording) setConfirmingCancel(false);
  }, [isRecording]);

  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    try {
      await action();
    } finally {
      setBusy(false);
    }
  };

  return (
    <SettingsGroup title={t("settings.meeting.title")}>
      {isRecording ? (
        <div>
          <div className="flex items-center justify-between min-h-12 px-4 p-2">
            <div className="flex items-center gap-2">
              <span
                className="h-2 w-2 rounded-full bg-error animate-pulse"
                aria-hidden="true"
              />
              <span className="text-sm font-medium">
                {t("settings.meeting.recording")}
              </span>
              <span className="text-sm text-ink-3 tabular-nums">
                {formatDurationClock(elapsed)}
              </span>
            </div>
            <div className="flex items-center gap-2">
              <Button
                variant="danger-ghost"
                size="sm"
                onClick={() => setConfirmingCancel(true)}
                disabled={busy}
              >
                {t("settings.meeting.cancel")}
              </Button>
              <Button
                variant="primary"
                size="sm"
                onClick={() => run(stop)}
                disabled={busy}
              >
                {t("settings.meeting.stop")}
              </Button>
            </div>
          </div>
          <MeetingLiveTranscript />
        </div>
      ) : (
        <SettingContainer
          title={t("settings.meeting.sessionTitle")}
          description={t("settings.meeting.description")}
          descriptionMode="tooltip"
          grouped={true}
        >
          <Button
            variant="primary"
            size="sm"
            onClick={() => run(start)}
            disabled={busy}
          >
            {t("settings.meeting.start")}
          </Button>
        </SettingContainer>
      )}

      <MeetingReminderSetting />
      <SystemAudioSetting />
      <MeetingRetentionSelector />

      <Dialog
        open={confirmingCancel}
        onOpenChange={setConfirmingCancel}
        title={t("settings.meeting.cancelTitle")}
        closeLabel={t("common.close")}
        contentFades={false}
        footer={
          <>
            <Button
              variant="secondary"
              size="sm"
              onClick={() => setConfirmingCancel(false)}
            >
              {t("settings.meeting.cancelKeep")}
            </Button>
            <Button
              variant="danger"
              size="sm"
              onClick={() => {
                setConfirmingCancel(false);
                run(cancel);
              }}
            >
              {t("settings.meeting.cancelConfirm")}
            </Button>
          </>
        }
      >
        <p className="text-sm text-ink-2">
          {t("settings.meeting.cancelDescription")}
        </p>
      </Dialog>
    </SettingsGroup>
  );
};
