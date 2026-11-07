// Contrast self-check for the accent themes. The whole point of the pastel
// palettes is that they stay readable, so these assertions are the contract:
// body text 7:1, every other pair 4.5:1, and the pastel fill carries dark text.
import assert from "node:assert/strict";
import {
  ACCENTS,
  ACCENT_OPTIONS,
  contrast,
  customAccent,
  type Palettes,
} from "./accent";

const check = (mode: string, name: string, p: Palettes) => {
  const pairs: [string, number, number][] = [
    ["text/bg", contrast(p.text, p.bg), 7],
    ["text/surface", contrast(p.text, p.surface), 7],
    ["muted/bg", contrast(p.mid, p.bg), 4.5],
    ["deep/surface", contrast(p.deep, p.surface), 4.5],
    ["onAccent/accent", contrast(p.onAccent, p.accent), 4.5],
  ];
  for (const [label, value, min] of pairs) {
    assert.ok(
      value >= min,
      `${name} ${mode} ${label} = ${value.toFixed(2)}, needs ${min}`,
    );
  }
};

for (const id of ACCENT_OPTIONS) {
  if (id === "egen") continue;
  const theme = ACCENTS[id];
  check("light", id, theme.light);
  check("dark", id, theme.dark);
}

// A custom theme must hold the same contract at every hue, not just the ones
// that happened to look good on screen.
for (let hue = 0; hue < 360; hue += 15) {
  const theme = customAccent(hue);
  check("light", `hue ${hue}`, theme.light);
  check("dark", `hue ${hue}`, theme.dark);
}

assert.equal(ACCENT_OPTIONS.length, 6, "five presets plus custom");
console.log("accent themes: all contrast assertions passed");
