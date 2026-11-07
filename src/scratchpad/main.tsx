import React from "react";
import ReactDOM from "react-dom/client";
import { listen } from "@tauri-apps/api/event";
import Scratchpad from "./Scratchpad";
import {
  applyTheme,
  getStoredTheme,
  syncThemeFromSettings,
} from "@/lib/utils/theme";
import { applyAccent, getStoredAccent, getStoredHue } from "@/lib/utils/accent";
import type { Theme } from "@/bindings";
import { syncLanguageFromSettings } from "@/i18n";
import "@/i18n";

// A separate webview from the settings window, so the Scratchpad applies
// `data-theme` on its own document, exactly like the recording overlay:
// last-known theme before render, reconcile with the persisted setting, then
// follow live changes.
applyAccent(getStoredAccent(), getStoredHue());
applyTheme(getStoredTheme());
syncThemeFromSettings();
syncLanguageFromSettings();
listen<Theme>("theme-changed", (event) => applyTheme(event.payload));

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <Scratchpad />
  </React.StrictMode>,
);
