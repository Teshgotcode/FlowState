import assert from "node:assert/strict";
import {
  PILL_SIZE,
  audioScale,
  barDelayS,
  barHeightScale,
  levelFromBuckets,
  sizeFor,
  smoothLevel,
} from "./pillGeometry";

const close = (actual: number, expected: number, eps = 1e-4) =>
  assert.ok(
    Math.abs(actual - expected) <= eps,
    `${actual} is not within ${eps} of ${expected}`,
  );

// Wispr's Flow Bar sizes at the bottom edge (w x h).
assert.deepEqual(PILL_SIZE.resting, [40, 8]);
assert.deepEqual(PILL_SIZE.ready, [50, 30]);
assert.deepEqual(PILL_SIZE.initializing, [73, 30]);
assert.deepEqual(PILL_SIZE.listening_ptt, [73, 30]);
assert.deepEqual(PILL_SIZE.listening_handsfree, [102.5, 30]);
assert.deepEqual(PILL_SIZE.processing, [98, 30]);
assert.deepEqual(PILL_SIZE.error, [91, 30]);
assert.deepEqual(PILL_SIZE.hidden, [40, 8]);

// Side edges swap the axes; top keeps them.
assert.deepEqual(sizeFor("resting", "right"), [8, 40]);
assert.deepEqual(sizeFor("listening_handsfree", "left"), [30, 102.5]);
assert.deepEqual(sizeFor("ready", "top"), [50, 30]);

// Waveform maths.
close(barHeightScale(4), 0.9948);
close(barHeightScale(5), 0.9948);
close(barHeightScale(0), 0.578, 1e-3);
close(barHeightScale(9), 0.578, 1e-3);
assert.equal(barDelayS(0), 0);
close(barDelayS(4), 0.4);
close(barDelayS(5), -0.5);
close(barDelayS(9), -0.1);
assert.equal(audioScale(0), 1);
assert.equal(audioScale(0.5), 2.5);
assert.equal(smoothLevel(0, 1), 0.15);
assert.equal(smoothLevel(0.5, 0.5), 0.5);
// Repeated 0.6 levels settle just under 0.6: the 0.01 floor stops the
// smoother at 0.54 (0.85 * 0.54 + 0.09 = 0.549 → 0.54), so scale ≈ 2.7.
let s = 0;
for (let i = 0; i < 60; i++) s = smoothLevel(s, 0.6);
assert.ok(audioScale(s) >= 2.6 && audioScale(s) <= 3, `scale ${audioScale(s)}`);
assert.equal(levelFromBuckets([0.1, 0.7, 0.2]), 0.7);
assert.equal(levelFromBuckets([]), 0);
assert.equal(levelFromBuckets([2]), 1);

console.log("pillGeometry: all assertions passed");
