/**
 * Colour themes: six pastels plus a custom hue, applied as CSS variables on
 * the document root. `theme.ts` still owns light/dark; this only repaints the
 * palette underneath it.
 *
 * Every theme comes out of one generator (`pastelTheme`) working in OKLCH:
 * each role has a fixed lightness and chroma, the same in every theme, and
 * only the hue (plus a small chroma trim for the quieter hues) changes. That
 * keeps the themes siblings, and it keeps contrast the same in all of them:
 * pastel lives on the surfaces and tints, the ink stays dark (or light, in
 * dark mode). `accent.test.ts` enforces the contrast floor.
 *
 * Ids are the stored values from earlier releases and never change; the
 * labels can. `rod` is the peach theme and `gul` the sand theme.
 */

export type AccentId =
  | "bla"
  | "lilla"
  | "rosa"
  | "rod"
  | "gul"
  | "gronn"
  | "egen";

export interface Palettes {
  /** Canvas: window background, sidebar. */
  bg: string;
  /** Content panel, cards, inputs. */
  surface: string;
  /** Inset groups and quiet fills inside the panel. */
  surfaceSoft: string;
  /** Primary ink: body text, headings. */
  text: string;
  /** Secondary ink: descriptions, labels, inactive rows. */
  ink2: string;
  /** Tertiary ink: captions, timestamps, hints, idle icons. */
  ink3: string;
  /** Disabled ink. Decorative only; no contrast promise. */
  ink4: string;
  /** Hairline divider (alpha, sits on any surface). */
  line: string;
  /** Control edges: inputs, selects, secondary buttons (alpha). */
  lineStrong: string;
  /** Hover fill (alpha). */
  wash: string;
  /** Pastel fill: primary actions, toggles on, chart bars. */
  accent: string;
  /** Soft accent tint behind accent content (banners, active fields). */
  accentWash: string;
  /** Strong accent: accent text, icons, focus rings, links. */
  deep: string;
  /** Data marks: chart bars and heatmap cells (used with alpha steps). */
  chart: string;
  /** Text drawn on top of `accent`. */
  onAccent: string;
  /** Tint for shadows, so they are never pure black. */
  shadow: string;
}

export const ACCENT_OPTIONS: AccentId[] = [
  "bla",
  "lilla",
  "rosa",
  "rod",
  "gul",
  "gronn",
  "egen",
];

const ACCENT_STORAGE_KEY = "flowstate.accent";
const HUE_STORAGE_KEY = "flowstate.accentHue";

export interface AccentTheme {
  light: Palettes;
  dark: Palettes;
}

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

/** `#rrggbb` plus an alpha byte, for the alpha tokens (lines, hover wash). */
export const withAlpha = (hex: string, alpha: number): string =>
  hex +
  Math.round(Math.min(1, Math.max(0, alpha)) * 255)
    .toString(16)
    .padStart(2, "0");

/** Resolve an `#rrggbbaa` token over an opaque background. */
export const flatten = (color: string, bg: string): string =>
  color.length === 9
    ? mix(color.slice(0, 7), bg, parseInt(color.slice(7), 16) / 255)
    : color;

const oklchToLinear = (l: number, c: number, h: number): number[] => {
  const rad = (h * Math.PI) / 180;
  const a = c * Math.cos(rad);
  const b = c * Math.sin(rad);
  const l_ = (l + 0.3963377774 * a + 0.2158037573 * b) ** 3;
  const m_ = (l - 0.1055613458 * a - 0.0638541728 * b) ** 3;
  const s_ = (l - 0.0894841775 * a - 1.291485548 * b) ** 3;
  return [
    4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
    -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
    -0.0041960863 * l_ - 0.7034186147 * m_ + 1.707614701 * s_,
  ];
};

const gamma = (c: number) =>
  c <= 0.0031308 ? 12.92 * c : 1.055 * Math.pow(c, 1 / 2.4) - 0.055;

/** OKLCH to sRGB hex. Out-of-gamut colours lose chroma, never lightness. */
export const oklch = (l: number, c: number, h: number): string => {
  const inGamut = (rgb: number[]) =>
    rgb.every((v) => v >= -1e-4 && v <= 1 + 1e-4);
  let rgb = oklchToLinear(l, c, h);
  if (!inGamut(rgb)) {
    let lo = 0;
    let hi = c;
    for (let i = 0; i < 20; i++) {
      const mid = (lo + hi) / 2;
      if (inGamut(oklchToLinear(l, mid, h))) lo = mid;
      else hi = mid;
    }
    rgb = oklchToLinear(l, lo, h);
  }
  return toHex(rgb.map((v) => gamma(Math.min(1, Math.max(0, v)))));
};

/* ── the generator ───────────────────────────────────────────────────────── */

/** [lightness, chroma] per role. The same table drives every theme. */
type Role = Exclude<keyof Palettes, "line" | "lineStrong" | "wash">;
const STEPS: Record<"light" | "dark", Record<Role, [number, number]>> = {
  light: {
    bg: [0.958, 0.017],
    surface: [0.993, 0.005],
    surfaceSoft: [0.973, 0.013],
    accentWash: [0.947, 0.034],
    accent: [0.86, 0.075],
    chart: [0.72, 0.1],
    deep: [0.5, 0.115],
    onAccent: [0.25, 0.035],
    text: [0.245, 0.022],
    ink2: [0.44, 0.025],
    ink3: [0.54, 0.024],
    ink4: [0.72, 0.016],
    shadow: [0.3, 0.035],
  },
  dark: {
    bg: [0.195, 0.015],
    surface: [0.24, 0.018],
    surfaceSoft: [0.27, 0.02],
    accentWash: [0.305, 0.04],
    accent: [0.8, 0.07],
    chart: [0.78, 0.075],
    deep: [0.84, 0.075],
    onAccent: [0.23, 0.03],
    text: [0.945, 0.01],
    ink2: [0.8, 0.016],
    ink3: [0.69, 0.018],
    ink4: [0.52, 0.016],
    shadow: [0.12, 0.012],
  },
};

/** Alpha of the primary ink for the three structural tints. */
const ALPHAS = {
  light: { line: 0.085, lineStrong: 0.16, wash: 0.045 },
  dark: { line: 0.1, lineStrong: 0.18, wash: 0.06 },
} as const;

const palette = (
  mode: "light" | "dark",
  hue: number,
  chroma: number,
): Palettes => {
  const steps = STEPS[mode];
  const roles = Object.fromEntries(
    Object.entries(steps).map(([role, [l, c]]) => [
      role,
      oklch(l, c * chroma, hue),
    ]),
  ) as Record<Role, string>;
  const alphas = ALPHAS[mode];
  return {
    ...roles,
    line: withAlpha(roles.text, alphas.line),
    lineStrong: withAlpha(roles.text, alphas.lineStrong),
    wash: withAlpha(roles.text, alphas.wash),
  };
};

/** One hue in, a light and a dark palette out. */
export const pastelTheme = (hue: number, chroma = 1): AccentTheme => ({
  light: palette("light", hue, chroma),
  dark: palette("dark", hue, chroma),
});

/** Preset hues in OKLCH degrees, and a chroma trim for the quieter ones. */
export const PRESET_HUES: Record<
  Exclude<AccentId, "egen">,
  [number, number]
> = {
  bla: [250, 1], // sky
  lilla: [295, 1], // lavender
  rosa: [355, 1], // rose
  rod: [45, 1], // peach
  gul: [85, 0.85], // sand
  gronn: [155, 0.9], // sage
};

export const ACCENTS: Record<
  Exclude<AccentId, "egen">,
  AccentTheme
> = Object.fromEntries(
  Object.entries(PRESET_HUES).map(([id, [hue, chroma]]) => [
    id,
    pastelTheme(hue, chroma),
  ]),
) as Record<Exclude<AccentId, "egen">, AccentTheme>;

/** A custom theme is the same generator at any hue. */
export const customAccent = (hue: number): AccentTheme => pastelTheme(hue);

/* ── apply ───────────────────────────────────────────────────────────────── */

const CSS_VARS = (
  p: Palettes,
  mode: "light" | "dark",
): Record<string, string> => ({
  [`--${mode}-color-text`]: p.text,
  [`--${mode}-color-background`]: p.bg,
  [`--${mode}-color-surface`]: p.surface,
  [`--${mode}-color-surface-soft`]: p.surfaceSoft,
  [`--${mode}-color-ink-2`]: p.ink2,
  [`--${mode}-color-ink-3`]: p.ink3,
  [`--${mode}-color-ink-4`]: p.ink4,
  [`--${mode}-color-mid-gray`]: p.ink3,
  [`--${mode}-color-line`]: p.line,
  [`--${mode}-color-line-strong`]: p.lineStrong,
  [`--${mode}-color-wash`]: p.wash,
  // Accent text, icons, focus rings and control borders.
  [`--${mode}-color-logo-primary`]: p.deep,
  [`--${mode}-color-background-ui`]: p.accent,
  [`--${mode}-color-accent-wash`]: p.accentWash,
  [`--${mode}-color-chart`]: p.chart,
  [`--${mode}-color-on-accent`]: p.onAccent,
  [`--${mode}-color-shadow`]: p.shadow,
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
    const raw = localStorage.getItem(HUE_STORAGE_KEY);
    // Number(null) is 0, a valid hue, so a missing key must be ruled out first.
    const stored = raw === null ? NaN : Number(raw);
    if (Number.isFinite(stored) && stored >= 0 && stored <= 360) return stored;
  } catch {
    // ignore
  }
  return 264;
};
