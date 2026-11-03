import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { MousePttButton } from "@/bindings";

interface MousePttButtonProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Which mouse button (if any) acts as an extra push-to-talk trigger. It mirrors
 * the keyboard transcribe shortcut, including the activation mode chosen for
 * it: hold to record, release to transcribe.
 */
export const MousePttButtonSetting: React.FC<MousePttButtonProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const options = [
      {
        value: "off",
        label: t("settings.general.mousePtt.options.off"),
      },
      {
        value: "middle",
        label: t("settings.general.mousePtt.options.middle"),
      },
      {
        value: "back",
        label: t("settings.general.mousePtt.options.back"),
      },
      {
        value: "forward",
        label: t("settings.general.mousePtt.options.forward"),
      },
    ];

    const selected = getSetting("mouse_ptt_button") ?? "off";

    return (
      <SettingContainer
        title={t("settings.general.mousePtt.title")}
        description={t("settings.general.mousePtt.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={options}
          menuClassName="right-0 w-80 max-w-[calc(100vw-2rem)]"
          selectedValue={selected}
          onSelect={(value) =>
            updateSetting(
              "mouse_ptt_button",
              (value === "off" ? null : value) as MousePttButton | null,
            )
          }
          disabled={isUpdating("mouse_ptt_button")}
        />
      </SettingContainer>
    );
  },
);
