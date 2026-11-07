import React from "react";
import { useTranslation } from "react-i18next";
import { Page, PageHeader } from "@/components/hub/Primitives";
import { Snippets } from "@/components/settings/Snippets";

/** Snippets: a spoken trigger expands into a fixed block of text. */
export const SnippetsPage: React.FC = () => {
  const { t } = useTranslation();
  return (
    <Page>
      <PageHeader
        title={t("sidebar.snippets")}
        subtitle={t("snippets.subtitle")}
      />
      <Snippets descriptionMode="inline" grouped={false} />
    </Page>
  );
};
