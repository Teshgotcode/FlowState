# Diarisering — målte tall for de to lokale veiene (2. nov 2026)

Spike S-8. Spørsmålet: hvilken lokal diarisering skal S-29 bygge på? Alt er kjørt på
denne maskinen (M1 Pro, 16 GB): ingen sky, ingen API-nøkler. Måleoppsettet ligger i
`research/diar-spike/` og kan kjøres på nytt.

## Testmaterialet

- **Syntetisk norsk møte, 3 talere, 5:52 (352,4 s), 16 kHz mono.** macOS `say` med tre
  norske stemmer (Henrik Enhanced, Nora Enhanced, Alva), 36 replikkvekslinger, 0,35–0,85 s
  pause mellom. Bygget av `diar-spike/gen_meeting.py`, fasit i `ground_truth.rttm`.
- **Derfor er tallene optimistiske:** ren TTS uten overlapp, aksent, støy eller
  mikrofonsprekk er langt enklere enn et ekte møte. Tallene er en rangering av veiene,
  ikke en garanti for produksjonskvalitet. S-29 må måle på et ekte opptak før den lukkes.

## Vei A — sortformer 4spk v2.1, Q8_0 GGUF, via transcribe.cpp (Metal)

Modell: `diar_streaming_sortformer_4spk-v2.1-Q8_0.gguf` (144 MB), kjørt med appens
eksisterende transcribe-cpp-runtime.

| Måling | Resultat |
| --- | --- |
| DER | **1,6 %** (FA 3,8 s, MD 1,5 s, SC 0,1 s) |
| Talere funnet | 3 av 3 |
| Segmenter | 40 (fasit 36 replikker) |
| Talerbytter innen 0,5 s | 35 av 35 |
| Replikker til riktig taler | 36 av 36 |
| Hastighet, varm kjøring | **79–81× realtid** (4,5 s for 352 s lyd) |
| Kald sveip med modellast per kjøring | 86–137 s (2,6–4,1×) |

## Vei B — sherpa-onnx (pyannote-segmentering 3.0 + eres2net_en)

| Variant | Talere | DER | Hastighet |
| --- | --- | --- | --- |
| Python, oracle k=3, 4 tråder | 3 | 3,5 % | 86–137 s (2,6–4,1×) |
| Python, auto terskel 0,50 (standard) | 10 | 12,0 % | ~86 s |
| Python, auto 0,65 | 6 | 5,6 % | ~97 s |
| Python, auto 0,75 | 5 | 5,3 % | ~89 s |
| Python, auto 0,85 | 3 | 3,5 % | — |
| sherpa-rs 0.6.8 (sherpa-onnx 1.12.9), oracle | 3 | 3,5 % | 175 s (2,0×) |

Terskelen er den store fellen: standardverdien 0,50 ga **10 talere på et 3-talermøte**.
På dette opptaket måtte den helt opp i 0,85 for å treffe riktig antall, og det tallet er
avhengig av lyden — ikke en konstant vi kan shippinge blindt.

## Lisens og pakkestørrelse

- **sortformer:** NVIDIA Open Model License (kommersiell bruk og redistribution tillatt,
  men avtaleteksten og NOTICE-linjen «Licensed by NVIDIA Corporation under the NVIDIA Open
  Model License» må med — samme krav som S-10 beskriver). 144 MB modell, ingen ny runtime.
- **sherpa-onnx:** Apache-2.0 for biblioteket; modellfilene (pyannote-segmentering,
  eres2net) har egne lisenser som må verifiseres før distribusjon. `sherpa-rs` drar inn
  `libonnxruntime.1.17.1.dylib` på **50 MB** i bundelen, og det er en ny runtime ved siden
  av den vi allerede har.

## Anbefaling til S-29

**Bruk sortformer.** Den er både mer nøyaktig (1,6 % mot 3,5 % DER) og rundt 20× raskere
enn sherpa-onnx, og den kjører på runtime-en appen allerede bærer — ingen ny dylib, ingen
ny modellfamilie, ingen terskel å gjette.

Kjente grenser som må håndteres i S-29:

1. **Maks 4 talere** (modellnavnet er `4spk`). Flere talere må klippes til de fire mest
   aktive, eller vises uten etikett. Dette er en produktbeslutning, ikke en bug.
2. **Engelsk-trent.** Den scoret 1,6 % på norsk TTS, men ekte norsk med dialekt er ikke
   målt. S-29 skal kjøre ett ekte norsk møte før den lukkes — det er allerede
   akseptkriteriet.
3. **Ingen overlapp-håndtering** er målt her (TTS-materialet har ingen overlapp).

Sherpa-onnx er dokumentert som fallback hvis 4-talergrensen blir et problem: den har ingen
øvre grense og er språkuavhengig, men koster 50 MB ekstra runtime, er ~20× tregere, og
krever en terskel som må kalibreres per opptak.
