import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import { MeetingRetentionPeriod } from "@/bindings";

interface MeetingRetentionProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * How long finished meetings are kept. This is the meeting history's own
 * setting: the dictation history has its own retention above, and the two
 * never delete each other's entries. A meeting's audio, transcript segments
 * and note are deleted together, permanently, and only ever from this
 * device's disk.
 */
export const MeetingRetentionSelector: React.FC<MeetingRetentionProps> =
  React.memo(({ descriptionMode = "inline", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const selectedPeriod = getSetting("meeting_retention_period") || "forever";

    const handleSelect = async (period: string) => {
      await updateSetting(
        "meeting_retention_period",
        period as MeetingRetentionPeriod,
      );
    };

    const retentionOptions = [
      {
        value: "forever",
        label: t("settings.meeting.retentionForever"),
      },
      { value: "days_7", label: t("settings.meeting.retentionDays7") },
      { value: "days_30", label: t("settings.meeting.retentionDays30") },
      { value: "days_90", label: t("settings.meeting.retentionDays90") },
    ];

    return (
      <SettingContainer
        title={t("settings.meeting.retentionTitle")}
        description={t("settings.meeting.retentionDescription")}
        descriptionMode={descriptionMode}
        grouped={grouped}
        layout="stacked"
      >
        <div className="flex flex-col gap-2">
          <Dropdown
            options={retentionOptions}
            selectedValue={selectedPeriod}
            onSelect={handleSelect}
            placeholder={t("settings.meeting.retentionPlaceholder")}
            disabled={isUpdating("meeting_retention_period")}
          />
          <p className="text-sm text-ink-3">
            {t("settings.meeting.retentionWarning")}
          </p>
        </div>
      </SettingContainer>
    );
  });

MeetingRetentionSelector.displayName = "MeetingRetentionSelector";
