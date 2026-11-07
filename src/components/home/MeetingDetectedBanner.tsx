import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { listen } from "@tauri-apps/api/event";
import { Mic, X } from "lucide-react";

import { commands } from "@/bindings";

/**
 * A prompt, not a takeover: when another app has held the microphone long
 * enough to look like a meeting, offer to record it. One prompt per busy
 * spell comes from the backend; dismissing here only hides this one.
 */
export const MeetingDetectedBanner: React.FC = () => {
  const { t } = useTranslation();
  const [seen, setSeen] = useState(false);
  const [hidden, setHidden] = useState(false);

  useEffect(() => {
    let alive = true;
    let unlisten: (() => void) | undefined;
    listen("meeting-detected", () => {
      setSeen(true);
      setHidden(false);
    }).then((un) => {
      if (alive) unlisten = un;
      else un();
    });
    return () => {
      alive = false;
      unlisten?.();
    };
  }, []);

  if (!seen || hidden) return null;

  return (
    <div className="mb-[16px] flex items-center gap-[10px] rounded-[10px] border border-line bg-surface-soft px-[14px] py-[10px]">
      <span className="flex h-[26px] w-[26px] shrink-0 items-center justify-center rounded-[7px] bg-background-ui text-on-accent">
        <Mic size={14} />
      </span>
      <div className="min-w-0 flex-1">
        <p className="text-[12.5px] font-[600] text-text">
          {t("meeting.detected.title")}
        </p>
        <p className="text-[11.5px] text-ink-2">{t("meeting.detected.body")}</p>
      </div>
      <button
        type="button"
        onClick={() => {
          commands.startMeeting().catch(() => {});
          setHidden(true);
        }}
        className="h-[26px] shrink-0 cursor-pointer rounded-[7px] border border-logo-primary/40 bg-background-ui px-[10px] text-[11.5px] font-[600] text-on-accent"
      >
        {t("meeting.detected.start")}
      </button>
      <button
        type="button"
        onClick={() => setHidden(true)}
        aria-label={t("meeting.detected.dismiss")}
        className="flex h-[22px] w-[22px] shrink-0 cursor-pointer items-center justify-center rounded-[5px] text-ink-3 hover:bg-wash hover:text-text"
      >
        <X size={13} />
      </button>
    </div>
  );
};
