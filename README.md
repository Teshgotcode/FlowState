# FlowState

Diktering som aldri forlater maskinen.

Trykk en tast, snakk, få teksten der markøren sto. Gratis, åpen kildekode, og alt skjer på maskinen din.

Laget for Entreprenørskolen ved NTNU. FlowState funker for alle som vil diktere på norsk uten å sende lyden til noens server. Appen er en fork av [Handy](https://github.com/cjpais/Handy) av CJ Pais, med norske standarder og ES-ordliste innebygd.

## Hvorfor denne i stedet for skytjenestene?

Vi reverse-engineernet Wispr Flow 1.6.1174 for å være sikre på forskjellen. Appen deres koder lyden din til Opus lokalt og strømmer den til `api.wisprflow.ai` for transkripsjon, og de har tre analysestacker innebygd: Sentry, PostHog og Amplitude.

FlowState sender ingenting om deg.

- Lyd, tekst og metadata blir aldri sendt noe sted. Ingen konto, ingen telemetri, ingen krasjrapporter.
- Den eneste utgående forespørselen er oppdateringssjekken: appen spør GitHub om det finnes en nyere versjon. Den sender ingen identifikator, ingen lyd og ingen tekst — og kan slås helt av i Innstillinger.
- Historikken ligger i en lokal database på maskinen din, og kan slås helt av.
- Modellen lastes ned én gang fra Hugging Face, deretter funker alt uten nett.

## Norsk ut av boksen

- Norsk grensesnitt (bokmål) følger med, engelsk finnes også.
- Bokmål og nynorsk støttes av modellene, med automatisk språkdeteksjon.
- ES-ordlista er forhåndslastet: Entreprenørskolen, Solan, gründer, aksjekapital, kundevalidering, business model canvas og mer. Ordlista kan redigeres i innstillingene, og hjelper modellen med å stave navn og fagtermer riktig.

## Modeller

Forvalget er `whisper-large-v3-turbo` (0,89 GB). Den fikk 6,1 % feil på norsk og 1,0 % på engelsk i vår egen test, og bruker rundt ett sekund på fem sekunder tale. Vil du ha raskere engelsk, velg Parakeet. Metoden og alle tallene ligger i [docs/model-benchmark.md](docs/model-benchmark.md).

## Slik funker det

1. Trykk snarveien (forvalg: hold + slipp), og snakk
2. VAD (Silero) filtrerer stillheten, Whisper eller Parakeet V3 gjør lyden om til tekst, lokalt
3. Teksten limes inn i appen du var i

Vil du ha mer enn rå tekst? Etterbehandling kan kjøre en lokal språkmodell over det du sa: «Rydd notatet» fikser tegnsetting og fyllord, «Møtepunkter» lager sammendrag og tiltaksliste fra en gruppetime. På Mac med Apple Silicon brukes Apple Intelligence direkte på maskinen. På Windows setter du opp [Ollama](https://ollama.com) en gang, så peker FlowState på `localhost:11434`. Skytjenester kan slås på manuelt om du vil, men de er aldri på som standard.

## CLI: møtenotatene inn i Claude og ChatGPT

Wispr lar notatene dine brukes i Claude og ChatGPT, via skyen sin. FlowState gir deg det samme uten sky: fire lesekommandoer i samme binær som appen, som leser FlowStates egne data.

Hvorfor CLI og ikke MCP: hvert MCP-verktøyskjema sendes i hver eneste forespørsel, mens en kommando koster nesten ingenting. Anthropic målte en arbeidsflyt til 150 000 tokens via verktøyskjema mot 2 000 for samme jobb med kodebasert kall.

```bash
flowstate meetings --limit 20            # møter, nyeste først
flowstate meeting 42                     # ett møte med transkripsjon og notat
flowstate transcript 42                  # ren transkripsjon, møte eller diktering
flowstate search "regnskap" --limit 20   # søk i titler og tekster
```

`--json` på alle fire gir maskinlesbar output. Kommandoene ligger i den installerte appen:

```bash
~/Applications/FlowState.app/Contents/MacOS/flowstate meetings --json
```

**Koble til Claude eller ChatGPT:** åpne **Innstillinger → Integreringer** og trykk «Kopier til Claude». Blokken du får er ferdig å lime inn, med riktig sti på din maskin:

```
Jeg bruker FlowState lokalt. Du kan lese møtene mine med disse kommandoene:
  <binærsti> meetings --limit 20 --json
  <binærsti> meeting <id> --json
  <binærsti> transcript <id> --json
  <binærsti> search "<ord>" --json
Bruk dem når jeg spør om møtene mine. Alt er lokalt.
```

Databasen åpnes skrivebeskyttet (`SQLITE_OPEN_READ_ONLY`), og det finnes ingen skrivekommandoer. CLI-et kan ikke endre historikken. Mer i [docs/cli.md](docs/cli.md).


## Krav

- macOS 14+ (Apple Silicon anbefalt) eller Windows
- Første oppstart laster ned en modell (0,5–1,5 GB) fra Hugging Face. Deretter går alt uten nett.

## Bygg fra kildekode

```bash
bun install
bun run tauri dev     # utviklingskjøring
bun run tauri build   # DMG/MSI
```

Du trenger [Bun](https://bun.sh), Rust (nyeste stable) og CMake.

## Veikart

- **v1.0** Rebrand, norsk i18n, ES-ordliste, lokal LLM som standardvalg, oppdateringsvarsel på som standard
- **v1.1** Møtemodus: langopptak av gruppetime, transkriptvindu, punkter og tiltak via lokal modell
- **v1.2** Eksport til contac (MCP) for kontaktloggen i ES

## Ære der æren hører hjemme

FlowState fantes ikke uten [Handy](https://github.com/cjpais/Handy) (MIT) av CJ Pais og fellesskapet rundt den, og ikke uten transcribe-cpp, ggml og Whisper. Handy er under feature freeze oppstrøms, så ES-tilpasningene lever her i fork-en. Oppstrøms fikser haler vi inn jevnlig.

Møteopptak med systemlyd (maskinens egen lyd, f.eks. Zoom-møtets andre ende) følger oppsettet til [anarlog](https://github.com/milirsch/anarlog) (MIT): en CoreAudio-prosesstap i et privat aggregat, lest via [cidre](https://github.com/yury/cidre) (MIT). Ingen virtuell lyddriver, ingen sidecar-prosess, ingen tillatelsesdialog — se `NOTICE` og `research/2026-10-27-systemlyd-tcc.md`.

## Lisens

MIT, samme som oppstrøms. Se `LICENSE`.
