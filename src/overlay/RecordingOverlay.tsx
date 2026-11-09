import { emit, listen } from "@tauri-apps/api/event";
import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import "./RecordingOverlay.css";
import { commands, events } from "@/bindings";
import type {
  ActiveStyleInfo,
  StreamPhase,
  StreamPhaseEvent,
  StreamTextEvent,
  StreamWorkKind,
} from "@/bindings";
import { useMeetingClock } from "@/hooks/useMeetingClock";
import i18n, { syncLanguageFromSettings } from "@/i18n";
import type { ModelStateEvent } from "@/lib/types/events";
import { formatDurationClock } from "@/lib/utils/format";
import { getLanguageDirection } from "@/lib/utils/rtl";

type OverlayState =
  | "recording"
  | "streaming"
  | "transcribing"
  | "processing"
  | "command";

// The meeting bar's views — the Flow-style floating bar that shares this
// window with the dictation overlay. "recording" and "idle" follow the
// backend's meeting state; "wrapping" is frontend-only: the short "Saving…"
// pill shown while `stop_meeting` is in flight, until the history entry is
// in place.
type MeetingSession = "idle" | "recording";

// `show-overlay` carries the dictation states and the meeting bar's two
// backend-driven views.
const isDictationState = (state: string): state is OverlayState =>
  state === "recording" ||
  state === "streaming" ||
  state === "transcribing" ||
  state === "processing" ||
  state === "command";

// Number of reactive bars in the waveform (the simple, smoothed style shared by
// every overlay form). Mic levels arrive as 16 FFT buckets; we take the first N.
const WAVE_BARS = 9;

// A pointer must travel this far before a press on the meeting bar becomes a
// drag; anything less stays a click (start / stop the meeting).
const BAR_DRAG_THRESHOLD_PX = 6;

// Only call out a model load in the Live preview once it has run this long.
// Warm loads finish in well under this (~0.2s on Apple Silicon, ~1.5s on a
// Windows CPU backend), so the common case never flashes a loading notice while
// the user is speaking; cold loads can take 40s+.
const SLOW_MODEL_LOAD_MS = 2000;

// ---- The long-meeting prompt (S-24) ----
// How long the prompt stays up on its own: it dismisses itself so the bar
// never carries a stale question (and never blocks the app underneath for
// longer than a courtesy glance).
const MEETING_NUDGE_AUTO_DISMISS_MS = 30_000;

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // The tone/style the dictation will be cleaned up in (per-app rule or the
  // globally selected prompt), plus the little quick-switch menu.
  const [styleInfo, setStyleInfo] = useState<ActiveStyleInfo | null>(null);
  const [styleMenuPos, setStyleMenuPos] = useState<{
    x: number;
    y: number;
  } | null>(null);
  // `Stream::play()` returning does not mean hardware callbacks are flowing.
  // Stay visually in an arming state until the backend processes the first
  // actual microphone sample chunk.
  const [captureReady, setCaptureReady] = useState(false);
  // Command Mode: briefly shown when the command hotkey found no selection and
  // the take continues as normal dictation.
  const [commandHint, setCommandHint] = useState(false);
  // Recording starts while the model loads in the background. Say so, otherwise
  // a cold start looks like an empty Live preview that ignores speech, then a
  // hung "Transcribing..." spinner. Once recording stops, any in-flight load is
  // the wait, so the working label reflects it immediately; the Live notice
  // waits for the load to be slow so fast loads don't flash it mid-speech.
  const [modelLoading, setModelLoading] = useState(false);
  const [modelLoadSlow, setModelLoadSlow] = useState(false);
  const modelLoadTimerRef = useRef<ReturnType<typeof setTimeout>>();
  // Latched per Live session: once the loading notice has opened the panel, keep
  // it open after the load so it doesn't collapse and reopen when text arrives.
  const [loadNoticeShown, setLoadNoticeShown] = useState(false);
  const [levels, setLevels] = useState<number[]>(Array(WAVE_BARS).fill(0));
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  // Bumped on each new streaming session so the Live card remounts fresh (replays
  // the pop-in, and never animates in from the previous panel's open size).
  const [session, setSession] = useState(0);
  // Overlay placement (which edge of the screen). The Live panel grows downward
  // from a top overlay (oldest line under the pill) and upward from a bottom one;
  // left/right anchor the card to that edge and center it vertically.
  const [position, setPosition] = useState<"top" | "bottom" | "left" | "right">(
    "bottom",
  );
  // The meeting bar's own dock edge — the reference statusDockEdge: pinned by a drag
  // once one has happened, following the dictation overlay's edge until then.
  const [barPosition, setBarPosition] = useState<
    "top" | "bottom" | "left" | "right"
  >("bottom");
  // True once live text overflows the cap. A top overlay fades its top edge only
  // while overflowing, so the resting first line stays crisp flush under the pill.
  const [overflowing, setOverflowing] = useState(false);

  // ---- Meeting bar (the Flow-style floating bar) ----
  // The backend owns the session; this mirrors the meeting state it emits so
  // every window follows one session. `null` until the first status arrives.
  const [meetingSession, setMeetingSession] = useState<MeetingSession | null>(
    null,
  );
  // Set while a stop started from this bar is in flight: the "Saving…" pill
  // until the history entry is in place, then the idle disc again.
  const [meetingWrapping, setMeetingWrapping] = useState(false);
  const [meetingStartedAt, setMeetingStartedAt] = useState(0);
  // After this many minutes of recording, the discreet "still going?" prompt
  // appears beside the pill (0/off = never). Read from the settings; the
  // backend default is 60.
  const [reminderMinutes, setReminderMinutes] = useState(0);
  // Whether the prompt is on screen right now, plus the session (by its start
  // time) that has already had it: once per meeting, reference-style.
  const [nudgeOpen, setNudgeOpen] = useState(false);
  // The idle disc opens into its action row while the pointer is over it.
  const [discOpen, setDiscOpen] = useState(false);
  const [barError, setBarError] = useState<string | null>(null);
  const discTimerRef = useRef<number | null>(null);
  // Shrinking the window puts the pointer back over the disc; without a
  // cooldown it would immediately re-expand and flicker.
  const discShutAtRef = useRef(0);
  const nudgeDoneForRef = useRef(0);
  const nudgeOpenRef = useRef(false);
  const nudgeTimerRef = useRef<number | undefined>(undefined);

  const smoothedLevelsRef = useRef<number[]>(Array(16).fill(0));
  // Live-text scroll-back: the text region "sticks" to the newest line while the
  // user is at the bottom; if they scroll up to read history, auto-follow pauses
  // until they scroll back down.
  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const direction = getLanguageDirection(i18n.language);

  // The recording pill's clock, from the session's start time. Ticks only
  // while the pill is the active view.
  const meetingRecording = meetingSession === "recording" && !meetingWrapping;
  const meetingElapsed = useMeetingClock(
    meetingRecording ? meetingStartedAt : 0,
    meetingRecording,
  );

  // ---- The long-meeting prompt (S-24) ----
  // After `reminderMinutes` of recording (60 by default), a discreet prompt
  // appears beside the pill — "Meeting still going?" with a Stop button — so
  // a forgotten recording is caught while it can still be saved cleanly. It
  // asks the backend for the bar's wider footprint (the resize never focuses
  // the window, so the app underneath keeps the keyboard), dismisses itself
  // after 30 s, and shows at most once per session. It is never eligible
  // while a dictation card owns the window.
  const reminderSeconds = reminderMinutes * 60;
  const nudgeEligible =
    meetingRecording &&
    !isVisible &&
    reminderSeconds > 0 &&
    meetingElapsed >= reminderSeconds &&
    nudgeDoneForRef.current !== meetingStartedAt;

  useEffect(() => {
    if (nudgeEligible && !nudgeOpen) {
      setNudgeOpen(true);
      nudgeOpenRef.current = true;
      commands
        .setMeetingBarNudge(true)
        .catch((err) => console.warn("Failed to widen the meeting bar:", err));
      // Auto-dismiss: the prompt never lingers on the bar.
      clearTimeout(nudgeTimerRef.current);
      nudgeTimerRef.current = window.setTimeout(() => {
        nudgeDoneForRef.current = meetingStartedAt;
        setNudgeOpen(false);
        nudgeOpenRef.current = false;
        commands
          .setMeetingBarNudge(false)
          .catch((err) =>
            console.warn("Failed to shrink the meeting bar:", err),
          );
      }, MEETING_NUDGE_AUTO_DISMISS_MS);
    } else if (!nudgeEligible && nudgeOpen) {
      // The meeting ended, a dictation card took the window, or the reminder
      // was turned off: never leave the wider footprint behind.
      clearTimeout(nudgeTimerRef.current);
      setNudgeOpen(false);
      nudgeOpenRef.current = false;
      commands
        .setMeetingBarNudge(false)
        .catch((err) => console.warn("Failed to shrink the meeting bar:", err));
    }
  }, [nudgeEligible, nudgeOpen, meetingStartedAt]);

  useEffect(() => {
    // Re-read the reminder threshold from the settings: the overlay window
    // outlives the Settings window, so the threshold picked after it loaded
    // must be able to reach the bar (on load, and whenever a session starts).
    const refreshReminderMinutes = () => {
      commands
        .getAppSettings()
        .then((result) => {
          if (result.status !== "ok") return;
          const minutes = result.data.meeting_reminder_minutes;
          setReminderMinutes(
            typeof minutes === "number" && minutes > 0 ? minutes : 0,
          );
        })
        .catch((err) =>
          console.warn("Failed to read the reminder setting:", err),
        );
    };

    const setupEventListeners = async () => {
      const unlistenShow = await listen("show-overlay", async (event) => {
        const overlayState = event.payload as string;
        // A meeting-bar view owns the window: stop rendering the dictation
        // card (the bar — which follows the meeting state — takes over). A
        // dictation state arriving later re-shows the card as before.
        if (!isDictationState(overlayState)) {
          setIsVisible(false);
          // The bar's own states say which session it shows. Trusting only
          // the meeting-state event left the window blank whenever that event
          // arrived before this webview had its listener up.
          if (
            overlayState === "meeting-recording" ||
            overlayState === "meeting-recording-nudge"
          ) {
            setMeetingSession("recording");
          } else if (overlayState === "meeting-idle") {
            setMeetingSession("idle");
          }
          // A re-show of the recording pill re-applies the plain pill's
          // footprint; if the "still going?" prompt is up, ask for its wider
          // one again so it is never clipped.
          if (nudgeOpenRef.current) {
            commands
              .setMeetingBarNudge(true)
              .catch((err) =>
                console.warn("Failed to widen the meeting bar:", err),
              );
          }
          return;
        }
        // Reset synchronously before settings I/O. A fast microphone can emit
        // recording-ready while the awaits below are in flight; resetting after
        // them would overwrite that event and leave the overlay stuck arming.
        if (overlayState === "recording" || overlayState === "streaming") {
          setCaptureReady(false);
          smoothedLevelsRef.current = Array(16).fill(0);
          setLevels(Array(WAVE_BARS).fill(0));
          setStreamText({ committed: "", tentative: "" });
        }

        await syncLanguageFromSettings();
        // The Live panel flows downward from a top overlay and upward from a
        // bottom one; read the placement so the layout can flip to match.
        try {
          const settings = await commands.getAppSettings();
          if (settings.status === "ok") {
            setPosition(settings.data.overlay_position ?? "bottom");
          }
        } catch {
          // Keep the previous/default placement if settings can't be read.
        }
        setState(overlayState);
        if (overlayState === "streaming") {
          setPhase("listening");
          setWorkKind("transcribing");
          setElapsed(0);
          setLoadNoticeShown(false);
          setSession((s) => s + 1); // remount the card fresh for this session
        }
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        setIsVisible(false);
        setCaptureReady(false);
        // A pending hover timer must not re-open the row while a dictation
        // view owns the window.
        if (discTimerRef.current) window.clearTimeout(discTimerRef.current);
        setDiscOpen(false);
      });

      const unlistenReady = await listen("recording-ready", () => {
        setElapsed(0);
        setCaptureReady(true);
      });

      // The bead's menu starts a meeting without opening the app window; the
      // click path below is the only other way in, so both call the command.
      const unlistenStartMeeting = await listen("overlay-start-meeting", () => {
        commands
          .startMeeting()
          .catch((err) => console.warn("Failed to start the meeting:", err));
      });

      // Command Mode fell back to plain dictation (no text was selected):
      // a discreet hint, auto-dismissed so it never lingers.
      const unlistenCommandFallback = await listen(
        "command-mode-fallback",
        () => {
          setCommandHint(true);
          window.setTimeout(() => setCommandHint(false), 5000);
        },
      );

      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const newLevels = event.payload as number[];
        // Exponential smoothing across the 16 buckets, then take the first N
        // bars for the shared waveform.
        const smoothed = smoothedLevelsRef.current.map((prev, i) => {
          const target = newLevels[i] || 0;
          return prev * 0.7 + target * 0.3;
        });
        smoothedLevelsRef.current = smoothed;
        setLevels(smoothed.slice(0, WAVE_BARS));
      });

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      // The meeting bar follows the session wherever it changes — a click on
      // this bar, the Settings window, anywhere: the pill while a meeting
      // records, the idle disc once it stops or is cancelled.
      const unlistenMeeting = await events.meetingStateEvent.listen(
        ({ payload }) => {
          const status = payload.status;
          if (status.state === "recording") {
            setMeetingStartedAt(status.started_at);
            setMeetingSession("recording");
            refreshReminderMinutes();
          } else {
            // The stop's history entry is in place by the time Idle arrives;
            // the "Saving…" pill ends with it.
            setMeetingWrapping(false);
            setMeetingSession("idle");
          }
        },
      );

      // The idle sliver opens when the pointer comes near it and closes when
      // it leaves. The backend owns that decision — the sliver is 8 points
      // wide, so the pointer never reliably entered it — and this only mirrors
      // it into the view.
      const unlistenHover = await listen<boolean>("meeting-bar-hover", (event) => {
        if (discTimerRef.current) window.clearTimeout(discTimerRef.current);
        setDiscOpen(Boolean(event.payload));
      });

      // The window moves when the placement setting changes; the bar is a
      // persistent view, so re-read the edge to re-anchor without waiting for
      // the next state change.
      const unlistenPosition = await listen(
        "overlay-position-changed",
        async () => {
          try {
            const settings = await commands.getAppSettings();
            if (settings.status === "ok") {
              setPosition(settings.data.overlay_position ?? "bottom");
              setBarPosition(
                settings.data.meeting_bar_position ??
                  settings.data.overlay_position ??
                  "bottom",
              );
            }
          } catch {
            // Keep the previous placement if settings can't be read.
          }
        },
      );

      // The backend shows the window before this webview has finished loading,
      // so the state events above may have been missed. Ask once, after the
      // listeners are in place, and paint the bar for the current session.
      try {
        const [status, settings] = await Promise.all([
          commands.getMeetingStatus(),
          commands.getAppSettings(),
        ]);
        if (settings.status === "ok") {
          setPosition(settings.data.overlay_position ?? "bottom");
          setBarPosition(
            settings.data.meeting_bar_position ??
              settings.data.overlay_position ??
              "bottom",
          );
        }
        if (settings.status === "ok") {
          const minutes = settings.data.meeting_reminder_minutes;
          setReminderMinutes(
            typeof minutes === "number" && minutes > 0 ? minutes : 0,
          );
        }
        if (status.state === "recording") {
          setMeetingStartedAt(status.started_at);
          setMeetingSession("recording");
        } else {
          setMeetingSession("idle");
        }
      } catch (err) {
        // Without a status the bar still starts meetings from the disc.
        console.warn("Failed to read the meeting status:", err);
        setMeetingSession("idle");
      }

      // The backend ends every `loading_started` with exactly one of completed
      // or failed, so only those end a load. Other events (e.g. `unloaded` from
      // the idle watcher) aren't ordered against an in-flight load and must not
      // clear it.
      const unlistenModel = await listen<ModelStateEvent>(
        "model-state-changed",
        (event) => {
          const type = event.payload.event_type;
          if (type === "loading_started") {
            clearTimeout(modelLoadTimerRef.current);
            setModelLoading(true);
            setModelLoadSlow(false);
            modelLoadTimerRef.current = setTimeout(
              () => setModelLoadSlow(true),
              SLOW_MODEL_LOAD_MS,
            );
          } else if (
            type === "loading_completed" ||
            type === "loading_failed"
          ) {
            clearTimeout(modelLoadTimerRef.current);
            setModelLoading(false);
            setModelLoadSlow(false);
          }
        },
      );

      return () => {
        unlistenShow();
        unlistenHide();
        unlistenReady();
        unlistenStartMeeting();
        unlistenCommandFallback();
        unlistenLevel();
        unlistenStream();
        unlistenPhase();
        unlistenMeeting();
        unlistenHover();
        unlistenPosition();
        unlistenModel();
        clearTimeout(modelLoadTimerRef.current);
      };
    };

    setupEventListeners();
  }, []);

  // Elapsed capture timer starts only once microphone samples are flowing.
  useEffect(() => {
    if (state !== "streaming" || !isVisible || !captureReady) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible, captureReady]);

  // Stick to the bottom as text streams in — but only while pinned, so a user who
  // has scrolled up to read history isn't yanked back down by the next chunk.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    // Fade the top edge only once text actually overflows the cap.
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  // `session` re-runs this when a new Live session starts mid-load (e.g. a
  // retry during a cold load), since the reset doesn't change the other deps.
  useEffect(() => {
    if (modelLoadSlow && state === "streaming") setLoadNoticeShown(true);
  }, [modelLoadSlow, state, session]);

  // Each fresh streaming session starts pinned to the bottom, fade cleared.
  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  // ---- Meeting bar handlers ----
  // The bar can be dragged to another screen edge (disc and pill alike). A
  // press arms a drag; crossing the threshold hands the gesture to the native
  // window drag — the window follows the pointer across the whole screen,
  // which its own exact-fit bounds could never do — and the backend docks the
  // bar to the nearest edge once it settles, saving the choice. A drag never
  // starts or stops the meeting: the click handlers bail out once the gesture
  // became a drag, and the native drag swallows the mouse-up that would have
  // made it a click anyway.
  const dragOriginRef = useRef<{ x: number; y: number } | null>(null);
  const dragStartedRef = useRef(false);

  const handleBarPointerDown = (event: React.PointerEvent) => {
    // The primary button only — a right-click is the disc's context menu, and
    // the Stop button keeps its own click.
    if (event.button !== 0) return;
    if ((event.target as Element).closest(".mstop")) return;
    dragOriginRef.current = { x: event.clientX, y: event.clientY };
    // A leftover flag from a previous gesture must not eat the next click.
    dragStartedRef.current = false;
  };

  const handleBarPointerMove = (event: React.PointerEvent) => {
    const origin = dragOriginRef.current;
    if (!origin || dragStartedRef.current) return;
    const dx = event.clientX - origin.x;
    const dy = event.clientY - origin.y;
    if (dx * dx + dy * dy < BAR_DRAG_THRESHOLD_PX * BAR_DRAG_THRESHOLD_PX) {
      return;
    }
    dragStartedRef.current = true;
    commands
      .startMeetingBarDrag()
      .catch((err) => console.warn("Failed to drag the meeting bar:", err));
  };

  const handleBarPointerUp = () => {
    dragOriginRef.current = null;
  };

  // Right-click on the disc: the backend pops the native menu ("Hide for 1
  // hour") at the cursor. Native, because the disc's window is exactly the
  // disc — there is no room for HTML around it.
  const openDiscMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    commands
      .openMeetingBarMenu()
      .catch((err) => console.warn("Failed to open the bar menu:", err));
  };

  // A click on the disc starts the meeting; the state event turns the bar
  // into the pill. A failed start (e.g. a dictation holds the microphone)
  // leaves the disc as it was. A gesture that became a drag is not a click.
  const startMeeting = async () => {
    if (dragStartedRef.current) {
      dragStartedRef.current = false;
      return;
    }
    try {
      await commands.startMeeting();
    } catch (err) {
      // A dead sliver is the worst outcome: say what failed, and give the
      // microphone case its one useful action.
      const message = String(err);
      console.error("Failed to start the meeting:", message);
      setBarError(
        /permission|denied|microphone|input/i.test(message)
          ? t("overlay.meeting.micBlocked")
          : t("overlay.meeting.startFailed"),
      );
      setDiscOpen(false);
      void commands.setMeetingBarExpanded(true).catch(() => {});
      window.setTimeout(() => {
        setBarError(null);
        void commands.setMeetingBarExpanded(false).catch(() => {});
      }, 8000);
    }
  };

  // reference-style stop: save immediately, no confirmation dialog — the
  // recording lands in History. The pill shows "Saving…" until the stop
  // resolves (the history entry is in place by then) and the idle event
  // returns the bar to the disc.
  const stopMeeting = async () => {
    setMeetingWrapping(true);
    try {
      await commands.stopMeeting();
      // The Idle event usually arrived during the await; this is the net.
      setMeetingWrapping(false);
      setMeetingSession("idle");
    } catch (err) {
      console.warn("Failed to stop the meeting:", err);
      setMeetingWrapping(false);
    }
  };
  // Stop straight from the long-meeting prompt: the same save as the pill's
  // Stop, with the prompt's once-per-session bookkeeping done and the bar
  // back to the plain pill footprint before the wrap pill takes it.
  const stopMeetingFromNudge = () => {
    nudgeDoneForRef.current = meetingStartedAt;
    clearTimeout(nudgeTimerRef.current);
    setNudgeOpen(false);
    nudgeOpenRef.current = false;
    commands
      .setMeetingBarNudge(false)
      .catch((err) => console.warn("Failed to shrink the meeting bar:", err));
    void stopMeeting();
  };

  // ---- View routing ----
  // A dictation view always wins the window while it shows; the meeting bar
  // fills it otherwise (the bar is the window's resting state).
  if (!isVisible) {
    if (meetingSession === null) return null;

    // Saving…: the pill keeps its footprint while the stop is in flight.
    if (meetingWrapping) {
      return (
        <div dir={direction} className={`ov-stage ${barPosition}`}>
          <div
            className="mcard"
            onPointerDown={handleBarPointerDown}
            onPointerMove={handleBarPointerMove}
            onPointerUp={handleBarPointerUp}
            onPointerCancel={handleBarPointerUp}
          >
            <span className="sspinner" />
            <span className="mlabel">{t("overlay.meeting.saving")}</span>
          </div>
        </div>
      );
    }

    // Recording: red pulsing dot + running clock (mm:ss) + Stop, with the
    // long-meeting prompt beside the pill once the session has run past the
    // reminder threshold. The row carries the drag gesture so the pill stays
    // draggable while the prompt is up; the prompt's own Stop is a plain
    // click (the drag handler ignores presses on `.mstop`).
    if (meetingSession === "recording") {
      return (
        <div dir={direction} className={`ov-stage ${barPosition}`}>
          <div
            className="mrow"
            onPointerDown={handleBarPointerDown}
            onPointerMove={handleBarPointerMove}
            onPointerUp={handleBarPointerUp}
            onPointerCancel={handleBarPointerUp}
          >
            <div
              className="mcard"
              role="status"
              aria-label={t("overlay.meeting.recording")}
            >
              <span className="mdot" aria-hidden="true" />
              <span className="mtimer">
                {formatDurationClock(meetingElapsed)}
              </span>
              <button type="button" className="mstop" onClick={stopMeeting}>
                {t("overlay.meeting.stop")}
              </button>
            </div>
            {nudgeOpen && (
              <div className="mnu" role="status">
                <span className="mnu-text">
                  {t("overlay.meeting.stillGoing")}
                </span>
                <button
                  type="button"
                  className="mstop"
                  onClick={stopMeetingFromNudge}
                >
                  {t("overlay.meeting.stop")}
                </button>
              </div>
            )}
          </div>
        </div>
      );
    }

    // Idle: a disc with a mic. Hovering it grows the window into the action
    // row (start a meeting, open Notes, open the Scratchpad); the window is an
    // exact fit for whatever it shows, so the backend resizes with us.
    const discActions = (
      <>
        <button
          type="button"
          className="mact primary"
          aria-label={t("overlay.meeting.start")}
          title={t("overlay.meeting.start")}
          onClick={() => {
            setDiscOpen(false);
            commands.setMeetingBarExpanded(false);
            startMeeting();
          }}
        >
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <rect x="9" y="2.5" width="6" height="11" rx="3" />
            <path
              d="M5.5 11.5a6.5 6.5 0 0 0 13 0"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.7"
              strokeLinecap="round"
            />
            <path
              d="M12 18v3.5"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.7"
              strokeLinecap="round"
            />
          </svg>
        </button>
        <button
          type="button"
          className="mact"
          aria-label={t("overlay.quickNote")}
          title={t("overlay.quickNote")}
          onClick={() => {
            setDiscOpen(false);
            commands.setMeetingBarExpanded(false);
            // The main window owns which section is showing; ask it for Notes.
            emit("navigate-section", "notes").catch(() => {});
            commands.showMainWindowCommand().catch(() => {});
          }}
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.8"
            strokeLinecap="round"
            aria-hidden="true"
          >
            <rect x="4.5" y="3.5" width="15" height="17" rx="3" />
            <path d="M8.5 9h7M8.5 13h7M8.5 17h4" />
          </svg>
        </button>
        <button
          type="button"
          className="mact"
          aria-label={t("tray.scratchpad_open")}
          title={t("tray.scratchpad_open")}
          onClick={() => {
            setDiscOpen(false);
            commands.setMeetingBarExpanded(false);
            commands.openScratchpad().catch(() => {});
          }}
        >
          <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
            <path d="M13.2 2.5 4.8 13.4h5.1l-1.1 8.1 8.4-10.9h-5.1z" />
          </svg>
        </button>
      </>
    );

    return (
      <div
        dir={direction}
        className={`ov-stage ${barPosition}`}
        onPointerEnter={() => {
          // Re-entry cancels whatever is pending: a close in its grace
          // period, or a stale open timer.
          if (discTimerRef.current) window.clearTimeout(discTimerRef.current);
          if (discOpen) return;
          if (Date.now() - discShutAtRef.current < 400) return;
          discTimerRef.current = window.setTimeout(() => {
            discTimerRef.current = null;
            setDiscOpen(true);
            commands.setMeetingBarExpanded(true).catch(() => {});
          }, 140);
        }}
        onPointerLeave={() => {
          if (discTimerRef.current) window.clearTimeout(discTimerRef.current);
          if (!discOpen) return;
          // Close on a delay: leaving and coming straight back keeps the row
          // open, because re-entry cancels this timer.
          discTimerRef.current = window.setTimeout(() => {
            discTimerRef.current = null;
            discShutAtRef.current = Date.now();
            setDiscOpen(false);
            commands.setMeetingBarExpanded(false).catch(() => {});
          }, 200);
        }}
      >
        {discOpen ? (
          <div className="midle">{discActions}</div>
        ) : barError ? (
          <button
            type="button"
            className="merr"
            role="alert"
            onClick={() => {
              void commands.openMicrophonePrivacySettings().catch(() => {});
              setBarError(null);
              void commands.setMeetingBarExpanded(false).catch(() => {});
            }}
          >
            <span className="merr-line">{barError}</span>
            <span className="merr-action">{t("overlay.meeting.openSettings")}</span>
          </button>
        ) : (
          <button
            type="button"
            className="mdisc"
            aria-label={t("overlay.meeting.start")}
            title={t("overlay.meeting.start")}
            onClick={startMeeting}
            onContextMenu={openDiscMenu}
            onPointerDown={handleBarPointerDown}
            onPointerMove={handleBarPointerMove}
            onPointerUp={handleBarPointerUp}
            onPointerCancel={handleBarPointerUp}
          >
            <span className="mwave" aria-hidden="true">
              {Array.from({ length: 10 }, (_, index) => (
                <i key={index} />
              ))}
            </span>
          </button>
        )}
      </div>
    );
  }

  // Re-pin when the user is within ~a line of the bottom; unpin otherwise.
  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  const transcribingLabel = modelLoading
    ? t("overlay.loadingModel")
    : t("overlay.transcribing");

  // ---- Style (tone) chip + local quick-switch menu ----
  // The backend resolves the active style from the focused app; the overlay
  // only shows it (and lets the user switch it) — templates live in local
  // settings, nothing leaves the machine.
  useEffect(() => {
    if (!isVisible) return;
    let cancelled = false;
    commands
      .getActiveStyle()
      .then((info) => {
        if (!cancelled) setStyleInfo(info);
      })
      .catch((err) => console.warn("Failed to read the active style:", err));
    return () => {
      cancelled = true;
    };
  }, [isVisible, state]);

  const pickStyle = async (styleId: string) => {
    setStyleMenuPos(null);
    try {
      // When a per-app rule matched the focused app, retarget that rule;
      // otherwise switch the globally selected style.
      if (styleInfo?.matchedRuleKey) {
        const result = await commands.setAppStyleRule(
          styleInfo.matchedRuleKey,
          styleId,
        );
        if (result.status === "error") throw new Error(result.error);
      } else {
        const result = await commands.setPostProcessSelectedPrompt(styleId);
        if (result.status === "error") throw new Error(result.error);
      }
      const info = await commands.getActiveStyle();
      setStyleInfo(info);
    } catch (err) {
      console.warn("Failed to switch the dictation style:", err);
    }
  };

  const openStyleMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    setStyleMenuPos({
      x: Math.max(4, Math.min(event.clientX, window.innerWidth - 190)),
      y: Math.max(4, Math.min(event.clientY, window.innerHeight - 40)),
    });
  };

  const styleMenu =
    styleInfo && styleMenuPos ? (
      <>
        <div
          className="sstyle-backdrop"
          onPointerDown={() => setStyleMenuPos(null)}
        />
        <div
          className="sstyle-menu"
          style={{ left: styleMenuPos.x, top: styleMenuPos.y }}
          role="menu"
        >
          <div className="sstyle-menu-title">
            {t("overlay.style.menuTitle")}
          </div>
          {styleInfo.styles.map((style) => (
            <button
              key={style.id}
              type="button"
              role="menuitemradio"
              aria-checked={style.id === styleInfo.styleId}
              className={`sstyle-item ${
                style.id === styleInfo.styleId ? "active" : ""
              }`}
              onClick={() => pickStyle(style.id)}
            >
              {style.name}
            </button>
          ))}
        </div>
      </>
    ) : null;

  // ---- Shared building blocks (one visual language for every overlay form) ----
  const waveform = (
    <div className={`swave ${captureReady ? "ready" : "arming"}`}>
      {levels.map((v, i) => (
        <i
          key={i}
          style={{
            height: `${Math.max(3, Math.min(18, 3 + Math.pow(v, 0.7) * 15))}px`,
          }}
        />
      ))}
    </div>
  );

  const cancelBtn = (
    <button
      className="sx"
      aria-label="cancel"
      onClick={() => commands.cancelOperation()}
    >
      <svg viewBox="0 0 16 16" aria-hidden="true">
        <path
          d="M4 4 L12 12 M12 4 L4 12"
          stroke="currentColor"
          strokeWidth="1.6"
          strokeLinecap="round"
        />
      </svg>
    </button>
  );

  // dot (left) | waveform (center) | timer + cancel (right) — same structure for
  // pill & panel, so the Live morph is a pure width change.
  const listeningRow = (showTimer: boolean, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className={`sdot ${captureReady ? "ready" : "arming"}`} />
        {styleInfo?.postProcessEnabled && styleInfo.styleName ? (
          <span className="sstyle-chip" title={t("overlay.style.chipTitle")}>
            {styleInfo.styleName}
          </span>
        ) : null}
      </div>
      {waveform}
      {(state === "command" || commandHint) && (
        <span className="swork-label scmd-label">
          {state === "command"
            ? t("overlay.command")
            : t("overlay.commandFallback")}
        </span>
      )}
      <div className="sbase-r">
        {showTimer && <span className="stimer">{fmtTime(elapsed)}</span>}
        {showCancel && cancelBtn}
      </div>
    </div>
  );

  // spinner (left) | label (center) | cancel (right) — same 3-zone grid as the
  // listening row, so the label is centered.
  const workingRow = (label: string, showCancel: boolean) => (
    <div className="sbase">
      <div className="sbase-l">
        <span className="sspinner" />
      </div>
      <span className="swork-label">{label}</span>
      <div className="sbase-r">{showCancel && cancelBtn}</div>
    </div>
  );

  // ---- Live overlay: a pill that sculpts open into a panel ----
  if (state === "streaming") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const working = phase === "working";
    // While listening, a slow model load opens the panel with a notice where the
    // text would be, so speech that isn't transcribing yet doesn't look ignored.
    const loadingNotice = modelLoadSlow && !working && !hasText;
    // Keep the panel open whenever there's text — even while finalizing — so the
    // transcript stays put under a working spinner instead of collapsing and
    // squishing the text mid-stream. Only fall back to the small working pill
    // when there was no text to preserve.
    const open = hasText || loadingNotice || (loadNoticeShown && !working);
    const collapsed = working && !hasText;

    return (
      <div
        dir={direction}
        className={`ov-stage ${position}`}
        onContextMenu={openStyleMenu}
      >
        {styleMenu}
        <div
          key={session}
          className={`scard ${open ? "open" : ""} ${collapsed ? "working" : ""} ${
            isVisible ? "" : "leaving"
          }`}
        >
          <div className="stext">
            <div className="stext-clip">
              <div
                className={`stext-cap ${overflowing ? "overflowing" : ""}`}
                ref={capRef}
                onScroll={handleStreamScroll}
              >
                <p>
                  {loadingNotice && (
                    <span className="sloading">
                      {t("overlay.loadingModel")}
                    </span>
                  )}
                  <span className="committed">
                    {streamText.committed ? streamText.committed + " " : ""}
                  </span>
                  <span className="tentative">{streamText.tentative}</span>
                  {/* Drop the blinking caret once finalizing — it's no longer
                      capturing, and a static spinner conveys the work. */}
                  {!working && !loadingNotice && <span className="scaret" />}
                </p>
              </div>
            </div>
          </div>
          {working
            ? workingRow(
                workKind === "polishing"
                  ? t("overlay.processing")
                  : transcribingLabel,
                true,
              )
            : listeningRow(open, true)}
        </div>
      </div>
    );
  }

  // ---- Minimal overlay: exactly one row at a time — waveform (recording), or a
  // spinner + label (transcribing / processing). Never both. The pill animates its
  // width between them; the cancel button is in both rows so it stays put.
  const working = state === "transcribing" || state === "processing";
  const workLabel =
    state === "processing" ? t("overlay.processing") : transcribingLabel;

  return (
    <div
      dir={direction}
      className={`ov-stage ${position} ov-fade ${isVisible ? "show" : ""}`}
      onContextMenu={openStyleMenu}
    >
      {styleMenu}
      <div
        className={`scard compact ${working && isVisible ? "cworking" : ""}`}
      >
        {working ? workingRow(workLabel, true) : listeningRow(false, true)}
      </div>
    </div>
  );
};

export default RecordingOverlay;
