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

/** The `transcript-YYYY-MM-DD-HHMM.md` name the save dialog suggests, built
 * from the entry's timestamp so it is deterministic and filesystem-safe. */
export const suggestedExportFileName = (entry: HistoryEntry): string => {
  const date = new Date(entry.timestamp * 1000);
  if (isNaN(date.getTime())) return "transcript.md";
  const pad = (value: number) => String(value).padStart(2, "0");
  return `transcript-${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}-${pad(date.getHours())}${pad(date.getMinutes())}.md`;
};

/** The last path segment of a chosen file, for toasts. */
export const baseName = (path: string): string =>
  path.split(/[\\/]/).pop() || path;
