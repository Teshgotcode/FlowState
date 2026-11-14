# Design direction: soft and native

The app should read like a Mac app, not a web dashboard in a frame. Functionality,
flows and copy stay as they are; this file is about surface and feel.

## The five rules

1. **The system face sets everything.** `--flow-font-ui` and
   `--flow-font-display` both start with `-apple-system`, so macOS renders SF Pro.
   Figtree stays as the fallback for platforms without a system UI face. No serif
   titles: a settings pane does not switch typeface for its heading. EB Garamond
   is still bundled for the wordmark.
2. **Sentence case, size and weight do the hierarchy.** No uppercase, no wide
   tracking. A label is 12px at 560 weight in `ink-2`; a heading earns attention by
   being bigger, not by shouting. `uppercase tracking-[0.0Xem]` is not used
   anywhere in `src/components`.
3. **One rounding scale per job.** `--flow-radius-panel` 26 (the content panel),
   `card` 16, `control` 11, `chip` 8. Never a one-off radius.
4. **Shadows are light, wide and never black.** `--flow-shadow-card/panel/pop`
   are ink-tinted at low alpha. Cards barely lift; the content panel floats over
   the canvas; popovers get the only real shadow.
5. **Motion is a spring, not a bounce.** `--flow-ease-spring` with
   `--flow-dur-quick` (130ms) for anything under the pointer, `--flow-dur-base`
   (190ms) for things that appear. Buttons press to `scale(0.985)`. Nothing else
   animates.

## What that changed, concretely

- **Sidebar**: no border, no background of its own (the canvas shows through), no
  accent bar on the active row. Selection is a filled `text/[0.07]` rounded row,
  the way Finder and Notes do it. Icons go from `ink-3` to `text` and 1.8 to 2.0
  stroke when active.
- **Content panel**: one `--flow-radius-panel` round on all corners plus the panel
  shadow, instead of the old asymmetric `24px/16px` with a hard edge.
- **Buttons**: 11px radius, 560 weight, spring press, `focus-visible` ring at 3px
  tinted. Primary keeps the pastel accent fill (the accent picker owns that) with a
  hairline of the deep tone so it does not read as disabled.
- **Inputs**: white surface instead of a grey wash, hairline border, and a soft
  3px focus ring.
- **Home tip**: was a gradient promo card with a serif headline and a filled CTA.
  Now a quiet tinted strip: sparkle, two lines, a text link, a dismiss. It must not
  out-shout the mic field under it.
- **Transcript rows**: no per-row rules; a soft hover tint and a rounded row.
  Row actions appear on hover and on keyboard focus instead of sitting on every
  row like a table.
- **Right rail**: no drop shadow, translucent surface, 26px tabular numerals and
  13px labels.

## What not to do

- Do not add gradients, glows or glassmorphism.
- Do not put a filled primary button on a promotional card.
- Do not re-introduce uppercase micro-labels "for scannability".
- Do not animate more than transform and opacity; no spring overshoot.
- Do not hardcode a radius or shadow that the scale already covers.

## Where the tokens live

`src/styles/theme.css` (`--flow-*`), consumed by Tailwind arbitrary values like
`rounded-[var(--flow-radius-card)]` and `shadow-[var(--flow-shadow-card)]`. The
shared surfaces are in `components/hub/Primitives.tsx` and `components/ui/`.

The preview fixture must use production ids and production ordering
(`preview/main.tsx` sorts entries newest first), or a screenshot will show a bug
the app does not have.
