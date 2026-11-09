import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  FileText,
  Folder as FolderIcon,
  Mic,
  RefreshCcw,
  Search,
  Sparkles,
  Trash2,
  X,
} from "lucide-react";

import { commands, type HistoryEntry } from "@/bindings";
import { FAILURE_MESSAGE_KEYS } from "@/components/settings/history/MeetingAskPanel";
import { TranscriptDialog } from "@/components/settings/history/TranscriptDialog";
import { Page, PageHeader } from "@/components/hub/Primitives";
import { Spinner } from "@/components/ui/Spinner";
import {
  loadAssignments,
  loadFolders,
  newFolder,
  saveAssignments,
  saveFolders,
  type Folder,
} from "@/lib/notetaker/folders";

type Tab = "notes" | "folders";

/** A meeting has no name of its own, so the first sentence stands in. */
const durationOf = (entry: HistoryEntry) => {
  const seconds = entry.metadata?.duration_secs;
  if (!seconds) return null;
  const minutes = Math.round(seconds / 60);
  return minutes < 60 ? `${minutes} min` : `${Math.floor(minutes / 60)} t ${minutes % 60} min`;
};

const titleOf = (entry: HistoryEntry) => {
  const text = (entry.post_processed_text || entry.transcription_text || "").trim();
  const first = text.split(/(?<=[.!?])\s/)[0] ?? text;
  return first.length > 64 ? `${first.slice(0, 61).trimEnd()}...` : first;
};

/**
 * Notetaker: the meetings in one column, grouped by day, with the selected
 * note's text beside them. Folders live in localStorage, like the rest of the
 * local-first state, so they never leave the machine.
 */
export const NotetakerPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || "nb";
  const [meetings, setMeetings] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [tab, setTab] = useState<Tab>("notes");
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [query, setQuery] = useState("");
  const [searchOpen, setSearchOpen] = useState(false);
  const [askQuestion, setAskQuestion] = useState("");
  const [asking, setAsking] = useState(false);
  const [askResult, setAskResult] = useState<{
    question: string;
    answer: string;
    source: string;
  } | null>(null);
  const [notesDraft, setNotesDraft] = useState("");
  const [notesSaved, setNotesSaved] = useState(false);
  const [folders, setFolders] = useState<Folder[]>(() => loadFolders());
  const [assignments, setAssignments] = useState<Record<string, string>>(() =>
    loadAssignments(),
  );
  const [activeFolder, setActiveFolder] = useState<string | null>(null);
  const [draftFolder, setDraftFolder] = useState("");

  const reload = React.useCallback(() => {
    setLoading(true);
    commands
      .getHistoryEntries(null, 200)
      .then((result) => {
        if (result.status !== "ok") return;
        const list = result.data.entries
          .filter((entry) => entry.source === "meeting")
          .sort((a, b) => b.timestamp - a.timestamp);
        setMeetings(list);
        setSelectedId((current) => current ?? list[0]?.id ?? null);
      })
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  /** Delete a meeting and everything that belongs to it: audio, segments,
   * note and its folder assignment. */
  const removeMeeting = async (entry: HistoryEntry) => {
    const result = await commands.deleteHistoryEntry(entry.id);
    if (result.status !== "ok") {
      toast.error(t("notetaker.deleteFailed"));
      return;
    }
    setSelectedId(null);
    const stored = loadAssignments();
    delete stored[String(entry.id)];
    saveAssignments(stored);
    setAssignments(stored);
    reload();
  };

  /** Ask across every meeting. The ask endpoint answers from a single
   * transcript, so the meeting whose text overlaps the question most is the
   * one asked; a question that matches nothing says so instead of guessing.
   * shortcut: one meeting per question, upgrade to multi-meeting retrieval
   * when answers start missing context spread over several calls. */
  const askAcrossMeetings = async () => {
    const question = askQuestion.trim();
    if (!question || asking) return;
    const words = question
      .toLowerCase()
      .split(/[^\p{L}\p{N}]+/u)
      .filter((word) => word.length > 2);
    const scored = meetings
      .map((entry) => {
        const haystack = `${entry.post_processed_text ?? ""} ${
          entry.transcription_text ?? ""
        }`.toLowerCase();
        const score = words.reduce(
          (sum, word) => sum + (haystack.includes(word) ? 1 : 0),
          0,
        );
        return { entry, score };
      })
      .sort((a, b) => b.score - a.score);
    const best = scored[0];
    if (!best || best.score === 0) {
      toast.error(t("notetaker.askNoMatch"));
      return;
    }
    setAsking(true);
    const result = await commands.askMeetingTranscript(best.entry.id, question);
    setAsking(false);
    if (result.status !== "ok" || result.data.outcome === "failed") {
      const reason =
        result.status === "ok" && result.data.outcome === "failed"
          ? result.data.reason
          : null;
      toast.error(
        reason ? t(FAILURE_MESSAGE_KEYS[reason]) : t("notetaker.askFailed"),
      );
      return;
    }
    setAskResult({
      question,
      answer: result.data.answer.answer,
      source: titleOf(best.entry),
    });
  };

  /** Save the field on blur; an unchanged field writes nothing. */
  const saveNotes = async () => {
    if (!selected) return;
    if (notesDraft.trim() === (selected.metadata?.user_notes ?? "").trim()) return;
    const result = await commands.setMeetingUserNotes(selected.id, notesDraft);
    if (result.status !== "ok") {
      toast.error(t("notetaker.notesFailed"));
      return;
    }
    const updated = result.data;
    setMeetings((list) => list.map((entry) => (entry.id === updated.id ? updated : entry)));
    setNotesSaved(true);
    setTimeout(() => setNotesSaved(false), 1500);
  };

  const timeFormat = useMemo(
    () => new Intl.DateTimeFormat(locale, { hour: "2-digit", minute: "2-digit" }),
    [locale],
  );
  const dayFormat = useMemo(
    () =>
      new Intl.DateTimeFormat(locale, {
        weekday: "short",
        day: "numeric",
        month: "short",
      }),
    [locale],
  );

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    return meetings.filter((entry) => {
      if (activeFolder && assignments[String(entry.id)] !== activeFolder) return false;
      if (!needle) return true;
      return `${titleOf(entry)} ${entry.transcription_text}`
        .toLowerCase()
        .includes(needle);
    });
  }, [meetings, assignments, activeFolder, query]);

  // Today's meetings already sit in the card above, so the list starts at
  // yesterday, the way the reference splits them.
  const past = useMemo(
    () =>
      visible.filter(
        (entry) => new Date(entry.timestamp * 1000).toDateString() !== new Date().toDateString(),
      ),
    [visible],
  );

  const groups = useMemo(() => {
    const byDay = new Map<string, HistoryEntry[]>();
    for (const entry of past) {
      const key = new Date(entry.timestamp * 1000).toDateString();
      const list = byDay.get(key) ?? [];
      list.push(entry);
      byDay.set(key, list);
    }
    return [...byDay.entries()];
  }, [past]);

  const selected = meetings.find((entry) => entry.id === selectedId) ?? null;

  /** The user's own notes for the selected meeting, saved when the field
   * loses focus. Kept out of the transcript and the generated note. */
  useEffect(() => {
    setNotesDraft(selected?.metadata?.user_notes ?? "");
  }, [selected]);
  const [transcriptOpen, setTranscriptOpen] = useState(false);

  const today = new Date().toDateString();
  const todayMeetings = visible.filter(
    (entry) => new Date(entry.timestamp * 1000).toDateString() === today,
  );

  const counts = useMemo(() => {
    const out: Record<string, number> = {};
    for (const folderId of Object.values(assignments)) {
      out[folderId] = (out[folderId] ?? 0) + 1;
    }
    return out;
  }, [assignments]);

  const persistFolders = (next: Folder[]) => {
    setFolders(next);
    saveFolders(next);
  };

  const persistAssignments = (next: Record<string, string>) => {
    setAssignments(next);
    saveAssignments(next);
  };

  const addFolder = () => {
    const name = draftFolder.trim();
    if (!name) return;
    persistFolders([...folders, newFolder(name)]);
    setDraftFolder("");
  };

  const removeFolder = (id: string) => {
    persistFolders(folders.filter((folder) => folder.id !== id));
    const next = { ...assignments };
    for (const [noteId, folderId] of Object.entries(next)) {
      if (folderId === id) delete next[noteId];
    }
    persistAssignments(next);
    if (activeFolder === id) setActiveFolder(null);
  };

  const setNoteFolder = (noteId: number, folderId: string) => {
    const next = { ...assignments };
    if (folderId) next[String(noteId)] = folderId;
    else delete next[String(noteId)];
    persistAssignments(next);
  };

  return (
    <Page>
      <PageHeader
        title={t("sidebar.notetaker")}
        subtitle={t("notetaker.subtitle")}
        action={
          <button
            type="button"
            onClick={() => commands.startMeeting().catch(() => {})}
            className="flex h-[32px] cursor-pointer items-center gap-[6px] rounded-[8px] border border-logo-primary/40 bg-background-ui px-[12px] text-[13px] font-[600] text-on-accent transition-opacity hover:opacity-90"
          >
            <Mic size={14} />
            {t("notetaker.start")}
          </button>
        }
      />

      <form
        onSubmit={(event) => {
          event.preventDefault();
          void askAcrossMeetings();
        }}
        className="mb-[18px] flex items-center gap-[8px]"
      >
        <input
          value={askQuestion}
          onChange={(event) => setAskQuestion(event.target.value)}
          placeholder={t("notetaker.askPlaceholder")}
          className="h-[38px] flex-1 rounded-[10px] border border-line bg-surface px-[12px] text-[13px] text-text outline-none placeholder:text-ink-3 focus:border-text/25"
        />
        <button
          type="submit"
          disabled={asking || !askQuestion.trim()}
          className="flex h-[38px] cursor-pointer items-center gap-[6px] rounded-[10px] border border-line bg-surface px-[14px] text-[13px] font-[550] text-text transition-colors hover:bg-wash disabled:cursor-default disabled:opacity-40"
        >
          {asking ? <Spinner /> : <Sparkles size={14} />}
          {t("notetaker.ask")}
        </button>
      </form>

      {askResult && (
        <div className="mb-[18px] rounded-[12px] border border-line bg-wash px-[16px] py-[12px]">
          <div className="flex items-start gap-[10px]">
            <div className="min-w-0 flex-1">
              <p className="text-[12px] text-ink-3">
                {t("notetaker.askSource", { title: askResult.source })}
              </p>
              <p className="mt-[6px] whitespace-pre-wrap text-[13px] leading-[21px] text-text">
                {askResult.answer}
              </p>
            </div>
            <button
              type="button"
              onClick={() => setAskResult(null)}
              aria-label={t("common.close")}
              className="flex h-[22px] w-[22px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 hover:bg-surface hover:text-text"
            >
              <X size={13} />
            </button>
          </div>
        </div>
      )}

      <div className="flex items-start gap-[24px]">
        <div className="min-w-0 flex-1">
          <section className="rounded-[12px] border border-line px-[20px] py-[16px]">
            <div className="flex items-center justify-between">
              <p className="text-[12px] font-[600] uppercase tracking-[0.06em] text-ink-3">
                {t("notetaker.today")}
              </p>
              <button
                type="button"
                onClick={reload}
                aria-label={t("notetaker.refresh")}
                title={t("notetaker.refresh")}
                className="flex h-[24px] w-[24px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
              >
                <RefreshCcw size={14} />
              </button>
            </div>
            {todayMeetings.length === 0 ? (
              <p className="py-[18px] text-center text-[15px] text-text/70">
                {t("notetaker.noMeetingsToday")}
              </p>
            ) : (
              <div className="mt-[8px] flex flex-col">
                {todayMeetings.map((entry) => (
                  <button
                    key={entry.id}
                    type="button"
                    onClick={() => setSelectedId(entry.id)}
                    className="flex cursor-pointer items-center gap-[10px] rounded-[8px] px-[6px] py-[6px] text-start transition-colors hover:bg-surface"
                  >
                    <span className="text-[12px] tabular-nums text-ink-3">
                      {timeFormat.format(new Date(entry.timestamp * 1000))}
                    </span>
                    <span className="min-w-0 flex-1 truncate text-[13px] text-text">
                      {titleOf(entry)}
                    </span>
                  </button>
                ))}
              </div>
            )}
          </section>

          <div className="mt-[18px] flex items-center gap-[16px] border-b border-line">
            {(["notes", "folders"] as Tab[]).map((id) => (
              <button
                key={id}
                type="button"
                onClick={() => {
                  setTab(id);
                  setActiveFolder(null);
                }}
                className={`-mb-px cursor-pointer border-b-2 pb-[8px] text-[13px] transition-colors ${
                  tab === id
                    ? "border-text font-[550] text-text"
                    : "border-transparent text-ink-3 hover:text-text"
                }`}
              >
                {t(id === "notes" ? "notetaker.tabNotes" : "notetaker.tabFolders")}
              </button>
            ))}
            <div className="ms-auto flex items-center pb-[8px]">
              {searchOpen ? (
                <input
                  autoFocus
                  value={query}
                  onChange={(event) => setQuery(event.target.value)}
                  onBlur={() => {
                    if (!query) setSearchOpen(false);
                  }}
                  onKeyDown={(event) => {
                    if (event.key === "Escape") {
                      setQuery("");
                      setSearchOpen(false);
                    }
                  }}
                  placeholder={t("notetaker.search")}
                  className="h-[28px] w-[160px] rounded-[6px] border border-line bg-surface px-[8px] text-[12px] text-text outline-none placeholder:text-ink-3"
                />
              ) : (
                <button
                  type="button"
                  onClick={() => setSearchOpen(true)}
                  aria-label={t("notetaker.search")}
                  className="flex h-[24px] w-[24px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
                >
                  <Search size={14} />
                </button>
              )}
            </div>
          </div>

          {activeFolder && (
            <button
              type="button"
              onClick={() => setActiveFolder(null)}
              className="mt-[12px] flex cursor-pointer items-center gap-[6px] rounded-[6px] bg-wash px-[8px] py-[4px] text-[12px] text-text"
            >
              <FolderIcon size={12} />
              {folders.find((folder) => folder.id === activeFolder)?.name}
              <X size={12} className="text-ink-3" />
            </button>
          )}

          {loading ? (
            <p className="py-[40px] text-center text-[13px] text-ink-3">
              {t("home.loading")}
            </p>
          ) : tab === "folders" ? (
            <div className="mt-[12px]">
              <div className="flex items-center gap-[8px]">
                <input
                  value={draftFolder}
                  onChange={(event) => setDraftFolder(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Enter") addFolder();
                  }}
                  placeholder={t("notetaker.folderName")}
                  className="h-[32px] w-[200px] rounded-[8px] border border-line bg-surface px-[10px] text-[13px] text-text outline-none placeholder:text-ink-3 focus:border-text/25"
                />
                <button
                  type="button"
                  onClick={addFolder}
                  disabled={!draftFolder.trim()}
                  className="h-[32px] cursor-pointer rounded-[8px] border border-logo-primary/40 bg-background-ui px-[12px] text-[13px] font-[600] text-on-accent disabled:cursor-not-allowed disabled:opacity-45"
                >
                  {t("notetaker.newFolder")}
                </button>
              </div>
              {folders.length === 0 ? (
                <p className="mt-[16px] text-[13px] text-ink-3">
                  {t("notetaker.noFolders")}
                </p>
              ) : (
                <div className="mt-[14px] overflow-hidden rounded-[10px] border border-line">
                  {folders.map((folder, index) => (
                    <div
                      key={folder.id}
                      className={`flex h-[48px] items-center gap-[8px] px-[12px] ${
                        index > 0 ? "border-t border-line" : ""
                      }`}
                    >
                      <button
                        type="button"
                        onClick={() => {
                          setActiveFolder(folder.id);
                          setTab("notes");
                        }}
                        className="flex min-w-0 flex-1 cursor-pointer items-center gap-[8px] text-start"
                      >
                        <FolderIcon size={15} className="shrink-0 text-ink-3" />
                        <span className="truncate text-[14px] text-text">
                          {folder.name}
                        </span>
                        <span className="shrink-0 text-[12px] text-ink-3">
                          {t("notetaker.folderCount", { count: counts[folder.id] ?? 0 })}
                        </span>
                      </button>
                      <button
                        type="button"
                        onClick={() => removeFolder(folder.id)}
                        aria-label={t("notetaker.deleteFolder")}
                        className="flex h-[24px] w-[24px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
                      >
                        <Trash2 size={13} />
                      </button>
                    </div>
                  ))}
                </div>
              )}
            </div>
          ) : groups.length === 0 ? (
            <p className="py-[40px] text-center text-[13px] text-ink-3">
              {activeFolder ? t("notetaker.noNotes") : t("notetaker.emptyBody")}
            </p>
          ) : (
            <div className="mt-[12px] flex flex-col gap-[16px]">
              {groups.map(([day, entries]) => (
                <div key={day}>
                  <p className="mb-[8px] text-[11px] font-[600] uppercase tracking-[0.06em] text-ink-3">
                    {dayFormat.format(new Date(entries[0].timestamp * 1000))}
                  </p>
                  <div className="overflow-hidden rounded-[12px] border border-line bg-surface">
                    {entries.map((entry, index) => (
                      <div
                        key={entry.id}
                        role="button"
                        tabIndex={0}
                        onClick={() => setSelectedId(entry.id)}
                        onKeyDown={(event) => {
                          if (event.key === "Enter" || event.key === " ") {
                            event.preventDefault();
                            setSelectedId(entry.id);
                          }
                        }}
                        className={`group relative flex w-full cursor-pointer items-center gap-[12px] px-[12px] py-[12px] text-start transition-colors hover:bg-wash ${
                          index > 0 ? "border-t border-line" : ""
                        } ${selectedId === entry.id ? "bg-wash" : ""}`}
                      >
                        {selectedId === entry.id && (
                          <span className="absolute inset-y-[6px] start-0 w-[2.5px] rounded-full bg-logo-primary" />
                        )}
                        <span className="flex h-[32px] w-[32px] shrink-0 items-center justify-center rounded-[8px] bg-surface-soft text-ink-3">
                          <Mic size={15} />
                        </span>
                        <span className="min-w-0 flex-1">
                          <span className="block truncate text-[14px] font-[550] text-text">
                            {titleOf(entry)}
                          </span>
                          <span className="mt-[2px] block text-[12px] text-ink-3">
                            {timeFormat.format(new Date(entry.timestamp * 1000))}
                          </span>
                        </span>
                        {/* Folder assignment on the row itself, the reference's
                            "Add to folder". */}
                        <select
                          value={assignments[String(entry.id)] ?? ""}
                          onClick={(event) => event.stopPropagation()}
                          onChange={(event) => {
                            const value = event.target.value;
                            const next = { ...assignments };
                            if (value) next[String(entry.id)] = value;
                            else delete next[String(entry.id)];
                            setAssignments(next);
                            saveAssignments(next);
                          }}
                          aria-label={t("notetaker.addToFolder")}
                          title={t("notetaker.addToFolder")}
                          className={`h-[24px] max-w-[120px] cursor-pointer rounded-[6px] border border-line bg-surface px-[4px] text-[11.5px] text-ink-3 opacity-0 outline-none transition-opacity focus:opacity-100 group-hover:opacity-100 ${
                            assignments[String(entry.id)] ? "opacity-100" : ""
                          }`}
                        >
                          <option value="">{t("notetaker.addToFolder")}</option>
                          {folders.map((folder) => (
                            <option key={folder.id} value={folder.id}>
                              {folder.name}
                            </option>
                          ))}
                        </select>
                      </div>
                    ))}
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>

        <aside className="w-[360px] shrink-0 overflow-y-auto border-s border-line ps-[26px]">
          {selected ? (
            <>
              <h2 className="font-display text-[20px] font-semibold leading-[27px] tracking-[-0.005em] text-text">
                {titleOf(selected)}
              </h2>
              <p className="mt-[5px] text-[11.5px] text-ink-3">
                {dayFormat.format(new Date(selected.timestamp * 1000))} ·{" "}
                {timeFormat.format(new Date(selected.timestamp * 1000))}
                {durationOf(selected) ? ` · ${durationOf(selected)}` : ""}
                {selected.metadata?.meeting_app
                  ? ` · ${selected.metadata.meeting_app}`
                  : ""}
              </p>
              <div className="mt-[12px] flex items-center gap-[8px]">
                <button
                  type="button"
                  onClick={() => setTranscriptOpen(true)}
                  className="flex h-[30px] cursor-pointer items-center gap-[6px] rounded-[7px] border border-line bg-surface px-[10px] text-[12.5px] font-[550] text-text transition-colors hover:bg-wash"
                >
                  <FileText size={13} />
                  {t("notetaker.openTranscript")}
                </button>
                <button
                  type="button"
                  onClick={() => void removeMeeting(selected)}
                  aria-label={t("notetaker.deleteMeeting")}
                  title={t("notetaker.deleteMeeting")}
                  className="flex h-[30px] w-[30px] cursor-pointer items-center justify-center rounded-[7px] border border-line text-ink-3 transition-colors hover:bg-wash hover:text-text"
                >
                  <Trash2 size={14} />
                </button>
              </div>
              <div className="mt-[16px] border-t border-line pt-[16px]">
                <p className="flex items-center gap-[8px] text-[11px] font-[600] uppercase tracking-[0.06em] text-ink-3">
                  {t("notetaker.myThoughts")}
                  {notesSaved && (
                    <span className="text-[10.5px] normal-case tracking-normal text-ink-3">
                      {t("notetaker.notesSaved")}
                    </span>
                  )}
                </p>
                <textarea
                  value={notesDraft}
                  onChange={(event) => setNotesDraft(event.target.value)}
                  onBlur={() => void saveNotes()}
                  placeholder={t("notetaker.myThoughtsPlaceholder")}
                  rows={4}
                  className="mt-[8px] w-full resize-y rounded-[10px] border border-line bg-surface px-[10px] py-[8px] text-[12.5px] leading-[20px] text-text outline-none placeholder:text-ink-3 focus:border-text/25"
                />
              </div>
              <div className="mt-[16px] border-t border-line pt-[16px]">
                <p className="text-[11px] font-[600] uppercase tracking-[0.06em] text-ink-3">
                  {t("notetaker.overview")}
                </p>
                <p className="mt-[8px] max-h-[42vh] overflow-y-auto whitespace-pre-wrap text-[12.5px] leading-[21px] text-text/85">
                  {selected.post_processed_text || selected.transcription_text}
                </p>
              </div>
              <label className="mt-[20px] block text-[10.5px] font-[600] uppercase tracking-[0.08em] text-ink-3">
                {t("notetaker.folder")}
              </label>
              <select
                value={assignments[String(selected.id)] ?? ""}
                onChange={(event) => setNoteFolder(selected.id, event.target.value)}
                className="mt-[6px] h-[30px] w-full cursor-pointer appearance-none rounded-[7px] border border-line bg-surface px-[9px] text-[12.5px] text-text outline-none focus:border-text/25"
              >
                <option value="">{t("notetaker.noFolder")}</option>
                {folders.map((folder) => (
                  <option key={folder.id} value={folder.id}>
                    {folder.name}
                  </option>
                ))}
              </select>
            </>
          ) : (
            <p className="py-[40px] text-center text-[13px] text-ink-3">
              {t("notetaker.pickNote")}
            </p>
          )}
        </aside>
      </div>
      {selected && (
        <TranscriptDialog
          entry={selected}
          open={transcriptOpen}
          onOpenChange={(open) => {
            setTranscriptOpen(open);
            if (!open) reload();
          }}
        />
      )}
    </Page>
  );
};
