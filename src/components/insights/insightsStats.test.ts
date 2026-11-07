// Self-checks for the insights stats helpers the rewritten page adds.
// Run: bun src/components/insights/insightsStats.test.ts
import assert from "node:assert/strict";
import type { HistoryEntry } from "@/bindings";
import {
  sourceCounts,
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

console.log("insightsStats: all assertions passed");
