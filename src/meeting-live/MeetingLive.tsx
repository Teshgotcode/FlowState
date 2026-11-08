import React, { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { commands } from "@/bindings";
import type { MeetingSegment, MeetingStateEvent } from "@/bindings";
import "./MeetingLive.css";

/**
 * The panel that follows a meeting while it records: what is being said,
 * segment by segment, from the live decode the recording already runs. It
 * opens and closes with the session; the finished entry owns the transcript.
 */
const DEMO_SEGMENTS: MeetingSegment[] = [
  { index: 0, t0_ms: 1200, t1_ms: 5200, text: "Vi må bli enige om budsjettet før fredag." },
  {
    index: 1,
    t0_ms: 5400,
    t1_ms: 9800,
    text: "Jeg tar markedsføringen, du tar forretningsmodellen.",
  },
  { index: 2, t0_ms: 10100, t1_ms: 13600, text: "Da snakkes vi på mandag." },
];

const MeetingLive: React.FC<{ demo?: boolean }> = ({ demo = false }) => {
  const { t, i18n } = useTranslation();
  const [segments, setSegments] = useState<MeetingSegment[]>(
    demo ? DEMO_SEGMENTS : [],
  );
  const [startedAt, setStartedAt] = useState<number | null>(() =>
    demo ? Math.floor(Date.now() / 1000) - 96 : null,
  );
  const [now, setNow] = useState(() => Date.now());
  const listRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);

  useEffect(() => {
    if (demo) return;
    const unlisteners: Array<() => void> = [];
    let alive = true;

    const track = (un: () => void) => {
      if (alive) unlisteners.push(un);
      else un();
    };

    listen<MeetingSegment>("meeting-segment", (event) => {
      const segment = event.payload;
      setSegments((current) =>
        current.some((item) => item.index === segment.index)
          ? current
          : [...current, segment].sort((a, b) => a.index - b.index),
      );
    }).then(track);

    listen<MeetingStateEvent>("meeting-state", (event) => {
      const status = event.payload.status;
      if (status.state === "recording") {
        setStartedAt(status.started_at);
      } else {
        setSegments([]);
        setStartedAt(null);
        void getCurrentWindow().close();
      }
    }).then(track);

    commands.getMeetingStatus().then((status) => {
      if (status.state === "recording") setStartedAt(status.started_at);
    });

    return () => {
      alive = false;
      unlisteners.forEach((un) => un());
    };
  }, [demo]);

  // The clock under the title.
  useEffect(() => {
    if (startedAt == null) return;
    const id = window.setInterval(() => setNow(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [startedAt]);

  // Follow the newest line unless the reader scrolled up.
  useEffect(() => {
    const el = listRef.current;
    if (el && pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [segments]);

  const elapsed = startedAt == null ? 0 : Math.max(0, Math.floor(now / 1000) - startedAt);
  const clock = `${String(Math.floor(elapsed / 60)).padStart(2, "0")}:${String(
    elapsed % 60,
  ).padStart(2, "0")}`;

  const stamp = (ms: number) => {
    const total = Math.floor(ms / 1000);
    return `${String(Math.floor(total / 60)).padStart(2, "0")}:${String(
      total % 60,
    ).padStart(2, "0")}`;
  };

  return (
    <div className="ml-root">
      <header className="ml-head" data-tauri-drag-region>
        <span className="ml-dot" aria-hidden="true" />
        <span className="ml-title" data-tauri-drag-region>
          {t("meetingLive.title")}
        </span>
        <span className="ml-clock">{clock}</span>
        <button
          type="button"
          className="ml-stop"
          onClick={() => void commands.stopMeeting().catch(() => {})}
        >
          {t("meetingLive.stop")}
        </button>
      </header>

      <div
        ref={listRef}
        className="ml-list"
        onScroll={() => {
          const el = listRef.current;
          if (!el) return;
          pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 24;
        }}
      >
        {segments.length === 0 ? (
          <p className="ml-empty">{t("meetingLive.listening")}</p>
        ) : (
          segments.map((segment) => (
            <div className="ml-seg" key={segment.index}>
              <span className="ml-time">{stamp(segment.t0_ms)}</span>
              <p className="ml-text" lang={i18n.language}>
                {segment.text}
              </p>
            </div>
          ))
        )}
      </div>

      <footer className="ml-foot">{t("meetingLive.footnote")}</footer>
    </div>
  );
};

export default MeetingLive;
