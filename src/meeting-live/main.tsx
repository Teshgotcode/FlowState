import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import MeetingLive from "./MeetingLive";
import {
  applyTheme,
  getStoredTheme,
  syncThemeFromSettings,
} from "@/lib/utils/theme";
import { applyAccent, getStoredAccent, getStoredHue } from "@/lib/utils/accent";
import type { Theme } from "@/bindings";
import { syncLanguageFromSettings } from "@/i18n";
import "@/i18n";

// Its own webview, like the overlay: last-known theme before the first paint,
// then follow the persisted setting and live changes.
applyAccent(getStoredAccent(), getStoredHue());
applyTheme(getStoredTheme());
syncThemeFromSettings();
syncLanguageFromSettings();
listen<Theme>("theme-changed", (event) => applyTheme(event.payload));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <MeetingLive demo={new URLSearchParams(window.location.search).has("demo")} />
  </React.StrictMode>,
);
