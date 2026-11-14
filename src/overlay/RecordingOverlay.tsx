import { emit, listen } from "@tauri-apps/api/event";
import { type as currentOsType } from "@tauri-apps/plugin-os";
import React, {
  useCallback,
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
} from "react";
import { Trans, useTranslation } from "react-i18next";
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
import { MIC_SILENCE_WARN_MS, micHasSound } from "@/lib/utils/micSilence";
import { getLanguageDirection } from "@/lib/utils/rtl";
import { barLanguageOptions } from "@/lib/utils/barLanguages";
import { languageName } from "@/lib/constants/languages";
import { suggestedLanguages } from "@/lib/utils/languageSuggestions";
import { keycapLabel, type OSType } from "@/lib/utils/keyboard";
import {
  RECLICK_CANCEL_MS,
  WAVE_BAR_COUNT,
  audioScale,
  barDelayS,
  barHeightScale,
  isSideEdge,
  levelFromBuckets,
  sizeFor,
  smoothLevel,
  type BarEdge,
  type PillState,
} from "./pillGeometry";

// The Flow Bar: one black pill that rests at the screen edge, opens into the
// "ready" pill on hover, and morphs through the dictation states, plus the
// meeting pill while a meeting records. Sizes, colours and timings follow
// Wispr Flow's bar (see pillGeometry.ts and RecordingOverlay.css).

type OverlayState =
  | "recording"
  | "streaming"
  | "transcribing"
  | "processing"
  | "command"
  | "error";

type MeetingSession = "idle" | "recording";

// `show-overlay` carries the dictation states and the bar's own views.
const isDictationState = (state: string): state is OverlayState =>
  state === "recording" ||
  state === "streaming" ||
  state === "transcribing" ||
  state === "processing" ||
  state === "command" ||
  state === "error";

const detectOs = (): OSType => {
  try {
    const os = currentOsType();
    return os === "macos" || os === "windows" || os === "linux"
      ? os
      : "unknown";
  } catch {
    return "unknown";
  }
};
const OS: OSType = detectOs();

// macOS runs the bar in one fixed, click-through window (overlay.rs): the
// webview reports its hit rects, the backend drives hover, and the pill sits
// 14 px from the edge inside the window. Elsewhere the window is an exact fit
// and the webview handles hover itself.
const FIXED_FRAME = OS === "macos";

// A pointer must travel this far before a press on the bar becomes a drag;
// anything less stays a click.
const BAR_DRAG_THRESHOLD_PX = 6;

// Only call out a model load in the Live card once it has run this long, so
// warm loads never flash a notice while the user is speaking.
const SLOW_MODEL_LOAD_MS = 2000;

// How long the "nothing selected" command hint stays up.
const COMMAND_HINT_MS = 5000;

const isListeningState = (state: OverlayState) =>
  state === "recording" || state === "streaming" || state === "command";

// ---- The long-meeting prompt (S-24) ----
const MEETING_NUDGE_AUTO_DISMISS_MS = 30_000;

/** Hover open/close delays for the exact-fit windows (no backend poll). */
const HOVER_OPEN_MS = 140;
const HOVER_CLOSE_MS = 200;

const shortLanguageCode = (code: string) =>
  code === "auto" ? "A" : code.split(/[-_]/)[0].toUpperCase();

const shortcutLabel = (binding: string | undefined) => {
  if (!binding) return "fn";
  const label = keycapLabel(binding, OS);
  return label === "Fn" ? "fn" : label;
};

// ---- Building blocks ----

const Waveform = React.forwardRef<
  HTMLDivElement,
  { animated: boolean; live: boolean; vertical: boolean; bars?: number }
>(({ animated, live, vertical, bars = WAVE_BAR_COUNT }, ref) => (
  <div
    ref={ref}
    className={`pwave ${live ? "live" : ""} ${vertical ? "vertical" : ""} ${
      bars !== WAVE_BAR_COUNT ? "meeting" : ""
    }`}
    aria-hidden="true"
  >
    {Array.from({ length: bars }, (_, i) => (
      <i
        key={i}
        className={animated ? "wave-anim" : undefined}
        style={
          {
            "--bar-height-scale": barHeightScale(i, bars),
            animationDelay: `${barDelayS(i, bars)}s`,
          } as React.CSSProperties
        }
      />
    ))}
  </div>
));
Waveform.displayName = "Waveform";

const Spinner: React.FC = () => (
  <span className="pspin" aria-hidden="true">
    {Array.from({ length: 8 }, (_, i) => (
      <i
        key={i}
        style={{
          transform: `rotate(${45 * i}deg) translate(0, -6px)`,
          animationDelay: `${-(7 - i) * 0.1375}s`,
        }}
      />
    ))}
  </span>
);

const MicIcon: React.FC = () => (
  <svg
    viewBox="0 0 24 24"
    width="16"
    height="16"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <rect x="9" y="2.5" width="6" height="12" rx="3" />
    <path d="M5.5 11a6.5 6.5 0 0 0 13 0M12 17.5V21" />
  </svg>
);

const NotesIcon: React.FC = () => (
  <svg
    viewBox="0 0 24 24"
    fill="none"
    stroke="currentColor"
    strokeWidth="2"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <rect x="5" y="3.5" width="14" height="17" rx="2.5" />
    <path d="M9 9h6M9 13h6M9 17h3" />
  </svg>
);

const BoltIcon: React.FC = () => (
  <svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d="M13.2 2.5 4.8 13.4h5.1l-1.1 8.1 8.4-10.9h-5.1z" />
  </svg>
);

const Tooltip: React.FC<{ children: React.ReactNode; gap?: number }> = ({
  children,
  gap = 4,
}) => (
  <span
    className="btip"
    role="tooltip"
    style={{ "--tip-gap": `${gap}px` } as React.CSSProperties}
  >
    {children}
  </span>
);

interface BarPrefs {
  spokenLanguages: string[];
  selectedLanguage: string;
  scratchpad: boolean;
  styleChip: boolean;
  shortcut: string;
}

const RecordingOverlay: React.FC = () => {
  const { t } = useTranslation();
  const [isVisible, setIsVisible] = useState(false);
  const [state, setState] = useState<OverlayState>("recording");
  // Which failure the error pill is showing; the backend sends a code, never
  // localized copy, so the app language decides the wording.
  const [errorCode, setErrorCode] = useState<string>("start_failed");
  // A failed take rolls the overlay back to idle right after it reports, and
  // that hide would erase the message before it is read.
  const stateRef = useRef(state);
  stateRef.current = state;
  const errorUntilRef = useRef(0);
  const [styleInfo, setStyleInfo] = useState<ActiveStyleInfo | null>(null);
  // `Stream::play()` returning does not mean hardware callbacks are flowing:
  // the pill stays "initializing" until the first microphone samples.
  const [captureReady, setCaptureReady] = useState(false);
  // Hold-to-talk vs. hands-free (`overlay-mode` from the coordinator).
  const [handsFree, setHandsFree] = useState(false);
  // This take is a Command Mode take: orange from listening through
  // processing, until the overlay hides.
  const [commandTake, setCommandTake] = useState(false);
  // Command Mode found no selection; the take continues as dictation.
  const [commandHint, setCommandHint] = useState(false);
  // The mic has delivered only silence since this take started.
  const [micSilent, setMicSilent] = useState(false);
  const heardSoundRef = useRef(false);
  const silenceTimerRef = useRef<number | undefined>(undefined);
  const [modelLoading, setModelLoading] = useState(false);
  const [modelLoadSlow, setModelLoadSlow] = useState(false);
  const modelLoadTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const [loadNoticeShown, setLoadNoticeShown] = useState(false);
  const [streamText, setStreamText] = useState<StreamTextEvent>({
    committed: "",
    tentative: "",
  });
  const [phase, setPhase] = useState<StreamPhase>("listening");
  const [workKind, setWorkKind] = useState<StreamWorkKind>("transcribing");
  const [elapsed, setElapsed] = useState(0);
  const [session, setSession] = useState(0);
  const [overlayStyle, setOverlayStyle] = useState<string>("minimal");
  // Dictation edge, and the bar's own edge (pinned by a drag, else the
  // dictation edge).
  const [position, setPosition] = useState<BarEdge>("bottom");
  const [barPosition, setBarPosition] = useState<BarEdge>("bottom");
  const [overflowing, setOverflowing] = useState(false);

  // ---- The bar at rest and the meeting pill ----
  const [meetingSession, setMeetingSession] = useState<MeetingSession | null>(
    null,
  );
  const [meetingWrapping, setMeetingWrapping] = useState(false);
  const [meetingStartedAt, setMeetingStartedAt] = useState(0);
  const [reminderMinutes, setReminderMinutes] = useState(0);
  const [nudgeOpen, setNudgeOpen] = useState(false);
  // The resting bar is hovered: it opens into the "ready" pill.
  const [hovered, setHovered] = useState(false);
  const hoverTimerRef = useRef<number | null>(null);
  // Hidden at rest; the hide's undo card shows while this is set.
  const [barHidden, setBarHidden] = useState(false);
  const [barError, setBarError] = useState<string | null>(null);
  const [langMenuOpen, setLangMenuOpen] = useState(false);
  // Which element's tooltip is showing (pointer over it).
  const [tipFor, setTipFor] = useState<string | null>(null);
  const [prefs, setPrefs] = useState<BarPrefs>({
    spokenLanguages: [],
    selectedLanguage: "auto",
    scratchpad: false,
    styleChip: false,
    shortcut: "fn",
  });
  const nudgeDoneForRef = useRef(0);
  const nudgeOpenRef = useRef(false);
  const nudgeTimerRef = useRef<number | undefined>(undefined);
  // When a click on the bar started the running dictation (0 = it didn't).
  const barStartedAtRef = useRef(0);

  // Waveform level: the latest bucket max, smoothed per animation frame and
  // written to the waveform as `--audio-scale` (no React render per frame).
  const levelTargetRef = useRef(0);
  const levelRef = useRef(0);
  const waveRef = useRef<HTMLDivElement>(null);

  const capRef = useRef<HTMLDivElement>(null);
  const pinnedRef = useRef(true);
  const stageRef = useRef<HTMLDivElement>(null);
  const lastHitRectsRef = useRef("");
  const direction = getLanguageDirection(i18n.language);

  const meetingRecording = meetingSession === "recording" && !meetingWrapping;
  const meetingElapsed = useMeetingClock(
    meetingRecording ? meetingStartedAt : 0,
    meetingRecording,
  );

  // ---- The long-meeting prompt (S-24) ----
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
      clearTimeout(nudgeTimerRef.current);
      setNudgeOpen(false);
      nudgeOpenRef.current = false;
      commands
        .setMeetingBarNudge(false)
        .catch((err) => console.warn("Failed to shrink the meeting bar:", err));
    }
  }, [nudgeEligible, nudgeOpen, meetingStartedAt]);

  // The bar's own settings: languages for the language circle, the optional
  // Scratchpad circle and style chip, and the shortcut the tooltips name.
  // Re-read on mount, on hover and on each show: the overlay outlives the
  // Settings window.
  const refreshPrefs = useCallback(() => {
    commands
      .getAppSettings()
      .then((result) => {
        if (result.status !== "ok") return;
        const s = result.data;
        setPrefs({
          spokenLanguages: s.spoken_languages ?? [],
          selectedLanguage: s.selected_language ?? "auto",
          scratchpad: Boolean(s.scratchpad_in_flow_bar),
          styleChip: Boolean(s.flow_bar_style_chip),
          shortcut: shortcutLabel(s.bindings?.transcribe?.current_binding),
        });
        setOverlayStyle(s.overlay_style ?? "minimal");
        const minutes = s.meeting_reminder_minutes;
        setReminderMinutes(
          typeof minutes === "number" && minutes > 0 ? minutes : 0,
        );
      })
      .catch((err) => console.warn("Failed to read the bar settings:", err));
  }, []);

  useEffect(() => {
    const setupEventListeners = async () => {
      // Warn about a silent mic during the take. The watch arms at dictation
      // start; the first non-silent level event clears it.
      const armMicSilenceWatch = () => {
        window.clearTimeout(silenceTimerRef.current);
        heardSoundRef.current = false;
        setMicSilent(false);
        silenceTimerRef.current = window.setTimeout(() => {
          if (!heardSoundRef.current && isListeningState(stateRef.current)) {
            setMicSilent(true);
          }
        }, MIC_SILENCE_WARN_MS);
      };
      const clearMicSilenceWatch = () => {
        window.clearTimeout(silenceTimerRef.current);
        setMicSilent(false);
      };

      const unlistenShow = await listen<string>(
        "show-overlay",
        async (event) => {
          let overlayState = event.payload;
          // The bar's own views. A dictation state arriving later re-shows
          // the dictation pill.
          if (
            !isDictationState(overlayState) &&
            overlayState !== "recording-handsfree"
          ) {
            setIsVisible(false);
            setLangMenuOpen(false);
            setBarHidden(overlayState === "bar-hidden");
            if (
              overlayState === "meeting-recording" ||
              overlayState === "meeting-recording-nudge"
            ) {
              setMeetingSession("recording");
            } else if (
              overlayState === "meeting-idle" ||
              overlayState === "bar-resting" ||
              overlayState === "bar-hidden"
            ) {
              setMeetingSession("idle");
            }
            if (nudgeOpenRef.current) {
              commands
                .setMeetingBarNudge(true)
                .catch((err) =>
                  console.warn("Failed to widen the meeting bar:", err),
                );
            }
            return;
          }
          // `recording-handsfree` is the recording pill already known to be
          // hands-free (the preview, or a backend that says so up front).
          const handsFreeShow = overlayState === "recording-handsfree";
          if (handsFreeShow) overlayState = "recording";
          // Reset synchronously before settings I/O: a fast microphone can
          // emit recording-ready (and the coordinator its mode) while the
          // awaits below are in flight.
          if (
            overlayState === "recording" ||
            overlayState === "streaming" ||
            overlayState === "command"
          ) {
            setCaptureReady(false);
            setHandsFree(handsFreeShow);
            setCommandTake(overlayState === "command");
            levelTargetRef.current = 0;
            levelRef.current = 0;
            setStreamText({ committed: "", tentative: "" });
            armMicSilenceWatch();
          }
          setHovered(false);
          setLangMenuOpen(false);
          setBarHidden(false);
          stateRef.current = overlayState as OverlayState;
          setState(overlayState as OverlayState);
          if (overlayState === "streaming") {
            setPhase("listening");
            setWorkKind("transcribing");
            setElapsed(0);
            setLoadNoticeShown(false);
            setSession((s) => s + 1);
          }
          setIsVisible(true);

          await syncLanguageFromSettings();
          try {
            const settings = await commands.getAppSettings();
            if (settings.status === "ok") {
              setPosition(settings.data.overlay_position ?? "bottom");
              setOverlayStyle(settings.data.overlay_style ?? "minimal");
              setPrefs((prev) => ({
                ...prev,
                styleChip: Boolean(settings.data.flow_bar_style_chip),
                shortcut: shortcutLabel(
                  settings.data.bindings?.transcribe?.current_binding,
                ),
              }));
            }
          } catch {
            // Keep the previous placement if settings can't be read.
          }
        },
      );

      const unlistenError = await listen<string>("overlay-error", (event) => {
        stateRef.current = "error";
        errorUntilRef.current = Date.now() + 4000;
        setErrorCode(event.payload);
        setState("error");
        setIsVisible(true);
      });

      const unlistenHide = await listen("hide-overlay", () => {
        if (
          stateRef.current === "error" &&
          Date.now() < errorUntilRef.current
        ) {
          return;
        }
        setIsVisible(false);
        setCaptureReady(false);
        setHandsFree(false);
        setCommandTake(false);
        barStartedAtRef.current = 0;
        clearMicSilenceWatch();
        if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
        setHovered(false);
      });

      const unlistenReady = await listen("recording-ready", () => {
        setElapsed(0);
        setCaptureReady(true);
      });

      const unlistenMode = await listen<string>("overlay-mode", (event) => {
        setHandsFree(event.payload === "handsfree");
      });

      // The bar's menu starts a meeting without opening the app window.
      const unlistenStartMeeting = await listen("overlay-start-meeting", () => {
        commands
          .startMeeting()
          .catch((err) => console.warn("Failed to start the meeting:", err));
      });

      const unlistenCommandFallback = await listen(
        "command-mode-fallback",
        () => {
          setCommandHint(true);
          window.setTimeout(() => setCommandHint(false), COMMAND_HINT_MS);
        },
      );

      const unlistenLevel = await listen<number[]>("mic-level", (event) => {
        const buckets = event.payload ?? [];
        if (!heardSoundRef.current && micHasSound(buckets)) {
          heardSoundRef.current = true;
          setMicSilent(false);
        }
        levelTargetRef.current = levelFromBuckets(buckets);
      });

      const unlistenStream = await events.streamTextEvent.listen((event) => {
        setStreamText(event.payload);
      });

      const unlistenPhase = await events.streamPhaseEvent.listen((event) => {
        const payload: StreamPhaseEvent = event.payload;
        setPhase(payload.phase);
        if (payload.kind) setWorkKind(payload.kind);
      });

      const unlistenMeeting = await events.meetingStateEvent.listen(
        ({ payload }) => {
          const status = payload.status;
          if (status.state === "recording") {
            setMeetingStartedAt(status.started_at);
            setMeetingSession("recording");
            refreshPrefs();
          } else {
            setMeetingWrapping(false);
            setMeetingSession("idle");
          }
        },
      );

      // The resting bar opens when the pointer comes near it. On macOS the
      // backend owns that decision (its cursor poll measures the distance to
      // the pill); this only mirrors it into the view.
      const unlistenHover = await listen<boolean>(
        "meeting-bar-hover",
        (event) => {
          if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
          const open = Boolean(event.payload);
          setHovered(open);
          if (open) refreshPrefs();
          else {
            setLangMenuOpen(false);
            setTipFor(null);
          }
        },
      );

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

      // The backend shows the window before this webview has finished
      // loading, so the state events above may have been missed. Ask once.
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
        refreshPrefs();
        if (status?.state === "recording") {
          setMeetingStartedAt(status.started_at);
          setMeetingSession("recording");
        } else {
          setMeetingSession("idle");
        }
      } catch (err) {
        console.warn("Failed to read the meeting status:", err);
        setMeetingSession("idle");
      }

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

      return [
        unlistenShow,
        unlistenHide,
        unlistenError,
        unlistenReady,
        unlistenMode,
        unlistenStartMeeting,
        unlistenCommandFallback,
        unlistenLevel,
        unlistenStream,
        unlistenPhase,
        unlistenMeeting,
        unlistenHover,
        unlistenPosition,
        unlistenModel,
      ];
    };

    // The setup is async, so it can outlive this effect (StrictMode mounts
    // twice before any listen resolves); the flag hands the just-resolved
    // listeners straight to unlisten in that case instead of leaking them.
    let cancelled = false;
    let unlisteners: Array<() => void> = [];
    setupEventListeners().then(
      (fns) => {
        if (cancelled) {
          for (const un of fns) un();
        } else {
          unlisteners = fns;
        }
      },
      (err) => console.warn("Failed to set up the overlay listeners:", err),
    );

    return () => {
      cancelled = true;
      for (const un of unlisteners) un();
      clearTimeout(modelLoadTimerRef.current);
      clearTimeout(silenceTimerRef.current);
    };
  }, [refreshPrefs]);

  // Elapsed capture timer (Live card) starts once samples are flowing.
  useEffect(() => {
    if (state !== "streaming" || !isVisible || !captureReady) return;
    const id = setInterval(() => setElapsed((e) => e + 1), 1000);
    return () => clearInterval(id);
  }, [state, isVisible, captureReady]);

  // Stick to the newest line as text streams in, while pinned.
  useLayoutEffect(() => {
    const el = capRef.current;
    if (!el) return;
    setOverflowing(el.scrollHeight > el.clientHeight + 1);
    if (pinnedRef.current) el.scrollTop = el.scrollHeight;
  }, [streamText]);

  useEffect(() => {
    if (modelLoadSlow && state === "streaming") setLoadNoticeShown(true);
  }, [modelLoadSlow, state, session]);

  useEffect(() => {
    pinnedRef.current = true;
    setOverflowing(false);
  }, [session]);

  // The active dictation style, for the optional style chip.
  useEffect(() => {
    if (!isVisible || !prefs.styleChip) return;
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
  }, [isVisible, state, prefs.styleChip]);

  // ---- Which pill shows ----
  const working =
    isVisible &&
    (state === "transcribing" ||
      state === "processing" ||
      (state === "streaming" && phase === "working"));
  const listening = isVisible && !working && isListeningState(state);
  let pillState: PillState;
  if (isVisible) {
    if (state === "error") pillState = "error";
    else if (working) pillState = "processing";
    else if (!captureReady) pillState = "initializing";
    else pillState = handsFree ? "listening_handsfree" : "listening_ptt";
  } else if (barHidden) {
    pillState = "hidden";
  } else {
    pillState = hovered ? "ready" : "resting";
  }
  const commandStyle = isVisible && (commandTake || state === "command");
  const liveCapture = listening && captureReady;

  // Smooth the level per animation frame while the mic is live.
  useEffect(() => {
    if (!liveCapture) {
      waveRef.current?.style.setProperty("--audio-scale", "1");
      return;
    }
    let frame = 0;
    const tick = () => {
      levelRef.current = smoothLevel(levelRef.current, levelTargetRef.current);
      waveRef.current?.style.setProperty(
        "--audio-scale",
        audioScale(levelRef.current).toFixed(3),
      );
      frame = window.requestAnimationFrame(tick);
    };
    frame = window.requestAnimationFrame(tick);
    return () => window.cancelAnimationFrame(frame);
  }, [liveCapture, pillState]);

  // ---- Hit rects (macOS fixed window) ----
  // Every element marked `data-hit` takes clicks; the rest of the window is
  // click-through. Reported after each render and once more after the 100 ms
  // morph settles; unchanged rects are not re-sent.
  const reportHitRects = useCallback(() => {
    if (!FIXED_FRAME) return;
    const stage = stageRef.current;
    const rects = stage
      ? Array.from(stage.querySelectorAll<HTMLElement>("[data-hit]"))
          .map((el) => el.getBoundingClientRect())
          .filter((r) => r.width > 0 && r.height > 0)
          .map((r) => ({
            x: Math.round(r.left * 10) / 10,
            y: Math.round(r.top * 10) / 10,
            width: Math.round(r.width * 10) / 10,
            height: Math.round(r.height * 10) / 10,
          }))
      : [];
    const key = JSON.stringify(rects);
    if (key === lastHitRectsRef.current) return;
    lastHitRectsRef.current = key;
    commands.setBarHitRects(rects).catch(() => {});
  }, []);

  useLayoutEffect(() => {
    if (!FIXED_FRAME) return;
    const frame = window.requestAnimationFrame(reportHitRects);
    const settle = window.setTimeout(reportHitRects, 140);
    return () => {
      window.cancelAnimationFrame(frame);
      window.clearTimeout(settle);
    };
  });

  // ---- Bar handlers ----
  const dragOriginRef = useRef<{ x: number; y: number } | null>(null);
  const dragStartedRef = useRef(false);

  const handleBarPointerDown = (event: React.PointerEvent) => {
    if (event.button !== 0) return;
    if ((event.target as Element).closest("button")) return;
    dragOriginRef.current = { x: event.clientX, y: event.clientY };
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
      .catch((err) => console.warn("Failed to drag the bar:", err));
  };

  const handleBarPointerUp = () => {
    dragOriginRef.current = null;
  };

  const consumeDrag = () => {
    if (!dragStartedRef.current) return false;
    dragStartedRef.current = false;
    return true;
  };

  // Right-click: the native menu, only on the resting/ready bar.
  const showBarMenu = () => {
    if (isVisible || barHidden || meetingSession === "recording") return;
    const codes = Array.from(
      new Set([...prefs.spokenLanguages, prefs.selectedLanguage]),
    ).filter((code) => code && code !== "auto");
    const labels: [string, string][] = codes.map((code) => [
      code,
      languageName(code, i18n.language),
    ]);
    setLangMenuOpen(false);
    commands
      .openMeetingBarMenu(labels)
      .catch((err) => console.warn("Failed to open the bar menu:", err));
  };
  const onContextMenu = (event: React.MouseEvent) => {
    event.preventDefault();
    event.stopPropagation();
    showBarMenu();
  };

  // A click on the pill: ready → hands-free dictation; hold-to-talk → finish;
  // a second click within half a second of a bar start → cancel.
  const onPillClick = () => {
    if (consumeDrag()) return;
    const now = Date.now();
    if (
      isVisible &&
      barStartedAtRef.current &&
      now - barStartedAtRef.current < RECLICK_CANCEL_MS
    ) {
      barStartedAtRef.current = 0;
      commands.cancelOperation().catch(() => {});
      return;
    }
    if (pillState === "ready" || pillState === "resting") {
      barStartedAtRef.current = now;
      setHovered(false);
      setLangMenuOpen(false);
      commands.toggleDictationFromBar().catch(() => {});
      return;
    }
    if (pillState === "listening_ptt" || pillState === "initializing") {
      commands.toggleDictationFromBar().catch(() => {});
    }
  };

  const onPillKeyDown = (event: React.KeyboardEvent) => {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onPillClick();
    }
  };

  const startMeeting = async () => {
    setHovered(false);
    try {
      await commands.startMeeting();
    } catch (err) {
      const message = String(err);
      console.error("Failed to start the meeting:", message);
      setBarError(
        /permission|denied|microphone|input/i.test(message)
          ? t("overlay.meeting.micBlocked")
          : t("overlay.meeting.startFailed"),
      );
      if (!FIXED_FRAME) {
        void commands.setMeetingBarExpanded(true).catch(() => {});
      }
      window.setTimeout(() => {
        setBarError(null);
        if (!FIXED_FRAME) {
          void commands.setMeetingBarExpanded(false).catch(() => {});
        }
      }, 8000);
    }
  };

  // Stop saves immediately; the pill shows a spinner until the stop resolves.
  const stopMeeting = async () => {
    setMeetingWrapping(true);
    try {
      await commands.stopMeeting();
      setMeetingWrapping(false);
      setMeetingSession("idle");
    } catch (err) {
      console.warn("Failed to stop the meeting:", err);
      setMeetingWrapping(false);
      setBarError(t("settings.meeting.stopFailed"));
      window.setTimeout(() => setBarError(null), 8000);
    }
  };
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

  const openMainSection = (section: string) => {
    emit("navigate-section", section).catch(() => {});
    commands.showMainWindowCommand().catch(() => {});
  };

  const pickLanguage = async (code: string) => {
    setLangMenuOpen(false);
    setPrefs((prev) => ({ ...prev, selectedLanguage: code }));
    try {
      const result = await commands.changeSelectedLanguageSetting(code);
      if (result.status === "error") throw new Error(result.error);
    } catch (err) {
      console.warn("Failed to switch the dictation language:", err);
    }
  };

  // Exact-fit windows (Windows/Linux): the webview drives hover itself.
  const onStagePointerEnter = () => {
    if (FIXED_FRAME || isVisible || barHidden || meetingSession !== "idle") {
      return;
    }
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    if (hovered) return;
    hoverTimerRef.current = window.setTimeout(() => {
      hoverTimerRef.current = null;
      setHovered(true);
      refreshPrefs();
      commands.setMeetingBarExpanded(true).catch(() => {});
    }, HOVER_OPEN_MS);
  };
  const onStagePointerLeave = () => {
    if (FIXED_FRAME) return;
    if (hoverTimerRef.current) window.clearTimeout(hoverTimerRef.current);
    if (!hovered) return;
    hoverTimerRef.current = window.setTimeout(() => {
      hoverTimerRef.current = null;
      setHovered(false);
      setLangMenuOpen(false);
      commands.setMeetingBarExpanded(false).catch(() => {});
    }, HOVER_CLOSE_MS);
  };

  const handleStreamScroll = () => {
    const el = capRef.current;
    if (!el) return;
    pinnedRef.current = el.scrollHeight - el.scrollTop - el.clientHeight <= 16;
  };

  const fmtTime = (s: number) =>
    `${Math.floor(s / 60)}:${String(s % 60).padStart(2, "0")}`;

  const tipProps = (id: string) => ({
    onPointerEnter: () => setTipFor(id),
    onPointerLeave: () => setTipFor((cur) => (cur === id ? null : cur)),
  });

  // ---- Render ----
  if (!isVisible && meetingSession === null) return null;

  const edge: BarEdge = isVisible ? position : barPosition;
  const vertical = isSideEdge(edge);
  const stageClass = `ov-stage ${edge} ${FIXED_FRAME ? "fixed" : ""}`;
  const keyLabel = <b className="key" />;

  const cards: React.ReactNode[] = [];
  const card = (
    key: string,
    title: React.ReactNode,
    options: {
      body?: React.ReactNode;
      action?: { label: string; onClick: () => void };
      role?: "alert" | "status";
    } = {},
  ) => (
    <div className="ncard" key={key} role={options.role ?? "status"} data-hit>
      <div className="ncard-text">
        <span className="ncard-title">{title}</span>
        {options.body && <span className="ncard-body">{options.body}</span>}
      </div>
      {options.action && (
        <button
          type="button"
          className="ncard-action"
          onClick={options.action.onClick}
        >
          {options.action.label}
        </button>
      )}
    </div>
  );

  // ---- The meeting pill ----
  if (!isVisible && meetingSession === "recording") {
    if (barError) {
      cards.push(card("bar-error", barError, { role: "alert" }));
    }
    return (
      <div
        dir={direction}
        ref={stageRef}
        className={`${stageClass} bar-view`}
        data-bar-position={edge}
        onContextMenu={(event) => event.preventDefault()}
      >
        {cards.length > 0 && <div className="slot">{cards}</div>}
        <div className={`pill-anchor ${vertical ? "vertical" : ""}`}>
          <div
            className="pill meeting"
            role="button"
            tabIndex={0}
            aria-label={t("overlay.meeting.recording")}
            data-hit
            onClick={() => {
              if (consumeDrag()) return;
              openMainSection("notetaker");
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                openMainSection("notetaker");
              }
            }}
            onPointerDown={handleBarPointerDown}
            onPointerMove={handleBarPointerMove}
            onPointerUp={handleBarPointerUp}
            onPointerCancel={handleBarPointerUp}
            {...tipProps("meeting")}
          >
            <Waveform animated live vertical={false} bars={5} />
            {meetingWrapping ? (
              <Spinner />
            ) : (
              <button
                type="button"
                className="mstop"
                aria-label={t("overlay.meeting.stop")}
                onClick={(event) => {
                  event.stopPropagation();
                  void stopMeeting();
                }}
                {...tipProps("meeting-stop")}
              >
                <i />
                {tipFor === "meeting-stop" && (
                  <Tooltip gap={10}>{t("overlay.meeting.stop")}</Tooltip>
                )}
              </button>
            )}
          </div>
          {tipFor === "meeting" && (
            <Tooltip>
              {meetingWrapping
                ? t("overlay.meeting.saving")
                : t("overlay.bar.showNotepad")}
            </Tooltip>
          )}
          {nudgeOpen && (
            <div className="side after">
              <div className="ncard nudge" role="status" data-hit>
                <span className="ncard-title">
                  {t("overlay.meeting.stillGoing")}
                </span>
                <button
                  type="button"
                  className="ncard-action"
                  onClick={stopMeetingFromNudge}
                >
                  {t("overlay.meeting.stop")}
                </button>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  // ---- Cards above the pill ----
  if (isVisible && state === "error") {
    cards.push(
      card(
        "error",
        t(`overlay.error.${errorCode}`, {
          defaultValue: t("overlay.error.start_failed"),
        }),
        { role: "alert" },
      ),
    );
  }
  if (isVisible && micSilent && listening) {
    cards.push(
      card("silent", t("overlay.bar.silentTitle"), {
        body: t("overlay.bar.silentBody"),
        action: {
          label: t("overlay.bar.selectMicrophone"),
          onClick: () => openMainSection("general"),
        },
      }),
    );
  }
  if (isVisible && commandHint) {
    cards.push(card("command", t("overlay.commandFallback")));
  }
  if (!isVisible && barHidden) {
    cards.push(
      card("hidden", t("overlay.bar.hiddenTitle"), {
        action: {
          label: t("overlay.bar.undo"),
          onClick: () => {
            setBarHidden(false);
            commands.undoHideFlowBar().catch(() => {});
          },
        },
      }),
    );
  }
  if (!isVisible && barError) {
    cards.push(
      card("bar-error", barError, {
        role: "alert",
        action: {
          label: t("overlay.meeting.openSettings"),
          onClick: () => {
            setBarError(null);
            void commands.openMicrophonePrivacySettings().catch(() => {});
          },
        },
      }),
    );
  }

  // ---- Live: the text card above the pill (opt-in overlay style) ----
  let liveCard: React.ReactNode = null;
  if (isVisible && state === "streaming" && overlayStyle === "live") {
    const hasText =
      streamText.committed.length > 0 || streamText.tentative.length > 0;
    const streamWorking = phase === "working";
    const loadingNotice = modelLoadSlow && !streamWorking && !hasText;
    const open =
      hasText || loadingNotice || (loadNoticeShown && !streamWorking);
    if (open) {
      liveCard = (
        <div className="lcard" key={`live-${session}`} data-hit>
          <div
            className={`stext-cap ${overflowing ? "overflowing" : ""}`}
            ref={capRef}
            onScroll={handleStreamScroll}
          >
            <p>
              {loadingNotice && (
                <span className="sloading">{t("overlay.loadingModel")}</span>
              )}
              <span className="committed">
                {streamText.committed ? streamText.committed + " " : ""}
              </span>
              <span className="tentative">{streamText.tentative}</span>
              {!streamWorking && !loadingNotice && (
                <span className="scaret" />
              )}
            </p>
          </div>
          {captureReady && !streamWorking && (
            <span className="ltimer">{fmtTime(elapsed)}</span>
          )}
        </div>
      );
    }
  }

  // ---- The pill ----
  const [pillW, pillH] = sizeFor(pillState, edge);
  const showLanguageCircle = prefs.spokenLanguages.length >= 2;
  const languageOptions = barLanguageOptions(
    prefs.spokenLanguages,
    suggestedLanguages(),
  );
  const ready = pillState === "ready";
  const clickable =
    pillState === "resting" ||
    pillState === "ready" ||
    pillState === "initializing" ||
    pillState === "listening_ptt";
  const workingLabel =
    workKind === "polishing" || state === "processing"
      ? t("overlay.processing")
      : modelLoading
        ? t("overlay.loadingModel")
        : t("overlay.transcribing");
  const plain = (text: string) => text.replace(/<\/?key>/g, "");

  let pillContent: React.ReactNode = null;
  if (pillState === "ready") {
    pillContent = <MicIcon />;
  } else if (
    pillState === "initializing" ||
    pillState === "listening_ptt" ||
    pillState === "error"
  ) {
    pillContent = (
      <Waveform
        ref={waveRef}
        animated={liveCapture}
        live={liveCapture}
        vertical={vertical}
      />
    );
  } else if (pillState === "listening_handsfree") {
    pillContent = (
      <>
        <button
          type="button"
          className="pbtn cancel"
          aria-label={t("overlay.cancel")}
          onClick={(event) => {
            event.stopPropagation();
            barStartedAtRef.current = 0;
            commands.cancelOperation().catch(() => {});
          }}
          {...tipProps("cancel")}
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="3"
            strokeLinecap="round"
            aria-hidden="true"
          >
            <path d="M18 6 6 18M6 6l12 12" />
          </svg>
          {tipFor === "cancel" && (
            <Tooltip gap={10}>{t("overlay.cancel")}</Tooltip>
          )}
        </button>
        <Waveform ref={waveRef} animated live vertical={vertical} />
        <button
          type="button"
          className="pbtn stop"
          aria-label={t("overlay.finishAndPaste")}
          onClick={(event) => {
            event.stopPropagation();
            barStartedAtRef.current = 0;
            commands.toggleDictationFromBar().catch(() => {});
          }}
          {...tipProps("stop")}
        >
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="3"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <rect x="5" y="5" width="14" height="14" rx="2" />
          </svg>
          {tipFor === "stop" && (
            <Tooltip gap={10}>{t("overlay.finishAndPaste")}</Tooltip>
          )}
        </button>
      </>
    );
  } else if (pillState === "processing") {
    pillContent = (
      <>
        <Waveform animated={false} live={false} vertical={vertical} />
        <Spinner />
      </>
    );
  }

  let pillLabel: string | undefined;
  if (pillState === "processing") pillLabel = workingLabel;
  else if (pillState === "ready" || pillState === "resting") {
    pillLabel = plain(t("overlay.bar.dictate", { key: prefs.shortcut }));
  } else if (pillState === "listening_handsfree") {
    pillLabel = plain(t("overlay.bar.pressToFinish", { key: prefs.shortcut }));
  } else if (pillState === "error") {
    pillLabel = t(`overlay.error.${errorCode}`, {
      defaultValue: t("overlay.error.start_failed"),
    });
  }

  // The pill's own tooltip: "Dictate fn" whenever the bar is ready (unless a
  // side button's own tooltip shows), "Press fn to finish and paste" while
  // the pointer is on the hands-free pill.
  let pillTip: React.ReactNode = null;
  if (ready && !langMenuOpen && (tipFor === null || tipFor === "pill")) {
    pillTip = (
      <Tooltip>
        <Trans
          i18nKey="overlay.bar.dictate"
          values={{ key: prefs.shortcut }}
          components={{ key: keyLabel }}
        />
      </Tooltip>
    );
  } else if (pillState === "listening_handsfree" && tipFor === "pill") {
    pillTip = (
      <Tooltip>
        <Trans
          i18nKey="overlay.bar.pressToFinish"
          values={{ key: prefs.shortcut }}
          components={{ key: keyLabel }}
        />
      </Tooltip>
    );
  }

  const sideOut = !ready;
  const styleChip =
    isVisible &&
    prefs.styleChip &&
    styleInfo?.postProcessEnabled &&
    styleInfo.styleName ? (
      <div className="side before">
        <span className="pchip" title={t("overlay.style.chipTitle")}>
          {styleInfo.styleName}
        </span>
      </div>
    ) : null;

  const barView = !isVisible;
  const showPill = pillState !== "hidden";

  return (
    <div
      dir={direction}
      ref={stageRef}
      className={`${stageClass} ${barView ? "bar-view" : ""}`}
      data-bar-position={edge}
      onContextMenu={onContextMenu}
      onPointerEnter={onStagePointerEnter}
      onPointerLeave={onStagePointerLeave}
    >
      {(liveCard || cards.length > 0) && (
        <div className="slot">
          {liveCard}
          {cards}
        </div>
      )}
      {showPill && (
        <div className={`pill-anchor ${vertical ? "vertical" : ""}`}>
          {barView && (
            <div className="side before">
              {showLanguageCircle && (
                <button
                  type="button"
                  className={`circ lang ${sideOut ? "out" : ""}`}
                  aria-label={t("overlay.bar.language")}
                  aria-haspopup="menu"
                  aria-expanded={langMenuOpen}
                  tabIndex={sideOut ? -1 : 0}
                  data-hit={sideOut ? undefined : ""}
                  onClick={() => {
                    // The exact-fit windows have no room for an HTML menu:
                    // the native menu's Languages submenu stands in.
                    if (FIXED_FRAME) setLangMenuOpen((open) => !open);
                    else showBarMenu();
                  }}
                  {...tipProps("lang")}
                >
                  {shortLanguageCode(prefs.selectedLanguage)}
                  {tipFor === "lang" && !langMenuOpen && (
                    <Tooltip>{t("overlay.bar.language")}</Tooltip>
                  )}
                  {langMenuOpen && (
                    <span className="bmenu" role="menu" data-hit>
                      {["auto", ...languageOptions].map((code) => {
                        const active = prefs.selectedLanguage === code;
                        return (
                          <span
                            key={code}
                            role="menuitemradio"
                            aria-checked={active}
                            tabIndex={0}
                            className="bmenu-item"
                            onClick={(event) => {
                              event.stopPropagation();
                              void pickLanguage(code);
                            }}
                            onKeyDown={(event) => {
                              if (event.key === "Enter" || event.key === " ") {
                                event.preventDefault();
                                void pickLanguage(code);
                              }
                            }}
                          >
                            {code === "auto"
                              ? t("overlay.language.auto")
                              : languageName(code, i18n.language)}
                          </span>
                        );
                      })}
                    </span>
                  )}
                </button>
              )}
            </div>
          )}
          {styleChip}
          <div
            className={`pill ${pillState} ${vertical ? "vertical" : ""} ${
              commandStyle ? "command" : ""
            }`}
            style={{ width: pillW, height: pillH }}
            role={clickable ? "button" : "status"}
            tabIndex={clickable ? 0 : undefined}
            aria-label={pillLabel}
            data-hit
            data-state={pillState}
            onClick={clickable ? onPillClick : undefined}
            onKeyDown={clickable ? onPillKeyDown : undefined}
            onPointerDown={barView ? handleBarPointerDown : undefined}
            onPointerMove={barView ? handleBarPointerMove : undefined}
            onPointerUp={barView ? handleBarPointerUp : undefined}
            onPointerCancel={barView ? handleBarPointerUp : undefined}
            {...tipProps("pill")}
          >
            {pillContent}
          </div>
          {pillTip}
          {barView && (
            <div className="side after">
              <button
                type="button"
                className={`circ ${sideOut ? "out" : ""}`}
                aria-label={t("overlay.bar.takeNotes")}
                tabIndex={sideOut ? -1 : 0}
                data-hit={sideOut ? undefined : ""}
                onClick={() => void startMeeting()}
                {...tipProps("notes")}
              >
                <NotesIcon />
                {tipFor === "notes" && (
                  <Tooltip>{t("overlay.bar.takeNotes")}</Tooltip>
                )}
              </button>
              {prefs.scratchpad && (
                <button
                  type="button"
                  className={`circ ${sideOut ? "out" : ""}`}
                  aria-label={t("tray.scratchpadOpen")}
                  tabIndex={sideOut ? -1 : 0}
                  data-hit={sideOut ? undefined : ""}
                  onClick={() => {
                    setHovered(false);
                    commands.openScratchpad().catch(() => {});
                  }}
                  {...tipProps("scratchpad")}
                >
                  <BoltIcon />
                  {tipFor === "scratchpad" && (
                    <Tooltip>{t("tray.scratchpadOpen")}</Tooltip>
                  )}
                </button>
              )}
            </div>
          )}
        </div>
      )}
    </div>
  );
};

export default RecordingOverlay;
