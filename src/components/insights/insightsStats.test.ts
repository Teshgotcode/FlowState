// Self-checks for the insights stats helpers the rewritten page adds.
// Run: bun src/components/insights/insightsStats.test.ts
import assert from "node:assert/strict";
import type { HistoryEntry } from "@/bindings";
import {
  corrections,
  editsBetween,
  sourceCounts,
  speechSpeed,
  weekdayActivity,
  wordsByLanguage,
} from "@/lib/insights/stats";

// HistoryEntry timestamps are Unix seconds.
const at = (y: number, m: number, d: number, h = 12) =>
  Math.floor(new Date(y, m - 1, d, h).getTime() / 1000);

const entry = (
  timestamp: number,
  text: string,
  language: string | null,
  source: HistoryEntry["source"],
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
    source,
    metadata: null,
  }) as HistoryEntry;

const meeting = entry(at(2026, 3, 10), "saksnr 42", "no", "meeting");
const plain = entry(at(2026, 3, 9), "hei på deg", "no", null);

assert.deepEqual(sourceCounts([]), { meeting: 0, other: 0 });
assert.deepEqual(sourceCounts([plain, meeting]), { meeting: 1, other: 1 });
// Only "meeting" is its own segment; notes and plain dictations share one.
assert.deepEqual(
  sourceCounts([meeting, entry(at(2026, 3, 8), "notat", null, "note")]),
  { meeting: 1, other: 1 },
);

// 2026-03-08 Sun, 2026-03-09 Mon, 2026-03-10 Tue → Mon, Tue, Sun each once.
assert.deepEqual(
  weekdayActivity([
    entry(at(2026, 3, 8), "a", null, null),
    entry(at(2026, 3, 9), "b", null, null),
    entry(at(2026, 3, 10), "c", null, null),
  ]),
  [1, 1, 0, 0, 0, 0, 1],
);
assert.deepEqual(weekdayActivity([]), [0, 0, 0, 0, 0, 0, 0]);

// Entries without a language drop out; the rest sort by words, desc.
const languages = wordsByLanguage([
  plain,
  entry(at(2026, 3, 8), "hello there", "en", null),
  entry(at(2026, 3, 8), "goddag", "no", null),
  entry(at(2026, 3, 7), "uten språk", null, null),
]);
assert.deepEqual(
  languages.map((slot) => slot.language),
  ["no", "en"],
);
assert.deepEqual(
  languages.map((slot) => [slot.words, slot.count]),
  [
    [4, 2],
    [2, 1],
  ],
);
assert.deepEqual(wordsByLanguage([]), []);

const timed = (
  text: string,
  secs: number,
  source: HistoryEntry["source"] = null,
): HistoryEntry => ({
  ...entry(at(2026, 3, 11), text, "no", source),
  metadata: { duration_secs: secs },
});

const polished = (raw: string, fixed: string): HistoryEntry => ({
  ...entry(at(2026, 3, 11), raw, "no", null),
  post_processed_text: fixed,
});

const sixty = Array.from({ length: 60 }, () => "ord").join(" ");

// Nothing timed yet: null rather than 0, so the page can say it is unmeasured.
assert.deepEqual(speechSpeed([]), { words: 0, seconds: 0, wpm: null, timed: 0 });

// 60 words in 30 seconds is 120 a minute.
assert.equal(speechSpeed([timed(sixty, 30)]).wpm, 120);
assert.equal(speechSpeed([timed(sixty, 30)]).timed, 1);

// A meeting's length is mostly silence, so meetings never enter the average.
assert.equal(speechSpeed([timed(sixty, 30), timed(sixty, 600, "meeting")]).timed, 1);
assert.equal(speechSpeed([timed(sixty, 600, "meeting")]).wpm, null);

// Entries saved before the length existed are skipped, not counted as instant.
assert.equal(speechSpeed([entry(at(2026, 3, 11), sixty, "no", null)]).timed, 0);

// Case and trailing punctuation are the polish pass's normal work, not a fix.
assert.equal(editsBetween("jeg heter tesla", "Jeg heter Tesla."), 0);
// A swapped word in the middle counts both sides: i/morgen out, på/torsdag in.
assert.equal(editsBetween("vi møtes i morgen", "vi møtes på torsdag"), 4);
assert.equal(editsBetween("", "hei"), 1);
assert.equal(editsBetween("hei", ""), 1);

assert.equal(corrections([polished("vi møtes i morgen", "vi møtes på torsdag")]), 4);
// No post-processing, nothing counted.
assert.equal(corrections([entry(at(2026, 3, 11), "uten puss", "no", null)]), 0);

console.log("insightsStats: all assertions passed");
