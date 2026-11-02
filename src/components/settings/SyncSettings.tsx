import React, { useCallback, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { open, save } from "@tauri-apps/plugin-dialog";
import { CloudOff, FolderOpen, Package, RefreshCw, ShieldAlert } from "lucide-react";
import { toast } from "sonner";
import { commands, type ConflictRecord, type SyncStatus } from "@/bindings";
import { SettingContainer } from "../ui/SettingContainer";
import { SettingsGroup } from "../ui/SettingsGroup";
import { Switch } from "../ui/Switch";
import { Button } from "../ui/Button";
import { PathDisplay } from "../ui/PathDisplay";

/**
 * Sync between your own Macs without a cloud: a folder you pick (iCloud
 * Drive, Dropbox, OneDrive, USB) is the only channel. Works on school
 * networks where machines can't see each other, works in airplane mode —
 * it's all file copy. "Off" stops every sync activity.
 */
export const SyncSettings: React.FC = () => {
  const { t, i18n } = useTranslation();
  const [status, setStatus] = useState<SyncStatus | null>(null);
  const [conflicts, setConflicts] = useState<ConflictRecord[]>([]);
  const [syncing, setSyncing] = useState(false);
  const [busy, setBusy] = useState(false);

  const formatTime = (ms: number) =>
    new Intl.DateTimeFormat(i18n.language, {
      year: "numeric",
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    }).format(new Date(ms));
  const mounted = useRef(true);

  const refresh = useCallback(async () => {
    const result = await commands.syncGetStatus();
    if (result.status === "ok" && mounted.current) {
      setStatus(result.data);
    }
    const log = await commands.syncGetConflicts(10);
    if (log.status === "ok" && mounted.current) {
      setConflicts(log.data);
    }
  }, []);

  useEffect(() => {
    mounted.current = true;
    refresh();
    return () => {
      mounted.current = false;
    };
  }, [refresh]);

  // While this page is open, keep the shared folder in step automatically.
  useEffect(() => {
    if (!status?.enabled) return;
    const timer = setInterval(() => {
      commands.syncNow().then((result) => {
        if (result.status === "ok") setStatus(result.data.status);
      });
    }, 60_000);
    return () => clearInterval(timer);
  }, [status?.enabled]);

  const runSync = async (manual: boolean) => {
    setSyncing(true);
    try {
      const result = await commands.syncNow();
      if (result.status === "ok") {
        setStatus(result.data.status);
        const found = result.data.conflicts.length;
        if (manual) {
          toast.success(
            found > 0
              ? t("settings.sync.toasts.doneWithConflicts", { count: found })
              : t("settings.sync.toasts.done", {
                  history: result.data.history_imported,
                }),
          );
        }
        await refresh();
      } else if (manual) {
        toast.error(result.error);
      }
    } finally {
      setSyncing(false);
    }
  };

  const chooseFolder = async () => {
    const picked = await open({ directory: true, multiple: false });
    if (!picked) return;
    setBusy(true);
    try {
      const result = await commands.syncSetFolder(picked as string);
      if (result.status === "ok") {
        setStatus(result.data);
        toast.success(t("settings.sync.toasts.folderSet"));
      } else {
        toast.error(result.error);
      }
    } finally {
      setBusy(false);
    }
  };

  const setEnabled = async (enabled: boolean) => {
    setBusy(true);
    try {
      const result = await commands.syncSetEnabled(enabled);
      if (result.status === "ok") {
        setStatus(result.data);
        if (enabled) await runSync(true);
      } else {
        toast.error(
          status?.folder ? result.error : t("settings.sync.toasts.needFolder"),
        );
      }
    } finally {
      setBusy(false);
    }
  };

  const setSyncHistory = async (value: boolean) => {
    const result = await commands.syncSetSyncHistory(value);
    if (result.status === "ok") setStatus(result.data);
  };

  const exportBundle = async () => {
    const target = await save({
      title: t("settings.sync.bundle.exportTitle"),
      defaultPath: `flowstate-bundle-${new Date().toISOString().slice(0, 10)}.zip`,
      filters: [{ name: "FlowState", extensions: ["zip"] }],
    });
    if (!target) return;
    setBusy(true);
    try {
      const result = await commands.syncExportBundle(
        target,
        status?.sync_history ?? false,
      );
      if (result.status === "ok") {
        toast.success(t("settings.sync.toasts.exported"));
      } else {
        toast.error(result.error);
      }
    } finally {
      setBusy(false);
    }
  };

  const importBundle = async () => {
    const picked = await open({
      multiple: false,
      filters: [{ name: "FlowState", extensions: ["zip"] }],
    });
    if (!picked) return;
    setBusy(true);
    try {
      const result = await commands.syncImportBundle(picked as string);
      if (result.status === "ok") {
        toast.success(
          t("settings.sync.toasts.imported", {
            words: result.data.words_added,
            snippets: result.data.snippets_added,
            history: result.data.history_imported,
          }),
        );
        if (result.data.conflicts.length > 0) {
          toast.warning(
            t("settings.sync.toasts.importedConflicts", {
              count: result.data.conflicts.length,
            }),
          );
        }
        await refresh();
      } else {
        toast.error(result.error);
      }
    } finally {
      setBusy(false);
    }
  };

  if (!status) {
    return (
      <div className="animate-pulse space-y-3">
        <div className="h-4 bg-line rounded w-1/3"></div>
        <div className="h-8 bg-wash rounded"></div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <SettingsGroup title={t("settings.sync.title")}>
        <SettingContainer
          title={t("settings.sync.enabled.title")}
          description={t("settings.sync.enabled.description")}
          descriptionMode="inline"
          layout="horizontal"
        >
          <Switch
            checked={status.enabled}
            onChange={setEnabled}
            disabled={busy}
          />
        </SettingContainer>

        <div className="px-4 py-3 text-xs text-ink-3 space-y-1">
          <p>{t("settings.sync.explain.offline")}</p>
          <p>{t("settings.sync.explain.off")}</p>
        </div>
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.sync.folder.title")}
        description={t("settings.sync.folder.description")}
      >
        <SettingContainer
          title={t("settings.sync.folder.current")}
          description=""
          descriptionMode="inline"
          layout="stacked"
        >
          <div className="flex items-center gap-2">
            {status.folder ? (
              <PathDisplay path={status.folder} onOpen={() => commands.syncOpenFolder()} />
            ) : (
              <span className="text-sm text-ink-3">
                {t("settings.sync.folder.none")}
              </span>
            )}
            <Button
              variant="secondary"
              size="sm"
              onClick={chooseFolder}
              disabled={busy}
            >
              <FolderOpen className="w-4 h-4 mr-1.5" />
              {t("settings.sync.folder.choose")}
            </Button>
          </div>
        </SettingContainer>

        <SettingContainer
          title={t("settings.sync.history.title")}
          description={t("settings.sync.history.description")}
          descriptionMode="inline"
          layout="horizontal"
        >
          <Switch
            checked={status.sync_history}
            onChange={setSyncHistory}
            disabled={busy}
          />
        </SettingContainer>

        <SettingContainer
          title={t("settings.sync.now.title")}
          description={
            status.last_sync_at
              ? t("settings.sync.now.last", {
                  time: formatTime(status.last_sync_at),
                })
              : t("settings.sync.now.never")
          }
          descriptionMode="inline"
          layout="horizontal"
        >
          <div className="flex items-center gap-2">
            <Button
              variant="primary"
              size="sm"
              onClick={() => runSync(true)}
              disabled={!status.enabled || syncing}
            >
              <RefreshCw
                className={`w-4 h-4 mr-1.5 ${syncing ? "animate-spin" : ""}`}
              />
              {t("settings.sync.now.button")}
            </Button>
            <span className="text-xs text-ink-3">
              {t("settings.sync.counts", {
                words: status.word_count,
                snippets: status.snippet_count,
              })}
            </span>
          </div>
        </SettingContainer>

        {status.last_error && (
          <div className="px-4 py-2 text-xs text-danger flex items-center gap-1.5">
            <CloudOff className="w-3.5 h-3.5" />
            {status.last_error}
          </div>
        )}
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.sync.bundle.title")}
        description={t("settings.sync.bundle.description")}
      >
        <SettingContainer
          title={t("settings.sync.bundle.actions")}
          description=""
          descriptionMode="inline"
          layout="horizontal"
        >
          <div className="flex items-center gap-2">
            <Button
              variant="secondary"
              size="sm"
              onClick={exportBundle}
              disabled={busy}
            >
              <Package className="w-4 h-4 mr-1.5" />
              {t("settings.sync.bundle.export")}
            </Button>
            <Button
              variant="secondary"
              size="sm"
              onClick={importBundle}
              disabled={busy}
            >
              <Package className="w-4 h-4 mr-1.5" />
              {t("settings.sync.bundle.import")}
            </Button>
          </div>
        </SettingContainer>
        <div className="px-4 py-3 text-xs text-ink-3">
          {t("settings.sync.bundle.sealed")}
        </div>
      </SettingsGroup>

      <SettingsGroup
        title={t("settings.sync.conflicts.title")}
        description={t("settings.sync.conflicts.description")}
      >
        {conflicts.length === 0 ? (
          <div className="px-4 py-3 text-sm text-ink-3">
            {t("settings.sync.conflicts.none")}
          </div>
        ) : (
          <div className="px-4 py-3 space-y-2">
            {conflicts.map((conflict, index) => (
              <div
                key={`${conflict.at}-${index}`}
                className="text-xs space-y-0.5"
              >
                <div className="flex items-center gap-1.5 font-medium">
                  <ShieldAlert className="w-3.5 h-3.5 text-warning shrink-0" />
                  <span>
                    {conflict.kind === "word"
                      ? t("settings.sync.conflicts.word", {
                          key: conflict.key,
                        })
                      : t("settings.sync.conflicts.snippet", {
                          key: conflict.key,
                        })}
                  </span>
                </div>
                <div className="text-ink-3 pl-5">
                  {t("settings.sync.conflicts.device", {
                    device: conflict.remote_device,
                  })}{" "}
                  ·{" "}
                  {formatTime(conflict.at)} ·{" "}
                  {conflict.resolution}
                </div>
              </div>
            ))}
          </div>
        )}
      </SettingsGroup>
    </div>
  );
};
