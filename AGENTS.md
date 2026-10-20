# AGENTS.md

This file provides guidance to AI coding assistants working with code in this repository.

## What this repo is

FlowState is a local-first dictation app built for students at NTNU Entreprenørskolen (ES). Press a shortcut, speak, get text pasted where the cursor was. All processing happens on the user's machine: no account, no telemetry, no crash reporting, update checks off by default.

FlowState is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais, MIT licensed. Upstream is under a feature freeze, so ES-specific work lives here. Do not edit `LICENSE` or `README-upstream.md`; they keep the upstream record intact.

Repo layout:

- `origin` = [Teshgotcode/FlowState](https://github.com/Teshgotcode/FlowState) (this fork)
- `upstream` = [cjpais/Handy](https://github.com/cjpais/Handy)

## Development Commands

**Prerequisites:**

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager

**Core Development:**

```bash
# Install dependencies
bun install

# Run in development mode
bun run tauri dev
# If cmake error on macOS:
CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev

# Build for production
bun run tauri build

# Frontend only development
bun run dev        # Start Vite dev server
bun run build      # Build frontend (TypeScript + Vite)
bun run preview    # Preview built frontend
```

**Linting and Formatting (run before committing):**

```bash
bun run lint              # ESLint for frontend
bun run lint:fix          # ESLint with auto-fix
bun run format            # Prettier + cargo fmt
bun run format:check      # Check formatting without changes
bun run format:frontend   # Prettier only
bun run format:backend   # cargo fmt only
bun run check:translations  # Verify all locales have the same keys
```

**Model Setup (Required for Development):**

```bash
mkdir -p src-tauri/resources/models
curl -L -o src-tauri/resources/models/silero-vad-v6.2-F32.gguf https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
```

For detailed platform-specific build setup, see [BUILD.md](BUILD.md).

## Architecture Overview

FlowState is a cross-platform desktop speech-to-text application built with Tauri 2.x (Rust backend + React/TypeScript frontend).

### Backend Structure (src-tauri/src/)

- `lib.rs` - Main entry point, Tauri setup, manager initialization
- `managers/` - Core business logic:
  - `audio.rs` - Audio recording and device management
  - `model.rs` - Model downloading and management
  - `transcription.rs` - Speech-to-text processing pipeline
  - `history.rs` - Transcription history storage
- `audio_toolkit/` - Low-level audio processing:
  - `audio/` - Recording sink (VAD, levels, live streaming feed), output devices, WAV I/O
  - `vad/` - Voice Activity Detection (Silero VAD)
- `commands/` - Tauri command handlers for frontend communication
- `cli.rs` - CLI argument definitions (clap derive)
- `shortcut.rs` - Global keyboard shortcut handling
- `settings.rs` - Application settings management, including the seeded ES word list
- `overlay.rs` - Recording overlay window (platform-specific)
- `signal_handle.rs` - `send_transcription_input()` reusable function
- `utils.rs` - Platform detection helpers

### Frontend Structure (src/)

- `App.tsx` - Main component with onboarding flow
- `components/` - React UI components:
  - `settings/` - Settings UI
  - `model-selector/` - Model management interface
  - `onboarding/` - First-run experience
  - `overlay/` - Recording overlay UI
  - `update-checker/` - App update notifications
  - `shared/`, `ui/`, `icons/`, `footer/` - Shared components
- `hooks/useSettings.ts` - Settings state management hook
- `stores/settingsStore.ts` - Zustand store for settings
- `bindings.ts` - Auto-generated Tauri type bindings (via tauri-specta)
- `overlay/` - Recording overlay window entry point
- `lib/types.ts` - Shared TypeScript type definitions

### Key Architecture Patterns

**Manager Pattern:** Core functionality organized into managers (Audio, Model, Transcription) initialized at startup and managed via Tauri state.

**Command-Event Architecture:** Frontend → Backend via Tauri commands; Backend → Frontend via events.

**Pipeline Processing:** Audio → VAD → Whisper/Parakeet → Text output → Clipboard/Paste

**State Flow:** Zustand → Tauri Command → Rust State → Persistence (tauri-plugin-store)

### Technology Stack

**Core Libraries:**

- `transcribe-cpp` - Local speech recognition (Whisper, Parakeet, Moonshine, SenseVoice, etc.; GGML/GGUF) with GPU acceleration
- `handy-recorder` - Microphone capture: device selection, resampling to 16 kHz, failure detection. This is a real upstream crate dependency; do not rename it.
- Silero VAD v6 (CPU-only, in-process via transcribe-cpp) / `earshot` - Voice Activity Detection
- `rdev` - Global keyboard shortcuts
- `rodio` - Audio playback for feedback sounds

### Application Flow

1. **Initialization:** App starts minimized to tray, loads settings, initializes managers
2. **Model Setup:** First-run downloads preferred Whisper model (Small/Medium/Turbo/Large)
3. **Recording:** Global shortcut triggers audio recording with VAD filtering
4. **Processing:** Audio sent to Whisper model for transcription
5. **Output:** Text pasted to active application via system clipboard

### Settings System

Settings are stored using Tauri's store plugin with reactive updates:

- Keyboard shortcuts (configurable, supports push-to-talk)
- Audio devices (microphone/output selection)
- Model preferences (Small/Medium/Turbo/Large Whisper variants)
- Audio feedback and translation options
- Custom words (seeded with an ES vocabulary: Entreprenørskolen, gründer, aksjekapital, kundevalidering and more; editable in settings)

### Single Instance Architecture

The app enforces single instance behavior: launching when already running brings the settings window to front rather than creating a new process. Remote control flags (`--toggle-transcription`, etc.) work by launching a second instance that sends args to the running instance via `tauri_plugin_single_instance`, then exits.

## Environment Variables

The canonical prefix is `FLOWSTATE_*`. The old `HANDY_*` names are accepted as a fallback for one release, so existing scripts and Nix configurations keep working. New code reads `FLOWSTATE_*` first, then falls back to `HANDY_*`.

User-facing variables:

- `FLOWSTATE_DISABLE_UPDATER` (legacy `HANDY_DISABLE_UPDATER`) - force-disable the self-updater at runtime without touching the persisted setting; used by the Nix package, where self-update cannot work against an immutable `/nix/store`
- `FLOWSTATE_FORCE_AI_STUB` (legacy `HANDY_FORCE_AI_STUB`) - stub out AI post-processing
- `FLOWSTATE_KEEP_VULKAN_IMPLICIT_LAYERS` (legacy `HANDY_KEEP_VULKAN_IMPLICIT_LAYERS`) - keep implicit Vulkan layers enabled on Windows
- `FLOWSTATE_NO_GTK_LAYER_SHELL` (legacy `HANDY_NO_GTK_LAYER_SHELL`) - disable the GTK layer shell overlay on Linux

Test-only variables (`*_TEST_*`, `*_WORKER_FAULT*`) stay internal to the test suite and do not need the fallback.

## Internationalization (i18n)

All user-facing strings must use i18next translations. ESLint enforces this (no hardcoded strings in JSX). The app ships 27 locales.

- English (`en`) is the source language and the code default.
- Norwegian bokmål (`no`) is the translation this fork maintains; keep it complete for every new key.

**Adding new text:**

1. Add key to `src/i18n/locales/en/translation.json`
2. Add the Norwegian bokmål translation to `src/i18n/locales/no/translation.json`
3. Use in component: `const { t } = useTranslation(); t('key.path')`
4. Run `bun run check:translations`

**File structure:**

```
src/i18n/
├── index.ts           # i18n setup
├── languages.ts       # Language metadata
└── locales/
    ├── en/translation.json  # English (source)
    ├── no/translation.json  # Norwegian bokmål (maintained by this fork)
    └── ...                  # 25 more locales
```

For translation contribution guidelines, see [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

## Code Style

**Rust:**

- Run `cargo fmt` and `cargo clippy` before committing
- Handle errors explicitly (avoid unwrap in production)
- Use descriptive names, add doc comments for public APIs

**TypeScript/React:**

- Strict TypeScript, avoid `any` types
- Functional components with hooks
- Tailwind CSS for styling
- Path aliases: `@/` → `./src/`

**User-facing copy** is Norwegian bokmål by default. Short sentences, one idea per sentence, no exclamation marks, no em dashes, no marketing filler. Brand names are never translated: FlowState, transcribe.cpp, ggml, OpenAI.

## CLI Parameters

The binary and CLI are `flowstate`. It supports command-line parameters on all platforms for integration with scripts, window managers, and autostart configurations.

**Implementation:** `cli.rs` (definitions), `main.rs` (parsing), `lib.rs` (applying), `signal_handle.rs` (shared logic)

| Flag                     | Description                                                |
| ------------------------ | ---------------------------------------------------------- |
| `--toggle-transcription` | Toggle recording on/off on a running instance              |
| `--toggle-post-process`  | Toggle recording with post-processing on/off               |
| `--cancel`               | Cancel the current operation on a running instance         |
| `--start-hidden`         | Launch without showing the main window (tray icon visible) |
| `--no-tray`              | Launch without system tray (closing window quits the app)  |
| `--debug`                | Enable debug mode with verbose (Trace) logging             |

**Key design decisions:**

- CLI flags are runtime-only overrides: they do NOT modify persisted settings
- Remote control flags work via `tauri_plugin_single_instance`: second instance sends args, then exits
- `send_transcription_input()` in `signal_handle.rs` is shared between signal handlers and CLI

## Debug Mode

Access debug features: `Cmd+Shift+D` (macOS) or `Ctrl+Shift+D` (Windows/Linux)

## Platform Notes

- **macOS**: Metal acceleration, accessibility permissions required for keyboard shortcuts. Post-processing can use Apple Intelligence locally on Apple Silicon.
- **Windows**: Vulkan acceleration, code signing. Implicit Vulkan layers (overlays, capture hooks) are disabled for the FlowState process via `VK_LOADER_LAYERS_DISABLE=~implicit~` set in `main.rs`; opt out with `FLOWSTATE_KEEP_VULKAN_IMPLICIT_LAYERS=1` or by setting `VK_LOADER_LAYERS_DISABLE` yourself
- **Linux**: OpenBLAS + Vulkan, limited Wayland support, overlay uses GTK layer shell (disable with `FLOWSTATE_NO_GTK_LAYER_SHELL=1`)

## Upstream-merge workflow

This fork tracks upstream and pulls fixes regularly. Keep that cheap:

- Fetch and merge upstream regularly: `git fetch upstream && git merge upstream/main`
- Keep ES changes small, focused, and clearly separated (ES defaults in `settings.rs`, the `no` locale, the ES word list, local-first post-processing defaults). Avoid scattering fork edits through shared files; each upstream refactor then lands with small conflicts.
- Do not rename upstream crate names in `src-tauri/Cargo.toml` / `Cargo.lock` (`handy-recorder`, `handy-cpal`, `handy-keys`, `@handy-computer/models`). They are real dependencies, not brand strings.
- Brand strings that DO change with the fork: product name FlowState, binary and CLI `flowstate`, bundle id `com.teshgotcode.flowstate`, window titles, `FLOWSTATE_*` env vars.
- Never use the Solan mascot in code, docs, or assets.
- Conventional commits: `feat:`, `fix:`, `docs:`, `refactor:`, `chore:`. Focus the message on why, not what.
- Fixes that are generally useful belong upstream: open a PR against `cjpais/Handy` there, and keep the fork's copy identical.

## Troubleshooting

See [docs/troubleshooting/](docs/troubleshooting/).

## GitHub workflow for AI coding assistants

**MANDATORY. Before opening any PR or issue in this repo: you MUST read the relevant template file and follow it strictly.** That includes sections that look "ceremonial": checklists, AI Assistance disclosures, the human-written description. A generic Summary/Test-plan layout is not acceptable.

- **Opening a PR:** Read [`.github/PULL_REQUEST_TEMPLATE.md`](.github/PULL_REQUEST_TEMPLATE.md). Every section listed there is mandatory. Submit one fix or feature per PR. If a section requires a human-written paragraph, leave a clear TODO placeholder and ask the human contributor to fill it in; do not invent their voice. AI-generated code is welcome, but a human submits and describes the PR.
- **Opening an issue:** Read [`.github/ISSUE_TEMPLATE/`](.github/ISSUE_TEMPLATE/). Blank issues are disabled; pick the right template (`bug_report.md` for bugs).
- **Translations:** Follow [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).
- **Full contributor workflow:** [CONTRIBUTING.md](CONTRIBUTING.md).
