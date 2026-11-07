import React from "react";
import { useTranslation } from "react-i18next";
import { Page, PageHeader } from "@/components/hub/Primitives";
import { CustomWords } from "@/components/settings/CustomWords";

/** Dictionary: the words the transcriber should hear as written. */
export const DictionaryPage: React.FC = () => {
  const { t } = useTranslation();
  return (
    <Page>
      <PageHeader
        title={t("sidebar.dictionary")}
        subtitle={t("dictionary.subtitle")}
      />
      <div className="rounded-[16px] border border-line bg-surface px-[24px] py-[8px]">
        <CustomWords descriptionMode="inline" grouped={false} />
      </div>
    </Page>
  );
};
