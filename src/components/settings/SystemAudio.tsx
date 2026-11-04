import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { ToggleSwitch } from "../ui/ToggleSwitch";

/**
 * Meetings capture two tracks (S-5): the microphone, and — on macOS 14.4 or
 * newer, where the CoreAudio process tap exists — whatever the machine
 * itself is playing, so the far end of a Zoom or Teams call lands in the
 * transcript too, merged in by timestamp. The tap is a local CoreAudio
 * feature: no permission dialog, no virtual driver, and both tracks stay
 * on the machine. An older Mac gets the switch dimmed with a line saying
 * why — never an error, never a crash; its meetings are mic-only.
 */
export const SystemAudioSetting: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [supported, setSupported] = useState<boolean | null>(null);

  useEffect(() => {
    let mounted = true;
    commands
      .systemAudioSupported()
      .then((value) => {
        if (mounted) setSupported(value);
      })
      .catch((error) => {
        console.error("Failed to check system audio support:", error);
        if (mounted) setSupported(false);
      });
    return () => {
      mounted = false;
    };
  }, []);

  if (supported === null) return null;

  const enabled = getSetting("record_system_audio") ?? true;

  return (
    <ToggleSwitch
      checked={supported && enabled}
      onChange={async (checked) => {
        try {
          await updateSetting("record_system_audio", checked);
        } catch (error) {
          console.error("Failed to update the system audio setting:", error);
        }
      }}
      disabled={!supported}
      label={t("settings.meeting.systemAudioTitle")}
      description={
        supported
          ? t("settings.meeting.systemAudioDescription")
          : t("settings.meeting.systemAudioUnsupported")
      }
      descriptionMode="inline"
      grouped={true}
    />
  );
};
