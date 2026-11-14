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

/**
 * Sidebar bands, top to bottom: where text gets made, what shapes it, how it
 * is going. `system` is the foot pinned to the bottom edge (Settings, Help);
 * hidden system sections are Settings tabs and light up the Settings row.
 */
type SectionGroup = "capture" | "words" | "progress" | "system";

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
    group: "capture",
  },
  scratchpad: {
    labelKey: "sidebar.scratchpad",
    icon: SquarePen,
    component: null,
    enabled: () => true,
    group: "capture",
  },
  notetaker: {
    labelKey: "sidebar.notetaker",
    icon: Mic,
    component: null,
    enabled: () => true,
    group: "capture",
  },
  insights: {
    labelKey: "sidebar.insights",
    icon: ChartNoAxesColumn,
    component: null,
    enabled: () => true,
    group: "progress",
  },
  dictionary: {
    labelKey: "sidebar.dictionary",
    icon: BookOpen,
    component: null,
    enabled: () => true,
    group: "words",
  },
  snippets: {
    labelKey: "sidebar.snippets",
    icon: Scissors,
    component: null,
    enabled: () => true,
    group: "words",
  },
  style: {
    labelKey: "sidebar.style",
    icon: Paintbrush,
    component: null,
    enabled: () => true,
    group: "words",
  },
  transforms: {
    labelKey: "sidebar.transforms",
    icon: Sparkles,
    component: null,
    enabled: () => true,
    group: "words",
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

/** Band order and headings; rows keep their SECTIONS_CONFIG order inside. */
const BANDS = [
  { id: "capture", labelKey: "sidebar.groups.capture" },
  { id: "words", labelKey: "sidebar.groups.words" },
  { id: "progress", labelKey: "sidebar.groups.progress" },
] as const satisfies readonly { id: SectionGroup; labelKey: string }[];

interface SidebarProps {
  activeSection: SidebarSection;
  onSectionChange: (section: SidebarSection) => void;
}

/**
 * The rail: brand, the working screens in headed bands (scrolling when the
 * window is short), the "Kom i gang" card until it is dismissed, and a foot
 * pinned to the bottom edge with Settings, Help and what the app is doing.
 */
export const Sidebar: React.FC<SidebarProps> = ({
  activeSection,
  onSectionChange,
}) => {
  const { t } = useTranslation();
  const { settings, updateSetting } = useSettings();
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
    // Same as TopBar: persist the choice so the overlay hears theme-changed
    // and syncThemeFromSettings() keeps it after a restart.
    applyTheme(next);
    updateSetting("theme", next);
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
  const bands = BANDS.map((band) => ({
    ...band,
    rows: visible.filter((section) => section.group === band.id),
  })).filter((band) => band.rows.length > 0);
  const system = visible.filter((section) => section.group === "system");
  // Settings tabs (Models, Sync, ...) are not rows of their own; while one of
  // them is open the Settings row is the one that reads as selected.
  const activeConfig = SECTIONS_CONFIG[activeSection] as {
    hidden?: boolean;
    group: SectionGroup;
  };
  const activeRow: SidebarSection =
    activeConfig?.hidden && activeConfig.group === "system"
      ? "general"
      : activeSection;

  const focusRing =
    "focus:outline-none focus-visible:ring-2 focus-visible:ring-focus";

  const renderRow = (section: (typeof visible)[number]) => {
    const Icon = section.icon;
    const isActive = activeRow === section.id;
    return (
      <button
        key={section.id}
        type="button"
        aria-current={isActive ? "page" : undefined}
        onClick={() => onSectionChange(section.id)}
        className={`relative flex h-[32px] shrink-0 cursor-pointer items-center gap-[9px] rounded-[8px] px-[9px] text-start transition-colors duration-[var(--flow-dur-quick)] ease-[var(--flow-ease-spring)] ${focusRing} ${
          isActive
            ? "bg-text/[0.07] font-[600] text-text"
            : "text-ink-2 hover:bg-text/[0.04] hover:text-text"
        }`}
      >
        <Icon
          size={15}
          strokeWidth={isActive ? 2 : 1.8}
          className={`shrink-0 ${isActive ? "text-text" : "text-ink-3"}`}
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
      className="flex h-full w-[228px] shrink-0 flex-col px-[12px] pt-[38px] pb-[10px]"
    >
      <div className="flex shrink-0 items-center gap-[8px] px-[6px] pb-[16px]">
        <FlowStateMark width={18} height={19} />
        {/* eslint-disable-next-line i18next/no-literal-string -- the wordmark */}
        <span className="font-ui text-[13.5px] font-[650] tracking-[-0.01em] text-text">
          FlowState
        </span>
      </div>

      {/* The bands scroll on a short window; the foot below never moves. The
          4px bleed keeps focus rings from being clipped by the scroller. */}
      <div className="-mx-[4px] flex min-h-0 flex-1 flex-col overflow-y-auto overscroll-contain px-[4px] pt-[2px] pb-[4px]">
        {bands.map((band, index) => {
          const headingId = `sidebar-band-${band.id}`;
          return (
            <div
              key={band.id}
              role="group"
              aria-labelledby={headingId}
              className={`flex flex-col gap-[2px] ${index > 0 ? "mt-[18px]" : ""}`}
            >
              <div
                id={headingId}
                className="truncate px-[9px] pb-[3px] text-[11.5px] leading-[16px] font-[560] text-ink-3"
              >
                {t(band.labelKey)}
              </div>
              {band.rows.map(renderRow)}
            </div>
          );
        })}

        {!promoHidden && (
          <div className="mt-auto shrink-0 pt-[20px]">
            <div className="rounded-[10px] border border-line bg-surface p-[12px]">
              <div className="flex items-center justify-between pe-[2px]">
                <span className="text-[12px] font-semibold text-text">
                  {t("sidebar.promoTitle")}
                </span>
                <button
                  type="button"
                  onClick={dismissPromo}
                  aria-label={t("home.dismiss")}
                  title={t("home.dismiss")}
                  className={`flex h-[20px] w-[20px] shrink-0 cursor-pointer items-center justify-center rounded-[5px] text-ink-3 transition-colors hover:bg-wash hover:text-text ${focusRing}`}
                >
                  <X size={12} />
                </button>
              </div>
              <div className="mt-[6px] flex flex-col gap-[2px]">
                <button
                  type="button"
                  onClick={() => onSectionChange("general")}
                  className={`flex cursor-pointer items-center gap-[8px] rounded-[6px] px-[6px] py-[5px] text-start text-[12px] text-ink-2 transition-colors hover:bg-wash hover:text-text ${focusRing}`}
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
                  className={`flex cursor-pointer items-center gap-[8px] rounded-[6px] px-[6px] py-[5px] text-start text-[12px] text-ink-2 transition-colors hover:bg-wash hover:text-text ${focusRing}`}
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
          </div>
        )}
      </div>

      {/* The foot: utilities pinned to the bottom edge, set off by a hairline
          instead of a heading. */}
      <div className="flex shrink-0 flex-col pt-[8px]">
        <div aria-hidden className="mx-[9px] mb-[8px] h-px bg-line" />
        <div
          role="group"
          aria-label={t("sidebar.system")}
          className="flex flex-col gap-[2px]"
        >
          {system.map(renderRow)}
        </div>
        <div className="mt-[6px] flex h-[26px] items-center gap-[14px] ps-[13px] pe-[3px] text-[11px] text-ink-3">
          <span className="h-[6px] w-[6px] shrink-0 rounded-full bg-moss" />
          <span className="min-w-0 flex-1 truncate">
            {modelName ?? t("sidebar.localModel")}
          </span>
          <button
            type="button"
            onClick={toggleTheme}
            aria-label={t("palette.toggleTheme")}
            title={t("palette.toggleTheme")}
            className={`flex h-[24px] w-[24px] shrink-0 cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors duration-[var(--flow-dur-quick)] hover:bg-text/[0.04] hover:text-text ${focusRing}`}
          >
            {dark ? <Sun size={13} /> : <Moon size={13} />}
          </button>
        </div>
      </div>
    </nav>
  );
};
