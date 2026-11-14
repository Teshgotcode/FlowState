// The Flow Bar's numbers in one place, so the CSS, the component and the Rust
// window geometry can be checked against each other. Pure: no React, no Tauri.
//
// Sizes are CSS px at the bottom edge (width x height); the side edges swap
// them. 1 CSS px = 1 macOS point in the overlay webview.

export type PillState =
  | "resting"
  | "ready"
  | "initializing"
  | "listening_ptt"
  | "listening_handsfree"
  | "processing"
  | "error"
  | "hidden";

export type BarEdge = "bottom" | "top" | "left" | "right";

export const PILL_SIZE: Record<PillState, readonly [number, number]> = {
  resting: [40, 8],
  ready: [50, 30],
  initializing: [73, 30],
  listening_ptt: [73, 30],
  listening_handsfree: [102.5, 30],
  processing: [98, 30],
  error: [91, 30],
  hidden: [40, 8],
};

export const isSideEdge = (edge: BarEdge) => edge === "left" || edge === "right";

export const sizeFor = (
  state: PillState,
  edge: BarEdge,
): readonly [number, number] =>
  isSideEdge(edge)
    ? ([PILL_SIZE[state][1], PILL_SIZE[state][0]] as const)
    : PILL_SIZE[state];

export const WAVE_BAR_COUNT = 10;

/** Static per-bar height factor: tallest in the middle, ~0.58 at the ends. */
export const barHeightScale = (i: number, n = WAVE_BAR_COUNT) =>
  Math.max(0, 1 - Math.pow(Math.abs((n - 1) / 2 - i), 2) / 48);

/** Animation delay per bar (s): 0, .1 … .4 for the first half, -.5 … -.1 after. */
export const barDelayS = (i: number, n = WAVE_BAR_COUNT) =>
  0.1 * (i < Math.ceil(n / 2) ? i : i - n);

/** One frame of level smoothing, floored to 0.01. */
export const smoothLevel = (prev: number, level: number) =>
  Math.floor((0.85 * prev + 0.15 * level) * 100) / 100;

/** Waveform scale from the smoothed level: never below 1, gain 5. */
export const audioScale = (smoothed: number) => Math.max(1, 5 * smoothed);

/** Collapse the backend's 16 FFT buckets into one 0..1 level. */
export const levelFromBuckets = (buckets: readonly number[]) => {
  let max = 0;
  for (const value of buckets) {
    if (Number.isFinite(value) && value > max) max = value;
  }
  return Math.min(1, Math.max(0, max));
};

/** A second click on the bar this soon after it started dictation cancels. */
export const RECLICK_CANCEL_MS = 500;

/** The pill's distance from the screen edge inside the fixed window. */
export const EDGE_MARGIN_PX: Record<BarEdge, number> = {
  bottom: 14,
  left: 14,
  right: 14,
  top: 12,
};

/** The fixed overlay window (macOS): bottom/top vs. side edges. */
export const BAR_WINDOW_SIZE: Record<"horizontal" | "vertical", [number, number]> =
  {
    horizontal: [440, 320],
    vertical: [490, 440],
  };
