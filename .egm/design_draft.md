# Design: menneskelig replica-utseende, MCP ut, CLI inn, diskret perle

Status: godkjent av bruker 7. november 2026, revidert etter blind goldfish-review
(syv reviewere). Ingen kode skrevet ennå.

## Oversikt

1. **Utseendet tilbake, full meny.** Forsiden, toppstripen, hele railen med alle featurene, banneret, høyrekolonnen, søket, radhandlingene, Innstikt-kortene, Notatblokk-heroen og «Kom i gang»-kortet skal tilbake. Funksjonene fra «Rolig» beholdes.
2. **MCP ut, CLI inn.** MCP-serveren fjernes helt. CLI-et blir appens eneste integrasjonsflate mot AI, synlig i Innstillinger med en ferdig tekstblokk å lime inn i Claude eller ChatGPT.
3. **Varsel ved ekstern møtebehandling.** Blokkerende første gang per leverandørvert, med leverandørnavn og OK.
4. **Perlen.** Samme 40 px sirkel, utvides på hover, mer diskret i hvile.

## Hvorfor CLI i stedet for MCP

Forskningen er entydig for et lite, lokalt verktøy som dette:

- Anthropic sin egen gjennomgang viser at en arbeidsflyt gjennom verktøyskjema i konteksten kostet 150 000 tokens, der kodebasert kall gjorde samme jobb på 2 000, altså 98,7 prosent mindre (`anthropic.com/research/code-execution-with-mcp`).
- Hver MCP-server sender hele verktøykatalogen på nytt i hver forespørsel. Fire verktøy er lite, men kostnaden vokser med katalogen, ikke med oppgaven.
- En sammenligning fant at et enkelt spørsmål kostet 1 365 tokens via CLI og 44 026 via MCP, nesten alt fra skjemainjeksjon (`harness.io/blog/harness-mcp-server-redesign`).
- En studie av agenter med og uten MCP fant CLI-only 5 til 28 ganger billigere, og at 12,9 prosent av MCP-kostnaden gikk til kjøringer som ikke fullførte noe, mot 2,2 prosent for CLI (`arxiv.org/pdf/2608.08654`).

For FlowState betyr det fire lesekommandoer, stabil JSON, og en hjelpetekst skrevet for å leses av en agent. Ingen server, ingen port, ingen token, ingen livssyklus i appen.

**Motargumentet, verdt å kjenne:** en godt designet MCP-server gir autorisasjon og oppdagbarhet som CLI ikke gir. Det er ikke relevant her, fordi CLI-et åpner databasen skrivebeskyttet og bare leser.

## Mål

- Forsiden skal se ut som `p-home.png` (`/Users/tesh/.hermes/cache/scratch/flow-shots/p-home.png`).
- Railen skal ha alle radene igjen: Diktering, Notetaker, Innsikt, Ordbok, Snippets, Stil, Transforms, Hurtignotat, Notatblokk, Innstillinger, Hjelp.
- `⌘K`, levende transkripsjon, rå/ryddet, aksent, mørk modus og dragbar perle virker.
- Ingen MCP-kode, ingen MCP-innstillinger, ingen MCP-binær i bundelen.
- Én kopierbar tekstblokk i Innstillinger gjør at Claude eller ChatGPT kan lese møter og dikteringer.
- Et møtenotat sendes ikke eksternt før brukeren har bekreftet verten.

## Ikke-mål

- Ingen skrivekommandoer. AI kan lese og velge selv, ikke endre historikken.
- Ingen ny transport, ingen server, ingen port.
- Ingen endring i modellvalg, aksentpalettene eller vindusmål.
- Ingen konto, ingen sky, ingen deling.

## Beslutninger

### D1. Full meny og menneskelig utseende

**Valgt:** replicaen, med alle rail-rader og alle flater: `TopBar` (panelbryter, tema, konto-ikon), banner øverst, høyrekolonne 250 px, søkefelt, fire radhandlinger, ni innholdsrader pluss Innstillinger og Hjelp, fylte Innstikt-kort, Notatblokk-hero, «Kom i gang»-kort.

**Hvordan:** `DictationHome.tsx` og `TopBar.tsx` er urørt på disk. `App.tsx` ruter `home` tilbake, monterer `TopBar`, setter panelklassene tilbake og fjerner `library` fra `DAILY_SECTIONS`. `Sidebar.tsx` fjerner `hidden` fra de fire radene og får «Kom i gang»-kortet tilbake. `LibraryPage` blir urutet, men blir liggende.

**Konsekvens:** `DikteringPage.tsx` blir død kode og slettes. Komponisten med `useDictationStream` og Råtekst/Ryddet flyttes inn i `DictationHome`, som i dag ikke har noen av delene.

**«Kom i gang»-kortet finnes ikke på disk** og skrives på nytt, ca. 30 linjer, med `flowstate.sidebarPromoDismissed`.

**Alternativ som ble forkastet:** hybrid med dokumentflaten. Brukeren vil ha det tidligere utseendet, ikke ett element av det.

### D2. MCP fjernes helt

**Valgt:** slett serveren og alle innganger til den.

| Del | Fil |
| --- | --- |
| MCP-binær | `src-tauri/src/bin/flowstate-mcp.rs` |
| Server | `src-tauri/src/mcp/` (db, http, manager, protocol, stdio, token, mod) |
| Kommandoer | `src-tauri/src/commands/mcp.rs` og registreringene i `lib.rs` |
| Innstillinger | `mcp_enabled`, `mcp_port`, `mcp_token` i `settings.rs`, pluss `change_mcp_*` og `regenerate_mcp_token` |
| Frontend | MCP-kortet i `IntegrationsSettings.tsx`, `McpStatus`-bindings |
| Bundel | sidecar-oppføring i `tauri.conf.json` hvis den finnes |
| Tester | MCP-spesifikke tester |

Leselogikken i `mcp/db.rs` blir igjen: den flyttes til `src-tauri/src/history_read.rs` og brukes av CLI-et. Navnet skal ikke love MCP.

**Alternativ som ble forkastet:** å beholde MCP som skjult fallback. Dobbelt vedlikehold for en flate ingen bruker, og brukeren ba eksplisitt om å fjerne det.

### D3. CLI-et som AI-flate

**Valgt:** fire lesekommandoer, stabil JSON, exit-koder som betyr noe, og en `--help` skrevet for agenter.

| Kommando | Formål |
| --- | --- |
| `flowstate meetings --limit N --json` | liste møter |
| `flowstate meeting <id> --json` | ett møte med transkripsjon og notat |
| `flowstate transcript <id> --json` | ren transkripsjon |
| `flowstate search "<ord>" --limit N --json` | søk i titler og tekster |

Prinsipper: JSON-formen endres ikke uten versjonsbump, feil går til stderr med exit 1, tomt treff er exit 0 med tom liste, `--help` har eksempler, og ingen skrivekommandoer (databasen åpnes `SQLITE_OPEN_READ_ONLY`).

**I appen:** nytt kort i Innstillinger, «Kommandolinje», med de fire kommandoene i `<pre>` og en «Kopier til Claude»-knapp som kopierer:

```
Jeg bruker FlowState lokalt. Du kan lese møtene mine med disse kommandoene:
  /Applications/FlowState.app/Contents/MacOS/flowstate meetings --limit 20 --json
  /Applications/FlowState.app/Contents/MacOS/flowstate meeting <id> --json
  /Applications/FlowState.app/Contents/MacOS/flowstate transcript <id> --json
  /Applications/FlowState.app/Contents/MacOS/flowstate search "<ord>" --json
Bruk dem når jeg spør om møtene mine. Alt er lokalt.
```

Stien kommer fra en hjelper rundt `std::env::current_exe` (logikken finnes i `mcp/manager.rs::binary_path()` i dag, flyttes til en nøytral hjelper). Returverdi er en `String`. Blokken er fast engelsk fordi den skal inn i en engelsk agent; bare panelet oversettes.

### D4. Varsel ved ekstern møtebehandling

**Valgt:** blokkerende. `generateMeetingNote` kalles ikke før OK. Lukking uten OK avbryter.

**Nøkkel per vert:** `flowstate.meetingExternalAck` er en liste med bekreftede verter, ikke en boolean.

| `base_url` | Dom |
| --- | --- |
| ikke-http-skjema (`apple-intelligence://`) | lokal |
| `localhost`, `127.0.0.1`, `[::1]` | lokal |
| tom eller uleselig | ekstern |
| alt annet | ekstern |

Leverandør slås opp med samme fallback som appen: `post_process_provider_id`, første provider, `openai`. Kallsted: `src/components/settings/history/MeetingNoteSection.tsx:78`.

**Alternativ som ble forkastet:** toast. Kan avvises uten å leses.

### D5. Perlen: diagnose først

`apply_overlay_geometry_sized` regner allerede posisjonen fra dock-kanten, og 132×40 inneholder 40×40 i alle kanter, så «ankre geometrien» ville vært et no-op. Planen:

1. Reproduser med logging på `show-overlay`, `set_ignore_cursor_events`, `set_size` og `set_position`.
2. Fiks det loggen viser. Mistenkte, i rekkefølge: hit-shape-pollingen (`overlay.rs:1358-1403`), rekkefølgen `set_size` før `set_position` (`overlay.rs:712-716`), 400 ms cooldown.
3. Utvidelse kun ved hover, lukkeforsinkelse 200 ms, re-entry kansellerer lukking.

Skyggen er native (`set_shadow`), ikke CSS. Opasitet i hvile settes til 0,9.

## Teknisk plan

```
App.tsx
 ├─ TopBar (tilbake)
 ├─ Sidebar (alle rader, "Kom i gang" pa nytt)
 ├─ DictationHome (banner, hoyrekolonne, sok, radhandlinger)
 │   └─ NY: komponist med useDictationStream + ra/ryddet
 ├─ CommandPalette (beholdes)
 └─ Settings -> IntegrationsSettings: CLI-kort (MCP-kortet slettes)
Meeting note:
  MeetingNoteSection -> ExternalSendNotice (blokkerende, per vert)
Overlay:
  RecordingOverlay -> hover, diagnose forst
Backend:
  mcp/ slettes, history_read.rs blir igjen, cli.rs bruker den
```

## Detaljert implementasjonsplan

### Arbeidsstrøm 1: skall, meny og forside

Filer: `src/App.tsx`, `src/components/Sidebar.tsx`, `src/components/home/DictationHome.tsx`, `src/components/home/DikteringPage.tsx` (slettes).

- `App.tsx`: `home` til `DictationHome` med `onOpenShortcuts`, `TopBar` inn, panelklasser tilbake, `library` ut av `DAILY_SECTIONS` og `renderDailyContent`, `CommandPalette` og ⌘K/⌘\-lytterne beholdes.
- `Sidebar.tsx`: alle rader synlige, `library`-raden ut, «Kom i gang» tilbake.
- `DictationHome.tsx`: komponist med `useDictationStream`, tidtaker, `⌥ Space`, Råtekst/Ryddet på valgt rad.
- `DikteringPage.tsx`: slettes når komponisten er flyttet.
- Ingen endring i locale-filer: `diktering.*`-nøklene finnes i alle 27.

Ferdig når: forsiden matcher `p-home.png`, ⌘K virker, `tsc` og `lint` grønne.

### Arbeidsstrøm 2: MCP ut, CLI inn

Filer: `src-tauri/src/bin/flowstate-mcp.rs` (slett), `src-tauri/src/mcp/` (slett, `db.rs` flyttes til `src-tauri/src/history_read.rs`), `src-tauri/src/commands/mcp.rs` (slett), `src-tauri/src/lib.rs`, `src-tauri/src/settings.rs`, `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/cli.rs`, `src-tauri/src/utils.rs`, `src/components/settings/integrations/IntegrationsSettings.tsx`, `src/components/settings/history/MeetingNoteSection.tsx`, `src/components/meeting/ExternalSendNotice.tsx` (ny), `src/components/settings/settings-utils.ts` (ny), alle 27 `src/i18n/locales/*/translation.json`, `docs/cli.md`.

- Slett MCP-flaten som i D2, flytt leselogikken, la `cli.rs` bruke `history_read`.
- Ny kommando `get_cli_info` som returnerer binærsti.
- CLI-kort i Innstillinger med kopiknapp.
- `isExternalHost` med enhetstest (sju rader).
- `ExternalSendNotice` + gating i `MeetingNoteSection`.
- `bindings.ts` regenereres ved å starte debug-appen; aldri håndredigeres.
- `docs/cli.md` oppdateres, MCP-omtale fjernes fra dokumentasjonen.

Ferdig når: `cargo check` og `cargo test` grønne, `grep -ri mcp src src-tauri/src` gir null treff, kopiknappen gir riktig tekst, `check:translations` grønn.

### Arbeidsstrøm 3: perlen

Filer: `src/overlay/RecordingOverlay.tsx`, `src/overlay/RecordingOverlay.css`, ved behov `src-tauri/src/overlay.rs`.

Som D5.

### Arbeidsstrøm 4: verifisering

- `bunx tsc --noEmit`, `bun run lint`, `bun run check:translations`, `cargo check`, `cargo test`.
- Skjermbilder med `bun /Users/tesh/.hermes/cache/scratch/sweep.mjs` mot `p-home.png`.
- Bygg med `bunx tauri build --config src-tauri/tauri.local.json`, installer, `codesign --verify --deep --strict`.
- Vindusmål: ytre 1350×850, innvendig 1350×822 (`utils.rs:29`).
- Bekreft at `flowstate-mcp` ikke ligger i bundelen: `ls FlowState.app/Contents/MacOS`.

## Subagentstruktur

| Bølge | Agent | Oppgave | Filer |
| --- | --- | --- | --- |
| 1 | A | Skall, meny, forside, komponist | `App.tsx`, `Sidebar.tsx`, `DictationHome.tsx`, `DikteringPage.tsx` |
| 1 | B | MCP ut, CLI inn, varsel, i18n | alt i arbeidsstrøm 2 |
| 1 | C | Perle-diagnose og hover | `RecordingOverlay.tsx`, `.css`, `overlay.rs` |
| 2 | D | Verifisering og bygg | ingen, bare kommandoer |
| 2 | E | Goldfish-review av diffen | ingen |

A og B deler ingen filer: A rører ikke locale-filer, B eier alle 27. C rører bare overlay.

## Tester

| Ny logikk | Test |
| --- | --- |
| `isExternalHost` | enhetstest, sju rader |
| CLI-subkommandoer | `cargo test --lib cli::tests` (finnes) |
| MCP-fjerning | `grep -ri mcp src src-tauri/src` gir null treff |
| Perle-hover | manuell sjekk med logg, dokumentert |

## Risiko

- **R1:** Revert av `App.tsx` kan miste ⌘K eller `navigate-section`. Tiltak: D sjekker begge.
- **R2:** Komponisten kan kollidere med `DictationHome`-layouten. Tiltak: A leverer skjermbilde før og etter.
- **R3:** MCP-fjerning kan dra med seg noe CLI trenger, siden `cli.rs` importerer `mcp::db` i dag. Tiltak: flytt `db.rs` før sletting, og la `cargo check` styre.
- **R4:** Perle-feilen kan sitte i hit-shape-pollingen. Tiltak: ingen endring før loggen peker på den.
- **R5:** `get_cli_info` returnerer `target/debug`-stien under utvikling. Sant, og akseptabelt.
