import React, { useMemo } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";

/**
 * How long a meeting may record before the floating bar shows its discreet
 * "still going?" prompt with a Stop button (S-24) — the catch for a session
 * forgotten in the background. "Off" never prompts.
 */
export const MeetingReminderSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();

  const options = useMemo(
    () => [
      { value: "off", label: t("settings.meeting.reminderOff") },
      { value: "30", label: t("settings.meeting.reminderMin30") },
      { value: "60", label: t("settings.meeting.reminderHour1") },
      { value: "120", label: t("settings.meeting.reminderHours2") },
    ],
    [t],
  );

  const minutes = getSetting("meeting_reminder_minutes");
  const currentValue =
    typeof minutes === "number" && minutes > 0 ? String(minutes) : "off";

  return (
    <SettingContainer
      title={t("settings.meeting.reminderTitle")}
      description={t("settings.meeting.reminderDescription")}
      descriptionMode="tooltip"
      grouped={true}
    >
      <Dropdown
        options={options}
        selectedValue={currentValue}
        onSelect={async (value) => {
          const next = value === "off" ? null : Number(value);
          try {
            await commands.setMeetingReminderMinutes(next);
            await updateSetting("meeting_reminder_minutes", next);
          } catch (error) {
            console.error("Failed to update the meeting reminder:", error);
          }
        }}
        disabled={false}
      />
    </SettingContainer>
  );
};
