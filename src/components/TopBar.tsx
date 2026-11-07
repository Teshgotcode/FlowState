import React from "react";
import { useTranslation } from "react-i18next";
import { Moon, PanelLeft, Sun, UserRound } from "lucide-react";
import { useSettings } from "@/hooks/useSettings";
import { applyTheme } from "@/lib/utils/theme";
import type { Theme } from "@/bindings";

interface TopBarProps {
  onOpenSettings: () => void;
  onToggleSidebar: () => void;
}

/** The 44px strip: sidebar toggle on the left, theme and account on the right.
 *  It is also the window's drag handle, since the window has no title bar. */
export const TopBar: React.FC<TopBarProps> = ({
  onOpenSettings,
  onToggleSidebar,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const isDark = settings?.theme === "dark";

  const toggleTheme = () => {
    const next: Theme = isDark ? "light" : "dark";
    applyTheme(next);
    updateSetting("theme", next);
  };

  const button =
    "flex h-[28px] w-[28px] shrink-0 cursor-pointer items-center justify-center rounded-[6px] text-text/80 transition-colors hover:bg-wash hover:text-text";

  return (
    <div
      data-tauri-drag-region
      className="flex h-[44px] shrink-0 items-center justify-between pe-[8px] ps-[86px]"
    >
      <button
        type="button"
        onClick={onToggleSidebar}
        aria-label={t("topbar.toggleSidebar")}
        title={t("topbar.toggleSidebar")}
        className={button}
      >
        <PanelLeft size={17} strokeWidth={1.8} />
      </button>
      <div className="flex items-center gap-[8px]">
        <button
          type="button"
          onClick={toggleTheme}
          aria-label={t("theme.title")}
          title={t("theme.title")}
          className={button}
        >
          {isDark ? (
            <Moon size={17} strokeWidth={1.8} />
          ) : (
            <Sun size={17} strokeWidth={1.8} />
          )}
        </button>
        <button
          type="button"
          onClick={onOpenSettings}
          aria-label={t("sidebar.about")}
          title={t("sidebar.about")}
          className={button}
        >
          <UserRound size={17} strokeWidth={1.8} />
        </button>
      </div>
    </div>
  );
};
