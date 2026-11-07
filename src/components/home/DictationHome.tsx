import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import {
  Check,
  Copy,
  Ellipsis,
  MessageSquare,
  Mic,
  Play,
  Search,
  Trash2,
  X,
} from "lucide-react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { toast } from "sonner";
import { commands, type HistoryEntry } from "@/bindings";
import { useDictationStream } from "@/hooks/useDictationStream";
import { useMeetingClock } from "@/hooks/useMeetingClock";
import { useOsType } from "@/hooks/useOsType";
import { useSettings } from "@/hooks/useSettings";
import { activityByDay, entryText, summarize } from "@/lib/insights/stats";
import { copyToClipboard } from "@/components/settings/history/clipboard";
import { useNumberFormat } from "@/components/hub/Primitives";
import { Keycaps } from "@/components/shared/Keycaps";

const PAGE_SIZE = 50;

/** Which wording a row shows: the raw transcript or the cleaned version. */
type Mode = "raw" | "polished";

const isSameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() &&
  a.getMonth() === b.getMonth() &&
  a.getDate() === b.getDate();

const dayLabelKey = (timestampSecs: number): string | null => {
  const date = new Date(timestampSecs * 1000);
  const today = new Date();
  const yesterday = new Date();
  yesterday.setDate(today.getDate() - 1);
  if (isSameDay(date, today)) return "home.today";
  if (isSameDay(date, yesterday)) return "home.yesterday";
  return null;
};

const clock = (timestampSecs: number, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(timestampSecs * 1000));

const longDate = (timestampSecs: number, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    weekday: "long",
    day: "numeric",
    month: "long",
  }).format(new Date(timestampSecs * 1000));

/** The recording's file, as a URL the webview can play (the reference's Play). */
const audioUrl = async (fileName: string) => {
  const result = await commands.getAudioFilePath(fileName);
  return result.status === "ok" ? convertFileSrc(result.data, "asset") : null;
};

const ActionButton: React.FC<{
  label: string;
  onClick: () => void;
  children: React.ReactNode;
}> = ({ label, onClick, children }) => (
  <button
    type="button"
    onClick={onClick}
    title={label}
    aria-label={label}
    className="flex h-[28px] w-[28px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
  >
    {children}
  </button>
);

/** One dictation: time rail 98, text 500, actions 138 — the reference app's row. */
const DictationRow: React.FC<{
  entry: HistoryEntry;
  locale: string;
  selected: boolean;
  mode: Mode;
  onSelect: () => void;
  onModeChange: (mode: Mode) => void;
  onDeleted: (id: number) => void;
}> = ({ entry, locale, selected, mode, onSelect, onModeChange, onDeleted }) => {
  const { t } = useTranslation();
  const [copied, setCopied] = useState(false);
  const [menuOpen, setMenuOpen] = useState(false);
  const [playing, setPlaying] = useState<HTMLAudioElement | null>(null);
  const hasPolished = Boolean(entry.post_processed_text);
  const text = mode === "raw" ? entry.transcription_text : entryText(entry);

  const copy = async () => {
    const ok = await copyToClipboard(text);
    if (!ok) {
      toast.error(t("home.copyFailed"));
      return;
    }
    setCopied(true);
    window.setTimeout(() => setCopied(false), 1600);
  };

  const play = async () => {
    if (playing) {
      playing.pause();
      setPlaying(null);
      return;
    }
    const url = await audioUrl(entry.file_name);
    if (!url) {
      toast.error(t("home.audioMissing"));
      return;
    }
    const audio = new Audio(url);
    audio.onended = () => setPlaying(null);
    setPlaying(audio);
    void audio.play();
  };

  const remove = async () => {
    setMenuOpen(false);
    const result = await commands.deleteHistoryEntry(entry.id);
    if (result.status === "error") {
      toast.error(t("home.deleteFailed"));
      return;
    }
    onDeleted(entry.id);
  };

  return (
    <div
      onClick={onSelect}
      className={`group flex cursor-pointer items-stretch border-t border-line first:border-t-0 hover:bg-surface-soft/40 ${
        selected ? "bg-surface-soft/60" : ""
      }`}
    >
      <div className="flex w-[98px] shrink-0 items-center p-[16px] font-ui text-[12px] leading-[20px] text-ink-2">
        {clock(entry.timestamp, locale)}
      </div>
      <div className="min-w-0 flex-1 p-[16px]">
        <p className="whitespace-pre-wrap text-[14px] leading-[20px] text-text">
          {text || t("home.emptyTranscript")}
        </p>
        {selected && hasPolished && (
          <span
            onClick={(event) => event.stopPropagation()}
            className="mt-[10px] inline-flex rounded-[7px] border border-line bg-surface p-[2px] shadow-[0_1px_2px_rgba(0,0,0,0.06)]"
          >
            {(["raw", "polished"] as Mode[]).map((option) => (
              <button
                key={option}
                type="button"
                onClick={() => onModeChange(option)}
                className={`cursor-pointer rounded-[5px] px-[9px] py-[3px] text-[11.5px] ${
                  mode === option
                    ? "bg-background-ui font-semibold text-on-accent"
                    : "text-ink-2"
                }`}
              >
                {t(option === "raw" ? "diktering.raw" : "diktering.polished")}
              </button>
            ))}
          </span>
        )}
      </div>
      <div
        onClick={(event) => event.stopPropagation()}
        className="relative flex w-[138px] shrink-0 items-center gap-[6px] pe-[8px]"
      >
        <ActionButton label={t("home.play")} onClick={play}>
          <Play size={16} />
        </ActionButton>
        <ActionButton label={t("home.copy")} onClick={copy}>
          {copied ? <Check size={16} /> : <Copy size={16} />}
        </ActionButton>
        <ActionButton
          label={t("home.feedback")}
          onClick={() =>
            window.open(
              "https://github.com/flowstate-app/flowstate/issues",
              "_blank",
            )
          }
        >
          <MessageSquare size={16} />
        </ActionButton>
        <ActionButton
          label={t("home.more")}
          onClick={() => setMenuOpen((open) => !open)}
        >
          <Ellipsis size={16} />
        </ActionButton>
        {menuOpen && (
          <div className="absolute end-[8px] top-[46px] z-20 w-[160px] overflow-hidden rounded-[8px] border border-line bg-surface shadow-[var(--flow-shadow-pop)]">
            <button
              type="button"
              onClick={remove}
              className="flex w-full cursor-pointer items-center gap-[8px] px-[12px] py-[8px] text-start text-[13px] text-text hover:bg-wash"
            >
              <Trash2 size={14} /> {t("home.delete")}
            </button>
          </div>
        )}
      </div>
    </div>
  );
};

export const DictationHome: React.FC<{ onOpenShortcuts: () => void }> = ({
  onOpenShortcuts,
}) => {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || "nb";
  const osType = useOsType();
  const { settings } = useSettings();
  const formatNumber = useNumberFormat();

  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [hasMore, setHasMore] = useState(false);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [modes, setModes] = useState<Record<number, Mode>>({});
  const stream = useDictationStream();
  const seconds = useMeetingClock(stream.startedAt ?? 0, stream.recording);
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<HistoryEntry[] | null>(null);
  const loadingRef = useRef(false);
  const [searchOpen, setSearchOpen] = useState(false);
  const [bannerHidden, setBannerHidden] = useState(() => {
    try {
      return localStorage.getItem("flowstate.homeBannerDismissed") === "1";
    } catch {
      return false;
    }
  });
  const searchSeq = useRef(0);
  const shortcut = settings?.bindings?.transcribe?.current_binding;

  const load = useCallback(async (cursor?: number) => {
    if (loadingRef.current) return;
    loadingRef.current = true;
    try {
      const result = await commands.getHistoryEntries(
        cursor ?? null,
        PAGE_SIZE,
      );
      if (result.status === "ok") {
        const { entries: page, has_more } = result.data;
        setEntries((prev) =>
          cursor === undefined
            ? page
            : [
                ...prev,
                ...page.filter(
                  (entry) => !prev.some((existing) => existing.id === entry.id),
                ),
              ],
        );
        setHasMore(has_more);
      }
    } catch (error) {
      console.error("Failed to load dictations:", error);
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, []);

  useEffect(() => {
    void load();
  }, [load]);

  // A finished dictation arrives as history; refresh once it lands.
  useEffect(() => {
    if (stream.recording) return;
    const id = window.setTimeout(load, 800);
    return () => window.clearTimeout(id);
  }, [stream.recording, load]);

  // Search runs on the backend so it reaches the whole history, not the page.
  useEffect(() => {
    const trimmed = query.trim();
    if (!trimmed) {
      setResults(null);
      return;
    }
    const seq = ++searchSeq.current;
    const timer = window.setTimeout(async () => {
      try {
        const result = await commands.searchHistory(trimmed, PAGE_SIZE, 0);
        if (seq !== searchSeq.current) return;
        setResults(result.status === "ok" ? result.data.entries : []);
      } catch (error) {
        console.error("Failed to search dictations:", error);
        if (seq === searchSeq.current) setResults([]);
      }
    }, 180);
    return () => window.clearTimeout(timer);
  }, [query]);

  const visible = results ?? entries;
  const stats = useMemo(() => summarize(entries), [entries]);
  const days = useMemo(() => activityByDay(entries, 30), [entries]);

  const groups = useMemo(() => {
    const byDay = new Map<string, { label: string; entries: HistoryEntry[] }>();
    for (const entry of visible) {
      const key = new Date(entry.timestamp * 1000).toDateString();
      const labelKey = dayLabelKey(entry.timestamp);
      if (!byDay.has(key)) {
        byDay.set(key, {
          label: labelKey ? t(labelKey) : longDate(entry.timestamp, locale),
          entries: [],
        });
      }
      byDay.get(key)!.entries.push(entry);
    }
    return [...byDay.values()];
  }, [visible, locale, t]);

  return (
    <div className="pb-[40px]">
      <h1 className="font-ui text-[24px] font-semibold leading-[32px] tracking-[-0.02em] text-text">
        {t("home.overline")}
      </h1>

      {/* 1044 = 770 + 24 + 250, the reference's exact column split. */}
      <div className="mt-[24px] flex w-[1044px] items-start gap-[24px]">
        <div className="min-w-0 flex-1">
          {/* The banner is the only serif surface in the app. */}
          <section
            hidden={bannerHidden}
            className="relative overflow-hidden rounded-[16px] border border-line bg-surface-soft px-[32px] py-[32px]"
          >
            <button
              type="button"
              onClick={() => {
                setBannerHidden(true);
                try {
                  localStorage.setItem("flowstate.homeBannerDismissed", "1");
                } catch {
                  // the banner just comes back next launch
                }
              }}
              aria-label={t("home.dismiss")}
              title={t("home.dismiss")}
              className="absolute end-[16px] top-[16px] z-10 flex h-[24px] w-[24px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
            >
              <X size={14} />
            </button>
            <div
              aria-hidden="true"
              className="pointer-events-none absolute inset-0"
              style={{
                background:
                  "radial-gradient(120% 160% at 100% 0%, color-mix(in srgb, var(--color-background-ui) 55%, transparent) 0%, transparent 68%)",
              }}
            />
            <div className="relative max-w-[600px]">
              <h2 className="font-display text-[28px] leading-[28px] font-normal tracking-[-0.01em] text-text">
                {t("home.bannerTitle")}
              </h2>
              <p className="mt-[12px] text-[14px] leading-[20px] text-text/85">
                {t("home.bannerBody")}
              </p>
            </div>
            <button
              type="button"
              onClick={onOpenShortcuts}
              className="relative mt-[16px] h-[36px] cursor-pointer rounded-[8px] border border-logo-primary bg-background-ui px-[16px] text-[14px] font-semibold text-on-accent transition-opacity hover:opacity-90"
            >
              {t("home.bannerCta")}
            </button>
          </section>

          {/* The live dictation, where the text lands: same stream as the
              overlay, with the words as they commit and the one in flight. */}
          <div
            className={`mt-[20px] rounded-[12px] border px-[16px] py-[13px] transition-colors ${
              stream.recording
                ? "border-logo-primary/60 bg-wash shadow-[0_0_0_3px_var(--color-wash)]"
                : "border-line bg-surface-soft"
            }`}
          >
            <div className="flex items-center gap-[11px]">
              <span
                className={`flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-[8px] ${
                  stream.recording
                    ? "bg-background-ui text-on-accent"
                    : "bg-wash text-ink-2"
                }`}
              >
                <Mic size={15} strokeWidth={2} />
              </span>
              {stream.recording ? (
                <span className="text-[12.5px] text-ink-2">
                  {t("diktering.recording")}{" "}
                  <b className="tabular-nums font-semibold text-logo-primary">
                    {`0:${String(seconds % 60).padStart(2, "0")}`}
                  </b>
                </span>
              ) : (
                <span className="text-[13px] text-ink-2">
                  {t("diktering.hint")}
                </span>
              )}
              {stream.recording && (
                <span className="flex h-[22px] items-center gap-[3px]">
                  {[6, 12, 19, 10, 22, 14, 8, 16, 11, 5].map((h, i) => (
                    <i
                      key={i}
                      className="w-[2.5px] rounded-full bg-background-ui"
                      style={{ height: h }}
                    />
                  ))}
                </span>
              )}
              {/* eslint-disable-next-line i18next/no-literal-string -- a key cap */}
              <kbd className="ms-auto rounded-[5px] border border-line bg-surface px-[5px] py-[1px] text-[11px] text-ink-3">
                ⌥ Space
              </kbd>
            </div>
            {stream.recording && (stream.committed || stream.tentative) && (
              <p className="mt-[10px] max-w-[62ch] font-display text-[16.5px] leading-[25px] text-text">
                {stream.committed}{" "}
                <span className="text-ink-3">{stream.tentative}</span>
                <span className="ms-[2px] inline-block h-[15px] w-[2px] translate-y-[2px] bg-logo-primary" />
              </p>
            )}
          </div>

          {/* Day label row, then the bordered list card — 52px rhythm as in the reference. */}
          {loading ? (
            <div className="py-16 text-center text-[13px] text-ink-3">
              {t("home.loading")}
            </div>
          ) : visible.length === 0 ? (
            <div className="mt-[24px] rounded-[16px] border border-line px-[24px] py-[64px] text-center">
              <Mic size={26} className="mx-auto text-ink-3" />
              <p className="mt-3 text-[14px] font-medium text-text">
                {query ? t("home.noResults") : t("home.emptyTitle")}
              </p>
              <p className="mx-auto mt-1 max-w-[380px] text-[13px] leading-5 text-ink-2">
                {query ? t("home.noResultsBody") : t("home.emptyBody")}
              </p>
              {shortcut && (
                <div className="mt-4 flex items-center justify-center gap-2 text-ink-2">
                  <span className="text-[12px]">{t("home.holdToTalk")}</span>
                  <Keycaps binding={shortcut} osType={osType} size="line" />
                </div>
              )}
            </div>
          ) : (
            <>
              <div className="flex h-[52px] items-center justify-between gap-[16px] pt-[4px]">
                <span className="font-ui text-[12px] font-semibold uppercase tracking-[0.06em] text-ink-3">
                  {groups[0]?.label ?? t("home.historyHeading")}
                </span>
                {searchOpen || query ? (
                  <div className="relative w-[200px]">
                    <Search
                      size={14}
                      className="pointer-events-none absolute start-2 top-1/2 -translate-y-1/2 text-ink-3"
                    />
                    <input
                      autoFocus
                      value={query}
                      onChange={(event) => setQuery(event.target.value)}
                      onBlur={() => {
                        if (!query) setSearchOpen(false);
                      }}
                      onKeyDown={(event) => {
                        if (event.key === "Escape") {
                          setQuery("");
                          setSearchOpen(false);
                        }
                      }}
                      placeholder={t("home.searchPlaceholder")}
                      aria-label={t("home.searchPlaceholder")}
                      className="h-[32px] w-full rounded-[6px] border border-line bg-surface-soft ps-[28px] pe-[28px] text-[12px] text-text outline-none transition-colors placeholder:text-ink-3 focus:border-text/25"
                    />
                    {query && (
                      <button
                        type="button"
                        onClick={() => setQuery("")}
                        aria-label={t("home.clearSearch")}
                        className="absolute end-1.5 top-1/2 -translate-y-1/2 cursor-pointer rounded p-1 text-ink-3 hover:text-text"
                      >
                        <X size={12} />
                      </button>
                    )}
                  </div>
                ) : (
                  <button
                    type="button"
                    onClick={() => setSearchOpen(true)}
                    aria-label={t("home.searchPlaceholder")}
                    title={t("home.searchPlaceholder")}
                    className="flex h-[28px] w-[28px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
                  >
                    <Search size={16} />
                  </button>
                )}
              </div>

              {groups.map((group, index) => (
                <section
                  key={group.label}
                  className={index > 0 ? "mt-6" : undefined}
                >
                  {index > 0 && (
                    <div className="mb-2 font-ui text-[12px] font-semibold uppercase tracking-[0.06em] text-ink-3">
                      {group.label}
                    </div>
                  )}
                  <div className="overflow-hidden rounded-[16px] border border-line">
                    {group.entries.map((entry) => (
                      <DictationRow
                        key={entry.id}
                        entry={entry}
                        locale={locale}
                        selected={selectedId === entry.id}
                        mode={modes[entry.id] ?? "polished"}
                        onSelect={() =>
                          setSelectedId((current) =>
                            current === entry.id ? null : entry.id,
                          )
                        }
                        onModeChange={(option) =>
                          setModes((current) => ({
                            ...current,
                            [entry.id]: option,
                          }))
                        }
                        onDeleted={(id) =>
                          setEntries((prev) =>
                            prev.filter((item) => item.id !== id),
                          )
                        }
                      />
                    ))}
                  </div>
                </section>
              ))}

              {hasMore && !query && (
                <button
                  type="button"
                  onClick={() => void load(entries[entries.length - 1]?.id)}
                  className="mx-auto mt-4 block cursor-pointer rounded-lg px-3 py-2 text-[13px] font-medium text-ink-2 transition-colors hover:bg-surface-soft hover:text-text"
                >
                  {t("home.loadMore")}
                </button>
              )}
            </>
          )}
        </div>

        {/* Right rail: one 250px card, two sections that abut, numbers 28/400
            with 16px labels on a 44px rhythm. Measured off the reference. */}
        <aside className="w-[250px] shrink-0">
          <div className="overflow-hidden rounded-[16px] bg-surface-soft">
            <div className="flex flex-col gap-[16px] p-[24px]">
              <p className="flex items-baseline gap-[8px]">
                <span className="font-ui text-[28px] leading-[28px] text-text">
                  {formatNumber(stats.words)}
                </span>
                <span className="text-[16px] leading-[20px] text-text/70">
                  {t("home.railWords")}
                </span>
              </p>
              <p className="flex items-baseline gap-[8px]">
                <span className="font-ui text-[28px] leading-[28px] text-text">
                  {formatNumber(stats.dictations)}
                </span>
                <span className="text-[16px] leading-[20px] text-text/70">
                  {t("home.railDictations")}
                </span>
              </p>
              <p className="flex items-baseline gap-[8px]">
                <span className="font-ui text-[28px] leading-[28px] text-text">
                  {formatNumber(stats.streak)}
                </span>
                <span className="text-[16px] leading-[20px] text-text/70">
                  {t("home.railStreak")}
                </span>
              </p>
            </div>

            <div className="p-[24px]">
              <p className="text-[16px] font-semibold leading-[24px] text-text">
                {t("home.voiceTitle")}
              </p>
              <p className="mt-[4px] text-[12px] leading-[20px] text-text/70">
                {t("home.voiceBody")}
              </p>
              <div className="mt-[8px] h-[18px] overflow-hidden rounded-[9px] bg-text/10">
                <div
                  className="h-full rounded-[9px] bg-background-ui"
                  style={{
                    width: `${Math.min(100, (days.filter((d) => d.count > 0).length / 30) * 100)}%`,
                  }}
                />
              </div>
            </div>
          </div>
        </aside>
      </div>
    </div>
  );
};
