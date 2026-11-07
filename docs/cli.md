# CLI

`flowstate` uten argumenter starter appen. Med en lesekommando leser den
`history.db` og avslutter, uten vindu, mikrofon eller modell.

```bash
flowstate meetings --limit 20           # møter, nyeste først
flowstate meeting 42                    # ett møte med transkribert tekst og notat
flowstate transcript 42                 # ren transkribert tekst, møte eller diktering
flowstate search "regnskap" --limit 20  # søk i titler og tekster
```

`--json` på alle fire gir maskinlesbar output. `--db <fil>` peker på en annen
database (standard: appens eget dataområde).

## For en AI-agent

CLI-et er appens eneste integrasjonsflate. Det er valgt framfor en MCP-server
fordi hvert MCP-verktøyskjema sendes i hver forespørsel, mens en kommando koster
nesten ingenting og agenten kan velge selv etter å ha lest hjelpeteksten.

Lim denne blokken inn i Claude, ChatGPT eller en annen agent:

```
Jeg bruker FlowState lokalt. Du kan lese møtene mine med disse kommandoene:
  /Applications/FlowState.app/Contents/MacOS/flowstate meetings --limit 20 --json
  /Applications/FlowState.app/Contents/MacOS/flowstate meeting <id> --json
  /Applications/FlowState.app/Contents/MacOS/flowstate transcript <id> --json
  /Applications/FlowState.app/Contents/MacOS/flowstate search "<ord>" --json
Bruk dem når jeg spør om møtene mine. Alt er lokalt.
```

Den samme blokken står i Innstillinger, med en kopiknapp.

Kommandoene ligger i samme binær som appen:

```bash
~/Applications/FlowState.app/Contents/MacOS/flowstate meetings
```

## Sikkerhet

Databasen åpnes `SQLITE_OPEN_READ_ONLY` gjennom `src-tauri/src/history_read.rs`.
Det finnes ingen skrivekommandoer, så CLI-et kan ikke skade historikken.

## Kode

- `src-tauri/src/cli.rs` - subkommandoene, utskriften og testen.
- `src-tauri/src/history_read.rs` - leseklassen CLI-et deler med appen.
- `src-tauri/src/main.rs` - `run_command` kjøres før Tauri starter.
