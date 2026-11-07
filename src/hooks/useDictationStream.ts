import { useEffect, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import type { RecordingStateEvent, StreamTextEvent } from "@/bindings";

export interface DictationStream {
  recording: boolean;
  locked: boolean;
  committed: string;
  tentative: string;
  startedAt: number | null;
}

const IDLE: DictationStream = {
  recording: false,
  locked: false,
  committed: "",
  tentative: "",
  startedAt: null,
};

/**
 * The live dictation, straight from the events the overlay already consumes:
 * the phase, and the text as it is committed plus the words still in flight.
 * The hub shows it so dictation can be watched where it lands, not in a pill.
 */
export const useDictationStream = (): DictationStream => {
  const [state, setState] = useState<DictationStream>(IDLE);
  const startedAt = useRef<number | null>(null);

  useEffect(() => {
    let alive = true;
    const unlisten: Array<() => void> = [];

    listen<RecordingStateEvent>("recording-state-event", (event) => {
      const phase = event.payload.phase;
      if (phase === "started" || phase === "locked") {
        startedAt.current = startedAt.current ?? Date.now();
        setState({
          recording: true,
          locked: phase === "locked",
          committed: "",
          tentative: "",
          startedAt: startedAt.current,
        });
        return;
      }
      startedAt.current = null;
      setState(IDLE);
    }).then((un) => (alive ? unlisten.push(un) : un()));

    listen<StreamTextEvent>("stream-text-event", (event) => {
      setState((current) => ({
        ...current,
        committed: event.payload.committed,
        tentative: event.payload.tentative,
      }));
    }).then((un) => (alive ? unlisten.push(un) : un()));

    return () => {
      alive = false;
      for (const un of unlisten) un();
    };
  }, []);

  return state;
};
