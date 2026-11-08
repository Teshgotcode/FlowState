import React, {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { useTranslation } from "react-i18next";
import { commands } from "@/bindings";
import { ArrowLeft, Pin, Plus, RefreshCw, Search, SquarePen, Trash2 } from "lucide-react";
import { wordCount } from "@/lib/insights/stats";
import {
  deleteNote,
  listNotes,
  saveNote,
  toggleNotePin,
  type Note,
} from "@/lib/notes/store";

const fmtDate = (ts: number, locale: string) =>
  new Intl.DateTimeFormat(locale, { month: "short", day: "2-digit" }).format(
    new Date(ts),
  );

const fmtTime = (ts: number, locale: string) =>
  new Intl.DateTimeFormat(locale, {
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(ts));

export const NotesPage: React.FC = () => {
  const { t, i18n } = useTranslation();
  const locale = i18n.language || "nb";

  const [notes, setNotes] = useState<Note[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [saving, setSaving] = useState(false);
  const [showSearch, setShowSearch] = useState(false);

  const titleRef = useRef<HTMLInputElement>(null);
  const saveTimerRef = useRef<number | null>(null);
  const pendingRef = useRef<Note | null>(null);
  const searchInputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    setNotes(listNotes());
  }, []);

  useEffect(() => {
    if (showSearch) searchInputRef.current?.focus();
  }, [showSearch]);

  // Flush on unmount so the last keystroke is never lost.
  useEffect(
    () => () => {
      if (saveTimerRef.current) {
        window.clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
      }
      if (pendingRef.current) saveNote(pendingRef.current);
    },
    [],
  );

  const selectedNote = useMemo(
    () => notes.find((n) => n.id === selectedId) ?? null,
    [notes, selectedId],
  );

  const filteredNotes = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return notes;
    return notes.filter(
      (n) =>
        n.title.toLowerCase().includes(q) || n.body.toLowerCase().includes(q),
    );
  }, [notes, query]);

  const sortedNotes = useMemo(
    () =>
      [...filteredNotes].sort((a, b) => {
        if (a.pinned && !b.pinned) return -1;
        if (!a.pinned && b.pinned) return 1;
        return b.updatedAt - a.updatedAt;
      }),
    [filteredNotes],
  );

  const queueSave = useCallback((note: Note) => {
    pendingRef.current = note;
    setSaving(true);
    if (saveTimerRef.current) window.clearTimeout(saveTimerRef.current);
    saveTimerRef.current = window.setTimeout(() => {
      saveTimerRef.current = null;
      pendingRef.current = null;
      saveNote(note);
      setSaving(false);
    }, 400);
  }, []);

  const createNote = useCallback(() => {
    const note: Note = {
      id: crypto.randomUUID(),
      title: "",
      body: "",
      createdAt: Date.now(),
      updatedAt: Date.now(),
    };
    saveNote(note);
    setNotes(listNotes());
    setSelectedId(note.id);
    setQuery("");
    setShowSearch(false);
    window.setTimeout(() => titleRef.current?.focus(), 0);
  }, []);

  const handleDelete = useCallback(
    (id: string) => {
      deleteNote(id);
      const all = listNotes();
      setNotes(all);
      if (saveTimerRef.current) {
        window.clearTimeout(saveTimerRef.current);
        saveTimerRef.current = null;
      }
      if (pendingRef.current && pendingRef.current.id !== id) {
        saveNote(pendingRef.current);
      }
      pendingRef.current = null;
      setSaving(false);
      if (selectedId === id) setSelectedId(null);
    },
    [selectedId],
  );

  const updateSelected = useCallback(
    (patch: Partial<Pick<Note, "title" | "body">>) => {
      if (!selectedNote) return;
      const updated: Note = {
        ...selectedNote,
        ...patch,
        updatedAt: Date.now(),
      };
      setNotes((prev) =>
        [updated, ...prev.filter((n) => n.id !== updated.id)].sort((a, b) => {
          if (a.pinned && !b.pinned) return -1;
          if (!a.pinned && b.pinned) return 1;
          return b.updatedAt - a.updatedAt;
        }),
      );
      setSelectedId(updated.id);
      queueSave(updated);
    },
    [selectedNote, queueSave],
  );

  const handleTogglePin = useCallback((id: string, e: React.MouseEvent) => {
    e.stopPropagation();
    toggleNotePin(id);
    setNotes(listNotes());
  }, []);

  const refreshNotes = useCallback(() => setNotes(listNotes()), []);

  // ── Editor view ────────────────────────────────────────────────────
  if (selectedNote) {
    const words = wordCount(selectedNote.body);
    return (
      <div className="flex h-full flex-col">
        <div className="mb-4 flex items-center justify-between">
          <button
            type="button"
            onClick={() => setSelectedId(null)}
            className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-md text-ink-3 transition-colors hover:bg-wash hover:text-text"
            aria-label={t("notes.back")}
            title={t("notes.back")}
          >
            <ArrowLeft size={18} />
          </button>
          <button
            type="button"
            onClick={() => {
              if (window.confirm(t("notes.deleteTitle"))) {
                handleDelete(selectedNote.id);
              }
            }}
            className="inline-flex h-8 w-8 cursor-pointer items-center justify-center rounded-md text-ink-3 transition-colors hover:bg-error-wash hover:text-error"
            aria-label={t("notes.delete")}
            title={t("notes.delete")}
          >
            <Trash2 size={16} />
          </button>
        </div>
        <input
          ref={titleRef}
          value={selectedNote.title}
          onChange={(e) => updateSelected({ title: e.target.value })}
          placeholder={t("notes.titlePlaceholder")}
          className="w-full border-0 bg-transparent pb-3 font-ui text-[24px] font-semibold leading-8 tracking-[-0.02em] text-text outline-none placeholder:text-ink-3"
        />
        <textarea
          value={selectedNote.body}
          onChange={(e) => updateSelected({ body: e.target.value })}
          placeholder={t("notes.bodyPlaceholder")}
          className="min-h-0 flex-1 resize-none border-0 bg-transparent text-[14px] leading-5 text-text outline-none placeholder:text-ink-3"
        />
        <div className="mt-2 flex items-center justify-between border-t border-line pt-2 text-[11.5px] text-ink-3">
          <span>{t("notes.words", { count: words })}</span>
          <span>{saving ? t("notes.saving") : t("notes.saved")}</span>
        </div>
      </div>
    );
  }

  // ── List view ──────────────────────────────────────────────────────
  return (
    <div className="pb-10">
      {/* Title row */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-2">
          <h1 className="font-ui text-[24px] font-semibold leading-8 tracking-[-0.02em] text-text">
            {t("sidebar.scratchpad")}
          </h1>
          <span className="inline-flex h-5 items-center rounded-full bg-text px-2 text-[11px] font-semibold text-surface">
            {t("notes.beta")}
          </span>
        </div>
        {/* The floating note, the reference's "open note in window". */}
        <button
          type="button"
          onClick={() => void commands.openScratchpad().catch(() => {})}
          className="inline-flex h-[30px] cursor-pointer items-center gap-[6px] rounded-[7px] border border-line bg-surface px-[10px] text-[12.5px] font-[550] text-text transition-colors hover:bg-wash"
        >
          <SquarePen size={13} />
          {t("notes.openWindow")}
        </button>
      </div>

      {/* Intro: one serif line and one sentence, the way a notebook opens. */}
      <section className="mt-[18px] border-t border-line pt-[20px]">
        <h2 className="max-w-[24ch] font-display text-[26px] font-semibold leading-[33px] tracking-[-0.01em] text-text">
          {t("notes.bannerTitle")}
        </h2>
        <p className="mt-[8px] max-w-[58ch] text-[13px] leading-[21px] text-ink-2">
          {t("notes.bannerBody")}
        </p>
        <button
          type="button"
          onClick={createNote}
          className="mt-[14px] inline-flex h-[32px] cursor-pointer items-center rounded-[8px] border border-logo-primary/40 bg-background-ui px-[13px] text-[12.5px] font-[600] text-on-accent"
        >
          {t("notes.bannerCta")}
        </button>
      </section>

      {notes.length === 0 ? (
        <p className="mt-8 text-center text-[14px] text-ink-2">
          {t("notes.emptyTitle")}
        </p>
      ) : (
        <>
          {/* "Siste" section header */}
          <div className="mt-6">
            <div className="flex h-[52px] items-center justify-between gap-4">
              <span className="font-ui text-[12px] font-semibold uppercase tracking-[0.06em] text-ink-3">
                {t("notes.latest")}
              </span>
              <div className="flex items-center gap-1">
                <button
                  type="button"
                  onClick={() => setShowSearch((s) => !s)}
                  className="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-md text-ink-3 transition-colors hover:bg-wash hover:text-text"
                  aria-label={t("notes.searchPlaceholder")}
                  title={t("notes.searchPlaceholder")}
                >
                  <Search size={16} />
                </button>
                <button
                  type="button"
                  onClick={createNote}
                  className="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-md text-ink-3 transition-colors hover:bg-wash hover:text-text"
                  aria-label={t("notes.newNote")}
                  title={t("notes.newNote")}
                >
                  <Plus size={16} />
                </button>
                <button
                  type="button"
                  onClick={refreshNotes}
                  className="inline-flex h-7 w-7 cursor-pointer items-center justify-center rounded-md text-ink-3 transition-colors hover:bg-wash hover:text-text"
                  aria-label={t("notes.refresh")}
                  title={t("notes.refresh")}
                >
                  <RefreshCw size={16} />
                </button>
              </div>
            </div>
            {showSearch && (
              <div className="relative mb-2">
                <input
                  ref={searchInputRef}
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder={t("notes.searchPlaceholder")}
                  aria-label={t("notes.searchPlaceholder")}
                  className="h-8 w-full rounded-md border border-line bg-surface px-3 text-[13px] text-text outline-none transition-colors placeholder:text-ink-3 focus:border-logo-primary/60"
                />
              </div>
            )}
            <div className="border-t border-line" />
          </div>

          {/* Note cards */}
          <div className="mt-4 flex flex-col gap-3">
            {sortedNotes.map((note) => (
              <div
                key={note.id}
                role="button"
                tabIndex={0}
                onClick={() => setSelectedId(note.id)}
                onKeyDown={(e) => {
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    setSelectedId(note.id);
                  }
                }}
                className="group cursor-pointer border-b border-line py-[18px] transition-colors hover:bg-wash"
              >
                <div className="flex items-start justify-between gap-3">
                  <h3 className="text-[16px] font-semibold leading-5 text-text">
                    {note.title || t("notes.untitledShort")}
                  </h3>
                  <button
                    type="button"
                    onClick={(e) => handleTogglePin(note.id, e)}
                    className={`shrink-0 rounded p-1 transition-opacity ${
                      note.pinned
                        ? "text-text opacity-100"
                        : "text-ink-3 opacity-0 group-hover:opacity-100"
                    } hover:bg-wash hover:text-text`}
                    aria-label={note.pinned ? t("notes.unpin") : t("notes.pin")}
                    title={note.pinned ? t("notes.unpin") : t("notes.pin")}
                  >
                    <Pin size={14} />
                  </button>
                </div>
                <p className="mt-2 line-clamp-3 text-[14px] leading-5 text-ink-2">
                  {note.body.trim() || t("notes.emptyList")}
                </p>
                <div className="mt-3 flex items-center justify-between text-[12px] text-ink-3">
                  <span>{fmtDate(note.updatedAt, locale)}</span>
                  <span>{fmtTime(note.updatedAt, locale)}</span>
                </div>
              </div>
            ))}
          </div>
        </>
      )}
    </div>
  );
};
