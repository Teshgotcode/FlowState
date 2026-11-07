import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingContainer } from "../ui/SettingContainer";
import {
  ACCENTS,
  ACCENT_OPTIONS,
  applyAccent,
  getStoredAccent,
  getStoredHue,
  type AccentId,
} from "@/lib/utils/accent";

interface AccentSelectorProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/** Five pastels plus a hue slider. Stored locally: it is a per-device choice,
 *  like the theme, so it never leaves the machine or the settings payload. */
export const AccentSelector: React.FC<AccentSelectorProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const [accent, setAccent] = useState<AccentId>(getStoredAccent);
    const [hue, setHue] = useState<number>(getStoredHue);

    const pick = (id: AccentId) => {
      setAccent(id);
      applyAccent(id, hue);
    };

    const tune = (value: number) => {
      setHue(value);
      setAccent("egen");
      applyAccent("egen", value);
    };

    const swatch = (id: AccentId) => {
      const color =
        id === "egen" ? `hsl(${hue} 72% 78%)` : ACCENTS[id].light.accent;
      const selected = accent === id;
      return (
        <button
          key={id}
          type="button"
          onClick={() => pick(id)}
          title={t(`theme.accent.${id}`)}
          aria-label={t(`theme.accent.${id}`)}
          aria-pressed={selected}
          className={`h-7 w-7 rounded-full border border-line-strong/60 transition-transform hover:scale-105 ${
            selected
              ? "ring-2 ring-logo-primary ring-offset-2 ring-offset-surface"
              : ""
          }`}
          style={{ backgroundColor: color }}
        />
      );
    };

    return (
      <SettingContainer
        title={t("theme.accent.title")}
        description={t("theme.accent.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex flex-col items-end gap-2">
          <div className="flex items-center gap-2">
            {ACCENT_OPTIONS.map(swatch)}
          </div>
          {accent === "egen" && (
            <label className="flex items-center gap-2 text-xs text-ink-2">
              {t("theme.accent.hue")}
              <input
                type="range"
                min={0}
                max={360}
                value={hue}
                onChange={(e) => tune(Number(e.target.value))}
                className="h-1 w-40 cursor-pointer accent-logo-primary"
              />
            </label>
          )}
        </div>
      </SettingContainer>
    );
  },
);

AccentSelector.displayName = "AccentSelector";
