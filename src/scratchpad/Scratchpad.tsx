import React, { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useTranslation } from "react-i18next";
import { Check, Copy } from "lucide-react";
import { commands } from "@/bindings";
import i18n from "@/i18n";
import { getLanguageDirection } from "@/lib/utils/rtl";
import "./Scratchpad.css";

// The note is written back shortly after the last keystroke; the close
// handler flushes a pending save so the text survives a quick open → type
// → close.
const SAVE_DEBOUNCE_MS = 400;
const COPY_FEEDBACK_MS = 1500;

/**
 * The Scratchpad: a small always-there note, opened from the meeting bar.
 * The caret is ready the moment it opens; a dictation taken while the
 * window is focused arrives as a `scratchpad-text` event and is inserted
 * at the caret — never pasted into another app. The text is persisted
 * locally by the backend between opens.
 */
const Scratchpad: React.FC = () => {
  const { t } = useTranslation();
  const areaRef = useRef<HTMLTextAreaElement>(null);
  const [content, setContent] = useState("");
  const [ready, setReady] = useState(false);
  const [copied, setCopied] = useState(false);
  // The note's latest text, mirrored from state so the late-arriving
  // dictation and close events always see the current note, not a stale one.
  const contentRef = useRef("");
  const saveTimerRef = useRef<ReturnType<typeof setTimeout>>();
  const copiedTimerRef = useRef<ReturnType<typeof setTimeout>>();

  const scheduleSave = (text: string) => {
    clearTimeout(saveTimerRef.current);
    saveTimerRef.current = setTimeout(() => {
      commands
        .saveScratchpadNote(text)
        .catch((err) => console.warn("Failed to save the note:", err));
    }, SAVE_DEBOUNCE_MS);
  };

  const applyContent = (next: string) => {
    contentRef.current = next;
    setContent(next);
    scheduleSave(next);
  };

  // Load the saved note once, then put the caret in it: the window opens
  // ready to dictate or type into.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    (async () => {
      try {
        const result = await commands.getScratchpadNote();
        if (result.status === "ok") {
          contentRef.current = result.data;
          setContent(result.data);
        }
      } catch (err) {
        // An unreadable note still opens; typing starts one anew.
        console.warn("Failed to read the note:", err);
      }
      setReady(true);
      areaRef.current?.focus();
      // A take dictated with this window focused lands here at the caret.
      unlisten = await listen<string>("scratchpad-text", (event) => {
        const el = areaRef.current;
        const note = contentRef.current;
        const start = el?.selectionStart ?? note.length;
        const end = el?.selectionEnd ?? note.length;
        const next = note.slice(0, start) + event.payload + note.slice(end);
        applyContent(next);
        requestAnimationFrame(() => {
          el?.focus();
          const caret = start + event.payload.length;
          el?.setSelectionRange(caret, caret);
        });
      });
    })();
    return () => {
      unlisten?.();
    };
  }, []);

  // The window's own close button: flush the pending save before the
  // webview goes away, so even a quick note survives.
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    getCurrentWindow()
      .onCloseRequested(async (event) => {
        event.preventDefault();
        clearTimeout(saveTimerRef.current);
        try {
          await commands.saveScratchpadNote(contentRef.current);
        } catch (err) {
          console.warn("Failed to save the note on close:", err);
        }
        await getCurrentWindow().destroy();
      })
      .then((un) => {
        unlisten = un;
      })
      .catch((err) => console.warn("Close handling failed:", err));
    return () => {
      unlisten?.();
    };
  }, []);

  // The title bar follows the language like the rest of the app.
  useEffect(() => {
    getCurrentWindow()
      .setTitle(t("scratchpad.title"))
      .catch((err) => console.warn("Failed to set the window title:", err));
  }, [t]);

  const copyNote = async () => {
    try {
      await writeText(contentRef.current);
      setCopied(true);
      clearTimeout(copiedTimerRef.current);
      copiedTimerRef.current = setTimeout(
        () => setCopied(false),
        COPY_FEEDBACK_MS,
      );
    } catch (err) {
      console.warn("Failed to copy the note:", err);
    }
  };

  const direction = getLanguageDirection(i18n.language);

  return (
    <div dir={direction} className="sp-root">
      <textarea
        ref={areaRef}
        className="sp-area"
        value={content}
        onChange={(event) => applyContent(event.target.value)}
        placeholder={ready ? t("scratchpad.placeholder") : ""}
        spellCheck={false}
        autoFocus
      />
      <footer className="sp-footer">
        <button
          type="button"
          className="sp-copy"
          onClick={copyNote}
          disabled={!content}
        >
          {copied ? (
            <Check className="h-3.5 w-3.5" aria-hidden="true" />
          ) : (
            <Copy className="h-3.5 w-3.5" aria-hidden="true" />
          )}
          {copied ? t("scratchpad.copied") : t("scratchpad.copy")}
        </button>
      </footer>
    </div>
  );
};

export default Scratchpad;
