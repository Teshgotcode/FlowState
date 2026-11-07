import React from "react";
import { useTranslation } from "react-i18next";
import { PageHeader } from "@/components/hub/Primitives";

/** Everything that used to be its own sidebar row lives here: the reference
 *  app has one Settings entry, with the sections as tabs inside it. The page
 *  bodies stay with App, which owns the props a few of them need. */
export const SETTINGS_TABS = [
  "general",
  "models",
  "postprocessing",
  "integrations",
  "sync",
  "advanced",
  "about",
  "debug",
] as const;

export type SettingsTabId = (typeof SETTINGS_TABS)[number];

export const SettingsHub: React.FC<{
  active: string;
  onSelect: (id: SettingsTabId) => void;
  render: (id: string) => React.ReactNode;
}> = ({ active, onSelect, render }) => {
  const { t } = useTranslation();
  return (
    <>
      <div className="w-full">
        <PageHeader title={t("sidebar.settings")} />
        <nav className="mb-[20px] flex flex-wrap gap-[6px]">
          {SETTINGS_TABS.map((tab) => (
            <button
              key={tab}
              type="button"
              onClick={() => onSelect(tab)}
              className={`cursor-pointer rounded-[6px] px-[10px] py-[6px] text-[13px] transition-colors ${
                tab === active
                  ? "bg-surface text-text shadow-[0_1px_2px_rgba(0,0,0,.08)]"
                  : "text-text/70 hover:bg-wash hover:text-text"
              }`}
            >
              {t(`settings.tab.${tab}`)}
            </button>
          ))}
        </nav>
      </div>
      {render(active)}
    </>
  );
};
