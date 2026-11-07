# Merket

Valgt 6. november 2026: **Terskelen**, variant 03 «Bred» fra `replica/logo-terskelen.html`.

To piller på samme gulv, ulik høyde. Idéen er en tilstandsendring, ikke en lydstolpe:
den høyre stolpen er kortere fordi den er på vei inn i en annen tilstand, ikke fordi den
er en tone i en equalizer.

## Geometri (24-rutenett)

| Stolpe | x | y | bredde | høyde | radius |
| --- | --- | --- | --- | --- | --- |
| Venstre | 4,6 | 6,8 | 5,6 | 12,4 | 2,8 |
| Høyre | 14,0 | 11,6 | 5,6 | 7,6 | 2,8 |

Gap er 3,8 enheter. Begge har full pille-radius (r = bredde / 2), samme som resten av
appens avrundingsspråk.

## Filer

- `src/components/icons/FlowStateMark.tsx`: selve merket, `fill="currentColor"`.
- `replica/app-icon.svg`: 1024-rute med squircle (superellipse, n = 5) i jade og merket i papir.
- `src-tauri/icons/`: generert med `bunx tauri icon replica/app-icon.svg`.

## Regler

- Merket er monokromt. Farge kommer fra `currentColor`, aldri fra en hardkodet hex.
- Minste størrelse er 16 px. Under det faller gapet sammen.
- Sidestolpen bruker 26 px, app-ikonet 40x skala i 1024-ruten.

## Forkastet

- «Pausen» (to like prikker) leste som tre prikker i UI.
- «Speilet» (to trekanter) mistet det negative rommet på 22 px.
- «Heng» og «Trapp» var sterkere som statement, men skjøre i små størrelser.
