# FlowState

Diktering som aldri forlater maskinen.

Trykk en tast, snakk, få teksten der markøren sto. Gratis, åpen kildekode, og alt skjer på maskinen din.

Laget for Entreprenørskolen ved NTNU. FlowState funker for alle som vil diktere på norsk uten å sende lyden til noens server. Appen er en fork av [Handy](https://github.com/cjpais/Handy) av CJ Pais, med norske standarder og ES-ordliste innebygd.

## Hvorfor denne i stedet for skytjenestene?

Vi reverse-engineernet Wispr Flow 1.6.1174 for å være sikre på forskjellen. Appen deres koder lyden din til Opus lokalt og strømmer den til `api.wisprflow.ai` for transkripsjon, og de har tre analysestacker innebygd: Sentry, PostHog og Amplitude.

FlowState sender ingenting. Noen gang.

- Lyd, tekst og metadata blir aldri sendt noe sted. Ingen konto, ingen telemetri, ingen krasjrapporter.
- Oppdateringssjekk er av som standard.
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

- **v1.0** Rebrand, norsk i18n, ES-ordliste, lokal LLM som standardvalg, oppdateringssjekk av
- **v1.1** Møtemodus: langopptak av gruppetime, transkriptvindu, punkter og tiltak via lokal modell
- **v1.2** Eksport til contac (MCP) for kontaktloggen i ES

## Ære der æren hører hjemme

FlowState fantes ikke uten [Handy](https://github.com/cjpais/Handy) (MIT) av CJ Pais og fellesskapet rundt den, og ikke uten transcribe-cpp, ggml og Whisper. Handy er under feature freeze oppstrøms, så ES-tilpasningene lever her i fork-en. Oppstrøms fikser haler vi inn jevnlig.

## Lisens

MIT, samme som oppstrøms. Se `LICENSE`.
