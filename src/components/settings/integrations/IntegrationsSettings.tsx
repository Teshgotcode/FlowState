import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { Copy } from "lucide-react";
import { commands } from "@/bindings";
import { Button } from "../../ui/Button";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { copyToClipboard } from "../history/clipboard";

/** The block the copy button hands over, fixed text around the live binary
 * path: it is pasted into an agent that runs the commands as written, so
 * only the panel around it is translated. */
const cliTextBlock = (cliPath: string) =>
  `Jeg bruker FlowState lokalt. Du kan lese møtene mine med disse kommandoene:
  ${cliPath} meetings --limit 20 --json
  ${cliPath} meeting <id> --json
  ${cliPath} transcript <id> --json
  ${cliPath} search "<ord>" --json
Bruk dem når jeg spør om møtene mine. Alt er lokalt.`;

export const IntegrationsSettings: React.FC = () => {
  const { t } = useTranslation();
  const [cliPath, setCliPath] = useState("");

  useEffect(() => {
    commands
      .getCliInfo()
      .then((result) => {
        if (result.status === "ok" && result.data) setCliPath(result.data);
      })
      .catch(console.error);
  }, []);

  // No path yet (or a failed invoke) still shows a usable block.
  const cliText = cliTextBlock(cliPath || "flowstate");

  const handleCopy = async () => {
    if (await copyToClipboard(cliText)) {
      toast.success(t("settings.integrations.cli.copied"));
    }
  };

  return (
    <div className="max-w-3xl w-full mx-auto space-y-6">
      <SettingsGroup title={t("settings.integrations.cli.title")}>
        <div className="p-4 space-y-3">
          <pre className="overflow-x-auto rounded-md bg-wash p-3 text-xs leading-relaxed text-text select-text">
            {cliText}
          </pre>
          <Button variant="secondary" size="sm" onClick={handleCopy}>
            <Copy width={14} height={14} />
            {t("settings.integrations.cli.copy")}
          </Button>
        </div>
      </SettingsGroup>
    </div>
  );
};
