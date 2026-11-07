import assert from "node:assert/strict";
import type { HistoryEntry } from "@/bindings";
import {
  activityByDay,
  activityByHour,
  currentStreak,
  longestStreak,
  peakHour,
  summarize,
  wordCount,
} from "./stats";

// HistoryEntry timestamps are Unix seconds.
const at = (y: number, m: number, d: number, h = 12, min = 0) =>
  Math.floor(new Date(y, m - 1, d, h, min).getTime() / 1000);

const entry = (
  timestamp: number,
  text: string,
  language = "no",
): HistoryEntry =>
  ({
    id: timestamp,
    file_name: "",
    timestamp,
    saved: false,
    title: "",
    transcription_text: text,
    post_processed_text: null,
    post_process_prompt: null,
    post_process_requested: false,
    source_name: null,
    language,
    language_source: null,
    source: null,
    metadata: null,
  }) as HistoryEntry;

// `now` stays in milliseconds; only entry timestamps are seconds.
const now = at(2026, 3, 10, 18) * 1000;

assert.equal(wordCount(""), 0);
assert.equal(wordCount("  hei   på   deg "), 3);

// A streak counts today and the days before it, and survives a day that has
// not started dictating yet.
const streakEntries = [
  entry(at(2026, 3, 10, 9), "én"),
  entry(at(2026, 3, 9, 9), "to"),
  entry(at(2026, 3, 8, 9), "tre"),
  entry(at(2026, 3, 6, 9), "hull"),
];
assert.equal(currentStreak(streakEntries, now), 3);
assert.equal(currentStreak([entry(at(2026, 3, 6, 9), "gammel")], now), 0);
assert.equal(
  currentStreak([entry(at(2026, 3, 9, 9), "i går")], now),
  1,
  "yesterday keeps the streak alive until today ends",
);
assert.equal(longestStreak(streakEntries), 3);

// Zero-filled window, oldest first, and entries land in their local day.
const days = activityByDay(streakEntries, 5, now);
assert.equal(days.length, 5);
assert.deepEqual(
  days.map((day) => day.count),
  [1, 0, 1, 1, 1],
);
assert.equal(days[4].words, 1);

const summary = summarize(streakEntries, now);
assert.equal(summary.dictations, 4);
assert.equal(summary.words, 4);
assert.equal(summary.wordsToday, 1);
assert.equal(summary.dictationsToday, 1);
assert.equal(summary.activeDays, 4);
assert.equal(summary.averageWords, 1);
assert.equal(summary.languages, 1);
assert.equal(peakHour(streakEntries), 9);

const hours = activityByHour(streakEntries);
assert.equal(hours.length, 24);
assert.equal(hours[9], 4);

console.log("insights stats: all assertions passed");
