# Målt spec fra referanseappen

Hentet 6. november 2026 fra den kjørende Wispr Flow-appen via Chrome DevTools Protocol
(`--remote-debugging-port=9222`), ikke fra skjermbilde. Vinduet var 1350x850 CSS-px ved
DPR 2. Alle tall under er CSS-px, delt på to der rådata var retina.

## Skall

| Element | Verdi |
| --- | --- |
| Vindu | 1350x850, minste 1100x700 |
| Toppstripe | 44 høy, ikoner 28x28 mot høyre |
| Sidestolpe | 216 bred, 12 px innvendig luft |
| Logo-rad | 32 høy, starter 8 px fra toppen |
| Navigasjonsrad | 36 høy, 4 px mellom, 8 px innvendig, 18 px ikon |
| Nav-tekst | 14 px, vekt 550, 8 px mellom ikon og tekst |
| Aktiv rad | fyll `#30302f`, radius 6 |
| Hovedpanel | innfelt 8 px høyre og ned, radius 24 oppe / 16 nede, luft 40 |
| Tittel | 24 px / 600, linjehøyde 32, `letter-spacing -0.02em` |
| Luft under tittel | 24 px |

## Hjem

| Element | Verdi |
| --- | --- |
| Kolonner | 770 innhold + 24 mellomrom + 250 høyrespalte |
| Banner | 770x182, radius 16, luft 32 |
| Banner-tittel | 28 px serif, linjehøyde 28 |
| Banner-brødtekst | 14 px / 20 |
| Banner-knapp | 36 høy, radius 8, luft 8/16, 14 px / 600 |
| Seksjonslabel | 12 px / 600, versaler, `tracking 0.06em`, rad 52 høy |
| Listekort | radius 16, ramme 1 px `#30302f` |
| Rad | tid 98 + tekst 532 + handlinger 138, luft 16 |
| Radtekst | 14 px / 20 |
| Radtid | 12 px / 400 / 20, midtstilt loddrett |
| Handlingsknapp | 28x28, radius 6, 6 px mellom |
| Høyrespalte-kort | 250 bred, radius 16, luft 24 |
| Store tall i spalten | 28 px / 400, linjehøyde 28 |
| Tall-label | 16 px / 400 |

## Innsikt

| Element | Verdi |
| --- | --- |
| Faner | 14 px, aktiv med understrek, skillelinje 1 px under |
| Tre kort | lik høyde, 16 px mellomrom, luft 24 |
| Store tall | 32 px / 600 |
| Små labels | 11-12 px / 600 versaler, `tracking 0.06em` |
| Aktivitetskort | flex-1, horisontale søyler med prosent i søylen |
| Kalenderkort | 380 bred, 7 rader, 10-26 uker avhengig av data |

## Fonter

Figtree (UI) og EB Garamond (display). Begge SIL OFL, hentet fra Google Fonts og
lagt i `src/assets/fonts/` som variable TTF. Lisensfilene ligger ved siden av.

## Fallgruve

Rot-fontstørrelsen i appen er 15 px, ikke 16. Tailwind-utilities som `h-9`, `px-10`,
`leading-5` og `gap-6` regner derfor 6,25 % mindre enn tallet antyder. Alle målte
dimensjoner bruker eksplisitte px-verdier i klassenavnet, for eksempel `h-[36px]`.
