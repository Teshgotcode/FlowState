import type { HistoryEntry, TranscriptSegment } from "@/bindings";

/** Milliseconds from the start of the recording as `mm:ss` (05:07). Minutes
 * keep counting past 59 (105:30) rather than growing a third unit, so any
 * length of meeting stays readable. */
export const formatSegmentTimestamp = (ms: number): string => {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const minutes = Math.floor(totalSeconds / 60);
  const seconds = totalSeconds % 60;
  return `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
};

/** One segment as the export line the task asks for: `[mm:ss] text`. */
export const segmentLine = (segment: TranscriptSegment): string =>
  `[${formatSegmentTimestamp(segment.t0_ms)}] ${segment.text.trim()}`;

/** Every segment as a `[mm:ss] text` line, in order. */
export const transcriptLines = (segments: TranscriptSegment[]): string[] =>
  segments.map(segmentLine);

/** The whole transcript as plain lines — what "copy transcript" puts on the
 * clipboard and the body of the Markdown export. */
export const transcriptText = (segments: TranscriptSegment[]): string =>
  transcriptLines(segments).join("\n");

/** Labels and header facts for the Markdown export; the caller passes them
 * localized (i18next) so the document reads in the app's language. */
export interface TranscriptMarkdownOptions {
  title: string;
  date: string;
  dateLabel: string;
  durationLabel: string;
  /** Seconds the recording ran, when the entry knows; `null`/absent omits
   * the duration line (old entries and dictations have none). */
  durationSeconds?: number | null;
}

/** The whole transcript as a Markdown document: a title heading, the date
 * and duration, then one `[mm:ss] text` line per segment in order. */
export const buildTranscriptMarkdown = (
  segments: TranscriptSegment[],
  options: TranscriptMarkdownOptions,
): string => {
  const lines = [
    `# ${options.title}`,
    "",
    `**${options.dateLabel}:** ${options.date}`,
  ];
  if (typeof options.durationSeconds === "number") {
    lines.push(
      `**${options.durationLabel}:** ${formatSegmentTimestamp(options.durationSeconds * 1000)}`,
    );
  }
  lines.push("", ...transcriptLines(segments));
  return `${lines.join("\n")}\n`;
};

/** The formats the transcript window offers. Markdown is the reading
 * export; SRT and VTT are subtitle files for video players. */
export type TranscriptExportFormat = "markdown" | "srt" | "vtt";

/** File extension per export format. */
export const EXPORT_EXTENSIONS: Record<TranscriptExportFormat, string> = {
  markdown: "md",
  srt: "srt",
  vtt: "vtt",
};

/** `00:00:12,340` (SRT) or `00:00:12.340` (VTT): the hour:minute:second
 * stamp subtitle files require, with milliseconds behind `separator`. */
const subtitleTimecode = (ms: number, separator: "," | "."): string => {
  const clamped = Math.max(0, Math.round(ms));
  const hours = Math.floor(clamped / 3_600_000);
  const minutes = Math.floor((clamped % 3_600_000) / 60_000);
  const seconds = Math.floor((clamped % 60_000) / 1000);
  const millis = clamped % 1000;
  const pad = (value: number, length = 2) =>
    String(value).padStart(length, "0");
  return `${pad(hours)}:${pad(minutes)}:${pad(seconds)}${separator}${pad(millis, 3)}`;
};

/** The SRT timestamp, comma before the milliseconds: `00:00:12,340`. */
export const formatSrtTimecode = (ms: number): string =>
  subtitleTimecode(ms, ",");

/** The WebVTT timestamp, a dot before the milliseconds: `00:00:12.340`. */
export const formatVttTimecode = (ms: number): string =>
  subtitleTimecode(ms, ".");

/** Subtitle players show one or two short rows per cue; 42 characters is the
 * classic line limit that keeps every row readable on a TV. */
export const SUBTITLE_MAX_LINE_LENGTH = 42;

/** Splits `text` into lines of at most `maxLength` characters, breaking at
 * spaces where possible and hard-splitting words longer than a whole line, so
 * no cue row ever overflows. */
export const wrapSubtitleText = (
  text: string,
  maxLength = SUBTITLE_MAX_LINE_LENGTH,
): string[] => {
  const lines: string[] = [];
  let current = "";
  const pushCurrent = () => {
    if (current) lines.push(current);
    current = "";
  };
  for (const word of text.trim().split(/\s+/).filter(Boolean)) {
    if (word.length > maxLength) {
      // A single token longer than a line (a URL, say) is hard-split.
      pushCurrent();
      for (let index = 0; index < word.length; index += maxLength) {
        lines.push(word.slice(index, index + maxLength));
      }
      continue;
    }
    if (!current) {
      current = word;
    } else if (current.length + 1 + word.length <= maxLength) {
      current += ` ${word}`;
    } else {
      pushCurrent();
      current = word;
    }
  }
  pushCurrent();
  return lines.length > 0 ? lines : [""];
};

/** When a cue ends, preferring the segment's own end and falling back to
 * one second so no cue is ever empty-length (players reject `t --> t`). */
const cueEndMs = (segment: TranscriptSegment): number =>
  segment.t1_ms > segment.t0_ms ? segment.t1_ms : segment.t0_ms + 1000;

/** The whole transcript as a SubRip (.srt) file: numbered cues, each with an
 * `00:00:12,340 --> 00:00:14,000` timing line and text wrapped to ≤42
 * characters per row, separated by blank lines. VLC and QuickTime open it
 * as a subtitle track with the segments' own timings. */
export const buildSrt = (segments: TranscriptSegment[]): string =>
  segments
    .map((segment, index) =>
      [
        String(index + 1),
        `${formatSrtTimecode(segment.t0_ms)} --> ${formatSrtTimecode(cueEndMs(segment))}`,
        ...wrapSubtitleText(segment.text),
        "",
      ].join("\n"),
    )
    .join("\n");

/** The whole transcript as a WebVTT (.vtt) file: the `WEBVTT` header, then
 * cues like SRT but with dot-millisecond times and no numbering — the format
 * browsers load natively for `<track>`. */
export const buildVtt = (segments: TranscriptSegment[]): string =>
  `WEBVTT\n\n${segments
    .map((segment) =>
      [
        `${formatVttTimecode(segment.t0_ms)} --> ${formatVttTimecode(cueEndMs(segment))}`,
        ...wrapSubtitleText(segment.text),
        "",
      ].join("\n"),
    )
    .join("\n")}`;

/** The entry's title as a filesystem-safe file-name stem: characters
 * Windows and macOS refuse are dropped, runs of whitespace collapse to one
 * space, and it caps at 60 characters. Entries without a usable title fall
 * back to "transcript". */
export const sanitizeTitleStem = (title: string): string => {
  const stem = title
    .replace(/[\\/:*?"<>|\u0000-\u001f]/g, "")
    .replace(/\s+/g, " ")
    .replace(/[. ]+$/, "")
    .trim()
    .slice(0, 60)
    .replace(/[. ]+$/, "");
  return stem || "transcript";
};

/** The `Title-YYYY-MM-DD-HHMM.md` name the save dialog suggests: the entry's
 * title plus its date so saved files read and sort nicely, deterministic and
 * filesystem-safe. */
export const suggestedExportFileName = (
  entry: HistoryEntry,
  format: TranscriptExportFormat = "markdown",
): string => {
  const extension = EXPORT_EXTENSIONS[format];
  const date = new Date(entry.timestamp * 1000);
  if (isNaN(date.getTime())) return `transcript.${extension}`;
  const pad = (value: number) => String(value).padStart(2, "0");
  const stamp = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}`;
  return `${sanitizeTitleStem(entry.title ?? "")}-${stamp}.${extension}`;
};

/** The last path segment of a chosen file, for toasts. */
export const baseName = (path: string): string =>
  path.split(/[\\/]/).pop() || path;
