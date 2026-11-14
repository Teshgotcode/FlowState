// Contrast self-check for the colour themes. The whole point of the pastel
// palettes is that they stay readable, so these assertions are the contract:
// body text 7:1, secondary ink and accent text 4.5:1, tertiary ink 4:1 on the
// canvas and 4.5:1 on the panel, the focus ring 3:1, and the pastel fill
// carries dark text.
import assert from "node:assert/strict";
import {
  ACCENTS,
  ACCENT_OPTIONS,
  contrast,
  customAccent,
  mix,
  type Palettes,
} from "./accent";

const check = (mode: string, name: string, p: Palettes) => {
  // --color-focus is the strong accent at 75% over whatever it rings.
  const ring = (bg: string) => contrast(mix(p.deep, bg, 0.75), bg);
  const pairs: [string, number, number][] = [
    ["text/bg", contrast(p.text, p.bg), 7],
    ["text/surface", contrast(p.text, p.surface), 7],
    ["text/surfaceSoft", contrast(p.text, p.surfaceSoft), 7],
    ["ink2/bg", contrast(p.ink2, p.bg), 4.5],
    ["ink2/surfaceSoft", contrast(p.ink2, p.surfaceSoft), 4.5],
    ["ink3/bg", contrast(p.ink3, p.bg), 4],
    ["ink3/surface", contrast(p.ink3, p.surface), 4.5],
    ["deep/surface", contrast(p.deep, p.surface), 4.5],
    ["deep/accentWash", contrast(p.deep, p.accentWash), 4.5],
    ["text/accentWash", contrast(p.text, p.accentWash), 7],
    ["onAccent/accent", contrast(p.onAccent, p.accent), 4.5],
    ["focus/bg", ring(p.bg), 3],
    ["focus/surface", ring(p.surface), 3],
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

assert.equal(ACCENT_OPTIONS.length, 7, "six presets plus custom");
console.log("accent themes: all contrast assertions passed");
