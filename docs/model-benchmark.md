# ASR model benchmark: Norwegian bokmål and English

FlowState ships several speech models. This note explains which one is the default and why. It is the evidence behind the `FLOWSTATE_RANKING` order in `src-tauri/src/catalog/mod.rs`.

## Method

- Engine: transcribe-cpp 0.4.0, Metal backend, Q8_0 GGUF files, same as the app on macOS arm64.
- Corpus: 30 sentences read by macOS text-to-speech. 15 Norwegian bokmål with the voice `Nora`, 15 English with `Samantha`. Each file is 16 kHz mono, 3 to 6 seconds long, 137.6 seconds of audio in total.
- Content: natural dictation for group work and startup talk. Six Norwegian sentences contain ES vocabulary: Solan, aksjekapital, pitchdeck, forretningsmodell, kundesegment, entreprenørskap. Three use spoken-style particles such as "altså" and "liksom".
- Language: the Norwegian files run with the hint `no`, the English files with `en`. One extra run measures autodetect on the Norwegian files.
- WER: word-level Levenshtein distance divided by the reference word count. Text is lowercased, punctuation is stripped, whitespace is collapsed. The combined number is a micro-average over all 30 files.
- Latency: median wall time for one `session.run` call. RTF is wall time divided by audio duration. Below 1.0 is faster than real time.

The corpus and the harness live outside the repo. Reproduce it with 30 recordings of your own and the same WER definition.

## Results

| Model | Norwegian WER | English WER | Combined WER | Norwegian latency | English latency | Norwegian RTF | English RTF |
|---|---|---|---|---|---|---|---|
| whisper-large-v3-turbo | 6.1% | 1.0% | 3.5% | 1085 ms | 1048 ms | 0.20 | 0.28 |
| whisper-small | 19.9% | 0.5% | 9.9% | 341 ms | 287 ms | 0.07 | 0.08 |
| parakeet-tdt-0.6b-v3 | not supported | 1.0% | n/a | n/a | 150 ms | n/a | 0.04 |
| nemotron-3.5-asr-streaming-0.6b | 72.4% (autodetect) | 1.6% | n/a | 336 ms | 270 ms | 0.06 | 0.07 |
| whisper-medium | not measured | not measured | n/a | n/a | n/a | n/a | n/a |

Notes on the rows:

- **Parakeet** has no Norwegian. The catalog lists 25 European languages, and Norwegian is not one of them. It is the fastest model for English by a wide margin.
- **Nemotron** claims `nb-NO` in the catalog, but the engine rejects every language hint for it (`unsupported language`). With autodetect it transcribes Norwegian badly: 72.4% WER. It is fine for English at 1.6%.
- **whisper-medium** was not measured. The download failed its SHA-256 check against the catalog and was dropped instead of trusted. The packaged FLEURS numbers below cover it.

## Cross-check against FLEURS

The model package ships measured error rates on FLEURS test data. They rank the same way as our corpus:

| Model | English error | Norwegian (nb) error |
|---|---|---|
| whisper-large-v3 | 4.03% | 8.19% |
| whisper-large-v3-turbo | 4.38% | 9.10% |
| whisper-medium | 4.64% | 13.66% |
| nemotron-3.5-asr-streaming-0.6b | 7.90% | 19.24% |
| whisper-small | 6.51% | 25.53% |

Source: `src-tauri/src/catalog/catalog.json`, `results` section. FLEURS is read speech with accents and noise, so its absolute numbers are higher than clean TTS audio. The ordering is what matters.

## The custom-word trap

The app seeds an ES vocabulary list (Solan, aksjekapital, pitchdeck, and so on). transcribe-cpp passes that list to Whisper as a text prompt. A prompt in Norwegian also pushes the decoder toward Norwegian, even when the audio is English.

Measured on whisper-large-v3-turbo with the seeded list:

| Run | Norwegian WER | English WER |
|---|---|---|
| No vocabulary | 6.1% | 1.0% |
| ES vocabulary on every run | 4.4% | 15.6% |

One English sentence decoded as "gl gustado kbre mene", and "cheap tools" became "Cheek Tools". The list helps Norwegian and damages English. `vocabulary_for_model` therefore sends the list only when the run language is known to be Norwegian. On auto the language is not known before the run, so the fuzzy post-correction applies the custom words afterwards instead. Users who pick Norwegian explicitly get the 4.4% result.

## Decision

`whisper-large-v3-turbo` is the default pick:

- Best combined result on the two languages our users actually speak.
- 0.89 GB, half the size of whisper-large-v3, and still fast enough to feel instant: about 1.1 s per 5 s utterance.
- Correctly detected Norwegian in 15 of 15 autodetect runs.
- Gets ES terms right when they are in the vocabulary: "Solan", "entreprenørskap", "kundesegmentene" all came through intact.

Trade-offs we accept:

- 1.05 s median latency per 5 s utterance on an M-series Mac. whisper-small is three times faster and much worse at Norwegian (19.9%).
- English users pay 1.0% WER instead of parakeet's 1.0% at 150 ms. Latency is the only loss, and it stays far below real time.

`parakeet-unified-en-0.6b` stays recommended as the English-only speed pick. `nemotron` is no longer recommended and sorts last among the models the app suggests.

## Limits

- TTS audio is clean. Real students have accents, background noise, and microphones that are not studio quality. Expect higher WER in the field.
- One machine. One run per file. 30 sentences in total.
- Enough to separate 6% from 20%, not enough to separate 6.1% from 5.8%.

No dialect recordings. Bokmål only.
