# Wispr parity plan

What is left between FlowState and the reference, derived from a live
reverse-engineering pass over Wispr Flow 1.6.1457 (Electron 43.8.3) rather than
from screenshots. The measurement dossier lives outside the repo at
`~/.hermes/cache/scratch/wispr/report/` (`00-README.md` is the index; `04` covers
the hub screens, `05` the visual language, `06` the click-to-setting map).

Re-measure before changing anything: the reference runs with
`--remote-debugging-port=9222`, and `references/wispr-parity.md` in the
`flowstate-dev` skill has the rail-click snippet. The rule from that skill holds:
parity is a measurement job, never a design opinion.

## What the reference actually does, per surface

Measured, not inferred:

- **Insights → Your usage** leads with *words per minute* (with a percentile
  badge), then **FIXES MADE BY FLOW** split into *words corrected* and
  *dictionary fixes*, then total words with a month-over-month delta, a
  desktop/mobile split, a task-type breakdown (work messages, personal messages,
  emails, documents, AI prompts, other), current and longest streak, and a
  weekday heatmap with a `More`/`Less` legend.
- **Insights → Your voice** is a generated profile: persona title, description,
  catchphrase, most used word, most corrected word, peak time, top app, plus
  `Created <date>` and `Next update in N more words`.
- **Dictionary** rows carry a star (favourite), a sort control
  (`dictionarySort`, default Newest), `phrase -> replacement` rows, and a
  sparkle marker for auto-learned entries (`Dictionary.manualEntry`).
- **Style** is per-context: `Personal messages` / `Work messages` / `Email` /
  `Other`, each a three-way register picker with a live example sentence, plus
  `Auto cleanup (Beta)`.
- **Transforms** have per-preset shortcuts, `Try it out`, and
  `Auto Apply After Dictation`.
- **The pill** has a fixed state vocabulary; failures are mostly silent
  (`polish_failed` collapses to resting) except microphone acquisition, which
  surfaces.
- **Meetings**: speaker names are read live from the call app's window via
  Accessibility, not diarized after the fact.

## Where FlowState stands

Already at parity, so nothing below touches them: eight rail entries with one
Scratchpad page; Style page with prompt cards and per-app rules; Dictionary
import; folder control on Notetaker rows; live transcript window; Flow Bar
geometry (8x40 capsule, ten 2 px bars); per-meeting open-transcript and delete;
the Notetaker global ask box; the meeting window's My thoughts / Summary /
Transcript tabs; the note-shaped overview.

Two honest deviations, both local-first: word learning exists as
`managers/word_suggestions.rs` (suggestions the user accepts, never silent
auto-add), and there are no cloud surfaces at all.

## Work items

Each item is one PR-sized change. `Effort` is engineering time, not calendar
time, and excludes the ~6 min build.

W1 and W2a are implemented: a dictation writes its clip length, and Insights
opens with a speed card and a fixes card. The rest are open.

### W1. Speed card on Insights (words per minute)

Reference: `Your usage` opens with words per minute. FlowState has no WPM
anywhere, because only meetings store `duration_secs`.

Cause, as found in the code: a dictation's `HistoryEntry` is written from
`actions.rs` with no duration, and `HistoryMetadata.duration_secs` is only set
by `save_meeting_entry`. The recorder already hands the clip back as
`Vec<f32>` at 16 kHz (`managers/audio.rs`, `WHISPER_SAMPLE_RATE`), so the
duration is exact and costs nothing: `sample_count / 16_000`.

Change:
- `managers/history.rs`: `set_duration(id, secs)`, following the existing
  `save_user_notes` shape (read metadata, set, write, announce).
- `actions.rs`: after a successful save, write the clip duration.
- `lib/insights/stats.ts`: `speed(entries)` -> total words over total timed
  minutes, plus how many entries carried a duration.
- `components/insights/InsightsPage.tsx`: one card.

Acceptance: a dictation made after the change shows a duration in its metadata;
the card reads a plausible WPM (120-180 for normal speech); old entries without
a duration are excluded, not counted as zero, and the card says so when no entry
has one yet.

Files: `src-tauri/src/managers/history.rs`, `src-tauri/src/actions.rs`,
`src/lib/insights/stats.ts`, `src/components/insights/InsightsPage.tsx`,
`src/i18n/locales/*/translation.json`.

### W2. Fixes made by flow

Reference: `FIXES MADE BY FLOW`, split into words corrected and dictionary
fixes. FlowState already computes a word diff for `word_suggestions.rs` at save
time, but nothing surfaces a count.

Change: `corrections(entries)` in `stats.ts` counting word-level edits between
`transcription_text` and `post_processed_text` on entries that have both
(common prefix and suffix trimmed, LCS on the remainder, capped). Dictionary
fixes are a separate counter: `audio_toolkit/text.rs` already walks custom-word
matches per transcription, so returning how many it rewrote is a small signature
change, then a metadata field. Split into W2a (words corrected, this change) and
W2b (dictionary fixes).

Acceptance: an entry with post-processing reports a non-zero count; an entry
without it reports zero and is not counted in the average; the number is stable
across reloads because it is derived, not stored.

**W2b stays open on purpose.** The count would have to travel from
`apply_custom_words` through the dictation pipeline into `HistoryMetadata`, and
the only place it could land is a number on a card. That is a multi-layer
signature change for one line of copy, so it waits until something else needs
the same plumbing.

### W3. Dictionary star, sort, and auto-learned marker

Reference rows: star toggle, `Sort by` (Newest, and a saved
`dictionarySort`), and a sparkle for entries the app learned rather than the
user typed.

**Done, at the cheap end.** Rows get a star toggle (`starred_words`, pinned to the
top of the list), a `learned` chip (`learned_words`, written when a suggestion is
accepted, because the learner retires the suggestion itself), and an A to Z /
newest sort (`dictionary_sort`). The star does not change matching; it is an
ordering tool, which is all the reference gives it.

FlowState has neither the star nor the sort, and `word_suggestions` keeps its
counts in a separate store, so the UI has no way to say "this came from a
correction". Change: two settings fields (`dictionary_sort`,
`starred_words`), the row affordances, and a marker on words whose suggestion
count is above `MIN_SIGHTINGS`. No migration: custom words are a settings
array, and a starred-words set is additive.

Files: `settings.rs`, `components/dictionary/*`, `components/settings/CustomWords.tsx`.

### W4. Style: four contexts plus Auto cleanup

Reference leads with four context tabs, one register picker each, and an
`Auto cleanup (Beta)` toggle. FlowState ships whole-prompt cards plus per-app
rules.

Do this only if the prompt cards start feeling wrong in use. Mapping four
contexts onto the current model means picking a prompt per context and keeping
per-app rules as an override, which is a real model change rather than a UI
change. The deviation is documented in `references/wispr-parity.md`; keep it
until there is a reason.

**Contexts and Auto cleanup are done.** The Style page leads with five tabs
(Default, Email, Chat, Docs, Terminal). Each non-default tab binds one prompt for
that writing context in `post_process_context_prompts`; `context_for` classifies
the focused app by bundle id and name (`style.rs`), and resolution is per-app
rule, then context, then the Neutral fallback, then the global pick. Unknown
apps keep the global pick, so nothing changes for someone who never opens a tab.
Two Rust tests pin the order and the classification.

**Auto cleanup is done.** The Style page now carries the existing filler-word
toggle under its own heading, reusing `FillerWordRemoval` and the
`filler_word_removal_enabled` setting rather than adding a second switch. The
four context tabs stay unbuilt: per-app rules already route a dictation to the
right prompt, and four fixed tabs would duplicate that with a coarser key.

### W5. Pill state vocabulary

Reference vocabulary, measured from the running app: `resting`, `ready`,
`active_popo`, `processing`, plus meeting states, and a rule that
`polish_failed` renders as resting rather than as an error. FlowState's overlay
knows `recording`, `streaming`, `transcribing`, `processing`, `command`.

Gap worth closing: nothing tells the user that a take was *heard but produced
nothing* (`no_audio`, `verification_failed`, `timeout`,
`mic_acquisition_failed`). Today `transcription-error` goes to the main window,
which is usually closed, so the pill just disappears.

Change: an `error` overlay state with the reference's restraint — silent for
formatting failures, visible for microphone and empty-audio failures.

**Done.** `overlay::show_error_overlay(app, code)` shows a 340x50 pill with one
localized line and hides it after 4.2s; the scheduled hide goes through
`hide_recording_overlay`, so a dictation that starts meanwhile wins. Codes:
`no_model`, `model_failed`, `start_failed`. Fired from the model loader (missing
model) and from the coordinator when a start did not begin while the model *is*
loaded (microphone), so the two causes never fight over the pill. Copy lives in
`overlay.error.*` in all 26 locales. Polish failures stay silent, as in the
reference.

**Verified end to end, with one finding.** The backend path fires (`overlay
'error'` in the log) and the event reaches the overlay: an instrumented build
read back `err:no_model` from inside the webview. But the overlay frontend never
receives `show-overlay` at all in this build, so it keeps rendering the meeting
bar and ignores the error state. That is pre-existing and independent of this
change: dictation pills and meeting pills ride the same event. Until it is fixed,
a missing model also reaches the user as a macOS notification from the existing
error path.

### Landed alongside W5: three ways the new UI could lose work

1. **Folder picker without Escape.** The menu closed on outside clicks only, so
   keyboard users could not dismiss it. Escape now closes it and puts focus back
   on the trigger.
2. **An open snippet edit discarded on row change.** Clicking another row used to
   swap the draft without saving. The row change now commits first.
3. **Undo after delete restored a stale list.** The toast action captured
   `snippets` from the render it was created in, so anything saved in between was
   overwritten. It now reads the live list through a ref and re-adds only the
   removed row.

### W6. Live speaker names from the call app (spike)

Reference reads names from the meeting app's Accessibility tree while the call
runs. FlowState diarizes afterwards and suggests names from the same AX tree on
demand (`meeting_names.rs`).

This stays a spike until `lsappinfo list | grep -iE 'zoom|teams|webex|meet'`
finds a running meeting app on the machine being tested. Reading an AX tree we
cannot observe is how you ship untested code.

### W7. Accessibility drop panel (optional)

Reference: a 560x144 panel 12 px under System Settings, repositioned every
500 ms, whose only affordance is a real OS drag of the app bundle
(`event.startDrag`). FlowState loses TCC grants on every ad-hoc-signed rebuild,
so this would remove a repeated manual step.

Cost: an Apple-specific window plus a native drag source, and the panel is
useless if the drag does not work. Do it only if the rebuild churn continues.

## UI pass on the three pages that read as drafts

Measured against the reference's own pages, and against how the rest of the hub
is built. Not parity items: craft items.

**Snippets** was a settings form on a page, with no list at all and a blank lower
two thirds. It is now a list first: a count, a search field, rows of
`trigger → expansion` with a case-sensitivity badge, hover edit and delete
(delete offers undo through a toast), the composer below, and an empty state that
points at the composer. The file parsing and import/export dialogs moved to
`lib/snippets/io.ts` so the settings screen and the page share one implementation.

**Notetaker** rows now carry `time · duration`, so the row's right side is not
dead space. "Add to folder" was a native `<select>` at `opacity-0` until hover,
which is why it read as bare text: it is now a folder icon at rest that becomes a
named chip once filed, with a small menu. `durationOf` no longer says "0 min"
for a short note.

**Style** had two intro paragraphs stacked (subtitle and intro) and a fourth card
with no copy. The paragraph is gone, and the blank card turned out to be the
preview fixture using a custom prompt id (`p1`) instead of the production ones;
the fixture now uses all six built-in ids, so the screenshot shows what ships.
The app-rules input was too narrow for its own placeholder.

## Non-goals

Nothing here changes the local-first contract: no `Shared with team`, no
leaderboard, no team workspaces, no server-side `/llm/*` endpoints, no
telemetry, no desktop/mobile split. Those surfaces exist in the reference only
because it is a cloud product.

Task-type breakdown (work messages / personal messages / emails / documents /
AI prompts / other) is a maybe: it needs a classifier over the pasted text. A
keyword pass would be wrong often enough to be worse than nothing.

## Verification protocol

Per change, in this order:

1. `bunx tsc --noEmit`
2. `bun run lint`
3. `bun run check:translations` (27 locales stay complete)
4. `bun src/components/insights/insightsStats.test.ts` and
   `cd src-tauri && cargo test --lib`
5. Preview harness at `http://localhost:1420/src/preview/index.html`, driven
   with Playwright: dump `document.querySelector("main").innerText` and read it,
   because a screenshot hides raw i18n keys and fixture fallbacks.
6. One build, then install and `codesign --verify --deep --strict`. Expect
   Microphone and Accessibility to need granting again.
