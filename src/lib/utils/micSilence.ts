/**
 * Silence-at-start detection for the recording overlay (Wispr parity:
 * warn during the take, not after it).
 *
 * Level events are FFT magnitudes from the recorder's visualizer; a muted or
 * missing device delivers exactly zero samples, so digital silence is exactly
 * zero buckets. A small epsilon keeps float dust from counting as sound.
 */
/** Buckets at or below this value count as silence. */
export const MIC_SILENCE_EPSILON = 1e-4;

/** How long the mic may stay silent from dictation start before the warning. */
export const MIC_SILENCE_WARN_MS = 2000;

export function micHasSound(levels: number[]): boolean {
  return levels.some((v) => v > MIC_SILENCE_EPSILON);
}
