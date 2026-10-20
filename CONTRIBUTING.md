# Contributing to FlowState

Thank you for your interest in contributing to FlowState! This guide will help you get started with this open source, local-first speech-to-text application.

FlowState is a fork of [Handy](https://github.com/cjpais/Handy) by CJ Pais, built for students at NTNU Entreprenørskolen (ES). Norwegian bokmål is the primary UI language; the app processes everything locally, with no telemetry and no account.

## What we prioritize

- **Local first**: everything runs on the user's machine. No telemetry, no crash reporting, update checks off by default.
- **Norwegian bokmål**: new UI strings land in `no` together with `en`.
- **Small, reviewable diffs**: this fork merges upstream fixes regularly, so keep changes focused and clearly separated.

## Getting Started

### Prerequisites

Before you begin, ensure you have the following installed:

- [Rust](https://rustup.rs/) (latest stable)
- [Bun](https://bun.sh/) package manager
- Platform-specific build tools (see [BUILD.md](BUILD.md))

### Setting Up Your Development Environment

1. **Fork the repository** on GitHub

2. **Clone your fork**:

   ```bash
   git clone git@github.com:YOUR_USERNAME/FlowState.git
   cd FlowState
   ```

3. **Add remotes** (if you plan to sync with upstream):

   ```bash
   git remote add upstream https://github.com/Teshgotcode/FlowState.git
   ```

4. **Install dependencies**:

   ```bash
   bun install
   ```

5. **Download required models**:

   ```bash
   mkdir -p src-tauri/resources/models
   curl -L -o src-tauri/resources/models/silero-vad-v6.2-F32.gguf https://huggingface.co/handy-computer/silero-vad-v6.2-gguf/resolve/main/silero-vad-v6.2-F32.gguf
   ```

6. **Run in development mode**:
   ```bash
   bun run tauri dev
   # On macOS if you encounter cmake errors:
   CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
   ```

For detailed platform-specific setup instructions, see [BUILD.md](BUILD.md).

### Understanding the Codebase

FlowState follows a clean architecture pattern:

**Backend (Rust - `src-tauri/src/`):**

- `lib.rs` - Main application entry point with Tauri setup
- `managers/` - Core business logic (audio, model, transcription)
- `audio_toolkit/` - Low-level audio processing (recording, VAD)
- `commands/` - Tauri command handlers for frontend communication
- `shortcut.rs` - Global keyboard shortcut handling
- `settings.rs` - Application settings management, including the seeded ES word list

**Frontend (React/TypeScript - `src/`):**

- `App.tsx` - Main application component
- `components/` - React UI components
- `hooks/` - Reusable React hooks
- `lib/types.ts` - Shared TypeScript types

For more details, see the Architecture section in [AGENTS.md](AGENTS.md).

## Reporting Bugs

### Before Submitting a Bug Report

1. **Search existing issues** at [github.com/Teshgotcode/FlowState/issues](https://github.com/Teshgotcode/FlowState/issues)
2. **Try the latest build** to see if the issue has been fixed
3. **Enable debug mode** (`Cmd/Ctrl+Shift+D`) to gather diagnostic information

### Submitting a Bug Report

When creating a bug report, please include:

**System Information:**

- App version (found in settings or about section)
- Operating System (e.g., macOS 14.1, Windows 11, Ubuntu 22.04)
- CPU (e.g., Apple M2, Intel i7-12700K, AMD Ryzen 7 5800X)
- GPU (e.g., Apple M2 GPU, NVIDIA RTX 4080, Intel UHD Graphics)

**Bug Details:**

- Clear description of the bug
- Steps to reproduce
- Expected behavior
- Actual behavior
- Screenshots or logs if applicable
- Information from debug mode if relevant

Use the [Bug Report template](.github/ISSUE_TEMPLATE/bug_report.md) when creating an issue.

## Suggesting Features

Open an issue at [github.com/Teshgotcode/FlowState/issues](https://github.com/Teshgotcode/FlowState/issues) and describe:

- The problem you're trying to solve
- Your proposed solution
- Any alternatives you've considered

Keep in mind what this fork is for: a local-first dictation tool for ES students. Features that need an account, a cloud service, or telemetry will not be accepted.

## Making Code Contributions

### Before You Start

1. **Search existing issues and PRs** at [github.com/Teshgotcode/FlowState/issues](https://github.com/Teshgotcode/FlowState/issues) - check both open AND closed issues and pull requests. Someone may have already addressed it.

2. **Keep the upstream in mind.** FlowState merges fixes from [cjpais/Handy](https://github.com/cjpais/Handy) regularly. Make changes easy to merge:
   - Keep ES changes small and clearly separated (ES defaults in `settings.rs`, the `no` locale, the ES word list).
   - Do not rename upstream crate names (`handy-recorder`, `handy-cpal`, `handy-keys`, `@handy-computer/models`). They are real dependencies.
   - Generally useful fixes should also go upstream as a PR against `cjpais/Handy`.

### Development Workflow

1. **Create a feature branch**:

   ```bash
   git checkout -b feature/your-feature-name
   # or
   git checkout -b fix/your-bug-fix
   ```

2. **Make your changes**:
   - Write clean, maintainable code
   - Follow existing code style and patterns
   - Keep commits focused and atomic

3. **Test thoroughly**:
   - `bun run build` (frontend) and `bun run check:translations` (locales)
   - Test on your target platform(s)
   - Verify existing functionality still works
   - Use debug mode to verify audio/transcription behavior

4. **Commit your changes** with conventional commit prefixes (`feat:`, `fix:`, `docs:`, `refactor:`, `chore:`). Focus the message on why, not what.

5. **Push and open a pull request** against [Teshgotcode/FlowState](https://github.com/Teshgotcode/FlowState) and fill out the PR template completely, including the human-written description.

### AI Assistance Disclosure

AI-assisted PRs are welcome. Use whatever tools help you contribute, just be upfront about it. In your PR description, please include:

- Whether AI was used (yes/no)
- Which tools were used (e.g., "Claude Code", "GitHub Copilot", "ChatGPT")
- How extensively it was used (e.g., "generated boilerplate", "helped debug", "wrote most of the code")

### Code Style Guidelines

**Rust:**

- Follow standard Rust formatting (`cargo fmt`)
- Run `cargo clippy` and address warnings
- Handle errors explicitly (avoid unwrap in production code)
- Add doc comments for public APIs

**TypeScript/React:**

- Use TypeScript strictly, avoid `any` types
- Use functional components with hooks
- Keep components small and focused
- Use Tailwind CSS for styling

**User-facing copy:**

- Norwegian bokmål by default, via i18next keys (never hardcoded strings)
- Short sentences, no exclamation marks, no em dashes, no marketing filler
- Never translate brand names (FlowState, transcribe.cpp, ggml, OpenAI)

### Testing Your Changes

**Manual Testing:**

- Run the app in development mode: `bun run tauri dev`
- Test your changes with debug mode enabled
- Test with different audio devices
- Try various transcription scenarios

**Building for Production:**

```bash
bun run tauri build
```

Test the production build to ensure it works as expected.

## Translations

See [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md). The app ships 27 locales; English is the source language and Norwegian bokmål is the translation this fork maintains.

## Getting Help

- **Issues**: [github.com/Teshgotcode/FlowState/issues](https://github.com/Teshgotcode/FlowState/issues)
- **Upstream**: the original project lives at [cjpais/Handy](https://github.com/cjpais/Handy); its issue tracker and discussions cover the shared engine code

## License

By contributing to FlowState, you agree that your contributions will be licensed under the MIT License. See [LICENSE](LICENSE) for details.

---

**Thank you for contributing to FlowState!**
