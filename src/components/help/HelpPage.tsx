import React from "react";
import { useTranslation } from "react-i18next";
import { ExternalLink } from "lucide-react";
import { Page, PageHeader } from "@/components/hub/Primitives";

/** Help: how dictation works, and the two links worth having in the app. */
export const HelpPage: React.FC = () => {
  const { t } = useTranslation();
  const steps = [t("help.step1"), t("help.step2"), t("help.step3")];

  return (
    <Page>
      <PageHeader title={t("sidebar.help")} subtitle={t("help.subtitle")} />
      <section className="border-t border-line pt-[18px]">
        <h2 className="font-display text-[17px] font-semibold tracking-[-0.005em] text-text">
          {t("help.howTitle")}
        </h2>
        <ol className="mt-[10px] flex flex-col gap-[8px] text-[13.5px] leading-[21px] text-ink-2">
          {steps.map((step, index) => (
            <li key={step} className="flex gap-[8px]">
              <span className="tabular-nums text-ink-3">{index + 1}.</span>
              <span>{step}</span>
            </li>
          ))}
        </ol>
      </section>
      <section className="mt-[26px] border-t border-line">
        <a
          href="https://github.com/flowstate-app/flowstate"
          target="_blank"
          rel="noreferrer"
          className="flex items-center justify-between border-b border-line py-[13px] text-[13.5px] text-text hover:text-logo-primary"
        >
          {t("help.github")}
          <ExternalLink size={15} className="text-ink-3" />
        </a>
        <a
          href="https://github.com/flowstate-app/flowstate/issues"
          target="_blank"
          rel="noreferrer"
          className="flex items-center justify-between py-[13px] text-[13.5px] text-text hover:text-logo-primary"
        >
          {t("help.issues")}
          <ExternalLink size={15} className="text-ink-3" />
        </a>
      </section>
      <p className="mt-[22px] max-w-[58ch] text-[12.5px] leading-[20px] text-ink-3">
        {t("help.localOnly")}
      </p>
    </Page>
  );
};
