import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { ChartNoAxesColumn } from "lucide-react";
import { commands, type HistoryEntry } from "@/bindings";
import {
  activityByDay,
  activityByHour,
  dayKey,
  entryText,
  peakHour,
  sourceCounts,
  summarize,
  weekdayActivity,
  wordCount,
  wordsByLanguage,
  type DayBucket,
} from "@/lib/insights/stats";
import { EmptyState, Page, useNumberFormat } from "@/components/hub/Primitives";

type Tab = "usage" | "voice" | "top";

const TABS: { id: Tab; key: string }[] = [
  { id: "usage", key: "insights.tabUsage" },
  { id: "voice", key: "insights.tabVoice" },
  { id: "top", key: "insights.tabTop" },
];

/** 0 = nothing, 1..4 = growing logo-primary fills. */
const HEAT_CELLS = [
  "bg-text/[0.06]",
  "bg-logo-primary/20",
  "bg-logo-primary/40",
  "bg-logo-primary/65",
  "bg-logo-primary/90",
] as const;

const heatLevel = (words: number, max: number) =>
  words === 0 ? 0 : 1 + Math.min(3, Math.floor((words / max) * 4));

const shortDate = (date: Date, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    day: "numeric",
    month: "short",
  }).format(date);

const hourLabel = (hour: number) => `${String(hour).padStart(2, "0")}:00`;

/** Monday-first weekday labels, locale-aware. 2024-01-01 is a Monday. */
const dayNames = (locale: string): string[] => {
  const format = new Intl.DateTimeFormat(locale, { weekday: "short" });
  return [1, 2, 3, 4, 5, 6, 7].map((day) =>
    format.format(new Date(2024, 0, day)),
  );
};

const StatCard: React.FC<{
  label: string;
  value: string;
  children?: React.ReactNode;
}> = ({ label, value, children }) => (
  <div className="flex flex-1 flex-col rounded-[12px] border border-line p-[22px]">
    <p className="font-ui text-[30px] font-[560] leading-none tracking-[-0.02em] tabular-nums text-text">
      {value}
    </p>
    <p className="mt-[7px] font-ui text-[11px] font-semibold uppercase tracking-[0.08em] text-ink-3">
      {label}
    </p>
    {children}
  </div>
);

/** One label · bar with % inside · count row, shared by the weekday and
 *  language lists. */
const BarRow: React.FC<{
  label: React.ReactNode;
  labelClassName?: string;
  percent: number;
  widthPercent: number;
  count: string;
}> = ({ label, labelClassName = "", percent, widthPercent, count }) => (
  <div className="flex items-center gap-4">
    <span
      className={`w-24 shrink-0 truncate font-ui text-[12.5px] text-ink-2 ${labelClassName}`}
    >
      {label}
    </span>
    <div className="h-[6px] min-w-0 flex-1 overflow-hidden rounded-full bg-line">
      <div
        className="h-full rounded-full bg-logo-primary/60"
        style={{ width: `${Math.max(2, widthPercent)}%` }}
      />
    </div>
    <span className="w-9 shrink-0 text-right font-ui text-[11.5px] tabular-nums text-ink-3">
      {percent}%
    </span>
    <span className="w-12 shrink-0 text-right font-ui text-[12px] tabular-nums text-ink-2">
      {count}
    </span>
  </div>
);

export const InsightsPage: React.FC<{
  onStartDictating?: () => void;
}> = ({ onStartDictating }) => {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || "nb";
  const formatNumber = useNumberFormat();
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<Tab>("usage");

  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const result = await commands.getHistoryEntries(null, 500);
        if (alive && result.status === "ok") setEntries(result.data.entries);
      } catch (error) {
        console.error("Failed to load history for insights:", error);
      } finally {
        if (alive) setLoading(false);
      }
    };
    void load();
    return () => {
      alive = false;
    };
  }, []);

  const stats = useMemo(() => summarize(entries), [entries]);
  const weekday = useMemo(() => weekdayActivity(entries), [entries]);
  const languages = useMemo(() => wordsByLanguage(entries), [entries]);
  const split = useMemo(() => sourceCounts(entries), [entries]);
  const hours = useMemo(() => activityByHour(entries), [entries]);
  const peak = useMemo(() => peakHour(entries), [entries]);
  const days = useMemo(() => dayNames(locale), [locale]);
  const languageNames = useMemo(() => {
    let names: Intl.DisplayNames | null = null;
    try {
      names = new Intl.DisplayNames([locale], { type: "language" });
    } catch {
      names = null;
    }
    return (code: string) => (names ? (names.of(code) ?? code) : code);
  }, [locale]);

  // "All time" busiest day: span from the oldest dictation, capped so one
  // ancient entry cannot zero the rest out.
  const busiestDay = useMemo<DayBucket | null>(() => {
    if (entries.length === 0) return null;
    const oldest = Math.min(...entries.map((entry) => entry.timestamp));
    const span = Math.min(
      3650,
      Math.max(30, Math.ceil(Date.now() / 1000 - oldest) / 86_400 + 1),
    );
    return activityByDay(entries, span).reduce<DayBucket | null>(
      (top, day) => (!top || day.words > top.words ? day : top),
      null,
    );
  }, [entries]);
  const longest = useMemo<HistoryEntry | null>(() => {
    if (entries.length === 0) return null;
    return entries.reduce((top, entry) =>
      wordCount(entryText(entry)) > wordCount(entryText(top)) ? entry : top,
    );
  }, [entries]);

  // The window hugs the data (6 to 26 weeks), so a young history does not read
  // as an empty grid. Monday-first, so rows are Mon..Sun.
  const heat = useMemo(() => {
    const oldest = entries.reduce(
      (min, entry) => Math.min(min, entry.timestamp),
      Number.MAX_SAFE_INTEGER,
    );
    const spanDays = Math.ceil((Date.now() / 1000 - oldest) / 86_400) + 1;
    const weeks = Math.min(26, Math.max(6, Math.ceil(spanDays / 7)));
    const buckets = activityByDay(entries, weeks * 7);
    const wordsByDay = new Map(
      buckets.map((bucket) => [bucket.key, bucket.words] as const),
    );
    const first = new Date(buckets[0].date);
    const start = new Date(first);
    start.setDate(start.getDate() - ((first.getDay() + 6) % 7));
    const max = Math.max(1, ...buckets.map((bucket) => bucket.words));
    const cells = Array.from({ length: weeks * 7 }, (_, i) => {
      const date = new Date(start);
      date.setDate(start.getDate() + i);
      return wordsByDay.get(dayKey(Math.floor(date.getTime() / 1000))) ?? 0;
    });
    // One label per month, on the first week that starts in it.
    const monthOf = (week: number) => {
      const date = new Date(start);
      date.setDate(start.getDate() + week * 7);
      return date.getMonth();
    };
    const months = Array.from({ length: weeks }, (_, week) => {
      if (week > 0 && monthOf(week) === monthOf(week - 1)) return "";
      const date = new Date(start);
      date.setDate(start.getDate() + week * 7);
      return new Intl.DateTimeFormat(locale, { month: "short" }).format(date);
    });
    return { cells, max, weeks, months };
  }, [entries, locale]);

  if (loading) {
    return (
      <Page width={1040}>
        <h1 className="font-display text-[27px] font-semibold leading-[34px] tracking-[-0.01em] text-text">
          {t("insights.screenTitle")}
        </h1>
        <div className="py-20 text-center text-[13px] text-ink-3">…</div>
      </Page>
    );
  }

  if (entries.length === 0) {
    return (
      <Page width={1040}>
        <h1 className="font-display text-[27px] font-semibold leading-[34px] tracking-[-0.01em] text-text">
          {t("insights.screenTitle")}
        </h1>
        <EmptyState
          icon={<ChartNoAxesColumn size={26} />}
          title={t("insights.emptyTitle")}
          body={t("insights.emptyBody")}
          action={
            onStartDictating ? (
              <button
                type="button"
                onClick={onStartDictating}
                className="cursor-pointer rounded-control border border-logo-primary bg-background-ui px-3.5 py-2 text-[12.5px] font-medium text-on-accent"
              >
                {t("insights.emptyCta")}
              </button>
            ) : undefined
          }
        />
      </Page>
    );
  }

  const meetingShare =
    split.meeting + split.other > 0
      ? Math.round((split.meeting / (split.meeting + split.other)) * 100)
      : 0;
  const weekAgo = Math.floor(Date.now() / 1000) - 7 * 86_400;
  const lastWeek = entries.filter((entry) => entry.timestamp >= weekAgo).length;
  const maxWeekday = Math.max(1, ...weekday);
  const totalLanguageWords = languages.reduce(
    (total, slot) => total + slot.words,
    0,
  );
  const maxLanguageWords = Math.max(1, ...languages.map((slot) => slot.words));

  const usageTab = (
    <div className="flex flex-col gap-4">
      <div className="flex gap-4">
        <StatCard
          label={t("insights.cardAverage")}
          value={formatNumber(stats.averageWords)}
        >
          <div className="mt-4 flex flex-col gap-1.5">
            <div className="flex items-baseline justify-between gap-3 text-[12px] text-ink-2">
              <span>{t("insights.cardLastWeek")}</span>
              <span className="font-medium tabular-nums text-text">
                {formatNumber(lastWeek)}
              </span>
            </div>
            <div className="flex items-baseline justify-between gap-3 text-[12px] text-ink-2">
              <span>{t("insights.cardWordsToday")}</span>
              <span className="font-medium tabular-nums text-text">
                {formatNumber(stats.wordsToday)}
              </span>
            </div>
          </div>
        </StatCard>
        <StatCard
          label={t("insights.cardDictations")}
          value={formatNumber(stats.dictations)}
        >
          <div className="mt-4 flex flex-col gap-1.5">
            <div className="flex items-baseline justify-between gap-3 text-[12px] text-ink-2">
              <span>{t("insights.cardWordsTotal")}</span>
              <span className="font-medium tabular-nums text-text">
                {formatNumber(stats.words)}
              </span>
            </div>
            <div className="flex items-baseline justify-between gap-3 text-[12px] text-ink-2">
              <span>{t("insights.cardActiveDays")}</span>
              <span className="font-medium tabular-nums text-text">
                {formatNumber(stats.activeDays)}
              </span>
            </div>
          </div>
        </StatCard>
        <StatCard
          label={t("insights.cardWordsTotal")}
          value={formatNumber(stats.words)}
        >
          <div className="mt-4 flex h-[18px] overflow-hidden rounded-full bg-logo-primary/30">
            <div
              className="h-full rounded-l-full bg-logo-primary"
              style={{ width: `${meetingShare}%` }}
            />
          </div>
          <div className="mt-2.5 flex gap-4 font-ui text-[11px] text-ink-2">
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full bg-logo-primary" />
              {t("insights.sourceMeeting")} {meetingShare}%
            </span>
            <span className="flex items-center gap-1.5">
              <span className="h-2 w-2 rounded-full bg-logo-primary/30" />
              {t("insights.sourceDictation")} {100 - meetingShare}%
            </span>
          </div>
        </StatCard>
      </div>

      <div className="flex gap-4">
        <div className="flex min-w-0 flex-1 flex-col rounded-[12px] border border-line p-[22px]">
          <div className="flex items-baseline justify-between gap-4">
            <h2 className="font-display text-[17px] font-semibold tracking-[-0.005em] text-text">
              {t("insights.activityCard")}
            </h2>
            <p className="font-ui text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-3">
              {t("insights.cardDictations")} · {formatNumber(stats.dictations)}
            </p>
          </div>
          <div className="mt-5 flex flex-1 flex-col justify-between gap-2.5">
            {weekday.map((count, i) => (
              <BarRow
                key={days[i]}
                label={
                  <span className="text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-3">
                    {days[i]}
                  </span>
                }
                percent={
                  stats.dictations
                    ? Math.round((count / stats.dictations) * 100)
                    : 0
                }
                widthPercent={(count / maxWeekday) * 100}
                count={formatNumber(count)}
              />
            ))}
          </div>
        </div>

        <div className="flex w-[380px] shrink-0 flex-col rounded-[12px] border border-line p-[22px]">
          <div className="flex items-baseline justify-between gap-4">
            <h2 className="font-display text-[17px] font-semibold tracking-[-0.005em] text-text">
              {t("insights.cardStreak")}
            </h2>
            <p className="font-ui text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-3">
              {t("insights.cardStreakHint", {
                longest: formatNumber(stats.longest),
              })}
            </p>
          </div>
          <div className="mt-5 flex flex-1 flex-col justify-between">
            {/* One grid holds the month row, the weekday labels and the cells, so
                every row keeps its height no matter how the card is sized. */}
            <div
              className="grid flex-1 grid-flow-col gap-[3px]"
              style={{
                gridTemplateColumns: `auto repeat(${heat.weeks}, minmax(0, 1fr))`,
                gridTemplateRows: "13px repeat(7, minmax(0, 1fr))",
              }}
            >
              <div />
              {days.map((name) => (
                <span
                  key={name}
                  className="flex items-center font-ui text-[10px] leading-none text-ink-3"
                >
                  {name}
                </span>
              ))}
              {heat.months.map((label, week) => (
                <React.Fragment key={`m${week}`}>
                  <span className="self-end font-ui text-[10px] leading-[13px] text-ink-3">
                    {label}
                  </span>
                  {Array.from({ length: 7 }, (_, row) => {
                    const words = heat.cells[week * 7 + row];
                    return (
                      <div
                        key={`c${week}-${row}`}
                        className={`h-full w-full rounded-[4px] ${HEAT_CELLS[heatLevel(words, heat.max)]}`}
                      />
                    );
                  })}
                </React.Fragment>
              ))}
            </div>
            <div className="mt-4 flex items-center gap-2 font-ui text-[11px] text-ink-3">
              <span>{t("insights.less")}</span>
              <div className="flex gap-1">
                {[1, 2, 3, 4].map((level) => (
                  <span
                    key={level}
                    className={`h-2.5 w-2.5 rounded-[3px] ${HEAT_CELLS[level]}`}
                  />
                ))}
              </div>
              <span>{t("insights.more")}</span>
            </div>
          </div>
        </div>
      </div>
    </div>
  );

  const voiceTab = (
    <div className="flex flex-col gap-4">
      <div className="flex flex-col rounded-[12px] border border-line p-[22px]">
        <div className="flex items-baseline justify-between gap-4">
          <h2 className="font-display text-[17px] font-semibold tracking-[-0.005em] text-text">
            {t("insights.cardLanguages")}
          </h2>
          {languages.length > 0 && (
            <p className="font-ui text-[11px] font-semibold uppercase tracking-[0.06em] text-ink-3">
              {t("insights.cardWordsTotal")} ·{" "}
              {formatNumber(totalLanguageWords)}
            </p>
          )}
        </div>
        {languages.length === 0 ? (
          <p className="mt-4 text-[13px] text-ink-2">
            {t("insights.voiceEmpty")}
          </p>
        ) : (
          <div className="mt-5 flex flex-1 flex-col justify-between gap-2.5">
            {languages.map((slot) => (
              <BarRow
                key={slot.language}
                label={languageNames(slot.language)}
                percent={
                  totalLanguageWords
                    ? Math.round((slot.words / totalLanguageWords) * 100)
                    : 0
                }
                widthPercent={(slot.words / maxLanguageWords) * 100}
                count={formatNumber(slot.words)}
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );

  const topTab =
    longest && busiestDay && peak !== null ? (
      <div className="flex gap-4">
        <StatCard
          label={t("insights.topLongest")}
          value={formatNumber(wordCount(entryText(longest)))}
        >
          <p className="mt-4 truncate text-[12px] text-ink-2">
            {shortDate(new Date(longest.timestamp * 1000), locale)}
            {longest.title.trim() ? ` · ${longest.title}` : ""}
          </p>
        </StatCard>
        <StatCard label={t("insights.cardPeak")} value={hourLabel(peak)}>
          <p className="mt-4 text-[12px] text-ink-2">
            {t("insights.topHourCount", { count: hours[peak] })}
          </p>
        </StatCard>
        <StatCard
          label={t("insights.topDayTitle")}
          value={shortDate(busiestDay.date, locale)}
        >
          <p className="mt-4 text-[12px] text-ink-2">
            {t("insights.topDayWords", { count: busiestDay.words })}
          </p>
        </StatCard>
      </div>
    ) : null;

  return (
    <Page width={1040}>
      <h1 className="font-display text-[27px] font-semibold leading-[34px] tracking-[-0.01em] text-text">
        {t("insights.screenTitle")}
      </h1>
      <div
        className="mt-5 flex items-center gap-6 border-b border-line"
        role="tablist"
      >
        {TABS.map(({ id, key }) => (
          <button
            key={id}
            type="button"
            role="tab"
            aria-selected={tab === id}
            onClick={() => setTab(id)}
            className={`relative -mb-px cursor-pointer pb-3 font-ui text-[14px] font-medium transition-colors ${
              tab === id ? "text-text" : "text-ink-3 hover:text-ink-2"
            }`}
          >
            {t(key)}
            {tab === id && (
              <span className="absolute inset-x-0 bottom-0 h-[1.5px] bg-text" />
            )}
          </button>
        ))}
      </div>
      <div className="mt-6">
        {tab === "usage" ? usageTab : tab === "voice" ? voiceTab : topTab}
      </div>
    </Page>
  );
};
