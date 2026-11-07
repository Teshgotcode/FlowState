import React from "react";
import { useTranslation } from "react-i18next";
import { HardDrive, Languages, Keyboard } from "lucide-react";
import { Button } from "../../ui/Button";
import { OnboardingShell } from "../OnboardingShell";
import { AppLanguageMenu } from "../AppLanguageMenu";
import { useSettings } from "@/hooks/useSettings";
import { getSupportedLanguage } from "@/i18n";

interface IntroProps {
  progress: { index: number; count: number };
  onContinue: () => void;
  preview?: boolean;
}

/**
 * Screen 0: the promise, before any setting is touched. Three claims, each of
 * which the app actually keeps: nothing leaves the machine, it works in every
 * app, and it speaks both languages from the first launch.
 */
export const Intro: React.FC<IntroProps> = ({
  progress,
  onContinue,
  preview = false,
}) => {
  const { t, i18n } = useTranslation();
  const { updateSetting } = useSettings();
  const uiLanguage = getSupportedLanguage(i18n.language) || "en";

  const changeUiLanguage = (code: string) => {
    i18n.changeLanguage(code);
    if (!preview) updateSetting("app_language", code);
  };

  const promises = [
    {
      icon: HardDrive,
      title: t("onboarding.intro.localTitle"),
      body: t("onboarding.intro.localBody"),
    },
    {
      icon: Keyboard,
      title: t("onboarding.intro.anywhereTitle"),
      body: t("onboarding.intro.anywhereBody"),
    },
    {
      icon: Languages,
      title: t("onboarding.intro.languageTitle"),
      body: t("onboarding.intro.languageBody"),
    },
  ];

  return (
    <OnboardingShell
      progress={progress}
      pill={<AppLanguageMenu value={uiLanguage} onChange={changeUiLanguage} />}
      primary={
        // The house primary is pastel; on this near-white screen it reads as
        // disabled, so the one action on the page is solid ink.
        <button
          type="button"
          onClick={onContinue}
          className="h-[38px] cursor-pointer rounded-[9px] border border-text bg-text px-[18px] text-[13px] font-[600] text-surface transition-opacity hover:opacity-90 focus-visible:ring-2 focus-visible:ring-logo-primary"
        >
          {t("onboarding.intro.cta")}
        </button>
      }
    >
      <div className="flex h-full flex-col justify-center ps-[28px] pe-[28px] pb-[8px]">
        <h1 className="max-w-[19ch] font-display text-[44px] font-semibold leading-[50px] tracking-[-0.02em] text-text">
          {t("onboarding.intro.title")}
        </h1>
        <p className="mt-[12px] max-w-[46ch] text-[14.5px] leading-[23px] text-ink-2">
          {t("onboarding.intro.subtitle")}
        </p>

        <ul className="mt-[26px] flex flex-col gap-[14px]">
          {promises.map(({ icon: Icon, title, body }) => (
            <li key={title} className="flex items-start gap-[11px]">
              <span className="mt-[1px] flex h-[30px] w-[30px] shrink-0 items-center justify-center rounded-[8px] border border-line bg-surface-soft text-ink-2">
                <Icon size={16} strokeWidth={1.9} />
              </span>
              <div className="min-w-0">
                <p className="text-[13px] font-[600] leading-[19px] text-text">
                  {title}
                </p>
                <p className="text-[12.5px] leading-[19px] text-ink-2">{body}</p>
              </div>
            </li>
          ))}
        </ul>
      </div>
    </OnboardingShell>
  );
};
