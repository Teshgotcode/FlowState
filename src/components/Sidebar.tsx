import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import {
  AudioLines,
  BookOpen,
  ChartNoAxesColumn,
  CircleHelp,
  Keyboard,
  Mic,
  Moon,
  NotebookPen,
  Paintbrush,
  Scissors,
  Settings2,
  Sparkles,
  SquarePen,
  Sun,
  X,
} from "lucide-react";
import { commands } from "@/bindings";
import FlowStateMark from "./icons/FlowStateMark";
import { useSettings } from "../hooks/useSettings";
import { useModelStore } from "../stores/modelStore";
import { applyTheme } from "../lib/utils/theme";
export type SidebarSection = keyof typeof SECTIONS_CONFIG;

interface IconProps {
  width?: number | string;
  height?: number | string;
  size?: number | string;
  className?: string;
  [key: string]: any;
}

/** Three bands: the three daily screens, the library, the app's own settings. */
type SectionGroup = "daily" | "system";

interface SectionConfig {
  labelKey: string;
  icon: React.ComponentType<IconProps>;
  /** `null` when App renders the section itself (Debug, with its handlers). */
  component: React.ComponentType | null;
  enabled: (settings: any) => boolean;
  group: SectionGroup;
  /** Rendered inside Settings instead of as its own sidebar row. */
  hidden?: boolean;
}

export const SECTIONS_CONFIG = {
  home: {
    labelKey: "sidebar.home",
    icon: AudioLines,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  notetaker: {
    labelKey: "sidebar.notetaker",
    icon: Mic,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  insights: {
    labelKey: "sidebar.insights",
    icon: ChartNoAxesColumn,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  dictionary: {
    labelKey: "sidebar.dictionary",
    icon: BookOpen,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  snippets: {
    labelKey: "sidebar.snippets",
    icon: Scissors,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  style: {
    labelKey: "sidebar.style",
    icon: Paintbrush,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  transforms: {
    labelKey: "sidebar.transforms",
    icon: Sparkles,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  scratchpad: {
    labelKey: "sidebar.scratchpad",
    icon: SquarePen,
    component: null,
    enabled: () => true,
    group: "daily",
  },
  general: {
    labelKey: "sidebar.settings",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
  },
  help: {
    labelKey: "sidebar.help",
    icon: CircleHelp,
    component: null,
    enabled: () => true,
    group: "system",
  },
  // Settings sections: reachable through Settings' own tabs, not the sidebar.
  models: {
    labelKey: "sidebar.models",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  postprocessing: {
    labelKey: "sidebar.postProcessing",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  integrations: {
    labelKey: "sidebar.integrations",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  sync: {
    labelKey: "sidebar.sync",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  advanced: {
    labelKey: "sidebar.advanced",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  about: {
    labelKey: "sidebar.about",
    icon: Settings2,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  history: {
    labelKey: "sidebar.history",
    icon: AudioLines,
    component: null,
    enabled: () => true,
    group: "system",
    hidden: true,
  },
  debug: {
    labelKey: "sidebar.debug",
    icon: Settings2,
    component: null,
    enabled: (settings) => settings?.debug_mode ?? false,
    group: "system",
    hidden: true,
  },
} as const satisfies Record<string, SectionConfig>;

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

/**
 * The rail: brand, the working screens, the "Kom i gang" card until it is
 * dismissed, and a foot that says what the app is doing right now.
 */
export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings } = useSettings();
  const { models, currentModel } = useModelStore();
  const modelName = models.find((model) => model.id === currentModel)?.name;
  // "System" leaves data-theme unset, so ask the OS which one is showing.
  const isDarkNow = () => {
    const attr = document.documentElement.dataset.theme;
    if (attr === "dark") return true;
    if (attr === "light") return false;
    return window.matchMedia("(prefers-color-scheme: dark)").matches;
  };
  const [dark, setDark] = useState(isDarkNow);
  const toggleTheme = () => {
    const next = isDarkNow() ? "light" : "dark";
    applyTheme(next);
    setDark(next === "dark");
  };
  const [promoHidden, setPromoHidden] = useState(() => {
    try {
      return localStorage.getItem("flowstate.sidebarPromoDismissed") === "1";
    } catch {
      return false;
    }
  });
  const dismissPromo = () => {
    setPromoHidden(true);
    try {
      localStorage.setItem("flowstate.sidebarPromoDismissed", "1");
    } catch {
      // the card just comes back next launch
    }
  };

  const visible = Object.entries(SECTIONS_CONFIG)
    .filter(
      ([_, config]) =>
        !(config as { hidden?: boolean }).hidden && config.enabled(settings),
    )
    .map(([id, config]) => ({ id: id as SidebarSection, ...config }));
  const daily = visible.filter((section) => section.group === "daily");
  const system = visible.filter((section) => section.group === "system");

  const renderRow = (section: (typeof visible)[number]) => {
    const Icon = section.icon;
    const isActive = activeSection === section.id;
    return (
      <button
        key={section.id}
        type="button"
        aria-current={isActive ? "page" : undefined}
        onClick={() => onSectionChange(section.id)}
        className={`relative flex h-[30px] cursor-pointer items-center gap-[9px] rounded-[6px] px-[9px] text-start transition-colors ${
          isActive
            ? "bg-surface font-[600] text-text shadow-[0_1px_3px_rgba(20,22,28,0.10)]"
            : "text-ink-2 hover:bg-wash hover:text-text"
        }`}
      >
        {isActive && (
          <span className="absolute start-[3px] top-[8px] bottom-[8px] w-[2.5px] rounded-full bg-logo-primary" />
        )}
        <Icon
          size={15}
          strokeWidth={1.8}
          className={`shrink-0 ${isActive ? "text-logo-primary" : "opacity-70"}`}
        />
        <span className="min-w-0 flex-1 truncate text-[13px] leading-[20px] tracking-[-0.005em]">
          {t(section.labelKey)}
        </span>
      </button>
    );
  };

  return (
    <nav
      aria-label={t("sidebar.mainNav")}
      data-tauri-drag-region
      className="flex h-full w-[210px] shrink-0 flex-col border-e border-line bg-surface-soft px-[10px] pt-[38px] pb-[12px]"
    >
      <div className="flex items-center gap-[8px] px-[6px] pb-[22px]">
        <FlowStateMark width={18} height={19} />
        {/* eslint-disable-next-line i18next/no-literal-string -- the wordmark */}
        <span className="font-ui text-[13.5px] font-[650] tracking-[-0.01em] text-text">
          FlowState
        </span>
      </div>

      <div className="flex flex-col gap-[2px]">{daily.map(renderRow)}</div>

      <div className="mt-auto flex flex-col gap-[2px]">
        {!promoHidden && (
          <div className="mb-[10px] rounded-[10px] border border-line bg-surface p-[12px]">
            <div className="flex items-center justify-between pe-[2px]">
              <span className="text-[12px] font-semibold text-text">
                {t("sidebar.promoTitle")}
              </span>
              <button
                type="button"
                onClick={dismissPromo}
                aria-label={t("home.dismiss")}
                title={t("home.dismiss")}
                className="flex h-[20px] w-[20px] shrink-0 cursor-pointer items-center justify-center rounded-[5px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
              >
                <X size={12} />
              </button>
            </div>
            <div className="mt-[6px] flex flex-col gap-[2px]">
              <button
                type="button"
                onClick={() => onSectionChange("general")}
                className="flex cursor-pointer items-center gap-[8px] rounded-[6px] px-[6px] py-[5px] text-start text-[12px] text-ink-2 transition-colors hover:bg-wash hover:text-text"
              >
                <Keyboard
                  size={13}
                  strokeWidth={1.8}
                  className="shrink-0 opacity-70"
                />
                {t("sidebar.promoShortcuts")}
              </button>
              <button
                type="button"
                onClick={() => onSectionChange("help")}
                className="flex cursor-pointer items-center gap-[8px] rounded-[6px] px-[6px] py-[5px] text-start text-[12px] text-ink-2 transition-colors hover:bg-wash hover:text-text"
              >
                <BookOpen
                  size={13}
                  strokeWidth={1.8}
                  className="shrink-0 opacity-70"
                />
                <span>{t("sidebar.promoDocs")}</span>
              </button>
            </div>
          </div>
        )}
        <div className="flex items-center gap-[6px] pb-[8px] ps-[9px] pe-[2px] text-[11px] text-ink-3">
          <span className="h-[6px] w-[6px] shrink-0 rounded-full bg-moss" />
          <span className="min-w-0 flex-1 truncate">
            {modelName ?? t("sidebar.localModel")}
          </span>
          <button
            type="button"
            onClick={toggleTheme}
            aria-label={t("palette.toggleTheme")}
            title={t("palette.toggleTheme")}
            className="flex h-[22px] w-[22px] shrink-0 cursor-pointer items-center justify-center rounded-[5px] text-ink-3 hover:bg-wash hover:text-text"
          >
            {dark ? <Sun size={13} /> : <Moon size={13} />}
          </button>
        </div>
        {system.map(renderRow)}
      </div>
    </nav>
  );
};
