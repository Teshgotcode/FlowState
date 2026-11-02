# Hvor «Handy» fortsatt står — og hvorfor

FlowState er en fork av [cjpais/Handy](https://github.com/cjpais/Handy) (MIT). Etter
rebranden skal brukeren aldri møte «Handy» i grensesnittet, men navnet må fortsatt finnes
der det er riktig. Denne fila er regelen, og `scripts/check-brand.py` håndhever den.

## Skal stå

| Hvor | Hvorfor |
| --- | --- |
| About-panelet: «Basert på Handy», «FlowState bygger på Handy (MIT) av cjpais og bidragsyterne» | MIT krever attribusjon, og det er sant: dette er en fork. |
| `README.md`, `CONTRIBUTING.md`, `AGENTS.md`, `BUILD.md`, oppstrøms-lenker | Historikk og opphav; filene beskriver forken. |
| `README-upstream.md`, `LICENSE` | Skal være urørt — det er oppstrøms dokumentasjon. |
| `handy_keys`, `HandyKeys*`, `handy-keys`-craten, `handy_recorder` | API- og kratenavn som bindings og lagrede innstillinger avhenger av. Å døpe om ville bryte dem. |
| `HANDY_*`-miljøvariabler | Bakoverkompatible aliaser; `FLOWSTATE_*` er den nye staven som leses først. |
| `handy.theme`, `handy.onboarding`, `handy-file-*.wav` | Lagrede nøkler og filnavn fra før rebranden; må leses som før, ellers mister brukere innstillinger og historikk. |
| `@handy-computer/models`, `handy.computer`-URL-er | Modellregisteret og dokumentasjonslenkene peker på reelle oppstrømsressurser. |
| `cjpais/Handy#1940` og lignende | Oppstrøms issue-referanser i kodekommentarer. |

## Skal ikke stå

Brukervendt tekst i `src/i18n/locales/**` og i TSX-literaler: knapper, etiketter,
hjelpetekster, feilmeldinger, onboarding. Der heter produktet FlowState.

S-9 fant én ekte rest: debug-innstillingen for tastaturimplementasjon het «Handy Keys» i
UI-et (verdien `handy_keys` er urørt). Den heter nå «FlowState Keys».

## Sjekk

```bash
python3 scripts/check-brand.py
# no user-facing 'Handy' strings — attribution and identifiers only
```

Skriptet leser alle locale-filer (verdier, ikke nøkler) og alle `.tsx`-literaler, og
tillater bare attribusjonsblokken og de tekniske unntakene over. Exit 1 ved treff, så det
kan henge i CI eller i gaten senere.
