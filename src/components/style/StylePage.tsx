import React from "react";
import { useTranslation } from "react-i18next";
import { Page, PageHeader } from "@/components/hub/Primitives";
import { StyleRules } from "@/components/settings/post-processing/StyleRules";

/** Style: per-app writing rules the post-processor applies. */
export const StylePage: React.FC = () => {
  const { t } = useTranslation();
  return (
    <Page>
      <PageHeader title={t("sidebar.style")} subtitle={t("style.subtitle")} />
      <div className="rounded-[16px] border border-line bg-surface px-[24px] py-[8px]">
        <StyleRules />
      </div>
    </Page>
  );
};
