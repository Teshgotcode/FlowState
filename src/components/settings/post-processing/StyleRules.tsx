import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { Trash2 } from "lucide-react";
import { commands } from "@/bindings";
import { Dropdown, SettingContainer } from "@/components/ui";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { useSettings } from "../../../hooks/useSettings";

// Per-app style rules: bind a style (a post-processing prompt preset) to an
// app, e.g. Slack = Short. Rules are stored locally in settings; when any
// rule exists, the focused app's rule wins and every other app falls back to
// the Neutral preset.
export const StyleRules: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings, isUpdating } = useSettings();
  const [newApp, setNewApp] = useState("");
  const [newStyle, setNewStyle] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const prompts = getSetting("post_process_prompts") || [];
  const rules = getSetting("post_process_app_rules") || {};

  const styleOptions = prompts.map((p) => ({
    value: p.id,
    label: p.name,
  }));

  const handleCapture = async () => {
    setError(null);
    try {
      const app = await commands.getFrontmostApp();
      const key = app?.bundleId ?? app?.name;
      if (!key) {
        setError(t("settings.postProcessing.styleRules.currentAppUnavailable"));
        return;
      }
      setNewApp(key);
    } catch (err) {
      console.error("Failed to detect the frontmost app:", err);
      setError(t("settings.postProcessing.styleRules.currentAppUnavailable"));
    }
  };

  const handleAdd = async () => {
    const key = newApp.trim();
    if (!key || !newStyle || busy) return;
    setBusy(true);
    setError(null);
    try {
      const result = await commands.setAppStyleRule(key, newStyle);
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      setNewApp("");
      setNewStyle("");
      await refreshSettings();
    } catch (err) {
      console.error("Failed to add style rule:", err);
    } finally {
      setBusy(false);
    }
  };

  const handleRuleChange = async (appKey: string, styleId: string) => {
    setBusy(true);
    setError(null);
    try {
      const result = await commands.setAppStyleRule(appKey, styleId);
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      await refreshSettings();
    } catch (err) {
      console.error("Failed to update style rule:", err);
    } finally {
      setBusy(false);
    }
  };

  const handleRemove = async (appKey: string) => {
    setBusy(true);
    setError(null);
    try {
      const result = await commands.setAppStyleRule(appKey, null);
      if (result.status === "error") {
        setError(result.error);
        return;
      }
      await refreshSettings();
    } catch (err) {
      console.error("Failed to remove style rule:", err);
    } finally {
      setBusy(false);
    }
  };

  const ruleEntries = Object.entries(rules);

  return (
    <SettingContainer
      title={t("settings.postProcessing.styleRules.title")}
      description={t("settings.postProcessing.styleRules.description")}
      descriptionMode="tooltip"
      layout="stacked"
      grouped={true}
    >
      <div className="space-y-3">
        {ruleEntries.length === 0 ? (
          <p className="text-sm text-ink-3">
            {t("settings.postProcessing.styleRules.empty")}
          </p>
        ) : (
          <div className="space-y-2">
            {ruleEntries.map(([appKey, styleId]) => (
              <div
                key={appKey}
                className="flex items-center gap-2 rounded-md border border-line bg-wash p-2"
              >
                <span
                  className="min-w-0 flex-1 truncate text-sm"
                  title={appKey}
                >
                  {appKey}
                </span>
                <Dropdown
                  options={styleOptions}
                  selectedValue={styleId ?? null}
                  onSelect={(value) => handleRuleChange(appKey, value)}
                  placeholder={t("settings.postProcessing.styleRules.style")}
                  disabled={busy || isUpdating("post_process_app_rules")}
                  className="w-48"
                />
                <Button
                  onClick={() => handleRemove(appKey)}
                  variant="secondary"
                  size="md"
                  disabled={busy}
                  aria-label={t("settings.postProcessing.styleRules.remove")}
                  className="h-10 w-10 shrink-0 p-0"
                >
                  <Trash2 className="h-4 w-4" aria-hidden="true" />
                </Button>
              </div>
            ))}
          </div>
        )}

        <div className="flex flex-wrap items-center gap-2 pt-1">
          <div className="flex items-center gap-2">
            <Input
              type="text"
              value={newApp}
              onChange={(e) => setNewApp(e.target.value)}
              placeholder={t(
                "settings.postProcessing.styleRules.appPlaceholder",
              )}
              variant="compact"
              className="min-w-[220px]"
            />
            <Button
              onClick={handleCapture}
              variant="secondary"
              size="md"
              disabled={busy}
            >
              {t("settings.postProcessing.styleRules.useCurrentApp")}
            </Button>
          </div>
          <Dropdown
            options={styleOptions}
            selectedValue={newStyle || null}
            onSelect={(value) => setNewStyle(value)}
            placeholder={t("settings.postProcessing.styleRules.style")}
            disabled={busy}
            className="w-48"
          />
          <Button
            onClick={handleAdd}
            variant="primary"
            size="md"
            disabled={!newApp.trim() || !newStyle || busy}
          >
            {t("settings.postProcessing.styleRules.addRule")}
          </Button>
        </div>

        {error && <p className="text-sm text-red-500">{error}</p>}
      </div>
    </SettingContainer>
  );
};
