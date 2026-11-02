import assert from "node:assert/strict";
import {
  baseName,
  buildTranscriptMarkdown,
  formatSegmentTimestamp,
  segmentLine,
  suggestedExportFileName,
  transcriptText,
} from "./transcriptExport";
import type { HistoryEntry, TranscriptSegment } from "@/bindings";

const segment = (t0Ms: number, text: string): TranscriptSegment => ({
  t0_ms: t0Ms,
  t1_ms: t0Ms + 1000,
  text,
});

assert.equal(formatSegmentTimestamp(0), "00:00");
assert.equal(formatSegmentTimestamp(5000), "00:05");
assert.equal(formatSegmentTimestamp(65000), "01:05");
assert.equal(formatSegmentTimestamp(650_000), "10:50");
assert.equal(formatSegmentTimestamp(75.5 * 60_000), "75:30");
assert.equal(formatSegmentTimestamp(-100), "00:00");

const segments = [
  segment(0, "  First thing said.  "),
  segment(72_400, "Second thing said."),
  segment(3_600_000, "An hour in."),
];

assert.equal(segmentLine(segments[0]), "[00:00] First thing said.");
assert.equal(
  transcriptText(segments),
  [
    "[00:00] First thing said.",
    "[01:12] Second thing said.",
    "[60:00] An hour in.",
  ].join("\n"),
);

const markdown = buildTranscriptMarkdown(segments, {
  title: "November 3, 2025 - 2:05PM",
  date: "3. november 2025, 14:05",
  dateLabel: "Dato",
  durationLabel: "Varighet",
  durationSeconds: 725,
});

// Header: title, date and duration, then every segment line in order.
const lines = markdown.split("\n");
assert.equal(lines[0], "# November 3, 2025 - 2:05PM");
assert.ok(lines.includes("**Dato:** 3. november 2025, 14:05"));
assert.ok(lines.includes("**Varighet:** 12:05"));
assert.ok(lines.includes("[00:00] First thing said."));
assert.ok(lines.includes("[01:12] Second thing said."));
assert.ok(lines.includes("[60:00] An hour in."));
assert.equal(lines.indexOf("[00:00] First thing said."), 5);
assert.ok(markdown.endsWith("\n"), "file ends with a newline");

// Without a duration, the duration line is omitted entirely.
const noDuration = buildTranscriptMarkdown(segments.slice(0, 1), {
  title: "T",
  date: "d",
  dateLabel: "Date",
  durationLabel: "Duration",
});
assert.ok(!noDuration.includes("Duration"), "no duration line when unknown");
assert.equal(
  noDuration.split("\n").length,
  6,
  "heading, blank, date, blank, segment, trailing newline",
);

const entry = {
  id: 1,
  file_name: "a.wav",
  timestamp: 1730642700, // 2024-11-03 14:05 UTC
  saved: false,
  title: "t",
  transcription_text: "",
  post_processed_text: null,
  post_process_prompt: null,
  post_process_requested: false,
  source_name: null,
  language: null,
  language_source: null,
  source: null,
  metadata: null,
} as HistoryEntry;

const local = new Date(1730642700 * 1000);
const pad = (value: number) => String(value).padStart(2, "0");
assert.equal(
  suggestedExportFileName(entry),
  `transcript-${local.getFullYear()}-${pad(local.getMonth() + 1)}-${pad(local.getDate())}-${pad(local.getHours())}${pad(local.getMinutes())}.md`,
);

assert.equal(baseName("/Users/es/Notes/transcript.md"), "transcript.md");
assert.equal(baseName("C:\\Users\\es\\notes.md"), "notes.md");
assert.equal(baseName("transcript.md"), "transcript.md");

console.log("transcriptExport: all assertions passed");
