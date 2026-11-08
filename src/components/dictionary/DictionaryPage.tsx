import React, { useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { FileUp } from "lucide-react";
import { Page, PageHeader } from "@/components/hub/Primitives";
import { CustomWords } from "@/components/settings/CustomWords";
import { Button } from "@/components/ui/Button";
import { Dialog } from "@/components/ui/Dialog";
import { useSettings } from "@/hooks/useSettings";

/** Accepts one word per line, commas, semicolons or tabs: the shapes people
 * paste out of a spreadsheet or a chat. */
const splitWords = (raw: string) =>
  raw
    .split(/[\n,;\t]+/)
    .map((word) => word.trim())
    .filter(Boolean);

/** Dictionary: the words the transcriber should hear as written. */
export const DictionaryPage: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, updateSetting } = useSettings();
  const [importOpen, setImportOpen] = useState(false);
  const [raw, setRaw] = useState("");
  const textareaRef = useRef<HTMLTextAreaElement | null>(null);

  const existing = getSetting("custom_words") || [];
  const fresh = useMemo(() => {
    const seen = new Set(existing.map((w) => w.toLowerCase()));
    return splitWords(raw).filter((word) => {
      const key = word.toLowerCase();
      if (seen.has(key)) return false;
      seen.add(key);
      return true;
    });
  }, [raw, existing]);

  const runImport = () => {
    if (fresh.length === 0) return;
    void updateSetting("custom_words", [...existing, ...fresh]);
    setRaw("");
    setImportOpen(false);
  };

  return (
    <Page>
      <PageHeader
        title={t("sidebar.dictionary")}
        subtitle={t("dictionary.subtitle")}
      />
      <div className="mb-[12px] flex justify-end">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => setImportOpen(true)}
        >
          <FileUp size={13} />
          {t("dictionary.import")}
        </Button>
      </div>
      <div className="rounded-[16px] border border-line bg-surface px-[24px] py-[8px]">
        <CustomWords descriptionMode="inline" grouped={false} />
      </div>

      <Dialog
        open={importOpen}
        onOpenChange={setImportOpen}
        title={t("dictionary.importTitle")}
        description={t("dictionary.importHint")}
        closeLabel={t("common.close")}
        initialFocusRef={textareaRef}
        footer={
          <>
            <Button
              variant="ghost"
              size="sm"
              onClick={() => setImportOpen(false)}
            >
              {t("common.cancel")}
            </Button>
            <Button
              variant="primary"
              size="sm"
              disabled={fresh.length === 0}
              onClick={runImport}
            >
              {t("dictionary.importCount", { count: fresh.length })}
            </Button>
          </>
        }
      >
        <textarea
          ref={textareaRef}
          value={raw}
          onChange={(event) => setRaw(event.target.value)}
          placeholder={t("dictionary.importPlaceholder")}
          rows={8}
          className="w-full resize-none rounded-[10px] border border-line bg-surface px-[12px] py-[10px] font-mono text-[12.5px] leading-[20px] text-text outline-none focus:border-logo-primary"
        />
      </Dialog>
    </Page>
  );
};
