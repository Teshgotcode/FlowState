import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import { commands, type Snippet } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import {
  buildDictionaryBundle,
  mergeDictionaryBundle,
  parseDictionaryBundle,
  type BundleParseError,
} from "@/lib/utils/dictionaryBundle";
import {
  ES_PACKAGE_NAME,
  ES_WORDS,
  isEsPackageInstalled,
  installEsPackage,
} from "@/lib/constants/esPackage";

interface ShareDictionaryProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const parseErrorMessage = (
  error: BundleParseError,
  t: (key: string, options?: Record<string, unknown>) => string,
): string => {
  switch (error.kind) {
    case "invalidJson":
      return t("settings.advanced.share.invalidJson");
    case "notBundle":
      return t("settings.advanced.share.notBundle");
    case "unsupportedVersion":
      return t("settings.advanced.share.unsupportedVersion");
    case "empty":
      return t("settings.advanced.share.importEmpty");
  }
};

/**
 * Share the dictionary with the class ("kullet"): one JSON file with the
 * custom words, the snippets and the ES subject words, named and dated. The
 * file never leaves the machine on its own — the user sends it to whoever
 * they want. Importing merges and never overwrites the recipient's own
 * entries; duplicates are counted, conflicts are shown.
 */
export const ShareDictionary: React.FC<ShareDictionaryProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [shareName, setShareName] = useState("");
    const [esInstalling, setEsInstalling] = useState(false);

    const customWords = getSetting("custom_words") || [];
    const snippets = getSetting("snippets") || [];
    const esInstalled = isEsPackageInstalled(customWords, snippets);
    const busy =
      isUpdating("custom_words") || isUpdating("snippets") || esInstalling;

    const handleExport = async () => {
      try {
        const date = new Date().toISOString().slice(0, 10);
        const path = await save({
          defaultPath: t("settings.advanced.share.exportFileName", { date }),
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (!path) return;
        // ES subject words travel as their own list so the class can see
        // which fagord came with the package; they are also in the
        // dictionary, and import deduplicates them.
        const esTerms = ES_WORDS.filter((word) =>
          customWords.some(
            (existing) => existing.toLowerCase() === word.toLowerCase(),
          ),
        );
        const bundle = buildDictionaryBundle(
          shareName,
          customWords,
          snippets,
          esTerms,
        );
        const result = await commands.exportTextFile(
          path,
          JSON.stringify(bundle, null, 2),
        );
        if (result.status === "ok") {
          toast.success(
            t("settings.advanced.share.exportSaved", { name: bundle.name }),
          );
        } else {
          toast.error(t("settings.advanced.share.exportFailed"));
        }
      } catch (error) {
        console.error("Failed to export dictionary bundle:", error);
        toast.error(t("settings.advanced.share.exportFailed"));
      }
    };

    const handleImport = async () => {
      try {
        const path = await open({
          multiple: false,
          directory: false,
          filters: [{ name: "JSON", extensions: ["json"] }],
        });
        if (!path) return;
        const data = await readFile(path);
        const raw = new TextDecoder("utf-8").decode(new Uint8Array(data));
        const { bundle, error } = parseDictionaryBundle(raw);
        if (error || !bundle) {
          toast.error(parseErrorMessage(error!, t));
          return;
        }
        const report = mergeDictionaryBundle(customWords, snippets, bundle);
        // Two settings, one merge: the backend applies both before the
        // success toast reports the counts.
        await updateSetting("custom_words", report.words);
        await updateSetting("snippets", report.snippets);
        toast.success(
          t("settings.advanced.share.importAdded", {
            words: report.wordsAdded,
            snippets: report.snippetsAdded,
          }),
        );
        if (report.wordsSkipped + report.snippetsSkipped > 0) {
          toast.info(
            t("settings.advanced.share.importSkipped", {
              words: report.wordsSkipped,
              snippets: report.snippetsSkipped,
            }),
          );
        }
        if (report.conflicts.length > 0) {
          // Kept as the user wrote them: the conflict is shown, not overwritten.
          toast.warning(
            t("settings.advanced.share.importConflicts", {
              triggers: report.conflicts.join(", "),
            }),
          );
        }
      } catch (error) {
        console.error("Failed to import dictionary bundle:", error);
        toast.error(t("settings.advanced.share.importReadFailed"));
      }
    };

    const handleInstallEsPackage = async () => {
      setEsInstalling(true);
      try {
        const merged = installEsPackage(customWords, snippets);
        await updateSetting("custom_words", merged.words);
        await updateSetting("snippets", merged.snippets);
        toast.success(
          t("settings.advanced.share.esApplied", {
            words: merged.wordsAdded,
            snippets: merged.snippetsAdded,
          }),
        );
      } finally {
        setEsInstalling(false);
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.share.title")}
          description={t("settings.advanced.share.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-40"
              value={shareName}
              onChange={(e) => setShareName(e.target.value)}
              placeholder={t("settings.advanced.share.namePlaceholder")}
              variant="compact"
              disabled={busy}
              aria-label={t("settings.advanced.share.nameLabel")}
            />
            <Button
              onClick={handleImport}
              disabled={busy}
              variant="secondary"
              size="sm"
            >
              {t("settings.advanced.share.import")}
            </Button>
            <Button
              onClick={() => void handleExport()}
              disabled={
                busy || (customWords.length === 0 && snippets.length === 0)
              }
              variant="secondary"
              size="sm"
            >
              {t("settings.advanced.share.export")}
            </Button>
          </div>
        </SettingContainer>
        <SettingContainer
          title={t("settings.advanced.share.esTitle")}
          description={t("settings.advanced.share.esDescription")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <span
              className="text-sm text-ink-3"
              data-installed={esInstalled || undefined}
            >
              {esInstalled
                ? t("settings.advanced.share.esInstalled")
                : t("settings.advanced.share.esNotInstalled")}
            </span>
            <Button
              onClick={() => void handleInstallEsPackage()}
              disabled={busy || esInstalled}
              variant={esInstalled ? "secondary" : "primary"}
              size="sm"
            >
              {esInstalled
                ? t("settings.advanced.share.esOn")
                : t("settings.advanced.share.esEnable")}
            </Button>
          </div>
        </SettingContainer>
      </>
    );
  },
);
