import React, { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import {
  ArrowRight,
  Pencil,
  Plus,
  Scissors,
  Search,
  Trash2,
} from "lucide-react";
import type { Snippet } from "@/bindings";
import {
  Card,
  EmptyState,
  Page,
  PageHeader,
  SectionLabel,
} from "@/components/hub/Primitives";
import { Button } from "@/components/ui/Button";
import { Input } from "@/components/ui/Input";
import { Switch } from "@/components/ui/Switch";
import { Textarea } from "@/components/ui/Textarea";
import { useSettings } from "@/hooks/useSettings";
import {
  normalizeTrigger,
  parseSnippetsError,
  parseSnippetsFile,
  pickSnippetsExportPath,
  pickSnippetsFile,
  readSnippetsFile,
  writeSnippetsFile,
} from "@/lib/snippets/io";

/** One labelled field, so the form never relies on placeholders as labels. */
const Field: React.FC<{ label: string; children: React.ReactNode }> = ({
  label,
  children,
}) => (
  <label className="flex min-w-0 flex-col gap-[6px]">
    <span className="font-ui text-[11px] font-semibold tracking-[-0.005em] text-ink-3">
      {label}
    </span>
    {children}
  </label>
);

/**
 * Snippets: a spoken trigger expands into a fixed block of text. The page is
 * the list first and the composer second, because reading what you already
 * have is what the page is for.
 */
export const SnippetsPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const snippets = getSetting("snippets") || [];
  const saving = isUpdating("snippets");

  const [trigger, setTrigger] = useState("");
  const [expansion, setExpansion] = useState("");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [query, setQuery] = useState("");
  const [editing, setEditing] = useState<string | null>(null);
  const [draft, setDraft] = useState<Snippet | null>(null);
  const triggerRef = useRef<HTMLInputElement>(null);
  // The list as it is right now. A toast's action outlives the render it was
  // created in, so reading `snippets` there would restore a stale snapshot and
  // drop anything saved in between.
  const liveSnippets = useRef(snippets);
  liveSnippets.current = snippets;

  const normalized = normalizeTrigger(trigger);
  const isDuplicate = (value: string, except?: string) =>
    snippets.some(
      (snippet) =>
        snippet.trigger !== except &&
        snippet.trigger.toLowerCase() === value.toLowerCase(),
    );

  const visible = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return snippets;
    return snippets.filter(
      (snippet) =>
        snippet.trigger.toLowerCase().includes(needle) ||
        snippet.expansion.toLowerCase().includes(needle),
    );
  }, [snippets, query]);

  const add = () => {
    if (!normalized) return;
    if (isDuplicate(normalized)) {
      toast.error(
        t("settings.advanced.snippets.duplicate", { trigger: normalized }),
      );
      return;
    }
    void updateSetting("snippets", [
      ...snippets,
      {
        trigger: normalized,
        expansion,
        case_sensitive: caseSensitive,
      },
    ]);
    setTrigger("");
    setExpansion("");
    setCaseSensitive(false);
    triggerRef.current?.focus();
  };

  const remove = (snippet: Snippet) => {
    void updateSetting(
      "snippets",
      snippets.filter((item) => item.trigger !== snippet.trigger),
    );
    // Deleting a template is a keystroke away from losing it; one toast is
    // cheaper than a confirm dialog nobody reads.
    toast(t("snippets.removed", { trigger: snippet.trigger }), {
      action: {
        label: t("snippets.undo"),
        onClick: () => {
          const current = liveSnippets.current;
          if (current.some((item) => item.trigger === snippet.trigger)) return;
          void updateSetting("snippets", [...current, snippet]);
        },
      },
    });
  };

  const commit = () => {
    if (!draft) return;
    const nextTrigger = normalizeTrigger(draft.trigger);
    if (!nextTrigger) return;
    if (isDuplicate(nextTrigger, editing ?? undefined)) {
      toast.error(
        t("settings.advanced.snippets.duplicate", { trigger: nextTrigger }),
      );
      return;
    }
    void updateSetting(
      "snippets",
      snippets.map((item) =>
        item.trigger === editing
          ? { ...draft, trigger: nextTrigger }
          : item,
      ),
    );
    setEditing(null);
    setDraft(null);
  };

  const exportAll = async () => {
    try {
      const path = await pickSnippetsExportPath();
      if (!path) return;
      if (await writeSnippetsFile(path, snippets)) {
        toast.success(t("settings.advanced.snippets.exportSaved"));
      } else {
        toast.error(t("settings.advanced.snippets.exportFailed"));
      }
    } catch (error) {
      console.error("Failed to export snippets:", error);
      toast.error(t("settings.advanced.snippets.exportFailed"));
    }
  };

  const importFile = async () => {
    try {
      const path = await pickSnippetsFile();
      if (!path) return;
      const { snippets: imported, error } = parseSnippetsFile(
        await readSnippetsFile(path),
      );
      if (error) {
        toast.error(parseSnippetsError(error, t));
        return;
      }
      if (imported.length === 0) {
        toast.error(t("settings.advanced.snippets.importEmpty"));
        return;
      }
      const merged = [...snippets];
      for (const snippet of imported) {
        const at = merged.findIndex(
          (item) =>
            item.trigger.toLowerCase() === snippet.trigger.toLowerCase(),
        );
        if (at >= 0) merged[at] = snippet;
        else merged.push(snippet);
      }
      void updateSetting("snippets", merged);
      toast.success(
        t("settings.advanced.snippets.imported", { count: imported.length }),
      );
    } catch (error) {
      console.error("Failed to import snippets:", error);
      toast.error(t("settings.advanced.snippets.importReadFailed"));
    }
  };

  const composer = (
    <Card className="p-[18px]">
      <div className="mb-[12px] flex items-center gap-[7px]">
        <Plus size={14} className="text-ink-3" />
        <SectionLabel>{t("snippets.newTitle")}</SectionLabel>
      </div>
      <div className="grid grid-cols-[minmax(140px,200px)_1fr] items-start gap-[10px]">
        <Field label={t("settings.advanced.snippets.triggerLabel")}>
          <Input
            ref={triggerRef}
            value={trigger}
            onChange={(event) => setTrigger(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                add();
              }
            }}
            placeholder={t("settings.advanced.snippets.triggerPlaceholder")}
            variant="compact"
            disabled={saving}
          />
        </Field>
        <Field label={t("settings.advanced.snippets.expansionLabel")}>
          <Textarea
            value={expansion}
            onChange={(event) => setExpansion(event.target.value)}
            placeholder={t("settings.advanced.snippets.expansionPlaceholder")}
            className="min-h-[64px] resize-y"
            variant="compact"
            disabled={saving}
          />
        </Field>
      </div>
      <div className="mt-[12px] flex items-center justify-between gap-3">
        <label className="flex items-center gap-[7px] text-[12px] text-ink-2">
          <Switch
            checked={caseSensitive}
            onChange={setCaseSensitive}
            disabled={saving}
            ariaLabel={t("settings.advanced.snippets.caseSensitive")}
          />
          {t("settings.advanced.snippets.caseSensitive")}
        </label>
        <Button
          onClick={add}
          disabled={!normalized || saving}
          variant="primary"
          size="md"
        >
          {t("settings.advanced.snippets.add")}
        </Button>
      </div>
    </Card>
  );

  const rows = (
    <Card>
      <div className="flex items-center justify-between gap-3 border-b border-line/60 px-[16px] py-[11px]">
        <div className="flex items-baseline gap-2">
          <SectionLabel>{t("snippets.listTitle")}</SectionLabel>
          <span className="text-[11.5px] tabular-nums text-ink-3">
            {t("snippets.count", { count: snippets.length })}
          </span>
        </div>
        <div className="relative">
          <Search
            size={13}
            className="pointer-events-none absolute start-[9px] top-1/2 -translate-y-1/2 text-ink-3"
          />
          <Input
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            placeholder={t("snippets.search")}
            aria-label={t("snippets.search")}
            className="w-[200px] ps-[27px]"
            variant="compact"
          />
        </div>
      </div>

      {visible.map((snippet) =>
        editing === snippet.trigger && draft ? (
          <div
            key={snippet.trigger}
            className="border-t border-line/60 px-[16px] py-[12px] first:border-t-0"
          >
            <div className="grid grid-cols-[minmax(140px,200px)_1fr] items-start gap-[10px]">
              <Field label={t("settings.advanced.snippets.triggerLabel")}>
                <Input
                  value={draft.trigger}
                  onChange={(event) =>
                    setDraft({ ...draft, trigger: event.target.value })
                  }
                  variant="compact"
                />
              </Field>
              <Field label={t("settings.advanced.snippets.expansionLabel")}>
                <Textarea
                  value={draft.expansion}
                  onChange={(event) =>
                    setDraft({ ...draft, expansion: event.target.value })
                  }
                  className="min-h-[64px] resize-y"
                  variant="compact"
                />
              </Field>
            </div>
            <div className="mt-[10px] flex items-center justify-between gap-3">
              <label className="flex items-center gap-[7px] text-[12px] text-ink-2">
                <Switch
                  checked={draft.case_sensitive ?? false}
                  onChange={(checked) =>
                    setDraft({ ...draft, case_sensitive: checked })
                  }
                  ariaLabel={t("settings.advanced.snippets.caseSensitive")}
                />
                {t("settings.advanced.snippets.caseSensitive")}
              </label>
              <div className="flex items-center gap-2">
                <Button
                  onClick={() => {
                    setEditing(null);
                    setDraft(null);
                  }}
                  variant="secondary"
                  size="sm"
                >
                  {t("common.cancel")}
                </Button>
                <Button onClick={commit} variant="primary" size="sm">
                  {t("snippets.done")}
                </Button>
              </div>
            </div>
          </div>
        ) : (
          <div
            key={snippet.trigger}
            className="group flex items-center gap-[10px] border-t border-line/60 px-[16px] py-[11px] transition-colors first:border-t-0 hover:bg-wash"
          >
            <button
              type="button"
              onClick={() => {
                if (editing && draft) commit();
                setEditing(snippet.trigger);
                setDraft({ ...snippet });
              }}
              className="flex min-w-0 flex-1 cursor-pointer items-center gap-[9px] text-start"
            >
              <code className="shrink-0 rounded-[5px] bg-surface-soft px-[7px] py-[2px] font-mono text-[11.5px] text-text">
                {snippet.trigger}
              </code>
              {snippet.case_sensitive && (
                <span
                  title={t("settings.advanced.snippets.caseSensitive")}
                  className="shrink-0 rounded-[4px] border border-line px-[4px] py-[1px] font-mono text-[10px] text-ink-3"
                >
                  {t("snippets.caseBadge")}
                </span>
              )}
              <ArrowRight size={11} className="shrink-0 text-ink-3" />
              <span className="truncate text-[12.5px] text-ink-2">
                {snippet.expansion.replace(/\s+/g, " ")}
              </span>
            </button>
            <div className="flex shrink-0 items-center gap-1 opacity-0 transition-opacity focus-within:opacity-100 group-hover:opacity-100">
              <Button
                onClick={() => {
                  if (editing && draft) commit();
                  setEditing(snippet.trigger);
                  setDraft({ ...snippet });
                }}
                variant="ghost"
                size="sm"
                aria-label={t("snippets.edit")}
              >
                <Pencil size={13} />
              </Button>
              <Button
                onClick={() => remove(snippet)}
                disabled={saving}
                variant="ghost"
                size="sm"
                aria-label={t("settings.advanced.snippets.remove")}
              >
                <Trash2 size={13} />
              </Button>
            </div>
          </div>
        ),
      )}

      {visible.length === 0 && snippets.length > 0 && (
        <p className="px-[16px] py-[22px] text-center text-[12.5px] text-ink-3">
          {t("snippets.noHits", { query: query.trim() })}
        </p>
      )}
    </Card>
  );

  return (
    <Page width={880}>
      <PageHeader
        title={t("sidebar.snippets")}
        subtitle={t("snippets.subtitle")}
        action={
          <div className="flex items-center gap-2">
            <Button onClick={importFile} disabled={saving} variant="secondary" size="sm">
              {t("settings.advanced.snippets.import")}
            </Button>
            <Button
              onClick={exportAll}
              disabled={saving || snippets.length === 0}
              variant="secondary"
              size="sm"
            >
              {t("settings.advanced.snippets.export")}
            </Button>
          </div>
        }
      />

      {snippets.length === 0 ? (
        <EmptyState
          icon={<Scissors size={22} />}
          title={t("snippets.emptyTitle")}
          body={t("snippets.emptyBody")}
          action={
            <Button
              onClick={() => triggerRef.current?.focus()}
              variant="primary"
              size="md"
            >
              {t("snippets.emptyCta")}
            </Button>
          }
        />
      ) : (
        rows
      )}

      <div className="mt-[14px]">{composer}</div>
    </Page>
  );
};
