import { useEffect, useState } from "react";

/** Seconds passed since `startedAt` (unix seconds), refreshed every second
 * while `active` is true and frozen otherwise. The backend records the start
 * on the same machine, so wall-clock arithmetic is exact; the result never
 * runs negative. */
export const useMeetingClock = (startedAt: number, active: boolean): number => {
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    if (!active) return;
    // Catch up at once, then tick: the clock never shows a stale start.
    setNowMs(Date.now());
    const tick = window.setInterval(() => setNowMs(Date.now()), 1000);
    return () => window.clearInterval(tick);
  }, [active]);

  if (!active) return 0;
  return Math.max(0, Math.floor(nowMs / 1000) - startedAt);
};
