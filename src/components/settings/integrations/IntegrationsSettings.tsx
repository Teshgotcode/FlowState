import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Cable, Copy, Eye, EyeOff, RefreshCw } from "lucide-react";
import { commands } from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Button } from "../../ui/Button";
import { Input } from "../../ui/Input";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { ToggleSwitch } from "../../ui/ToggleSwitch";
import { copyToClipboard } from "../history/clipboard";

export const IntegrationsSettings: React.FC = () => {
  const { t } = useTranslation();
  const { getSetting, refreshSettings } = useSettings();

  const enabled = getSetting("mcp_enabled") ?? false;
  const port = getSetting("mcp_port") ?? 7869;
  const token = getSetting("mcp_token") ?? "";

  // The token is shown once — at enable/regenerate — and masked otherwise;
  // `revealed` is UI state, reset when the panel remounts.
  const [revealed, setRevealed] = useState(false);
  const [running, setRunning] = useState<boolean | null>(null);
  const [busy, setBusy] = useState(false);
  // Port editing keeps a local draft so an unfinished number is not sent.
  const [portDraft, setPortDraft] = useState<string | null>(null);

  const refreshStatus = useCallback(async () => {
    const status = await commands.getMcpStatus();
    setRunning(status.running);
  }, []);

  useEffect(() => {
    refreshStatus();
  }, [refreshStatus, enabled, port, token]);

  const handleToggle = async (on: boolean) => {
    setBusy(true);
    try {
      const result = await commands.changeMcpEnabledSetting(on);
      if (result.status === "error") {
        toast.error(result.error);
      } else {
        // A first-time enable just generated the token: show it now, this
        // once, so the user can copy it into their MCP client.
        setRevealed(on);
        await refreshSettings();
        await refreshStatus();
      }
    } finally {
      setBusy(false);
    }
  };

  const handleRegenerate = async () => {
    setBusy(true);
    try {
      const result = await commands.regenerateMcpToken();
      if (result.status === "error") {
        toast.error(result.error);
      } else {
        setRevealed(true);
        await refreshSettings();
        await refreshStatus();
      }
    } finally {
      setBusy(false);
    }
  };

  const commitPort = async () => {
    const draft = portDraft;
    setPortDraft(null);
    if (draft === null) return;
    const parsed = Number.parseInt(draft, 10);
    if (Number.isNaN(parsed) || parsed < 1 || parsed > 65535) {
      toast.error(t("settings.integrations.mcp.portInvalid"));
      return;
    }
    if (parsed === port) return;
    const result = await commands.changeMcpPortSetting(parsed);
    if (result.status === "error") {
      toast.error(result.error);
    } else {
      await refreshSettings();
      await refreshStatus();
    }
  };

  const handleCopyToken = async () => {
    if (await copyToClipboard(token)) {
      toast.success(t("settings.integrations.mcp.tokenCopied"));
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.integrations.mcp.title")}>
        <ToggleSwitch
          checked={enabled}
          onChange={handleToggle}
          isUpdating={busy}
          label={t("settings.integrations.mcp.enable.label")}
          description={t("settings.integrations.mcp.enable.description")}
          descriptionMode="tooltip"
          grouped={true}
        />

        {enabled && (
          <div className="p-4 space-y-4">
            <div className="flex items-center gap-2 text-sm">
              <Cable width={16} height={16} className="shrink-0" />
              {running === null ? (
                <span className="text-ink-3">
                  {t("settings.integrations.mcp.statusChecking")}
                </span>
              ) : running ? (
                <span className="text-green-600 dark:text-green-400">
                  {t("settings.integrations.mcp.statusRunning", { port })}
                </span>
              ) : (
                <span className="text-ink-3">
                  {t("settings.integrations.mcp.statusNotRunning")}
                </span>
              )}
            </div>

            <div className="space-y-1">
              <p className="text-xs font-medium text-ink-3 uppercase tracking-wide">
                {t("settings.integrations.mcp.port.label")}
              </p>
              <Input
                type="number"
                min={1}
                max={65535}
                variant="compact"
                className="w-32"
                value={portDraft ?? String(port)}
                onChange={(event) => setPortDraft(event.target.value)}
                onBlur={commitPort}
                onKeyDown={(event) => {
                  if (event.key === "Enter") commitPort();
                }}
                aria-label={t("settings.integrations.mcp.port.label")}
              />
            </div>

            <div className="space-y-1">
              <p className="text-xs font-medium text-ink-3 uppercase tracking-wide">
                {t("settings.integrations.mcp.token.label")}
              </p>
              <div className="flex items-center gap-2">
                <Input
                  type={revealed ? "text" : "password"}
                  variant="compact"
                  className="flex-1 min-w-[200px] font-mono"
                  value={token}
                  readOnly
                  aria-label={t("settings.integrations.mcp.token.label")}
                />
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={() => setRevealed(!revealed)}
                  title={
                    revealed
                      ? t("settings.integrations.mcp.tokenHide")
                      : t("settings.integrations.mcp.tokenReveal")
                  }
                >
                  {revealed ? <EyeOff width={16} /> : <Eye width={16} />}
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleCopyToken}
                  title={t("settings.integrations.mcp.tokenCopy")}
                >
                  <Copy width={16} />
                </Button>
                <Button
                  variant="ghost"
                  size="sm"
                  onClick={handleRegenerate}
                  disabled={busy}
                  title={t("settings.integrations.mcp.tokenRegenerate")}
                >
                  <RefreshCw width={16} />
                </Button>
              </div>
              <p className="text-xs text-ink-3">
                {t("settings.integrations.mcp.tokenHint")}
              </p>
            </div>

            <p className="text-xs text-ink-3">
              {t("settings.integrations.mcp.setupHint")}
            </p>
          </div>
        )}
      </SettingsGroup>
    </div>
  );
};
