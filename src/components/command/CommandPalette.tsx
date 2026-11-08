import React, { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  ChartNoAxesColumn,
  CircleHelp,
  Mic,
  Moon,
  NotebookPen,
  Search,
  Settings2,
  SquarePen,
  Sun,
  AudioLines,
  Palette,
} from "lucide-react";

import { commands } from "@/bindings";
import type { SidebarSection } from "@/components/Sidebar";
import { getStoredAccent, applyAccent, type AccentId } from "@/lib/utils/accent";
import { getStoredTheme, applyTheme } from "@/lib/utils/theme";

interface Item {
  id: string;
  label: string;
  hint?: string;
  icon: React.ReactNode;
  run: () => void;
}

interface Props {
  open: boolean;
  onClose: () => void;
  onNavigate: (section: SidebarSection) => void;
}

const ACCENT_ORDER: AccentId[] = ["bla", "rosa", "gronn", "rod", "gul"];

/**
 * Cmd+K: everything the rail does not show, one keystroke away. The analysis
 * found that people could not find settings; this is the answer to that, not
 * more rows in the sidebar.
 */
export const CommandPalette: React.FC<Props> = ({ open, onClose, onNavigate }) => {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [active, setActive] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    setActive(0);
    const id = window.setTimeout(() => inputRef.current?.focus(), 20);
    return () => window.clearTimeout(id);
  }, [open]);

  const items = useMemo<Item[]>(() => {
    const go = (section: SidebarSection) => () => {
      onNavigate(section);
      onClose();
    };
    const cycleAccent = () => {
      const current = getStoredAccent();
      const next = ACCENT_ORDER[(ACCENT_ORDER.indexOf(current) + 1) % ACCENT_ORDER.length];
      applyAccent(next);
      onClose();
    };
    const toggleTheme = () => {
      const next = getStoredTheme() === "dark" ? "light" : "dark";
      applyTheme(next);
      onClose();
    };

    return [
      { id: "nav-home", label: t("sidebar.home"), icon: <AudioLines size={15} />, run: go("home") },
      { id: "nav-meetings", label: t("sidebar.notetaker"), icon: <Mic size={15} />, run: go("notetaker") },
      { id: "nav-insights", label: t("sidebar.insights"), icon: <ChartNoAxesColumn size={15} />, run: go("insights") },
      {
        id: "scratchpad",
        label: t("sidebar.scratchpad"),
        icon: <SquarePen size={15} />,
        run: () => {
          go("scratchpad");
          onClose();
        },
      },
      { id: "settings", label: t("sidebar.settings"), icon: <Settings2 size={15} />, run: go("general") },
      { id: "help", label: t("sidebar.help"), icon: <CircleHelp size={15} />, run: go("help") },
      {
        id: "theme",
        label: t("palette.toggleTheme"),
        hint: t("palette.themeHint"),
        icon:
          getStoredTheme() === "dark" ? <Sun size={15} /> : <Moon size={15} />,
        run: toggleTheme,
      },
      {
        id: "accent",
        label: t("palette.nextAccent"),
        hint: t("palette.accentHint"),
        icon: <Palette size={15} />,
        run: cycleAccent,
      },
      {
        id: "meeting",
        label: t("palette.startMeeting"),
        icon: <Mic size={15} />,
        run: () => {
          commands.startMeeting().catch(() => {});
          onClose();
        },
      },
    ];
  }, [t, onNavigate, onClose]);

  const results = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return items;
    return items.filter((item) =>
      `${item.label} ${item.hint ?? ""}`.toLowerCase().includes(needle),
    );
  }, [items, query]);

  useEffect(() => setActive(0), [query]);

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/25 pt-[14vh] backdrop-blur-[2px]"
      onMouseDown={onClose}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label={t("palette.title")}
        onMouseDown={(event) => event.stopPropagation()}
        className="w-[560px] overflow-hidden rounded-[12px] border border-line bg-surface shadow-[0_24px_64px_rgba(0,0,0,0.28)]"
      >
        <div className="flex items-center gap-[9px] border-b border-line px-[14px]">
          <Search size={15} className="text-ink-3" />
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") onClose();
              if (event.key === "ArrowDown") {
                event.preventDefault();
                setActive((current) => Math.min(results.length - 1, current + 1));
              }
              if (event.key === "ArrowUp") {
                event.preventDefault();
                setActive((current) => Math.max(0, current - 1));
              }
              if (event.key === "Enter") results[active]?.run();
            }}
            placeholder={t("palette.placeholder")}
            className="h-[44px] flex-1 bg-transparent text-[14px] text-text outline-none placeholder:text-ink-3"
          />
          {/* eslint-disable-next-line i18next/no-literal-string -- a key cap */}
          <kbd className="rounded-[5px] border border-line bg-surface-soft px-[5px] py-[1px] text-[11px] text-ink-3">
            esc
          </kbd>
        </div>
        <div className="max-h-[340px] overflow-y-auto py-[6px]">
          {results.length === 0 && (
            <p className="px-[14px] py-[12px] text-[13px] text-ink-3">
              {t("palette.noResults")}
            </p>
          )}
          {results.map((item, index) => (
            <button
              key={item.id}
              type="button"
              onMouseEnter={() => setActive(index)}
              onClick={item.run}
              className={`flex w-full cursor-pointer items-center gap-[10px] px-[14px] py-[8px] text-start text-[13px] ${
                index === active ? "bg-wash text-text" : "text-ink-2"
              }`}
            >
              <span className="text-ink-3">{item.icon}</span>
              <span className="flex-1 truncate">{item.label}</span>
              {item.hint && (
                <span className="text-[11px] text-ink-3">{item.hint}</span>
              )}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
};
