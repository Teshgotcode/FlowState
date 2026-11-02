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

## MCP: møtenotater rett inn i Claude Desktop og Cursor

Wispr lar notatene dine brukes i Claude og ChatGPT — via skyen sin. FlowState gir deg det samme uten sky: en MCP-server på din egen maskin, som leser FlowStates egne data.

Slå den på i **Innstillinger → Integrasjoner**. Du velger port, og en tilfeldig token vises én gang — kopier den med én gang. Serveren er per maskin og per bruker (hver bruker starter sin egen prosess mot sin egen app, vi drifter ingen felles server), lytter **kun på 127.0.0.1**, og krever token på hver forespørsel — feil token gir 401. Den leser kun fra historikkdatabasen (SQLite åpnes skrivebeskyttet) og skriver aldri noe. Slått av i innstillingene = ingen prosess, ingen port.

Serveren tilbyr fire verktøy:

| Verktøy | Hva det gjør |
|---|---|
| `list_meetings` | lister opptatte møter, nyest først |
| `get_meeting` | ett møte: transkript, tidsstempler og notatutkast |
| `get_transcript` | transkriptet til et møte eller en diktering, ordrett |
| `search_history` | søk i alle transkripsjoner etter ord eller frase |

### Koble til Claude Desktop (stdio)

Claude Desktop starter serveren selv, som egen prosess på maskinen din. Åpne konfigurasjonen (Claude Desktop → Settings → Developer → Edit Config) og legg til:

```json
{
  "mcpServers": {
    "flowstate": {
      "command": "/Applications/FlowState.app/Contents/MacOS/flowstate-mcp",
      "args": ["--stdio"]
    }
  }
}
```

Binæren ligger ved siden av app-binæren. Uten `--db` finner den historikkdatabasen selv — `~/Library/Application Support/com.teshgotcode.flowstate/history.db` på macOS, `%APPDATA%\com.teshgotcode.flowstate\history.db` på Windows. Bygger du fra kildekode, ligger den i byggmappen din; da kan du også peke eksplisitt med `--db`.

### Koble til Cursor (localhost HTTP)

Når serveren er påslått i FlowState, kobler du deg til den som allerede kjører. I Cursor: Settings → MCP → Add server:

```json
{
  "mcpServers": {
    "flowstate": {
      "url": "http://127.0.0.1:7869/mcp",
      "headers": { "Authorization": "Bearer <din-token>" }
    }
  }
}
```

Bytt ut porten mot din egen, og lim inn tokenen fra Innstillinger → Integrasjoner (den vises bare én gang; «Lag en ny token» lager en ny om du mistet den). Enhver MCP-klient som kan HTTP, kobler til på samme måte — Claude Desktop inkludert.

### Test serveren uten en klient

```bash
# HTTP: 401 uten token, 200 med riktig token
curl -X POST http://127.0.0.1:7869/mcp -H "Authorization: Bearer <din-token>" -d '{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"list_meetings","arguments":{}}}'

# stdio: én JSON-linje inn, én svarlinje ut
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | flowstate-mcp --stdio
```

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

## Lisens

MIT, samme som oppstrøms. Se `LICENSE`.
