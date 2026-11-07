import type { HistoryEntry } from "@/bindings";

/** HistoryEntry.timestamp is Unix seconds, not milliseconds. */
const toDate = (timestampSecs: number) => new Date(timestampSecs * 1000);

/** Words in a transcript, the way a reader counts them. */
export const wordCount = (text: string): number =>
  text.trim() ? text.trim().split(/\s+/).length : 0;

/** The text a row shows: the cleaned version when post-processing ran. */
export const entryText = (entry: HistoryEntry): string =>
  entry.post_processed_text?.trim() || entry.transcription_text;

/** Local calendar day, so "today" follows the clock the user reads. */
const dayKeyOf = (d: Date): string =>
  `${d.getFullYear()}-${d.getMonth() + 1}-${d.getDate()}`;

export const dayKey = (timestampSecs: number): string =>
  dayKeyOf(toDate(timestampSecs));

export interface DayBucket {
  key: string;
  /** Midnight of the day, for labels and axis ticks. */
  date: Date;
  words: number;
  count: number;
}

/** The last `days` calendar days, oldest first, zero-filled. */
export const activityByDay = (
  entries: HistoryEntry[],
  days: number,
  now = Date.now(),
): DayBucket[] => {
  const buckets = new Map<string, DayBucket>();
  const midnight = new Date(now);
  midnight.setHours(0, 0, 0, 0);

  for (let i = days - 1; i >= 0; i--) {
    const date = new Date(midnight);
    date.setDate(midnight.getDate() - i);
    const key = dayKeyOf(date);
    buckets.set(key, { key, date, words: 0, count: 0 });
  }

  for (const entry of entries) {
    const bucket = buckets.get(dayKey(entry.timestamp));
    if (!bucket) continue;
    bucket.words += wordCount(entryText(entry));
    bucket.count += 1;
  }

  return [...buckets.values()];
};

/**
 * Consecutive days with at least one dictation, counting back from today.
 * Yesterday still counts while today is young: a streak only breaks once a
 * whole day passed without a dictation.
 */
export const currentStreak = (
  entries: HistoryEntry[],
  now = Date.now(),
): number => {
  const active = new Set(entries.map((entry) => dayKey(entry.timestamp)));
  const cursor = new Date(now);
  cursor.setHours(0, 0, 0, 0);
  if (!active.has(dayKeyOf(cursor))) {
    cursor.setDate(cursor.getDate() - 1);
    if (!active.has(dayKeyOf(cursor))) return 0;
  }
  let streak = 0;
  while (active.has(dayKeyOf(cursor))) {
    streak += 1;
    cursor.setDate(cursor.getDate() - 1);
  }
  return streak;
};

/** The longest run of consecutive dictating days in the whole history. */
export const longestStreak = (entries: HistoryEntry[]): number => {
  const days = [...new Set(entries.map((entry) => dayKey(entry.timestamp)))]
    .map((key) => {
      const [y, m, d] = key.split("-").map(Number);
      return new Date(y, m - 1, d).getTime();
    })
    .sort((a, b) => a - b);

  let best = 0;
  let run = 0;
  let previous: number | null = null;
  for (const day of days) {
    const next = previous !== null && day - previous === 86_400_000;
    run = next ? run + 1 : 1;
    best = Math.max(best, run);
    previous = day;
  }
  return best;
};

/** The hour of day (0–23) the user most often dictates in, or null. */
export const peakHour = (entries: HistoryEntry[]): number | null => {
  if (entries.length === 0) return null;
  const hours = new Array(24).fill(0) as number[];
  for (const entry of entries) hours[toDate(entry.timestamp).getHours()] += 1;
  return hours.indexOf(Math.max(...hours));
};

export interface InsightsSummary {
  words: number;
  dictations: number;
  wordsToday: number;
  dictationsToday: number;
  streak: number;
  longest: number;
  peakHour: number | null;
  languages: number;
  averageWords: number;
  activeDays: number;
}

export const summarize = (
  entries: HistoryEntry[],
  now = Date.now(),
): InsightsSummary => {
  const today = dayKeyOf(new Date(now));
  const words = entries.reduce(
    (total, entry) => total + wordCount(entryText(entry)),
    0,
  );
  const todays = entries.filter((entry) => dayKey(entry.timestamp) === today);

  return {
    words,
    dictations: entries.length,
    wordsToday: todays.reduce(
      (total, entry) => total + wordCount(entryText(entry)),
      0,
    ),
    dictationsToday: todays.length,
    streak: currentStreak(entries, now),
    longest: longestStreak(entries),
    peakHour: peakHour(entries),
    languages: new Set(entries.map((entry) => entry.language).filter(Boolean))
      .size,
    averageWords: entries.length ? Math.round(words / entries.length) : 0,
    activeDays: new Set(entries.map((entry) => dayKey(entry.timestamp))).size,
  };
};

/** 24 hourly buckets, for the "when do you flow" strip. */
export const activityByHour = (entries: HistoryEntry[]): number[] => {
  const hours = new Array(24).fill(0) as number[];
  for (const entry of entries) hours[toDate(entry.timestamp).getHours()] += 1;
  return hours;
};

/** How many entries are meeting sessions vs plain dictations. */
export const sourceCounts = (
  entries: HistoryEntry[],
): { meeting: number; other: number } => {
  const meeting = entries.filter((entry) => entry.source === "meeting").length;
  return { meeting, other: entries.length - meeting };
};

/** Dictations per weekday, index 0 = Monday … 6 = Sunday. */
export const weekdayActivity = (entries: HistoryEntry[]): number[] => {
  const days = new Array(7).fill(0) as number[];
  for (const entry of entries) {
    days[(toDate(entry.timestamp).getDay() + 6) % 7] += 1;
  }
  return days;
};

export interface LanguageSplit {
  language: string;
  words: number;
  count: number;
}

/** Words and dictations per known language, most words first. */
export const wordsByLanguage = (entries: HistoryEntry[]): LanguageSplit[] => {
  const byLanguage = new Map<string, LanguageSplit>();
  for (const entry of entries) {
    if (!entry.language) continue;
    const slot = byLanguage.get(entry.language) ?? {
      language: entry.language,
      words: 0,
      count: 0,
    };
    slot.words += wordCount(entryText(entry));
    slot.count += 1;
    byLanguage.set(entry.language, slot);
  }
  return [...byLanguage.values()].sort((a, b) => b.words - a.words);
};
