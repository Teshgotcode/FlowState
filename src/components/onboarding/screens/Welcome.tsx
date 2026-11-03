import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "../../ui/Button";
import { Radio } from "../../ui/Radio";
import { Mascot } from "../../shared/Mascot";
import {
  LanguageOptions,
  LanguageSearchField,
  useLanguagePicker,
} from "../../shared/LanguagePicker";
import FlowStateLogo from "../../icons/FlowStateLogo";
import { OnboardingShell, ScreenHeader } from "../OnboardingShell";
import { AppLanguageMenu } from "../AppLanguageMenu";
import { useOnboardingStore } from "../onboardingStore";
import { CHINESE_SCRIPTS, isChinese } from "@/lib/utils/languageSuggestions";
import { CURATED_ASR, isWeakLanguage } from "@/lib/models/catalog";
import { modelMatchesLanguage } from "@handy-computer/models";
import { getSupportedLanguage } from "@/i18n";
import { languageList } from "@/lib/constants/languages";
import { useSettings } from "@/hooks/useSettings";
import {
  installEsPackage,
  isEsPackageInstalled,
} from "@/lib/constants/esPackage";

// The mascot waves on the first visit only.
let waved = false;

interface WelcomeProps {
  progress: { index: number; count: number };
  onContinue: () => void;
}

/** Screen 1: greeting, spoken languages, and Chinese script. */
export const Welcome: React.FC<WelcomeProps> = ({ progress, onContinue }) => {
  const { t, i18n } = useTranslation();
  const { settings, updateSetting } = useSettings();
  const { languages, script, modelId, preview, set } = useOnboardingStore();
  const picker = useLanguagePicker({
    languages,
    onChange: (next) => {
      // A model picked for other languages no longer counts as the pick.
      const picked = CURATED_ASR.find((m) => m.id === modelId);
      const stillFits =
        !picked || next.every((l) => modelMatchesLanguage(picked, l));
      set({
        languages: next,
        ...(stillFits ? {} : { modelId: null }),
      });
    },
  });
  const [wave] = useState(() => !waved);
  const [esInstalling, setEsInstalling] = useState(false);
  useEffect(() => {
    waved = true;
    const id = setTimeout(
      () => picker.inputRef.current?.focus({ preventScroll: true }),
      50,
    );
    return () => clearTimeout(id);
  }, []);

  const ui = i18n.language;
  const chinese = languages.filter(isChinese);
  const weak = languages.filter(isWeakLanguage);

  const hint = !languages.length
    ? t("onboarding.welcome.hint.pickOne")
    : weak.length
      ? t("onboarding.welcome.hint.weak", {
          count: weak.length,
          languages: languageList(weak, ui),
        })
      : "";

  // What the menus are in right now (a preview never writes the setting).
  const uiLanguage = getSupportedLanguage(ui) || "en";

  /* ---------- the built-in ES package (one click, first run) ---------- */
  const customWords = settings?.custom_words ?? [];
  const snippets = settings?.snippets ?? [];
  const esInstalled = isEsPackageInstalled(customWords, snippets);
  // Offered where it makes sense: Norwegian UI or spoken Norwegian.
  const showEsPackage =
    uiLanguage === "no" || languages.includes("nb") || languages.includes("nn");
  const installEs = async () => {
    // One click: the ES package merges into the dictionary and snippets
    // (nothing the user already has is overwritten). A preview never writes.
    if (preview || esInstalling) return;
    setEsInstalling(true);
    try {
      const merged = installEsPackage(customWords, snippets);
      await updateSetting("custom_words", merged.words);
      await updateSetting("snippets", merged.snippets);
    } finally {
      setEsInstalling(false);
    }
  };
  const changeUiLanguage = (code: string) => {
    i18n.changeLanguage(code);
    if (!preview) updateSetting("app_language", code);
  };

  return (
    <OnboardingShell
      progress={progress}
      pill={<AppLanguageMenu value={uiLanguage} onChange={changeUiLanguage} />}
      hint={hint}
      hintTone={weak.length ? "warning" : "quiet"}
      primary={
        <Button
          variant="primary"
          size="lg"
          disabled={!languages.length}
          onClick={onContinue}
        >
          {t("onboarding.shell.continue")}
        </Button>
      }
    >
      <div className="flex h-full min-h-0 flex-col items-center px-gutter pt-[6px] pb-[4px] text-center">
        <div className="mb-[10px] inline-flex items-end gap-[8px] leading-none">
          <Mascot size={54} animation={wave ? "wave" : null} />
          <FlowStateLogo width={112} className="mb-[4px] block" />
        </div>
        <ScreenHeader
          title={t("onboarding.welcome.title")}
          sub={t("onboarding.welcome.subtitle")}
        />

        <div className="mt-lead flex min-h-0 w-full flex-1 flex-col overflow-hidden rounded-panel border-[1.5px] border-line-strong bg-background">
          <LanguageSearchField
            picker={picker}
            className="border-b border-line"
            inputClassName="min-w-[120px] text-body"
          />
          <div
            role="group"
            aria-label={t("onboarding.welcome.languagesLabel")}
            className="grid min-h-0 flex-1 grid-cols-3 content-start gap-[2px] overflow-y-auto p-[6px] pb-[14px] text-start [mask-image:linear-gradient(to_bottom,black_calc(100%-40px),transparent_calc(100%-6px))]"
          >
            <LanguageOptions
              picker={picker}
              noMatch={t("onboarding.welcome.noMatch", {
                query: picker.query,
              })}
              emptyClassName="p-[24px]"
            />
          </div>
        </div>

        {chinese.length > 0 && (
          <div className="w-full animate-rise pt-[12px] text-start">
            <p className="mt-0 mb-[8px] text-small font-semibold text-text">
              {t("onboarding.welcome.chinese.question", {
                languages: languageList(chinese, ui),
              })}
            </p>
            <div
              role="radiogroup"
              aria-label={t("onboarding.welcome.chinese.label")}
              className="flex flex-wrap gap-[8px]"
            >
              {CHINESE_SCRIPTS.map((s) => {
                const picked = script === s.id;
                return (
                  <button
                    key={s.id}
                    type="button"
                    role="radio"
                    aria-checked={picked}
                    onClick={() => set({ script: s.id })}
                    className={`inline-flex h-[34px] cursor-pointer items-center gap-[8px] rounded-control border-[1.5px] bg-background ps-[10px] pe-[14px] text-small font-semibold text-text ${
                      picked
                        ? "border-plum-pick"
                        : "border-line-strong hover:border-ink-3"
                    }`}
                  >
                    <Radio checked={picked} />
                    <span>
                      <span lang={s.lang} className="font-cjk-subset">
                        {s.native}
                      </span>{" "}
                      {t(`onboarding.welcome.chinese.${s.id}`)}
                    </span>
                  </button>
                );
              })}
            </div>
          </div>
        )}

        {showEsPackage && (
          <div className="w-full animate-rise pt-[12px] text-start">
            <div className="flex items-center justify-between gap-[12px] rounded-control border-[1.5px] border-line-strong bg-background p-[10px]">
              <div className="min-w-0">
                <p className="m-0 text-small font-semibold text-text">
                  {t("onboarding.welcome.es.title")}
                </p>
                <p className="m-0 text-small text-ink-3">
                  {t("onboarding.welcome.es.description")}
                </p>
              </div>
              <Button
                variant={esInstalled ? "secondary" : "primary"}
                size="sm"
                disabled={esInstalled || esInstalling}
                onClick={() => void installEs()}
              >
                {esInstalled
                  ? t("onboarding.welcome.es.enabled")
                  : t("onboarding.welcome.es.enable")}
              </Button>
            </div>
          </div>
        )}
      </div>
    </OnboardingShell>
  );
};
