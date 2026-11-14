import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import type { Snippet } from "@/bindings";
import {
  normalizeTrigger,
  parseSnippetsError,
  parseSnippetsFile,
  pickSnippetsExportPath,
  pickSnippetsFile,
  readSnippetsFile,
  writeSnippetsFile,
} from "@/lib/snippets/io";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Textarea } from "../ui/Textarea";
import { Button } from "../ui/Button";
import { Switch } from "../ui/Switch";
import { SettingContainer } from "../ui/SettingContainer";

interface SnippetsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const Snippets: React.FC<SnippetsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const snippets = getSetting("snippets") || [];

    const [newTrigger, setNewTrigger] = useState("");
    const [newExpansion, setNewExpansion] = useState("");
    const [newCaseSensitive, setNewCaseSensitive] = useState(false);
    const normalizedTrigger = normalizeTrigger(newTrigger);

    const isDuplicate = (trigger: string) =>
      snippets.some(
        (snippet) => snippet.trigger.toLowerCase() === trigger.toLowerCase(),
      );

    const handleAdd = () => {
      if (!normalizedTrigger) return;
      if (isDuplicate(normalizedTrigger)) {
        toast.error(
          t("settings.advanced.snippets.duplicate", {
            trigger: normalizedTrigger,
          }),
        );
        return;
      }
      updateSetting("snippets", [
        ...snippets,
        {
          trigger: normalizedTrigger,
          expansion: newExpansion,
          case_sensitive: newCaseSensitive,
        },
      ]);
      setNewTrigger("");
      setNewExpansion("");
      setNewCaseSensitive(false);
    };

    const handleRemove = (trigger: string) => {
      updateSetting(
        "snippets",
        snippets.filter((snippet) => snippet.trigger !== trigger),
      );
    };

    /** Commits one row's edits; called on blur so typing stays local. */
    const commit = (index: number, next: Snippet) => {
      const current = snippets[index];
      if (
        current &&
        current.trigger === next.trigger &&
        current.expansion === next.expansion &&
        current.case_sensitive === next.case_sensitive
      ) {
        return;
      }
      const updated = [...snippets];
      updated[index] = next;
      updateSetting("snippets", updated);
    };

    const handleExport = async () => {
      try {
        const path = await pickSnippetsExportPath();
        if (!path) return;
        if (await writeSnippetsFile(path, snippets)) {
          toast.success(t("settings.advanced.snippets.exportSaved"));
        } else {
          toast.error(t("settings.advanced.snippets.exportFailed"));
        }
      } catch (error) {
        console.error("Failed to export snippets:", error);
        toast.error(t("settings.advanced.snippets.exportFailed"));
      }
    };

    const handleImport = async () => {
      try {
        const path = await pickSnippetsFile();
        if (!path) return;
        const { snippets: imported, error } = parseSnippetsFile(
          await readSnippetsFile(path),
        );
        if (error) {
          toast.error(parseSnippetsError(error, t));
          return;
        }
        if (imported.length === 0) {
          toast.error(t("settings.advanced.snippets.importEmpty"));
          return;
        }
        // Merge: imported snippets win on trigger collisions, existing ones stay.
        const merged = [...snippets];
        for (const snippet of imported) {
          const existing = merged.findIndex(
            (s) => s.trigger.toLowerCase() === snippet.trigger.toLowerCase(),
          );
          if (existing >= 0) {
            merged[existing] = snippet;
          } else {
            merged.push(snippet);
          }
        }
        updateSetting("snippets", merged);
        toast.success(
          t("settings.advanced.snippets.imported", { count: imported.length }),
        );
      } catch (error) {
        console.error("Failed to import snippets:", error);
        toast.error(t("settings.advanced.snippets.importReadFailed"));
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.snippets.title")}
          description={t("settings.advanced.snippets.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex flex-col gap-2 w-full">
            <div className="flex flex-wrap items-center gap-2">
              <Input
                type="text"
                className="max-w-40"
                value={newTrigger}
                onChange={(e) => setNewTrigger(e.target.value)}
                onKeyDown={(e) => {
                  if (e.key === "Enter") {
                    e.preventDefault();
                    handleAdd();
                  }
                }}
                placeholder={t("settings.advanced.snippets.triggerPlaceholder")}
                variant="compact"
                disabled={isUpdating("snippets")}
              />
              <Button
                onClick={handleAdd}
                disabled={!normalizedTrigger || isUpdating("snippets")}
                variant="primary"
                size="md"
              >
                {t("settings.advanced.snippets.add")}
              </Button>
              <div className="ml-auto flex items-center gap-2">
                <Button
                  onClick={handleImport}
                  disabled={isUpdating("snippets")}
                  variant="secondary"
                  size="sm"
                >
                  {t("settings.advanced.snippets.import")}
                </Button>
                <Button
                  onClick={handleExport}
                  disabled={snippets.length === 0 || isUpdating("snippets")}
                  variant="secondary"
                  size="sm"
                >
                  {t("settings.advanced.snippets.export")}
                </Button>
              </div>
            </div>
            <div className="flex items-start gap-2">
              <Textarea
                className="flex-1 min-h-16"
                value={newExpansion}
                onChange={(e) => setNewExpansion(e.target.value)}
                placeholder={t(
                  "settings.advanced.snippets.expansionPlaceholder",
                )}
                variant="compact"
                disabled={isUpdating("snippets")}
              />
              <label className="flex items-center gap-1 text-sm font-medium whitespace-nowrap">
                <Switch
                  checked={newCaseSensitive}
                  onChange={setNewCaseSensitive}
                  disabled={isUpdating("snippets")}
                  ariaLabel={t("settings.advanced.snippets.caseSensitive")}
                />
                {t("settings.advanced.snippets.caseSensitive")}
              </label>
            </div>
          </div>
        </SettingContainer>
        {snippets.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-line"} flex flex-col gap-2`}
          >
            {snippets.map((snippet, index) => (
              <div key={snippet.trigger} className="flex flex-col gap-1">
                <div className="flex flex-wrap items-center gap-2">
                  <Input
                    type="text"
                    className="max-w-40"
                    defaultValue={snippet.trigger}
                    onBlur={(e) => {
                      const trigger = normalizeTrigger(e.target.value);
                      if (!trigger || trigger === snippet.trigger) return;
                      if (isDuplicate(trigger) && snippet.trigger !== trigger) {
                        toast.error(
                          t("settings.advanced.snippets.duplicate", {
                            trigger,
                          }),
                        );
                        e.target.value = snippet.trigger;
                        return;
                      }
                      commit(index, { ...snippet, trigger });
                    }}
                    variant="compact"
                    aria-label={t("settings.advanced.snippets.triggerLabel")}
                    disabled={isUpdating("snippets")}
                  />
                  <Button
                    onClick={() => handleRemove(snippet.trigger)}
                    disabled={isUpdating("snippets")}
                    variant="secondary"
                    size="sm"
                    aria-label={t("settings.advanced.snippets.remove")}
                  >
                    <svg
                      className="w-3 h-3"
                      fill="none"
                      stroke="currentColor"
                      viewBox="0 0 24 24"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        strokeWidth={2}
                        d="M6 18L18 6M6 6l12 12"
                      />
                    </svg>
                  </Button>
                </div>
                <div className="flex items-start gap-2">
                  <Textarea
                    className="flex-1 min-h-16"
                    defaultValue={snippet.expansion}
                    onBlur={(e) =>
                      commit(index, { ...snippet, expansion: e.target.value })
                    }
                    variant="compact"
                    aria-label={t("settings.advanced.snippets.expansionLabel")}
                    disabled={isUpdating("snippets")}
                  />
                  <label className="flex items-center gap-1 text-sm font-medium whitespace-nowrap">
                    <Switch
                      checked={snippet.case_sensitive ?? false}
                      onChange={(checked) =>
                        commit(index, { ...snippet, case_sensitive: checked })
                      }
                      disabled={isUpdating("snippets")}
                      ariaLabel={t("settings.advanced.snippets.caseSensitive")}
                    />
                    {t("settings.advanced.snippets.caseSensitive")}
                  </label>
                </div>
              </div>
            ))}
          </div>
        )}
      </>
    );
  },
);
