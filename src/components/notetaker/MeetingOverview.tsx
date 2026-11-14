import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Sparkles } from "lucide-react";
import { commands, type HistoryEntry, type MeetingNote } from "@/bindings";
import { Spinner } from "@/components/ui/Spinner";

interface MeetingOverviewProps {
  entry: HistoryEntry;
  /** Called after a note was generated, so the list can reload the entry. */
  onGenerated: () => void;
}

/**
 * The meeting's overview: the note a local LLM wrote over the transcript, in
 * the shape the reference uses — one lead paragraph, then headed topic
 * sections with the concrete points as bullets, then any tasks and open
 * questions. The transcript itself is a different pane; it never stands in for
 * the overview.
 */
export const MeetingOverview: React.FC<MeetingOverviewProps> = ({
  entry,
  onGenerated,
}) => {
  const { t } = useTranslation();
  const [generating, setGenerating] = useState(false);
  const note = entry.metadata?.meeting_note ?? null;

  const generate = async () => {
    if (generating) return;
    setGenerating(true);
    try {
      const result = await commands.generateMeetingNote(entry.id);
      if (result.status !== "ok" || result.data.outcome !== "generated") {
        toast.error(t("notetaker.noteFailed"));
        return;
      }
      onGenerated();
    } catch {
      toast.error(t("notetaker.noteFailed"));
    } finally {
      setGenerating(false);
    }
  };

  if (!note) {
    return (
      <div className="mt-[8px]">
        <p className="text-[12.5px] leading-[21px] text-ink-3">
          {t("notetaker.noNote")}
        </p>
        <button
          type="button"
          onClick={() => void generate()}
          disabled={generating}
          className="mt-[10px] flex h-[30px] cursor-pointer items-center gap-[6px] rounded-[7px] border border-line bg-surface px-[10px] text-[12.5px] font-[550] text-text transition-colors hover:bg-wash disabled:cursor-default disabled:opacity-50"
        >
          {generating ? <Spinner /> : <Sparkles size={13} />}
          {t("notetaker.makeNote")}
        </button>
      </div>
    );
  }

  return <NoteBody note={note} />;
};

/** The strings a note field holds, with the gaps a partial answer leaves out. */
const textList = (values?: (string | undefined)[]): string[] =>
  (values ?? []).filter((value): value is string => Boolean(value && value.trim()));

/** The note itself, in reading order: lead, topics, tasks, open questions. */
const NoteBody: React.FC<{ note: MeetingNote }> = ({ note }) => {
  const { t } = useTranslation();
  // Notes written before topic sections existed fall back to the fixed
  // buckets, so an old meeting still reads as an overview.
  const ownSections = note.sections ?? [];
  const sections: { heading: string; bullets: string[] }[] =
    ownSections.length > 0
      ? ownSections.map((section) => ({
          heading: section.heading ?? "",
          bullets: (section.bullets ?? []).filter(
            (bullet): bullet is string => typeof bullet === "string",
          ),
        }))
      : [
          {
            heading: t("notetaker.noteDecisions"),
            bullets: textList(note.decisions),
          },
          {
            heading: t("notetaker.noteActions"),
            bullets: (note.action_items ?? [])
              .map((item) =>
                item.task
                  ? item.owner
                    ? `${item.task} (${item.owner})`
                    : item.task
                  : "",
              )
              .filter(Boolean),
          },
          {
            heading: t("notetaker.noteQuestions"),
            bullets: textList(note.open_questions),
          },
        ].filter((section) => section.bullets.length > 0);

  return (
    <div className="mt-[8px] max-h-[46vh] overflow-y-auto pe-[4px]">
      {note.summary && (
        <p className="text-[12.5px] leading-[21px] text-text/90">
          {note.summary}
        </p>
      )}
      {sections.map((section) => (
        <section key={section.heading} className="mt-[14px]">
          <h3 className="text-[11px] font-[600] tracking-[-0.005em] text-ink-3">
            {section.heading}
          </h3>
          <ul className="mt-[6px] flex flex-col gap-[4px]">
            {section.bullets.map((bullet) => (
              <li
                key={bullet}
                className="flex gap-[7px] text-[12.5px] leading-[20px] text-text/85"
              >
                <span aria-hidden="true" className="mt-[8px] h-[3px] w-[3px] shrink-0 rounded-full bg-ink-3" />
                <span>{bullet}</span>
              </li>
            ))}
          </ul>
        </section>
      ))}
    </div>
  );
};
