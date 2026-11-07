import React, { useState } from "react";
import { useTranslation } from "react-i18next";

import { DictionaryPage } from "@/components/dictionary/DictionaryPage";
import { SnippetsPage } from "@/components/snippets/SnippetsPage";
import { StylePage } from "@/components/style/StylePage";
import { TransformsPage } from "@/components/transforms/TransformsPage";

type Tab = "dictionary" | "snippets" | "transforms" | "style";

const TABS: { id: Tab; key: string }[] = [
  { id: "dictionary", key: "sidebar.dictionary" },
  { id: "snippets", key: "sidebar.snippets" },
  { id: "transforms", key: "sidebar.transforms" },
  { id: "style", key: "sidebar.style" },
];

/**
 * Bibliotek: the four things that change how the app writes, behind one row in
 * the rail. They were four rows before; the analysis said the rail was a menu
 * of menus.
 */
export const LibraryPage: React.FC = () => {
  const { t } = useTranslation();
  const [tab, setTab] = useState<Tab>("dictionary");

  return (
    <div>
      <div className="flex items-center gap-[16px] border-b border-line">
        {TABS.map((item) => (
          <button
            key={item.id}
            type="button"
            onClick={() => setTab(item.id)}
            className={`-mb-px cursor-pointer border-b-2 pb-[8px] text-[13px] transition-colors ${
              tab === item.id
                ? "border-text font-[550] text-text"
                : "border-transparent text-ink-3 hover:text-text"
            }`}
          >
            {t(item.key)}
          </button>
        ))}
      </div>
      <div className="pt-[20px]">
        {tab === "dictionary" && <DictionaryPage />}
        {tab === "snippets" && <SnippetsPage />}
        {tab === "transforms" && <TransformsPage />}
        {tab === "style" && <StylePage />}
      </div>
    </div>
  );
};
