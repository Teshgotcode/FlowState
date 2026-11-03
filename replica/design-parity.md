# design-parity — «nokså lik, men ikke altfor lik»

Målet fra S-26: samme informasjonsarkitektur og samme plassering av kontroller
som referanseappen, men FlowStates eget utseende. Dette dokumentet er avviks-
regnskapet; skjermkartet ligger i `replica/recon.md`.

## Metode

`replica-diff/imgdiff.py` sammenligner strukturblokker (rolle + normalisert
rektangel, 1000×1000 per skjerm) mellom referansekart
(`replica/wireframes/ref/`) og våre kart (`replica/wireframes/flow/`).
Målet er fargeblindt: farge, typografi, ikoner og tekst inngår ikke — kun
plassering av regioner. Poengsummen er arealveiet IoU per referanserolle:

    score = 100 · Σ(IoU_r · areal_r) / Σ(areal_r)

Vindusrammen er utelatt fra begge kart (begge apper har et vindu), og blokker
merket `"kind": "own"` finnes bare hos oss — de kan aldri øke poengsummen.
Målband: 75–90 %. Over 90 er for likt, under 75 er for fremmed.

## Målt per skjerm (kørt 2025, `python3 replica-diff/imgdiff.py --batch replica/wireframes`)

| Skjerm | Poeng | I bånd | Samme IA (rollene)                | Vårt eget |
| ------ | ----- | ------ | --------------------------------- | --------- |
| S01 Onboarding: velkomst | 87.5 | ✓ | topplinje → illustrasjon → overskrift → undertekst → CTA → fot | maskot + eget steg-fremdriftsdots i topplinjen, bredere tittelblokk |
| S02 Onboarding: tillatelser | 89.4 | ✓ | tittel → mikrofonkort → tilgangskort → fot | kortene er helbredde med egen avrunding; tittel høyere |
| S03 Onboarding: modell | 86.3 | ✓ | tittel → kort A → kort B → lagringsmerknad → fot | to kort med eget gap og egen kortavrunding (14 px) |
| S04 Onboarding: prøv den | 87.2 | ✓ | overskrift → demo → snarvei-hint → fot | egen tegnet laptop-demo (bredere, lavere) i stedet for vindu-mock |
| S05 Hjem | 85.3 | ✓ | sidestolpe → tittel → modellvelger → innhold → statuslinje | ikon-sidestolpe (w-40) med eget ordmerke; statuslinje med møteindikator |
| S06 Historikk | 85.0 | ✓ | sidestolpe → tittel → søk → liste → statuslinje | samme som S05 + egen liste-innrulling |
| S07 Innstillinger: generelt | 84.8 | ✓ | sidestolpe → tittel → innstillingsliste → statuslinje | `SettingsGroup`-seksjonstitler inline i listen; ingen egen seksjons-stripe |
| S08 Innstillinger: modeller | 84.8 | ✓ | sidestolpe → tittel → tabell → statuslinje | egen tabell-innrulling og kort-hover |
| S09 Innstillinger: integrasjoner | 84.8 | ✓ | sidestolpe → tittel → liste → statuslinje | som S07 |
| S10 Opptaksoverlegg | 79.9 | ✓ | bølgeform → tidtaker → hint → stopp | egen pill-geometri: bredere bølgeform, egen stopp-affordance |
| S11 Møte-linje | 79.1 | ✓ | tittel → tidtaker → kontroller | egen dock-kant (statusDockEdge) og egen kontrollrekkefølge |
| S12 Hurtignotat | 78.2 | ✓ | tittel → tekstfelt → lukk | native vinduskrom (ingen egen tittellinje), innrullet tekstfelt, egen kopi-fot |
| **snitt** | **84.4** | ✓ | | |

Alle 12 skjermer i 75–90-båndet; snitt 84.4 ligger midt i. Ingenting over 90
(for likt) eller under 75 (for fremmed).

## Hva som matcher referansen

- **Informasjonsarkitektur:** hver skjerm i `replica/recon.md` finnes hos oss
  med samme formål og samme hovedhandling; rollerekkefølgen per skjerm er den
  samme (se tabellen over).
- **Kontrollplassering:** primærhandling nederst til venstre i onboarding-foten
  (S01–S04), modellvelger/søk øverst til høyre (S05–S06), statuslinje i bunn
  av hovedvinduet (S05–S09), stopp-affordance nederst i overlegget (S10).
- **Tilstander:** optagende/idle, tillatelse gitt/ikke gitt, modell lastet/
  ikke lastet, møte pågår/avsluttet — samme tilstandsmengde per skjerm.

## Hva som er FlowStates eget

- **Palett:** uendret — alle farger går fortsatt gjennom `--flow-*`-tokenene i
  `src/styles/theme.css` (navy/blå/action/accent). Ingen referansefarger.
- **Typografi:** eget par, bundet lokalt (SIL OFL, ingen nettverkskall):
  *Bricolage Grotesque* (display, 400–800) + *Instrument Sans* (UI, 400–700),
  latin + latin-ext som variabel woff2 i `src/assets/fonts/`; alle andre
  skript faller tilbake på systemstabelen som før. Token:
  `--flow-font-display` / `--flow-font-ui`, satt som standard i `App.css`
  `@layer base` og brukt av overlegg og hurtignotat.
- **Ikoner:** Lucide-settet (allerede i bruk); eget ordmerke
  (`FlowStateLogo`), ingen referanselogikoener eller -illustrasjoner.
- **Avrundingsskala:** `--flow-radius-panel/card/control/chip` =
  20/14/10/7 px — rundere enn referansens 8–12, én verdi per jobb
  (`--radius-*` i App.css peker på skalaen).
- **Skyggeskala:** `--flow-shadow-card/panel/pop` — navy-tonet og myk (aldri
  hard svart skygge); overlegget flyter på `--flow-shadow-pop`, onboarding-
  panelet løftes med `--flow-shadow-panel`.
- **Bevegelse:** uendret `--ease-glide` + egne animasjoner (wave/slap/burst/
  rise/drop) — ingen referansebevegelser kopiert.
- **Eget krom:** steg-fremdrift i onboarding-topplinjen, ordmerke i
  sidestolpen, kopi-fot i hurtignotatet, tegnet laptop-demo, møteindikator i
  statuslinjen.

## Ingen kopiering

`replica-brand/sweep.py` søker gjennom app-kode og assets (inkludert
filnavn) etter referansenavn, produktnavn, deres skrifttyper og domener:
**0 funn**. Referansens logo, ikoner, illustrasjoner og tekst finnes ikke i repoet; alle
kommentarer som tidligere nevnte referansen er omskrevet til atferdsbeskrivelser.
