/**
 * Accent themes: five pastels plus a custom hue, applied as CSS variables on
 * the document root. `theme.ts` still owns light/dark; this only repaints the
 * palette underneath it.
 *
 * The rule the values follow (and `accent.test.ts` enforces): a pastel fill
 * carries dark text, never white. A deeper tone of the same hue carries accent
 * text, icons and control borders, because a pastel alone has too little
 * contrast against a white panel to read as a button.
 */

export type AccentId = "bla" | "rosa" | "gronn" | "rod" | "gul" | "egen";

export interface Palettes {
  /** Canvas. */
  bg: string;
  /** Cards and inputs. */
  surface: string;
  /** Inset rows, hover fills. */
  surfaceSoft: string;
  /** Body text. */
  text: string;
  /** Pastel fill: primary actions, active states, chart bars. */
  accent: string;
  /** Text-safe tone: accent text, icons, focus rings, control borders. */
  deep: string;
  /** Banner and wash behind accent content. */
  wash: string;
  /** Text drawn on top of `accent`. */
  onAccent: string;
  /** Grey steps (ink-3, line, wash) derive from this. */
  mid: string;
}

export const ACCENT_OPTIONS: AccentId[] = [
  "bla",
  "rosa",
  "gronn",
  "rod",
  "gul",
  "egen",
];

const ACCENT_STORAGE_KEY = "flowstate.accent";
const HUE_STORAGE_KEY = "flowstate.accentHue";

interface AccentTheme {
  light: Palettes;
  dark: Palettes;
}

export const ACCENTS: Record<Exclude<AccentId, "egen">, AccentTheme> = {
  bla: {
    light: {
      bg: "#f6f7fb",
      surface: "#ffffff",
      surfaceSoft: "#e6ebf6",
      text: "#16192b",
      accent: "#b9c8fa",
      deep: "#3b5bdb",
      wash: "#eef2ff",
      onAccent: "#16192b",
      mid: "#6b6d7a",
    },
    dark: {
      bg: "#0e1018",
      surface: "#141824",
      surfaceSoft: "#232a3b",
      text: "#e8ecf7",
      accent: "#a9bcf7",
      deep: "#c7d4fb",
      wash: "#1a2033",
      onAccent: "#0e1018",
      mid: "#7b7e88",
    },
  },
  rosa: {
    light: {
      bg: "#fbf6f8",
      surface: "#fffdfe",
      surfaceSoft: "#f6e7ee",
      text: "#241820",
      accent: "#f7bcd9",
      deep: "#be185d",
      wash: "#fff0f6",
      onAccent: "#241820",
      mid: "#766c72",
    },
    dark: {
      bg: "#140f13",
      surface: "#1b151a",
      surfaceSoft: "#2c222a",
      text: "#f4e9ef",
      accent: "#f4b6cf",
      deep: "#f9cfe1",
      wash: "#2a1c24",
      onAccent: "#140f13",
      mid: "#847c81",
    },
  },
  gronn: {
    light: {
      bg: "#f5f8f5",
      surface: "#fdfefd",
      surfaceSoft: "#e6f0e8",
      text: "#16211a",
      accent: "#a7dfb8",
      deep: "#237032",
      wash: "#ebfbee",
      onAccent: "#16211a",
      mid: "#6b736d",
    },
    dark: {
      bg: "#0d1210",
      surface: "#131a16",
      surfaceSoft: "#212c26",
      text: "#e7f0e9",
      accent: "#a3dcb4",
      deep: "#c6ecd1",
      wash: "#17251c",
      onAccent: "#0d1210",
      mid: "#7a817c",
    },
  },
  rod: {
    light: {
      bg: "#faf6f6",
      surface: "#fffdfd",
      surfaceSoft: "#f6e6e6",
      text: "#241819",
      accent: "#f5b3b3",
      deep: "#c92a2a",
      wash: "#fff5f5",
      onAccent: "#241819",
      mid: "#756c6d",
    },
    dark: {
      bg: "#140f0f",
      surface: "#1b1515",
      surfaceSoft: "#2c2222",
      text: "#f4eaea",
      accent: "#f3b0b0",
      deep: "#f8cfcf",
      wash: "#2a1a1a",
      onAccent: "#140f0f",
      mid: "#847c7c",
    },
  },
  gul: {
    light: {
      bg: "#faf8f2",
      surface: "#fffdf8",
      surfaceSoft: "#f4ecd9",
      text: "#221c14",
      accent: "#f8cf8a",
      deep: "#c2410c",
      wash: "#fff8e8",
      onAccent: "#221c14",
      mid: "#747068",
    },
    dark: {
      bg: "#12100c",
      surface: "#191612",
      surfaceSoft: "#2a251c",
      text: "#f3eee4",
      accent: "#f7cd85",
      deep: "#f8dfae",
      wash: "#241d12",
      onAccent: "#12100c",
      mid: "#827f78",
    },
  },
};

/* ── colour math ─────────────────────────────────────────────────────────── */

const toRgb = (hex: string): [number, number, number] => {
  const h = hex.replace("#", "");
  return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255) as [
    number,
    number,
    number,
  ];
};

const toHex = (rgb: number[]): string =>
  "#" +
  rgb
    .map((c) =>
      Math.round(Math.min(1, Math.max(0, c)) * 255)
        .toString(16)
        .padStart(2, "0"),
    )
    .join("");

const luminance = (hex: string): number => {
  const channel = (c: number) =>
    c <= 0.03928 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
  const [r, g, b] = toRgb(hex).map(channel);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
};

/** WCAG contrast ratio between two hex colours. */
export const contrast = (a: string, b: string): number => {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
};

/** Mix `fg` into `bg` at `alpha` (0..1). */
export const mix = (fg: string, bg: string, alpha: number): string => {
  const f = toRgb(fg);
  const b = toRgb(bg);
  return toHex(f.map((c, i) => c * alpha + b[i] * (1 - alpha)));
};

/** HSL (h in degrees, s/l in percent) to hex. */
export const hsl = (h: number, s: number, l: number): string => {
  const a = (s / 100) * Math.min(l / 100, 1 - l / 100);
  const channel = (n: number) => {
    const k = (n + h / 30) % 12;
    return l / 100 - a * Math.max(-1, Math.min(k - 3, 9 - k));
  };
  return toHex([channel(0), channel(8), channel(4)]);
};

/** First alpha at which text mixed into bg still clears 4.5:1. */
const mutedStep = (text: string, bg: string, target = 4.5): string => {
  for (let a = 0.45; a <= 0.75; a += 0.01) {
    const candidate = mix(text, bg, a);
    if (contrast(candidate, bg) >= target) return candidate;
  }
  return text;
};

/* ── custom theme ────────────────────────────────────────────────────────── */

/** Derive a full light/dark pair from one hue. Contrast-safe by construction. */
export const customAccent = (hue: number): AccentTheme => {
  const light: Palettes = {
    bg: hsl(hue, 32, 98),
    surface: hsl(hue, 40, 99.5),
    surfaceSoft: hsl(hue, 30, 95),
    text: hsl(hue, 26, 11),
    accent: hsl(hue, 72, 78),
    deep: hsl(hue, 62, 42),
    wash: hsl(hue, 85, 96),
    onAccent: hsl(hue, 40, 12),
    mid: "#000000",
  };
  // Darken until accent text clears 4.5:1 on the panel.
  for (
    let l = 42;
    l >= 18 && contrast(light.deep, light.surface) < 4.6;
    l -= 2
  ) {
    light.deep = hsl(hue, 62, l);
  }
  light.mid = mutedStep(light.text, light.bg);

  const dark: Palettes = {
    bg: hsl(hue, 24, 7),
    surface: hsl(hue, 20, 10),
    surfaceSoft: hsl(hue, 18, 14),
    text: hsl(hue, 30, 93),
    accent: hsl(hue, 66, 78),
    deep: hsl(hue, 60, 85),
    wash: hsl(hue, 30, 16),
    onAccent: hsl(hue, 24, 7),
    mid: "#000000",
  };
  dark.mid = mutedStep(dark.text, dark.bg);

  return { light, dark };
};

/* ── apply ───────────────────────────────────────────────────────────────── */

const CSS_VARS = (
  p: Palettes,
  mode: "light" | "dark",
): Record<string, string> => ({
  [`--${mode}-color-text`]: p.text,
  [`--${mode}-color-background`]: p.bg,
  [`--${mode}-color-surface`]: p.surface,
  [`--${mode}-color-surface-soft`]: p.surfaceSoft,
  // Accent text, icons, focus rings and control borders.
  [`--${mode}-color-logo-primary`]: p.deep,
  [`--${mode}-color-background-ui`]: p.accent,
  [`--${mode}-color-on-accent`]: p.onAccent,
  [`--${mode}-color-mid-gray`]: p.mid,
});

/** Repaint the document and remember the choice for the next launch. */
export const applyAccent = (id: AccentId, hue = 264): void => {
  const theme = id === "egen" ? customAccent(hue) : ACCENTS[id];
  const root = document.documentElement;
  for (const [name, value] of [
    ...Object.entries(CSS_VARS(theme.light, "light")),
    ...Object.entries(CSS_VARS(theme.dark, "dark")),
  ]) {
    root.style.setProperty(name, value);
  }
  root.dataset.accent = id;
  try {
    localStorage.setItem(ACCENT_STORAGE_KEY, id);
    localStorage.setItem(HUE_STORAGE_KEY, String(hue));
  } catch {
    // localStorage can be unavailable; boot just falls back to the default.
  }
};

const isAccentId = (value: unknown): value is AccentId =>
  ACCENT_OPTIONS.includes(value as AccentId);

/** Last-applied accent, for synchronous application before React mounts. */
export const getStoredAccent = (): AccentId => {
  try {
    const stored = localStorage.getItem(ACCENT_STORAGE_KEY);
    if (isAccentId(stored)) return stored;
  } catch {
    // ignore
  }
  return "bla";
};

export const getStoredHue = (): number => {
  try {
    const stored = Number(localStorage.getItem(HUE_STORAGE_KEY));
    if (Number.isFinite(stored) && stored >= 0 && stored <= 360) return stored;
  } catch {
    // ignore
  }
  return 264;
};
