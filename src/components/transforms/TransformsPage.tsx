import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check, Pencil, Plus, Sparkles, Trash2 } from "lucide-react";

import { commands, type LLMPrompt } from "@/bindings";
import { Card, EmptyState, Page, PageHeader } from "@/components/hub/Primitives";
import { useSettings } from "@/hooks/useSettings";

/**
 * Transforms: named instructions that rewrite a dictation after it lands, on
 * this machine or against your own model. The prompts live in AppSettings, so
 * the post-processing settings page and this page read the same list.
 */
export const TransformsPage: React.FC = () => {
  const { t } = useTranslation();
  const { settings, refreshSettings, updateSetting } = useSettings();
  const prompts: LLMPrompt[] = settings?.post_process_prompts ?? [];
  const activeId = settings?.post_process_selected_prompt_id ?? null;

  const [draftId, setDraftId] = useState<string | null>(null);
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [busy, setBusy] = useState(false);

  const startNew = () => {
    setDraftId("new");
    setName("");
    setText("");
  };

  const startEdit = (prompt: LLMPrompt) => {
    setDraftId(prompt.id);
    setName(prompt.name);
    setText(prompt.prompt);
  };

  const closeDraft = () => {
    setDraftId(null);
    setName("");
    setText("");
  };

  const canSave = name.trim().length > 0 && text.trim().length > 0 && !busy;

  const save = async () => {
    if (!canSave || !draftId) return;
    setBusy(true);
    try {
      if (draftId === "new") {
        const result = await commands.addPostProcessPrompt(
          name.trim(),
          text.trim(),
        );
        if (result.status === "ok") {
          await refreshSettings();
          await updateSetting("post_process_selected_prompt_id", result.data.id);
        }
      } else {
        await commands.updatePostProcessPrompt(draftId, name.trim(), text.trim());
        await refreshSettings();
      }
      closeDraft();
    } catch (error) {
      console.error("Failed to save transform:", error);
    } finally {
      setBusy(false);
    }
  };

  const use = async (prompt: LLMPrompt) => {
    await updateSetting("post_process_selected_prompt_id", prompt.id);
  };

  const remove = async (prompt: LLMPrompt) => {
    try {
      await commands.deletePostProcessPrompt(prompt.id);
      if (activeId === prompt.id) {
        await updateSetting("post_process_selected_prompt_id", null);
      }
      await refreshSettings();
      if (draftId === prompt.id) closeDraft();
    } catch (error) {
      console.error("Failed to delete transform:", error);
    }
  };

  return (
    <Page>
      <PageHeader
        title={t("sidebar.transforms")}
        subtitle={t("transforms.subtitle")}
      />

      <div className="mt-[24px] flex items-center justify-between">
        <p className="text-[12px] font-semibold uppercase tracking-[0.06em] text-ink-3">
          {t("transforms.library")}
        </p>
        <button
          type="button"
          onClick={startNew}
          className="flex h-[32px] cursor-pointer items-center gap-[6px] rounded-[8px] border border-line bg-surface-soft px-[12px] text-[13px] font-[550] text-text transition-colors hover:bg-wash"
        >
          <Plus size={14} />
          {t("transforms.new")}
        </button>
      </div>

      {draftId && (
        <div className="mt-[12px] rounded-[16px] border border-line bg-surface-soft p-[20px]">
          <label className="block text-[12px] font-[550] text-ink-3">
            {t("transforms.name")}
          </label>
          <input
            value={name}
            onChange={(event) => setName(event.target.value)}
            placeholder={t("transforms.namePlaceholder")}
            className="mt-[6px] h-[36px] w-full rounded-[8px] border border-line bg-surface px-[12px] text-[14px] text-text outline-none placeholder:text-ink-3 focus:border-text/25"
          />
          <label className="mt-[14px] block text-[12px] font-[550] text-ink-3">
            {t("transforms.prompt")}
          </label>
          <textarea
            value={text}
            onChange={(event) => setText(event.target.value)}
            placeholder={t("transforms.promptPlaceholder")}
            rows={4}
            className="mt-[6px] w-full resize-y rounded-[8px] border border-line bg-surface p-[12px] text-[14px] leading-[22px] text-text outline-none placeholder:text-ink-3 focus:border-text/25"
          />
          <div className="mt-[14px] flex items-center gap-[8px]">
            <button
              type="button"
              disabled={!canSave}
              onClick={save}
              className="h-[32px] cursor-pointer rounded-[8px] border border-line bg-background-ui px-[14px] text-[13px] font-[550] text-on-accent transition-opacity disabled:cursor-not-allowed disabled:opacity-45"
            >
              {t("transforms.save")}
            </button>
            <button
              type="button"
              onClick={closeDraft}
              className="h-[32px] cursor-pointer rounded-[8px] px-[12px] text-[13px] text-ink-3 transition-colors hover:text-text"
            >
              {t("transforms.cancel")}
            </button>
          </div>
        </div>
      )}

      <Card className="mt-[12px]">
        {prompts.length === 0 ? (
          <EmptyState
            icon={<Sparkles size={22} />}
            title={t("transforms.empty")}
            body={t("transforms.emptyHint")}
          />
        ) : (
          prompts.map((prompt, index) => (
            <div
              key={prompt.id}
              className={`flex items-start gap-[16px] px-[20px] py-[16px] ${
                index > 0 ? "border-t border-line" : ""
              }`}
            >
              <div className="min-w-0 flex-1">
                <div className="flex items-center gap-[8px]">
                  <p className="truncate text-[15px] font-[600] text-text">
                    {prompt.name}
                  </p>
                  {prompt.id === activeId && (
                    <span className="flex shrink-0 items-center gap-[4px] rounded-[6px] bg-background-ui px-[6px] py-[2px] text-[11px] font-[600] text-on-accent">
                      <Check size={10} />
                      {t("transforms.active")}
                    </span>
                  )}
                </div>
                <p className="mt-[2px] line-clamp-2 text-[13px] leading-[20px] text-text/70">
                  {prompt.prompt}
                </p>
              </div>
              <div className="flex shrink-0 items-center gap-[2px]">
                {prompt.id !== activeId && (
                  <button
                    type="button"
                    onClick={() => use(prompt)}
                    className="h-[28px] cursor-pointer rounded-[6px] border border-line px-[10px] text-[12px] font-[550] text-text transition-colors hover:bg-wash"
                  >
                    {t("transforms.use")}
                  </button>
                )}
                <button
                  type="button"
                  onClick={() => startEdit(prompt)}
                  aria-label={t("transforms.edit")}
                  title={t("transforms.edit")}
                  className="flex h-[28px] w-[28px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
                >
                  <Pencil size={14} />
                </button>
                <button
                  type="button"
                  onClick={() => remove(prompt)}
                  aria-label={t("transforms.delete")}
                  title={t("transforms.delete")}
                  className="flex h-[28px] w-[28px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            </div>
          ))
        )}
      </Card>
    </Page>
  );
};
