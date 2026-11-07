import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { readFile } from "@tauri-apps/plugin-fs";
import {
  Check,
  Copy,
  FileAudio,
  FilePlus,
  FolderOpen,
  Pencil,
  RotateCcw,
  ScrollText,
  Search,
  Star,
  Trash2,
  X,
} from "lucide-react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  commands,
  events,
  type HistoryEntry,
  type HistoryUpdatePayload,
} from "@/bindings";
import { useOsType } from "@/hooks/useOsType";
import {
  useTranscriptionJobsStore,
  type ActiveJobStatus,
} from "@/stores/transcriptionJobsStore";
import { formatDateTime } from "@/utils/dateFormat";
import { AudioPlayer, AudioPlayerGroup } from "../../ui/AudioPlayer";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { Textarea } from "../../ui/Textarea";
import { copyToClipboard } from "./clipboard";
import { MeetingBadge } from "./MeetingBadge";
import { NoteBadge } from "./NoteBadge";
import { splitHighlight } from "./searchHighlight";
import { TranscriptDialog } from "./TranscriptDialog";
import { pickAndTranscribeFiles } from "./transcribeFiles";

const IconButton: React.FC<{
  onClick: () => void;
  title: string;
  disabled?: boolean;
  active?: boolean;
  children: React.ReactNode;
}> = ({ onClick, title, disabled, active, children }) => (
  <button
    onClick={onClick}
    disabled={disabled}
    className={`p-1.5 rounded-md flex items-center justify-center transition-colors cursor-pointer disabled:cursor-not-allowed disabled:text-text/20 ${
      active
        ? "text-logo-primary hover:text-logo-primary"
        : "text-ink-3 hover:text-logo-primary"
    }`}
    title={title}
  >
    {children}
  </button>
);

const PAGE_SIZE = 30;

// Keystrokes rest this long before a search runs, so typing a word fires
// one query instead of one per letter.
const SEARCH_DEBOUNCE_MS = 250;

// Wraps every case-insensitive match of `query` in `text` with a <mark> so
// search hits stand out in an entry.
const HighlightedText: React.FC<{ text: string; query: string }> = ({
  text,
  query,
}) => (
  <>
    {splitHighlight(text, query).map((part, index) =>
      part.match ? (
        <mark
          key={index}
          className="rounded-sm bg-logo-primary/25 px-0.5 font-semibold text-text"
        >
          {part.text}
        </mark>
      ) : (
        <React.Fragment key={index}>{part.text}</React.Fragment>
      ),
    )}
  </>
);

// Long text folds to this many lines until "Show more".
const FOLDED_LINES = 4;

interface HeaderButtonProps {
  onClick: () => void;
  label: string;
  icon: React.ComponentType<{ className?: string }>;
}

const HeaderButton: React.FC<HeaderButtonProps> = ({
  onClick,
  label,
  icon: Icon,
}) => (
  <Button
    onClick={onClick}
    variant="secondary"
    size="sm"
    className="flex items-center gap-2"
    title={label}
  >
    <Icon className="w-4 h-4" />
    <span>{label}</span>
  </Button>
);

export const HistorySettings: React.FC = () => {
  const { t } = useTranslation();
  const osType = useOsType();
  const jobs = useTranscriptionJobsStore((state) => state.jobs);
  const [entries, setEntries] = useState<HistoryEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [hasMore, setHasMore] = useState(true);
  const sentinelRef = useRef<HTMLDivElement>(null);
  const entriesRef = useRef<HistoryEntry[]>([]);
  const loadingRef = useRef(false);

  // History search: the raw input, its debounced form, and the results
  // page-by-page. An empty query leaves the plain list untouched.
  const [query, setQuery] = useState("");
  const [debouncedQuery, setDebouncedQuery] = useState("");
  const [searchResults, setSearchResults] = useState<HistoryEntry[]>([]);
  const [searchTotal, setSearchTotal] = useState(0);
  const [searchHasMore, setSearchHasMore] = useState(false);
  const [searching, setSearching] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const [openRequest, setOpenRequest] = useState<{
    id: number;
    token: number;
  } | null>(null);
  const searchResultsRef = useRef<HistoryEntry[]>([]);
  const searchSeqRef = useRef(0);
  const searchLoadingRef = useRef(false);
  const openTokenRef = useRef(0);
  const isSearching = debouncedQuery.length > 0;

  // Keep refs in sync for use in IntersectionObserver callback
  useEffect(() => {
    entriesRef.current = entries;
  }, [entries]);

  useEffect(() => {
    searchResultsRef.current = searchResults;
  }, [searchResults]);

  // Debounce the query so a search runs once the user pauses, not per
  // keystroke.
  useEffect(() => {
    const timer = setTimeout(
      () => setDebouncedQuery(query.trim()),
      SEARCH_DEBOUNCE_MS,
    );
    return () => clearTimeout(timer);
  }, [query]);

  const loadPage = useCallback(async (cursor?: number) => {
    const isFirstPage = cursor === undefined;
    if (!isFirstPage && loadingRef.current) return;
    loadingRef.current = true;

    if (isFirstPage) setLoading(true);

    try {
      const result = await commands.getHistoryEntries(
        cursor ?? null,
        PAGE_SIZE,
      );
      if (result.status === "ok") {
        const { entries: newEntries, has_more } = result.data;
        setEntries((prev) => {
          if (isFirstPage) return newEntries;
          const existingIds = new Set(prev.map((entry) => entry.id));
          return [
            ...prev,
            ...newEntries.filter((entry) => !existingIds.has(entry.id)),
          ];
        });
        setHasMore(has_more);
      }
    } catch (error) {
      console.error("Failed to load history entries:", error);
    } finally {
      setLoading(false);
      loadingRef.current = false;
    }
  }, []);

  const searchPage = useCallback(
    async (searchQuery: string, offset: number) => {
      // Load-more pages wait their turn; a fresh query supersedes whatever is
      // in flight through the sequence number below.
      const isNewSearch = offset === 0;
      if (!isNewSearch && searchLoadingRef.current) return;
      searchLoadingRef.current = true;
      if (isNewSearch) setSearching(true);
      const seq = ++searchSeqRef.current;

      try {
        const result = await commands.searchHistory(
          searchQuery,
          PAGE_SIZE,
          offset,
        );
        // A newer search superseded this one; drop the stale page.
        if (seq !== searchSeqRef.current) return;
        if (result.status === "ok") {
          const { entries: newEntries, total, has_more } = result.data;
          setSearchResults((prev) =>
            isNewSearch
              ? newEntries
              : [
                  ...prev,
                  ...newEntries.filter(
                    (entry) =>
                      !prev.some((existing) => existing.id === entry.id),
                  ),
                ],
          );
          setSearchTotal(total);
          setSearchHasMore(has_more);
        }
      } catch (error) {
        console.error("Failed to search history entries:", error);
      } finally {
        if (seq === searchSeqRef.current) {
          if (isNewSearch) setSearching(false);
          searchLoadingRef.current = false;
        }
      }
    },
    [],
  );

  // A new query starts from its first page; an empty one ends the search
  // and the plain list shows again, unchanged.
  useEffect(() => {
    setActiveIndex(0);
    if (debouncedQuery) {
      void searchPage(debouncedQuery, 0);
    } else {
      searchSeqRef.current += 1;
      searchLoadingRef.current = false;
      setSearching(false);
      setSearchResults([]);
      setSearchTotal(0);
      setSearchHasMore(false);
    }
  }, [debouncedQuery, searchPage]);

  // Initial load
  useEffect(() => {
    loadPage();
  }, [loadPage]);

  // Infinite scroll via IntersectionObserver, for the plain list and for
  // search results alike.
  useEffect(() => {
    if (loading || searching) return;

    const sentinel = sentinelRef.current;
    if (!sentinel) return;
    if (isSearching ? !searchHasMore : !hasMore) return;

    const observer = new IntersectionObserver(
      (observerEntries) => {
        const first = observerEntries[0];
        if (!first.isIntersecting) return;
        if (isSearching) {
          const results = searchResultsRef.current;
          if (results.length > 0) {
            void searchPage(debouncedQuery, results.length);
          }
        } else {
          const lastEntry = entriesRef.current[entriesRef.current.length - 1];
          if (lastEntry) {
            loadPage(lastEntry.id);
          }
        }
      },
      { threshold: 0 },
    );

    observer.observe(sentinel);
    return () => observer.disconnect();
  }, [
    loading,
    searching,
    isSearching,
    hasMore,
    searchHasMore,
    debouncedQuery,
    loadPage,
    searchPage,
  ]);

  // Listen for new entries added from the transcription pipeline
  useEffect(() => {
    const unlisten = events.historyUpdatePayload.listen((event) => {
      const payload: HistoryUpdatePayload = event.payload;
      if (payload.action === "added") {
        setEntries((prev) => [
          payload.entry,
          ...prev.filter((entry) => entry.id !== payload.entry.id),
        ]);
      } else if (payload.action === "updated") {
        setEntries((prev) =>
          prev.map((e) => (e.id === payload.entry.id ? payload.entry : e)),
        );
      }
      // "deleted" and "toggled" are handled by optimistic updates only,
      // so we intentionally ignore them here to avoid double-mutation.
    });

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  // A corrected entry takes the place of the old one, in the list and in
  // any search results.
  const swapEntry = (updated: HistoryEntry) => {
    const swap = (prev: HistoryEntry[]) =>
      prev.map((e) => (e.id === updated.id ? updated : e));
    setEntries(swap);
    setSearchResults(swap);
  };

  const toggleSaved = async (id: number) => {
    // Optimistic update, in the plain list and in any search results
    const flip = (entry: HistoryEntry): HistoryEntry => ({
      ...entry,
      saved: !entry.saved,
    });
    const apply = (fn: (prev: HistoryEntry[]) => HistoryEntry[]) => {
      setEntries(fn);
      setSearchResults(fn);
    };
    apply((prev) => prev.map((e) => (e.id === id ? flip(e) : e)));
    try {
      const result = await commands.toggleHistoryEntrySaved(id);
      if (result.status !== "ok") {
        // Revert on failure
        apply((prev) => prev.map((e) => (e.id === id ? flip(e) : e)));
      }
    } catch (error) {
      console.error("Failed to toggle saved status:", error);
      // Revert on failure
      apply((prev) => prev.map((e) => (e.id === id ? flip(e) : e)));
    }
  };

  const getAudioUrl = useCallback(
    async (fileName: string) => {
      try {
        const result = await commands.getAudioFilePath(fileName);
        if (result.status === "ok") {
          if (osType === "linux") {
            const fileData = await readFile(result.data);
            const blob = new Blob([fileData], { type: "audio/wav" });
            return URL.createObjectURL(blob);
          }
          return convertFileSrc(result.data, "asset");
        }
        return null;
      } catch (error) {
        console.error("Failed to get audio file path:", error);
        return null;
      }
    },
    [osType],
  );

  const deleteAudioEntry = async (id: number) => {
    // Optimistically remove, from the plain list and any search results
    setEntries((prev) => prev.filter((e) => e.id !== id));
    setSearchResults((prev) => prev.filter((e) => e.id !== id));
    try {
      const result = await commands.deleteHistoryEntry(id);
      if (result.status !== "ok") {
        // Reload on failure
        loadPage();
      }
    } catch (error) {
      console.error("Failed to delete entry:", error);
      loadPage();
    }
  };

  const transcribeFile = async () => {
    try {
      await pickAndTranscribeFiles();
    } catch (error) {
      console.error("Failed to pick a file to transcribe:", error);
      toast.error(t("settings.history.transcribeFailed"));
    }
  };

  const openRecordingsFolder = async () => {
    try {
      const result = await commands.openRecordingsFolder();
      if (result.status !== "ok") {
        throw new Error(String(result.error));
      }
    } catch (error) {
      console.error("Failed to open recordings folder:", error);
    }
  };

  // Keyboard navigation over the search results: the arrows move between
  // hits, Enter opens the one under the cursor, Esc clears the search.
  const handleSearchKeyDown = (
    event: React.KeyboardEvent<HTMLInputElement>,
  ) => {
    if (event.key === "ArrowDown") {
      event.preventDefault();
      if (searchResults.length > 0) {
        setActiveIndex((index) =>
          Math.min(index + 1, searchResults.length - 1),
        );
      }
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActiveIndex((index) => Math.max(index - 1, 0));
    } else if (event.key === "Enter") {
      const entry = searchResults[activeIndex];
      if (entry) {
        openTokenRef.current += 1;
        setOpenRequest({ id: entry.id, token: openTokenRef.current });
      }
    } else if (event.key === "Escape") {
      setQuery("");
    }
  };

  const displayEntries = isSearching ? searchResults : entries;

  let content: React.ReactNode;

  if (isSearching && searching) {
    content = (
      <div className="px-4 py-3 text-center text-ink-2">
        {t("settings.history.searching")}
      </div>
    );
  } else if (isSearching && searchResults.length === 0) {
    content = (
      <div className="px-4 py-3 text-center text-ink-2">
        {t("settings.history.searchNoResults", { query: debouncedQuery })}
      </div>
    );
  } else if (loading) {
    content = (
      <div className="px-4 py-3 text-center text-ink-2">
        {t("settings.history.loading")}
      </div>
    );
  } else if (displayEntries.length === 0) {
    content = (
      <div className="px-4 py-3 text-center text-ink-2">
        {t("settings.history.empty")}
      </div>
    );
  } else {
    content = (
      <>
        <AudioPlayerGroup>
          <div className="divide-y divide-line">
            {displayEntries.map((entry, index) => (
              <HistoryEntryComponent
                key={entry.id}
                entry={entry}
                job={jobs[entry.id]}
                onToggleSaved={() => toggleSaved(entry.id)}
                onCopyText={() => copyToClipboard(entry.transcription_text)}
                onCorrected={swapEntry}
                getAudioUrl={getAudioUrl}
                deleteAudio={deleteAudioEntry}
                retryTranscription={commands.retryHistoryEntryTranscription}
                cancelTranscription={commands.cancelHistoryEntryTranscription}
                highlight={isSearching ? debouncedQuery : undefined}
                forceExpanded={isSearching}
                selected={isSearching && index === activeIndex}
                openRequest={openRequest}
              />
            ))}
          </div>
        </AudioPlayerGroup>
        {/* Sentinel for infinite scroll */}
        <div ref={sentinelRef} className="h-1" />
      </>
    );
  }

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <div className="space-y-2">
        <div className="px-4 flex items-center justify-between">
          <div>
            <h2 className="text-xs font-medium text-ink-3 uppercase tracking-wide">
              {t("settings.history.title")}
            </h2>
          </div>
          <div className="flex items-center gap-2">
            <HeaderButton
              onClick={transcribeFile}
              label={t("settings.history.transcribeFile")}
              icon={FilePlus}
            />
            <HeaderButton
              onClick={openRecordingsFolder}
              label={t("settings.history.openFolder")}
              icon={FolderOpen}
            />
          </div>
        </div>
        <div className="relative px-4">
          <Search
            className="pointer-events-none absolute top-1/2 left-6 h-3.5 w-3.5 -translate-y-1/2 text-ink-3"
            aria-hidden="true"
          />
          <Input
            variant="compact"
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={handleSearchKeyDown}
            placeholder={t("settings.history.searchPlaceholder")}
            aria-label={t("settings.history.searchPlaceholder")}
            className="w-full font-normal ps-8 pe-7"
          />
          {query && (
            <button
              type="button"
              onClick={() => setQuery("")}
              aria-label={t("common.clear")}
              className="absolute top-1/2 right-6 -translate-y-1/2 cursor-pointer rounded p-0.5 text-ink-3 transition-colors hover:text-text"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
            </button>
          )}
        </div>
        {isSearching && searchTotal > 0 && (
          <p className="px-4 text-xs text-ink-3">
            {t("settings.history.searchResultCount", { count: searchTotal })}
          </p>
        )}
        <div className="bg-background border border-line rounded-lg overflow-visible">
          {content}
        </div>
      </div>
    </div>
  );
};

interface HistoryEntryProps {
  entry: HistoryEntry;
  job?: ActiveJobStatus;
  onToggleSaved: () => void;
  onCopyText: () => Promise<boolean>;
  /** Receives the entry after a saved "correct this" edit. */
  onCorrected: (entry: HistoryEntry) => void;
  getAudioUrl: (fileName: string) => Promise<string | null>;
  deleteAudio: (id: number) => Promise<void>;
  retryTranscription: (id: number) => Promise<void>;
  cancelTranscription: (id: number) => Promise<void>;
  /** The active search query; its hits are marked in the text. */
  highlight?: string;
  /** Search results never fold: a hit can sit past the fold line. */
  forceExpanded?: boolean;
  /** The hit the keyboard cursor is on, while a search is active. */
  selected?: boolean;
  /** Opens this entry when `token` changes, from the search box's Enter. */
  openRequest?: { id: number; token: number } | null;
}

const HistoryEntryComponent: React.FC<HistoryEntryProps> = ({
  entry,
  job,
  onToggleSaved,
  onCopyText,
  onCorrected,
  getAudioUrl,
  deleteAudio,
  retryTranscription,
  cancelTranscription,
  highlight,
  forceExpanded,
  selected,
  openRequest,
}) => {
  const { t, i18n } = useTranslation();
  const [showCopied, setShowCopied] = useState(false);
  // Pressed cancel: a running job stops after its current chunk.
  const [cancelling, setCancelling] = useState(false);
  // The transcript window for this entry, with its timed segments.
  const [showTranscript, setShowTranscript] = useState(false);
  // "Rett denne": the user fixes the transcription right here, and the
  // before/after pair feeds the dictionary learner (when it's on).
  const [editing, setEditing] = useState(false);
  const [draft, setDraft] = useState("");
  const [savingCorrection, setSavingCorrection] = useState(false);
  const [expanded, setExpanded] = useState(false);
  const [overflows, setOverflows] = useState(false);
  const textRef = useRef<HTMLParagraphElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // Queued or being transcribed in the background.
  const retrying = job !== undefined;
  useEffect(() => {
    if (!retrying) setCancelling(false);
  }, [retrying]);
  const hasTranscription = entry.transcription_text.trim().length > 0;
  // Only entries the model gave timed segments have a transcript to show;
  // old dictations render exactly as before, without the button.
  const hasSegments = (entry.metadata?.segments?.length ?? 0) > 0;
  // Search results never fold, so a hit past the fold line stays visible.
  useEffect(() => {
    if (forceExpanded) setExpanded(true);
  }, [forceExpanded]);
  // Keep the keyboard cursor's entry in view.
  useEffect(() => {
    if (selected) {
      rootRef.current?.scrollIntoView({ block: "nearest" });
    }
  }, [selected]);
  // The search box opens this entry on Enter: its transcript window when it
  // has segments, the full text otherwise.
  useEffect(() => {
    if (!openRequest || openRequest.id !== entry.id) return;
    if (hasSegments || !hasTranscription) {
      setShowTranscript(true);
    } else {
      setExpanded(true);
    }
  }, [openRequest, hasSegments, hasTranscription, entry.id]);
  const folded = hasTranscription && !retrying && !expanded && !forceExpanded;

  // Offer "Show more" only when the folded text is actually cut off.
  useLayoutEffect(() => {
    const el = textRef.current;
    if (!el || !folded) return;
    const measure = () => setOverflows(el.scrollHeight > el.clientHeight + 1);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(el);
    return () => observer.disconnect();
  }, [folded, entry.transcription_text]);

  const handleLoadAudio = useCallback(
    () => getAudioUrl(entry.file_name),
    [getAudioUrl, entry.file_name],
  );

  const handleCopyText = async () => {
    if (!hasTranscription) {
      return;
    }

    const copied = await onCopyText();
    if (!copied) {
      toast.error(t("settings.history.copyError"));
      return;
    }

    setShowCopied(true);
    setTimeout(() => setShowCopied(false), 2000);
  };

  const startEditing = () => {
    setDraft(entry.transcription_text);
    setEditing(true);
  };

  const saveCorrection = async () => {
    setSavingCorrection(true);
    const result = await commands.correctHistoryEntry(entry.id, draft);
    setSavingCorrection(false);
    if (result.status === "error") {
      toast.error(t("settings.history.correctError"));
      return;
    }
    setEditing(false);
    onCorrected(result.data);
    toast.success(t("settings.history.correctSaved"));
  };

  const handleDeleteEntry = async () => {
    try {
      await deleteAudio(entry.id);
    } catch (error) {
      console.error("Failed to delete entry:", error);
      toast.error(t("settings.history.deleteError"));
    }
  };

  const formattedDate = formatDateTime(String(entry.timestamp), i18n.language);

  let progressText = t("settings.history.transcribing");
  if (job?.state === "queued") {
    progressText = t("settings.history.waiting");
  } else if (job?.state === "running" && job.percent > 0) {
    progressText = t("settings.history.transcribingPercent", {
      percent: job.percent,
    });
  }

  return (
    <div
      ref={rootRef}
      className={`px-4 py-2 pb-5 flex flex-col gap-3 ${
        selected ? "bg-logo-primary/10" : ""
      }`}
    >
      <div className="flex justify-between items-center gap-3">
        <div className="flex items-center gap-2 min-w-0">
          <p className="text-sm font-medium shrink-0">{formattedDate}</p>
          {entry.source === "meeting" && <MeetingBadge />}
          {entry.source === "note" && <NoteBadge />}
          {entry.source_name && (
            <span
              className="flex items-center gap-1 min-w-0 text-sm text-ink-3"
              title={entry.source_name}
            >
              <FileAudio width={14} height={14} className="shrink-0" />
              <span className="truncate">{entry.source_name}</span>
            </span>
          )}
        </div>
        <div className="flex items-center shrink-0">
          {hasSegments && (
            <IconButton
              onClick={() => setShowTranscript(true)}
              disabled={retrying}
              title={t("settings.history.transcript")}
            >
              <ScrollText width={16} height={16} />
            </IconButton>
          )}
          <IconButton
            onClick={handleCopyText}
            disabled={!hasTranscription || retrying}
            title={t("settings.history.copyToClipboard")}
          >
            {showCopied ? (
              <Check width={16} height={16} />
            ) : (
              <Copy width={16} height={16} />
            )}
          </IconButton>
          <IconButton
            onClick={startEditing}
            disabled={!hasTranscription || retrying}
            active={editing}
            title={t("settings.history.correct")}
          >
            <Pencil width={16} height={16} />
          </IconButton>
          <IconButton
            onClick={onToggleSaved}
            active={entry.saved}
            title={
              entry.saved
                ? t("settings.history.unsave")
                : t("settings.history.save")
            }
          >
            <Star
              width={16}
              height={16}
              fill={entry.saved ? "currentColor" : "none"}
            />
          </IconButton>
          {retrying ? (
            <IconButton
              onClick={() => {
                setCancelling(true);
                void cancelTranscription(entry.id);
              }}
              disabled={cancelling}
              title={t("settings.history.cancelTranscription")}
            >
              <X width={16} height={16} />
            </IconButton>
          ) : (
            <IconButton
              onClick={() => retryTranscription(entry.id)}
              title={t("settings.history.retranscribe")}
            >
              <RotateCcw width={16} height={16} />
            </IconButton>
          )}
          <IconButton
            onClick={handleDeleteEntry}
            title={t("settings.history.delete")}
          >
            <Trash2 width={16} height={16} />
          </IconButton>
        </div>
      </div>

      <div className="flex flex-col items-start gap-1 pb-2">
        {editing ? (
          <div className="w-full space-y-2 pt-1">
            <Textarea
              value={draft}
              onChange={(event) => setDraft(event.target.value)}
              variant="compact"
              className="w-full font-normal"
              disabled={savingCorrection}
              aria-label={t("settings.history.correct")}
              autoFocus
            />
            <div className="flex items-center gap-2">
              <Button
                variant="primary"
                size="sm"
                onClick={() => void saveCorrection()}
                disabled={
                  savingCorrection || draft === entry.transcription_text
                }
              >
                {t("settings.history.correctSave")}
              </Button>
              <Button
                variant="secondary"
                size="sm"
                onClick={() => setEditing(false)}
                disabled={savingCorrection}
              >
                {t("settings.history.correctCancel")}
              </Button>
            </div>
          </div>
        ) : (
          <p
            ref={textRef}
            className={`italic text-sm w-full ${
              retrying
                ? ""
                : hasTranscription
                  ? "text-text select-text cursor-text whitespace-pre-wrap break-words"
                  : "text-ink-3"
            }`}
            style={
              retrying
                ? { animation: "transcribe-pulse 3s ease-in-out infinite" }
                : folded
                  ? {
                      display: "-webkit-box",
                      WebkitLineClamp: FOLDED_LINES,
                      WebkitBoxOrient: "vertical",
                      overflow: "hidden",
                    }
                  : undefined
            }
          >
            {retrying && (
              <style>{`
            @keyframes transcribe-pulse {
              0%, 100% { color: var(--color-ink-3); }
              50% { color: var(--color-text); }
            }
          `}</style>
            )}
            {retrying ? (
              progressText
            ) : hasTranscription ? (
              highlight ? (
                <HighlightedText
                  text={entry.transcription_text}
                  query={highlight}
                />
              ) : (
                entry.transcription_text
              )
            ) : entry.source_name ? (
              // Cancelled, interrupted by a quit, or failed.
              t("settings.history.notTranscribed")
            ) : (
              t("settings.history.transcriptionFailed")
            )}
          </p>
        )}
        {hasTranscription &&
          !retrying &&
          !editing &&
          (expanded || overflows) && (
            <button
              type="button"
              onClick={() => setExpanded(!expanded)}
              className="text-xs font-medium text-ink-3 hover:text-logo-primary cursor-pointer"
            >
              {expanded
                ? t("settings.history.showLess")
                : t("settings.history.showMore")}
            </button>
          )}
      </div>

      <AudioPlayer onLoadRequest={handleLoadAudio} className="w-full" />

      <TranscriptDialog
        entry={entry}
        open={showTranscript}
        onOpenChange={setShowTranscript}
      />
    </div>
  );
};
