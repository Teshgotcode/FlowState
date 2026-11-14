// Browser preview of the recording overlay (the Flow Bar), no native build.
//
// Dev-only: `preview.html` is not a Vite build input, so nothing here ships.
// Open http://localhost:1420/src/overlay/preview.html (bunx vite dev) and drive
// the overlay from the console or Playwright:
//
//   __emit("show-overlay", "recording")       // any backend event + payload
//   __emit("recording-ready")
//   __emit("mic-level", Array(16).fill(0.6))
//   __emit("overlay-error", "no_model")
//   __emit("meeting-bar-hover", true)
//   __invokes                                  // every command the overlay ran
//   __previewSettings.spoken_languages = ["en"] // settings the stub returns
//
// Query string driver: ?state=recording&ready=1&level=0.5&edge=bottom&theme=dark
// &lang=no&mode=handsfree. `state` also takes "resting" / "ready" / "meeting".

type Handler = (args: Record<string, unknown>) => unknown;
type EventCallback = (event: {
  event: string;
  id: number;
  payload: unknown;
}) => void;

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>;
    __emit: (event: string, payload?: unknown) => void;
    __invokes: Array<{ cmd: string; args: Record<string, unknown> }>;
    __previewSettings: Record<string, unknown>;
    __previewMeeting: { state: string; started_at?: number };
  }
}

const params = new URLSearchParams(window.location.search);
const edge = params.get("edge") ?? "bottom";
document.body.dataset.edge = edge;

window.__previewSettings = {
  overlay_position: edge,
  meeting_bar_position: null,
  meeting_reminder_minutes: 60,
  selected_language: "auto",
  spoken_languages: ["en", "no"],
  overlay_style: params.get("style") ?? "minimal",
  theme: params.get("theme") ?? "system",
  app_language: params.get("lang") ?? "en",
  show_bar_at_all_times: true,
  scratchpad_in_flow_bar: params.get("scratchpad") === "1",
  flow_bar_style_chip: false,
  post_process_enabled: false,
  bindings: {
    transcribe: {
      id: "transcribe",
      name: "Transcribe",
      description: "",
      default_binding: "fn",
      current_binding: "fn",
    },
  },
};
window.__previewMeeting = { state: "idle" };
window.__invokes = [];

const handlers: Record<string, Handler> = {
  get_app_settings: () => window.__previewSettings,
  get_meeting_status: () => window.__previewMeeting,
  get_active_style: () => ({
    styleId: "default",
    styleName: "Default",
    matchedRuleKey: null,
    frontmost: null,
    styles: [],
    postProcessEnabled: false,
  }),
};

const callbacks = new Map<number, EventCallback>();
const listeners = new Map<string, Set<number>>();
let nextId = 0;

const invoke = async (cmd: string, args: Record<string, unknown> = {}) => {
  window.__invokes.push({ cmd, args });
  if (cmd === "plugin:event|listen") {
    const event = String(args.event);
    const handler = Number(args.handler);
    if (!listeners.has(event)) listeners.set(event, new Set());
    listeners.get(event)!.add(handler);
    return handler;
  }
  if (cmd === "plugin:event|unlisten") {
    listeners.get(String(args.event))?.delete(Number(args.eventId));
    return null;
  }
  const handler = handlers[cmd];
  return handler ? handler(args) : null;
};

window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: (event: string, id: number) => {
    listeners.get(event)?.delete(id);
  },
};

window.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "recording_overlay" },
    currentWebview: {
      label: "recording_overlay",
      windowLabel: "recording_overlay",
    },
  },
  invoke,
  transformCallback: (callback: EventCallback) => {
    const id = ++nextId;
    callbacks.set(id, callback);
    return id;
  },
  unregisterCallback: (id: number) => {
    callbacks.delete(id);
  },
  convertFileSrc: (path: string) => path,
};

Object.defineProperty(window, "__TAURI_OS_PLUGIN_INTERNALS__", {
  value: {
    platform: "macos",
    family: "unix",
    os_type: "macos",
    arch: "aarch64",
    version: "15.1.1",
    eol: "\n",
    exe_extension: "",
  },
  configurable: true,
});

window.__emit = (event: string, payload: unknown = null) => {
  for (const id of listeners.get(event) ?? []) {
    callbacks.get(id)?.({ event, id, payload });
  }
};

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

void (async () => {
  await import("./main");
  // Let the overlay register its listeners before driving it.
  for (let i = 0; i < 50 && !(listeners.get("show-overlay")?.size ?? 0); i++) {
    await sleep(20);
  }
  const state = params.get("state");
  if (!state) return;
  if (state === "resting" || state === "ready") {
    window.__emit("show-overlay", "bar-resting");
    if (state === "ready") window.__emit("meeting-bar-hover", true);
    return;
  }
  if (state === "meeting") {
    window.__previewMeeting = {
      state: "recording",
      started_at: Math.floor(Date.now() / 1000),
    };
    window.__emit("show-overlay", "meeting-recording");
    return;
  }
  if (state === "error") {
    window.__emit("show-overlay", "error");
    window.__emit("overlay-error", params.get("code") ?? "no_model");
    return;
  }
  window.__emit("show-overlay", state);
  if (params.get("mode") === "handsfree") {
    window.__emit("overlay-mode", "handsfree");
  }
  if (params.get("ready") === "1") {
    window.__emit("recording-ready");
    const level = Number(params.get("level") ?? "0.3");
    window.setInterval(
      () => window.__emit("mic-level", Array(16).fill(level)),
      33,
    );
  }
})();

export {};
