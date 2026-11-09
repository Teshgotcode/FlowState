# Hva Wispr-dataen viser, og hva vi bør gjøre med møteopptaket

Målt på denne maskinen: 117 møtemapper i
`~/Library/Application Support/Wispr Flow/meetings`, 111 med ferdig transkript,
378 MB lyd totalt, 67 timer møte.

## Det de lagrer per møte

- `refined.ndjson`: den ferdige transkripten, ett segment per linje, med
  `text`, `speaker.id` og starttid. Dette er den vi importerer.
- `live.ndjson`: samme møte som det ble gjenkjent live, med epoch-millisekunder,
  språkbevis per segment (`nativeLanguageEvidence`) og leverandør (soniox).
- `upload.ogg`: lyden.
- `audio-devices.json`: hvilke enheter som ble brukt. `meetingPlatform` var
  `null` i alle 117, så de lagrer ikke møteappen der.

## Tall

- Talere per møte: 1 (1 stk), 2 (39), 3 (38), 4 (19), 5 (7), 6 (4), 7 (1), 8 (2).
  Sortformer, som vi bruker, stopper på fire. De fleste møtene er innenfor,
  men 14 møter har flere enn fire stemmer og blir slått sammen hos oss.
- Live-segmenter: median 1,4 s, p90 8,7 s. Ferdig transkript: median 9 ord per
  segment. De kutter altså live-teksten smått og setter den sammen etterpå.
- `refined` skiller seg fra `live` i ordvalg, ikke bare tegnsetting:
  «Strandveien 12, etasje 9, for dekkhotell» ble «Strandvegen 12. Tast 9 for
  dekkhotell». De kjører altså en opprydding over transkripten etter møtet.
- Språk: flere møter er tospråklige. Eksempel fra importen: en forelesning på
  engelsk med norsk gruppediskusjon i samme opptak.

## Hva som er gjort

- **Komprimering.** Etter at et møte er transkribert, kodes WAV-en om til AAC
  (48 kbps, mono, 16 kHz) med `/usr/bin/afconvert`, resultatet dekodes tilbake
  og lengden sammenlignes før WAV-en slettes. Målt på en ekte oppføring: 532 KB
  ble 105 KB, samme lengde. Feiler noe, blir WAV-en stående.
- **Opprydding.** Samme sted kjøres en lokal-LLM-runde over transkripten med en
  streng «rett bare åpenbare gjenkjenningsfeil»-prompt, og svaret lagres som
  `post_processed_text`. En chunk som blir mye kortere eller lengre enn kilden
  forkastes, så en oppsummering aldri erstatter det som ble sagt. Uten et
  lokalt endepunkt gjør den ingenting.

## Tre endringer dataen peker mot

1. **Tospråklige møter.** Vi velger ett språk for hele møtet. Med en
   engelsk forelesning og norsk diskusjon blir halve opptaket feil. Neste steg
   er å la språket avgjøres per vindu, fortsatt bare blant brukerens
   `spoken_languages`, og bare bytte når to vinduer på rad er enige. Må testes
   med en modell lastet ned; ingen ASR-modell ligger på maskinen nå.
2. **Opprydding etter møtet.** Wispr har en `refined`-utgave av transkripten.
   Vi har bare råteksten pluss det genererte notatet. En lokal LLM-runde med
   streng «rett bare åpenbare gjenkjenningsfeil, ikke oppsummer»-prompt, lagret
   som `post_processed_text`, gir samme nytte uten å røre segmentene.
3. **Lagringsplass.** Wispr: 378 MB for 67 timer. Vi skriver WAV, 16 kHz mono,
   som er omtrent 115 MB i timen, altså rundt 7,7 GB for samme korpus. Å
   transkode ferdige møter til komprimert lyd og slette WAV-en er den billigste
   veien, men spilleren må da dekode det formatet.

## Import

`scripts/import-wispr-meetings.py` kopierer tekst, talernummer, tider og språk
inn i `transcription_history` som møteoppføringer. Lyden blir liggende der den
er; oppføringene peker ikke på en lydfil. Kjøres trygt flere ganger: en
oppføring med `wispr-<uuid>.ogg` som filnavn hoppes over.
