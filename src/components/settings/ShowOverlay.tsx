import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";
import type { OverlayPosition, OverlayStyle } from "@/bindings";

interface ShowOverlayProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const ShowOverlay: React.FC<ShowOverlayProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const styleOptions = [
      {
        value: "none",
        label: t("settings.advanced.overlay.style.options.none"),
      },
      {
        value: "minimal",
        label: t("settings.advanced.overlay.style.options.minimal"),
      },
      {
        value: "live",
        label: t("settings.advanced.overlay.style.options.live"),
      },
    ];

    const positionOptions = [
      {
        value: "bottom",
        label: t("settings.advanced.overlay.position.options.bottom"),
      },
      {
        value: "top",
        label: t("settings.advanced.overlay.position.options.top"),
      },
      {
        value: "left",
        label: t("settings.advanced.overlay.position.options.left"),
      },
      {
        value: "right",
        label: t("settings.advanced.overlay.position.options.right"),
      },
    ];

    // Minimal (Wispr's pill, no live text) is the default; a stored style wins.
    const selectedStyle = (getSetting("overlay_style") ||
      "minimal") as OverlayStyle;
    // All four edges are selectable; an unset store falls back to "bottom"
    // (a legacy "none" was already folded onto Bottom when the store loaded).
    const selectedPosition: OverlayPosition =
      getSetting("overlay_position") ?? "bottom";

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.overlay.style.title")}
          description={t("settings.advanced.overlay.style.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            options={styleOptions}
            selectedValue={selectedStyle}
            onSelect={(value) =>
              updateSetting("overlay_style", value as OverlayStyle)
            }
            disabled={isUpdating("overlay_style")}
          />
        </SettingContainer>

        {selectedStyle !== "none" && (
          <SettingContainer
            title={t("settings.advanced.overlay.position.title")}
            description={t("settings.advanced.overlay.position.description")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            <Dropdown
              options={positionOptions}
              selectedValue={selectedPosition}
              onSelect={(value) =>
                updateSetting("overlay_position", value as OverlayPosition)
              }
              disabled={isUpdating("overlay_position")}
            />
          </SettingContainer>
        )}

        <ToggleSwitch
          checked={getSetting("show_bar_at_all_times") ?? true}
          onChange={(enabled) =>
            updateSetting("show_bar_at_all_times", enabled)
          }
          isUpdating={isUpdating("show_bar_at_all_times")}
          label={t("settings.advanced.overlay.showBar.label")}
          description={t("settings.advanced.overlay.showBar.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        <ToggleSwitch
          checked={getSetting("scratchpad_in_flow_bar") ?? false}
          onChange={(enabled) =>
            updateSetting("scratchpad_in_flow_bar", enabled)
          }
          isUpdating={isUpdating("scratchpad_in_flow_bar")}
          label={t("settings.advanced.overlay.scratchpadInBar.label")}
          description={t(
            "settings.advanced.overlay.scratchpadInBar.description",
          )}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        <ToggleSwitch
          checked={getSetting("flow_bar_style_chip") ?? false}
          onChange={(enabled) => updateSetting("flow_bar_style_chip", enabled)}
          isUpdating={isUpdating("flow_bar_style_chip")}
          label={t("settings.advanced.overlay.styleChip.label")}
          description={t("settings.advanced.overlay.styleChip.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
      </>
    );
  },
);
