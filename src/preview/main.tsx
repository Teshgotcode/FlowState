/**
 * Browser-only preview of the hub: a fake Tauri bridge plus mock history, so
 * the real screens can be screenshotted and diffed without the Rust host.
 * `bunx vite dev` then open /src/preview/index.html.
 */
import type { HistoryEntry } from "@/bindings";
import React from "react";
import ReactDOM from "react-dom/client";
import "../i18n";
import "../App.css";
import { applyAccent, getStoredAccent, getStoredHue } from "@/lib/utils/accent";

applyAccent(getStoredAccent(), getStoredHue());

const MINUTE = 60;
const now = Math.floor(Date.now() / 1000);

interface Row {
  id: number;
  minutesAgo: number;
  text: string;
  language?: string;
  source?: "meeting" | null;
  polished?: string | null;
}

const SNIPPETS = [
  "Hei, jeg sitter og skriver litt på briefen til Solan, og jeg tenker at vi kanskje skal legge inn en kort introduksjon før vi går gjennom tallene.",
  "Tre ting jeg må huske: sende regnearket til Mattias, booke møterom for torsdag, og svare NTNU på den e-posten om dugnad.",
  "Kunden sa at de var fornøyde med prisen, men at de ville ha en tydeligere tidslinje for når vi er ferdig med prototypen.",
  "Jeg tror vi underestimerer hvor mye tid det tar å få testet modellen på ekte brukere. Vi bør sette av en hel dag bare til det.",
  "Husk å sjekke at hypotesen i business model canvas fortsatt stemmer med det vi lærte i intervjuene i forrige uke.",
  "Kan du sende meg presentasjonen fra i går, så legger jeg inn en slide om distribusjon og sender den tilbake før lunsj.",
  "Jeg lurer på om vi bør bytte navn på produktet. FlowState funker, men det er ingen som skjønner hva det betyr før jeg forklarer.",
  "Vi må få opp en landing page før markedsdagen, ellers ser det ut som om vi ikke har gjort noe som helst.",
  "Hva var det egentlig vi bestemte om fordelingen av aksjer? Jeg finner ikke igjen notatet.",
  "Skriv en kort beskjed til teamet om at vi møtes klokka ni i stedet for åtte på fredag, fordi jeg har en legetime først.",
  "Referat fra gruppetimen: markedet er mindre enn vi trodde, men viljen til å betale er høyere. Vi justerer pris og går videre på samme segment.",
  "Notat til meg selv: ring mamma og hør hvordan det går med hunden.",
  "Ide til pitch: vi selger ikke diktering, vi selger tid. Tre minutter spart per e-post, hver dag, hver ansatt.",
  "Sjekk om vi kan bruke samme modell for norsk og engelsk, eller om vi må laste ned to.",
];

/** Three weeks of history with quiet days in between, so the charts have shape. */
const PER_DAY = [3, 2, 3, 1, 0, 4, 2, 3, 1, 0, 2, 5, 3, 1, 0, 0, 2, 4, 3, 2, 5];
const ROWS: Row[] = [];
let rowId = 1;
for (let day = 0; day < PER_DAY.length; day++) {
  for (let n = 0; n < PER_DAY[day]; n++) {
    ROWS.push({
      id: rowId++,
      minutesAgo: day * 24 * 60 + 40 + n * 95,
      text: SNIPPETS[(day * 3 + n) % SNIPPETS.length],
      language: "no",
      source: day === 5 && n === 0 ? "meeting" : null,
      polished: n === 1 && day % 4 === 0 ? "Ryddet versjon av teksten." : null,
    });
  }
}

// Meetings for the Notetaker page: two days of them, none today, so the
// empty "today" card and the day groups both show.
const MEETINGS = [
  { hoursAgo: 13.4, text: "Rebus Takst - prosessdesign. Teamet så videoen om bachelorprosjektet for Rebill og planla sprinten: Rebus som utgangspunkt, kundeinnsikt utover Rebill, møte med Irene Berg, prototype og delt AI-oppsett." },
  { hoursAgo: 11.9, text: "DNB - svindelforebygging på telefon. Gikk gjennom hvordan svindlere bygger tillit over tid, og hvilke signaler banken kan se tidlig nok til å stoppe overføringen." },
  { hoursAgo: 11.3, text: "Bankfinansiering med AI. Vi diskuterte kredittvurdering på tynne data, og om modellen kan forklare avslagene godt nok for kunden." },
  { hoursAgo: 37.5, text: "DNB Regnskap - AI-automatisering. Prototypen leser bilag og foreslår kontering. Neste steg er å måle hvor ofte forslaget må rettes av et menneske." },
  { hoursAgo: 38.2, text: "Elsykler - oppstartsstøtte. Søknaden trenger en tydeligere markedsplan og et budsjett som viser første år med faktiske kostnader." },
  { hoursAgo: 62.0, text: "Solan - sprintplanlegging. Delte opp sprinten i tre: kundeintervjuer, prototype og en kortere presentasjon til linjeforeningen." },
];

const entries: HistoryEntry[] = ROWS.map((row) => {
  const timestamp = now - row.minutesAgo * MINUTE;
  return {
    id: row.id,
    file_name: "",
    timestamp,
    saved: false,
    title: new Date(timestamp * 1000).toISOString(),
    transcription_text: row.text,
    post_processed_text: row.polished ?? null,
    post_process_prompt: row.polished ? "Rydd notatet" : null,
    post_process_requested: Boolean(row.polished),
    source_name: row.source === "meeting" ? "gruppetime.wav" : null,
    language: row.language ?? null,
    language_source: "detected",
    source: row.source ?? null,
    metadata: null,
  };
});

entries.push(
  ...MEETINGS.map((meeting, index) => {
    const timestamp = now - Math.round(meeting.hoursAgo * 3600);
    return {
      id: 900 + index,
      file_name: "",
      timestamp,
      saved: true,
      title: new Date(timestamp * 1000).toISOString(),
      transcription_text: meeting.text,
      post_processed_text: meeting.text,
      post_process_prompt: null,
      post_process_requested: false,
      source_name: "mote.wav",
      language: "no",
      language_source: "detected" as const,
      source: "meeting" as const,
      metadata: { duration_secs: 1500 + index * 240 },
    };
  }),
);

const NOTE_SEED = [
  {
    id: "n1",
    title: "En idé som",
    body: "En idé som lar deg holde legathåndboken og andre kilder oppdatert, er at man kan oppgi hvem man er, eller at den allerede kjenner deg gjennom for eksempel CV, LinkedIn og sosiale medier.",
    createdAt: now - 3 * 86_400,
    updatedAt: now - 3 * 86_400,
    pinned: true,
  },
  {
    id: "n2",
    title: "Spørsmål om dyrehjem",
    body: "Hei! Jeg heter Tejas og er student ved NTNU Entreprenørskolen i Trondheim. Vi jobber med et prosjekt om dyrehjem og trenger å forstå hvordan hverdagen deres ser ut.",
    createdAt: now - 5 * 86_400,
    updatedAt: now - 5 * 86_400,
    pinned: false,
  },
  {
    id: "n3",
    title: "Har avtalt",
    body: "Har avtalt møte med Mattias på torsdag. Ta med tallene fra forrige uke og en kort oppsummering av hva vi lærte i intervjuene.",
    createdAt: now - 9 * 86_400,
    updatedAt: now - 9 * 86_400,
    pinned: false,
  },
];
if (!localStorage.getItem("flowstate.notes.v1")) {
  localStorage.setItem("flowstate.notes.v1", JSON.stringify(NOTE_SEED));
}

const settings = {
  settings_schema_version: 60,
  onboarding_completed: true,
  selected_model: "whisper-large-v3-turbo",
  selected_language: "no",
  app_language: "no",
  translate_to_english: false,
  theme: "light",
  debug_mode: false,
  custom_words: ["Tejas", "stud.ntnu.no", "FlowState"],
  post_process_enabled: true,
  post_process_prompts: [
    { id: "style_neutral", name: "Nøytral", prompt: "Rydd skrivefeil og fyllord." },
    { id: "style_formal", name: "Formell", prompt: "Formelt bokmål, uten slang." },
    { id: "style_email", name: "E-post", prompt: "Lag en ferdig e-post." },
    {
      id: "p1",
      name: "Kulepunkter",
      prompt: "Gjør teksten om til korte kulepunkter. Behold alle fakta og navn.",
    },
  ],
  post_process_selected_prompt_id: "style_neutral",
  bindings: {
    transcribe: {
      id: "transcribe",
      name: "Transcribe",
      description: "",
      default_binding: "option+space",
      current_binding: "option+space",
    },
  },
};

type Handler = (args: Record<string, unknown>) => unknown;

const handlers: Record<string, Handler> = {
  get_app_settings: () => settings,
  add_post_process_prompt: ({ name, prompt }: Record<string, any>) => {
    const item = { id: `p${Date.now()}`, name, prompt };
    settings.post_process_prompts.push(item);
    return item;
  },
  update_post_process_prompt: ({ id, name, prompt }: Record<string, any>) => {
    const item = settings.post_process_prompts.find((p) => p.id === id);
    if (item) {
      item.name = name;
      item.prompt = prompt;
    }
    return null;
  },
  delete_post_process_prompt: ({ id }: Record<string, any>) => {
    settings.post_process_prompts = settings.post_process_prompts.filter(
      (p) => p.id !== id,
    );
    return null;
  },
  set_post_process_selected_prompt: ({ id }: Record<string, any>) => {
    settings.post_process_selected_prompt_id = id;
    return null;
  },
  // The ask endpoint answers from one transcript; the preview stands in with a
  // fixed answer so the panel and the cross-meeting pick can be exercised.
  ask_meeting_transcript: ({ entryId, question }: Record<string, any>) => ({
    outcome: "answered",
    answer: {
      found: true,
      answer: `Preview-svar på «${question}» fra møte ${entryId}.`,
      quotes: [],
      model: "preview",
    },
  }),
  get_history_entries: ({ limit }) => ({
    entries: entries.slice(0, (limit as number) ?? 50),
    has_more: false,
  }),
  search_history: ({ query, limit }) => {
    const needle = String(query).toLowerCase();
    const matches = entries.filter((entry) =>
      entry.transcription_text.toLowerCase().includes(needle),
    );
    return {
      entries: matches.slice(0, (limit as number) ?? 50),
      total: matches.length,
      has_more: false,
    };
  },
  get_retired_models: () => [],
  change_theme_setting: () => null,
  get_windows_microphone_permission_status: () => ({
    supported: false,
    overall_access: "granted",
  }),
  is_laptop: () => true,
  get_detected_language: () => null,
  get_monitor_timestamp: () => now,
  get_available_microphones: () => [],
  get_available_output_devices: () => [],
  get_audio_devices: () => [],
  get_output_devices: () => [],
  get_available_models: () => [
    {
      id: "whisper-large-v3-turbo",
      name: "Whisper Large v3 Turbo",
      description: "",
      filename: "whisper-large-v3-turbo.bin",
      source: "Local",
      size_mb: 890,
      is_downloaded: true,
      is_downloading: false,
      partial_size: 0,
      accuracy_score: 4,
      speed_score: 4,
      supports_translation: true,
      is_recommended: true,
      supported_languages: ["no", "en"],
      supports_language_selection: true,
      is_custom: false,
      supports_streaming: true,
      supports_language_detection: true,
    },
  ],
  get_transcription_model_status: () => ({
    is_loaded: true,
    current_model: "whisper-large-v3-turbo",
  }),
  is_recording: () => false,
  get_current_model: () => "whisper-large-v3-turbo",
  get_app_dir: () => "/tmp",
  get_clipboard: () => "",
  get_meeting_status: () => null,
  is_update_checks_locked: () => false,
  // The event plugin hands back a listener id and unregisters by id; the
  // components only need the promise to resolve.
  "plugin:event|listen": () => 0,
  "plugin:event|unlisten": () => null,
  // The preview window already has whatever it needs; say yes to every gate.
  "plugin:macos-permissions|check_microphone_permission": () => true,
  "plugin:macos-permissions|check_accessibility_permission": () => true,
  "plugin:macos-permissions|check_screen_recording_permission": () => true,
};

declare global {
  interface Window {
    __TAURI_INTERNALS__?: Record<string, unknown>;
  }
}

let callbackId = 0;
const callbacks = new Map<number, (payload: unknown) => void>();

window.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
  unregisterListener: () => {},
};

window.__TAURI_INTERNALS__ = {
  metadata: {
    currentWindow: { label: "main" },
    currentWebview: { label: "main", windowLabel: "main" },
  },
  invoke: async (cmd: string, args: Record<string, unknown> = {}) => {
    const handler = handlers[cmd];
    if (!handler) return null;
    return handler(args);
  },
  transformCallback: (callback: (payload: unknown) => void) => {
    const id = ++callbackId;
    callbacks.set(id, callback);
    return id;
  },
  unregisterCallback: (id: number) => {
    callbacks.delete(id);
  },
  convertFileSrc: (path: string) => path,
};

// The OS plugin reads these as plain values, not calls.
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

void (async () => {
  const { default: i18n } = await import("../i18n");
  await i18n.changeLanguage("no");
  const container = document.getElementById("root")!;
  // ?intro=1 renders the onboarding intro alone, so the first screen can be
  // screenshotted without walking the flow.
  if (new URLSearchParams(window.location.search).has("welcome")) {
    const { Welcome } = await import("../components/onboarding/screens/Welcome");
    ReactDOM.createRoot(container).render(
      <React.StrictMode>
        <Welcome progress={{ index: 1, count: 7 }} onContinue={() => {}} />
      </React.StrictMode>,
    );
    return;
  }
  if (new URLSearchParams(window.location.search).has("intro")) {
    const { Intro } = await import("../components/onboarding/screens/Intro");
    ReactDOM.createRoot(container).render(
      <React.StrictMode>
        <Intro progress={{ index: 0, count: 6 }} onContinue={() => {}} preview />
      </React.StrictMode>,
    );
    return;
  }
  const { default: App } = await import("../App");
  ReactDOM.createRoot(container).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
})();
