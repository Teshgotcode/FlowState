import assert from "node:assert/strict";
import {
  EXPORT_EXTENSIONS,
  baseName,
  buildSrt,
  buildTranscriptMarkdown,
  buildVtt,
  formatSegmentTimestamp,
  formatSrtTimecode,
  formatVttTimecode,
  sanitizeTitleStem,
  segmentLine,
  suggestedExportFileName,
  transcriptText,
  wrapSubtitleText,
  SUBTITLE_MAX_LINE_LENGTH,
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

// --- Subtitle timecodes -------------------------------------------------

assert.equal(formatSrtTimecode(0), "00:00:00,000");
assert.equal(formatSrtTimecode(12_340), "00:00:12,340");
assert.equal(formatSrtTimecode(3_723_456), "01:02:03,456");
assert.equal(formatSrtTimecode(-5), "00:00:00,000", "negative clamps to 0");
assert.equal(formatVttTimecode(12_340), "00:00:12.340");
assert.equal(formatVttTimecode(3_723_456), "01:02:03.456");

// --- Subtitle line wrapping --------------------------------------------

assert.equal(SUBTITLE_MAX_LINE_LENGTH, 42);
assert.deepEqual(wrapSubtitleText("short"), ["short"]);
assert.deepEqual(wrapSubtitleText("  spaced   out  "), ["spaced out"]);

const longText =
  "The quick brown fox jumps over the lazy dog and then keeps on running far beyond the meadow";
const wrapped = wrapSubtitleText(longText);
assert.ok(
  wrapped.every((line) => line.length <= 42),
  "every subtitle row is at most 42 characters",
);
assert.equal(wrapped.join(" "), longText, "wrapping keeps every word");

// A single token longer than a line is hard-split, never dropped.
assert.deepEqual(wrapSubtitleText("x".repeat(100)), [
  "x".repeat(42),
  "x".repeat(42),
  "x".repeat(16),
]);

// --- SRT ----------------------------------------------------------------

const srt = buildSrt(segments);
assert.equal(
  srt,
  [
    "1",
    "00:00:00,000 --> 00:00:01,000",
    "First thing said.",
    "",
    "2",
    "00:01:12,400 --> 00:01:13,400",
    "Second thing said.",
    "",
    "3",
    "01:00:00,000 --> 01:00:01,000",
    "An hour in.",
    "",
  ].join("\n"),
);
assert.ok(srt.endsWith("\n"), "SRT ends with a newline");
for (const cue of srt.split("\n\n")) {
  const [index, timing, ...textLines] = cue.split("\n");
  assert.match(index, /^\d+$/, "cues are numbered");
  assert.match(
    timing,
    /^\d{2}:\d{2}:\d{2},\d{3} --> \d{2}:\d{2}:\d{2},\d{3}$/,
    "SRT timing lines use comma milliseconds",
  );
  assert.ok(textLines.length > 0, "each cue has text");
  assert.ok(
    textLines.every((line) => line.length <= 42),
    "each SRT row is at most 42 characters",
  );
}

// A segment whose end is not after its start still gets a playable cue.
const degenerate = buildSrt([{ t0_ms: 5000, t1_ms: 0, text: "Recovered" }]);
assert.ok(
  degenerate.includes("00:00:05,000 --> 00:00:06,000"),
  "zero-length cues get a one-second fallback",
);

// --- VTT ----------------------------------------------------------------

const vtt = buildVtt(segments);
assert.ok(vtt.startsWith("WEBVTT\n\n"), "VTT starts with the header");
assert.ok(vtt.endsWith("\n"), "VTT ends with a newline");
for (const cue of vtt.slice("WEBVTT\n\n".length).split("\n\n")) {
  const [timing, ...textLines] = cue.split("\n");
  assert.match(
    timing,
    /^\d{2}:\d{2}:\d{2}\.\d{3} --> \d{2}:\d{2}:\d{2}\.\d{3}$/,
    "VTT timing lines use dot milliseconds",
  );
  assert.ok(textLines.length > 0, "each cue has text");
  assert.ok(
    textLines.every((line) => line.length <= 42),
    "each VTT row is at most 42 characters",
  );
}
assert.ok(
  vtt.includes("00:00:00.000 --> 00:00:01.000\nFirst thing said."),
  "the first cue carries the first segment",
);

// --- File names ----------------------------------------------------------

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
const stamp = `${local.getFullYear()}-${pad(local.getMonth() + 1)}-${pad(local.getDate())}-${pad(local.getHours())}${pad(local.getMinutes())}`;

// The suggested name is the entry's title plus its date.
assert.equal(suggestedExportFileName(entry), `t-${stamp}.md`);
assert.equal(suggestedExportFileName(entry, "srt"), `t-${stamp}.srt`);
assert.equal(suggestedExportFileName(entry, "vtt"), `t-${stamp}.vtt`);
assert.equal(EXPORT_EXTENSIONS.srt, "srt");
assert.equal(EXPORT_EXTENSIONS.vtt, "vtt");

// Titles that filenames cannot carry are cleaned up.
const titled = { ...entry, title: 'Lecture: "Q3" <draft>?' } as HistoryEntry;
assert.equal(
  suggestedExportFileName(titled, "srt"),
  `Lecture Q3 draft-${stamp}.srt`,
);
assert.equal(suggestedTitleFallback(), `transcript-${stamp}.srt`);

function suggestedTitleFallback() {
  const untitled = { ...entry, title: "  ?  " } as HistoryEntry;
  return suggestedExportFileName(untitled, "srt");
}

// An unreadable date falls back to a plain name in the chosen format.
const noDate = { ...entry, timestamp: Number.NaN } as HistoryEntry;
assert.equal(suggestedExportFileName(noDate, "srt"), "transcript.srt");
assert.equal(suggestedExportFileName(noDate), "transcript.md");

assert.equal(sanitizeTitleStem(""), "transcript");
assert.equal(sanitizeTitleStem("name..."), "name");

assert.equal(baseName("/Users/es/Notes/transcript.md"), "transcript.md");
assert.equal(baseName("C:\\Users\\es\\notes.md"), "notes.md");
assert.equal(baseName("transcript.md"), "transcript.md");

// Speakers (S-29): a diarized meeting's segments carry a speaker number, and
// the exports put the speaker's label — the name the user gave the speaker,
// or the speaker's own number — in front of the text. Segments without a
// speaker (a dictation, or a meeting on a machine without the model) read
// exactly as they did before.

const speakerSegment = (
  t0Ms: number,
  text: string,
  speakerId: number,
): TranscriptSegment => ({
  t0_ms: t0Ms,
  t1_ms: t0Ms + 1000,
  text,
  speaker_id: speakerId,
});

const meeting = [
  speakerSegment(0, "Hei, Kari her.", 1),
  speakerSegment(5000, "Hei! Ola her.", 2),
  segment(12_000, "Uten taler."),
];
const names = ["Kari", null];

assert.equal(segmentLine(meeting[0], names), "[00:00] Kari: Hei, Kari her.");
assert.equal(segmentLine(meeting[1], names), "[00:05] S2: Hei! Ola her.");
assert.equal(segmentLine(meeting[2], names), "[00:12] Uten taler.");
// Without names the speaker's own number is the label.
assert.equal(segmentLine(meeting[0]), "[00:00] S1: Hei, Kari her.");

assert.equal(
  transcriptText(meeting, names),
  [
    "[00:00] Kari: Hei, Kari her.",
    "[00:05] S2: Hei! Ola her.",
    "[00:12] Uten taler.",
  ].join("\n"),
);

const speakerMarkdown = buildTranscriptMarkdown(
  meeting,
  {
    title: "Møte",
    date: "3. november 2025, 14:05",
    dateLabel: "Dato",
    durationLabel: "Varighet",
  },
  names,
);
assert.ok(speakerMarkdown.includes("[00:00] Kari: Hei, Kari her."));
assert.ok(speakerMarkdown.includes("[00:05] S2: Hei! Ola her."));

const speakerSrt = buildSrt(meeting, names);
assert.ok(speakerSrt.includes("Kari: Hei, Kari her."));
assert.ok(speakerSrt.includes("S2: Hei! Ola her."));

const speakerVtt = buildVtt(meeting, names);
assert.ok(speakerVtt.includes("Kari: Hei, Kari her."));
assert.ok(speakerVtt.includes("S2: Hei! Ola her."));

console.log("transcriptExport: all assertions passed");
