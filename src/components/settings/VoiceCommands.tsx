import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

/**
 * Spoken punctuation and emoji commands ("komma" → ",", "ny linje" → a line
 * break, "smilefjes" → 🙂). Purely local, and only for languages with a
 * command table (currently Norwegian and English).
 */
export const VoiceCommands: React.FC = React.memo(() => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const enabled = getSetting("voice_commands_enabled") ?? true;

  return (
    <ToggleSwitch
      checked={enabled}
      onChange={(nextEnabled) =>
        updateSetting("voice_commands_enabled", nextEnabled)
      }
      isUpdating={isUpdating("voice_commands_enabled")}
      label={t("settings.advanced.voiceCommands.title")}
      description={t("settings.advanced.voiceCommands.description")}
      descriptionMode="tooltip"
      grouped={true}
    />
  );
});
