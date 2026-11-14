import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Check } from "lucide-react";
import { Page, PageHeader } from "@/components/hub/Primitives";
import { FillerWordRemoval } from "@/components/settings/FillerWordRemoval";
import { StyleRules } from "@/components/settings/post-processing/StyleRules";
import { useSettings } from "@/hooks/useSettings";

/** The built-in presets, in the order the page shows them. Ids come from the
 * backend's default prompts; a user's own prompt is listed after these. */
/** The writing contexts a prompt can be bound to. `default` keeps the global
 * pick, which is what every unlisted app falls back to. */
const CONTEXTS = ["default", "email", "chat", "docs", "terminal"] as const;
type Context = (typeof CONTEXTS)[number];

const BUILT_IN = [
  "style_neutral",
  "style_formal",
  "style_email",
  "style_short",
  "style_note",
  "style_code_comment",
];

/**
 * Style: how dictated text is rewritten before it lands. The cards pick the
 * default; per-app rules below send a single app through a different one.
 */
export const StylePage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const prompts = getSetting("post_process_prompts") || [];
  const selected = getSetting("post_process_selected_prompt_id") ?? null;
  const enabled = getSetting("post_process_enabled") ?? false;
  const contextPrompts = getSetting("post_process_context_prompts") || {};
  const [context, setContext] = useState<Context>("default");

  const isPicked = (id: string) =>
    context === "default"
      ? selected === id
      : (contextPrompts[context] ?? null) === id;

  const pick = (id: string) => {
    if (context === "default") {
      void updateSetting("post_process_selected_prompt_id", id);
      return;
    }
    void updateSetting("post_process_context_prompts", {
      ...contextPrompts,
      [context]: id,
    });
  };

  const ordered = [
    ...BUILT_IN.map((id) => prompts.find((p) => p.id === id)).filter(
      (p): p is NonNullable<typeof p> => Boolean(p),
    ),
    ...prompts.filter((p) => !BUILT_IN.includes(p.id)),
  ];

  return (
    <Page>
      <PageHeader title={t("sidebar.style")} subtitle={t("style.intro")} />

      <div className="mb-[12px] inline-flex rounded-[10px] border border-line bg-surface p-[2px]">
        {CONTEXTS.map((name) => (
          <button
            key={name}
            type="button"
            onClick={() => setContext(name)}
            className={`h-[26px] cursor-pointer rounded-[8px] px-[11px] text-[12px] font-[560] transition-colors ${
              context === name ? "bg-wash text-text" : "text-ink-3 hover:text-text"
            }`}
          >
            {t(`style.contexts.${name}`)}
          </button>
        ))}
      </div>
      {context !== "default" && (
        <p className="mb-[12px] text-[11.5px] leading-[18px] text-ink-3">
          {t("style.contextHint")}
        </p>
      )}

      <div className="grid grid-cols-2 gap-[10px]">
        {ordered.map((prompt) => {
          const active = isPicked(prompt.id);
          return (
            <button
              key={prompt.id}
              type="button"
              onClick={() => pick(prompt.id)}
              className={`relative flex cursor-pointer flex-col gap-[6px] rounded-[12px] border p-[14px] text-start transition-colors ${
                active
                  ? "border-logo-primary/60 bg-background-ui/10"
                  : "border-line hover:bg-wash"
              }`}
            >
              <span className="flex items-center gap-[6px] text-[13px] font-[600] text-text">
                {t(`style.presets.${prompt.id}.name`, prompt.name)}
                {active && <Check size={13} className="text-logo-primary" />}
              </span>
              <span className="text-[12px] leading-[18px] text-ink-2">
                {t(`style.presets.${prompt.id}.description`, "")}
              </span>
              <span className="mt-[2px] border-t border-line pt-[6px] text-[12px] italic leading-[18px] text-ink-3">
                {t(`style.presets.${prompt.id}.sample`, "")}
              </span>
            </button>
          );
        })}
      </div>

      {!enabled && (
        <p className="mt-[12px] text-[11.5px] text-ink-3">
          {t("style.disabledHint")}
        </p>
      )}

      <h2 className="mt-[28px] text-[11px] font-[600] tracking-[-0.005em] text-ink-3">
        {t("style.autoCleanup")}
      </h2>
      <div className="mt-[10px] rounded-[16px] border border-line bg-surface px-[24px] py-[8px]">
        <FillerWordRemoval descriptionMode="inline" grouped={false} />
      </div>

      <h2 className="mt-[28px] text-[11px] font-[600] tracking-[-0.005em] text-ink-3">
        {t("style.perApp")}
      </h2>
      <div className="mt-[10px] rounded-[16px] border border-line bg-surface px-[24px] py-[8px]">
        <StyleRules />
      </div>
    </Page>
  );
};
