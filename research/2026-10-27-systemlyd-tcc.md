# S-5 steg 0: TCC-friksjon for systemlyd på macOS

Spørsmålet: hvilken rute til systemlyd koster minst tillatelsesfriksjon —
CoreAudio process tap (cidre) eller ScreenCaptureKit?

## Måling 2. nov (tidligere spike, gjenoppfrisket kontekst)

- **CoreAudio process tap**: virket **uten noen TCC-dialog**, både fra en bar
  CLI og fra en ad-hoc signert bundle. En prosesstap er en HAL-funksjon, ikke
  en TCC-beskyttet ressurs — ingen oppføring i System Settings → Personvern.
- **ScreenCaptureKit**: avvist uten Skjermopptak-tillatelse; ruten koster en
  dialog + tur til System Settings, og mikrofon-tillatelsen dekker den ikke.

## Gjenmåling i dag (denne maskinen)

Verifisert med `system_audio_tcc_spike` (ignorert test, kjøres med
`cargo test --lib system_audio_tcc_spike -- --ignored --nocapture`):

    macOS: Version 15.1.1 (Build 24B91)          (Darwin 24.1.0; maskinen melder 15.1.1, altså over 14.4)
    ScreenCaptureKit preflight (Screen Recording granted, no dialog): false
    process tap created with no permission dialog: 775776A8-4CA6-424C-86B0-78A21D387F7B
    tap running at 44100 Hz
    process tap captured 0 frames in 3s, peak 0.0000

- **Prosesstap**: opprettes og rives ned uten noen dialog. Det finnes ikke
  engang en preflight å spørre — fordi det ikke er noe å spørre om. Dette er
  målingens kjerne: **Vei A er TCC-fri**.
- **ScreenCaptureKit**: `CGPreflightScreenCaptureAccess() == false` — ruten
  ville krevd en dialog med et menneskelig klikk (i en interaktiv måling er
  det Tesh som klikker); preflighten gir samme svar uten å forstyrre noen.
- **Fangst på denne VM-en**: aggregatet starter (44,1 kHz klokke), men VM-ens
  null-lyddenhet kjører ingen IO-sykluser — 0 frames på 3 sekunder selv om
  `afplay` spiller av (exit 0). Oppstarten av aggregatet tok dessuten over
  60 sekunder her. Begge deler er VM-artefakter (ingen ekte lydmaskinvare);
  2.-nov-målingen på ekte maskin viste at fangsten virker dialogfritt.
  Produktkoden starter tappen på egen tråd etter mikrofonen, slik at et møte
  aldri venter på den.

## Konklusjon

**Vei A: CoreAudio process tap (cidre).** Ingen TCC-dialog, ingen ny
tillatelse, ingen virtuell driver (privat aggregat som dør med opptaket).
ScreenCaptureKit krever Skjermopptakstillatelse og avvises uten den — mer
friksjon, og feil tillatelsestype for oppgaven.

Begrensning, ærlig notert: på denne maskinen kunne ikke selve fangsten
verifiseres ende-til-ende (ingen IO-sykluser fra null-device). Tap-opprettelse
og aggregat-oppsett kompilerer, starter og rives ned uten feil; målingen fra
2. nov på ekte maskin dekker fangstdelen.
