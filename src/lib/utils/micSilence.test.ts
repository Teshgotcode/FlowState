import assert from "node:assert/strict";
import { MIC_SILENCE_EPSILON, micHasSound } from "./micSilence";

// A muted mic delivers exact zeros.
assert.equal(micHasSound([0, 0, 0, 0]), false);
assert.equal(micHasSound(Array(16).fill(0)), false);

// No level events yet (device delivering nothing) reads as silent.
assert.equal(micHasSound([]), false);

// Float dust stays silent; anything louder counts as sound.
assert.equal(micHasSound([MIC_SILENCE_EPSILON, 0]), false);
assert.equal(micHasSound([MIC_SILENCE_EPSILON / 2, 0]), false);
assert.equal(micHasSound([MIC_SILENCE_EPSILON * 2, 0, 0]), true);
assert.equal(micHasSound([0, 0, 0.4, 0]), true);

console.log("micSilence: all assertions passed");
