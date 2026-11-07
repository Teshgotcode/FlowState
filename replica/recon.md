# recon — skjermkart (screen map)

Paritetsarbeidets kart: 12 skjermer som dekker hele appen. Hver rad beskriver
referanse-IA (Wispr Flow, som offentlig beskrevet — kun informasjonsarkitektur,
aldri deres tekst, logo eller grafikk) og hvor FlowState implementerer samme
formål og samme hovedhandling. Målt med `replica-diff/imgdiff.py` mot
`replica/wireframes/` (struktur, ikke farge); tallene ligger i
`replica/design-parity.md`.

| ID  | Skjerm                     | Hovedhandling            | Referanse-IA (rollerekkefølge)                          | FlowState-implementasjon |
| --- | ------------------------- | ----------------------- | ------------------------------------------------------- | ------------------------ |
| S01 | Onboarding: velkomst      | Start / fortsett        | topplinje → illustrasjon høyre → overskrift → undertekst → primærknapp → bunnnavigasjon | `src/components/onboarding/screens/Welcome.tsx` i `OnboardingShell.tsx` (maskot + eget steg-fremdrift) |
| S02 | Onboarding: tillatelser   | Gi mikrofon-/tilgangstillatelse | topplinje → tittel → mikrofonkort → tilgjengelighetskort → bunnnavigasjon | `screens/Permissions.tsx` |
| S03 | Onboarding: modell        | Last ned / velg modell  | topplinje → tittel → modellkort A → modellkort B → lagringsmerknad → bunnnavigasjon | `screens/Model.tsx` |
| S04 | Onboarding: prøv den      | Prøv en diktering        | topplinje → overskrift → demo-vindu → snarvei-hint → bunnnavigasjon | `screens/TryIt.tsx` + `Demo.tsx` (egen tegnet laptop-demo) |
| S05 | Hjem / diktering          | Start diktering          | sidestolpe → tittel → modellvelger → innhold → statuslinje | `components/home/DictationHome.tsx` i `App.tsx`-skallet; `components/Sidebar.tsx` (tre bånd), `footer/` |
| S06 | Historikk                 | Åpne / kopier et tidligere diktat | sidestolpe → tittel → søk → liste → statuslinje | `components/settings/history/` |
| S07 | Innstillinger: generelt  | Endre en vanlig innstilling | sidestolpe → tittel → innstillingsliste (med seksjonsoverskrifter) → statuslinje | `components/settings/general/` (`SettingsGroup`) |
| S08 | Innstillinger: modeller   | Bytt modell / last ned  | sidestolpe → tittel → modelltabell → statuslinje | `components/settings/models/` |
| S09 | Innstillinger: integrasjoner (møter) | Slå møtefunksjon av/på | sidestolpe → tittel → integrasjonsliste → statuslinje | `components/settings/integrations/` |
| S10 | Opptaksoverlegg (pill)    | Stopp / avbryt opptak    | bølgeform → tidtaker → hint → stopp-affordance | `src/overlay/RecordingOverlay.tsx` (egen pill-geometri) |
| S11 | Møte-linje (bar)          | Stopp / spør om møtet    | tittel → tidtaker → kontroller | `components/footer/MeetingIndicator.tsx` |
| S12 | Hurtignotat (scratchpad)  | Skriv og kopier         | tittelinje → tekstfelt → lukk | `src/scratchpad/Scratchpad.tsx` (egen kopi-fot, native vinduskrom) |

Siden S-26 er hjem-skjermene delt i tre, slik referansen har dem:
Diktering (S05), Notatblokk (`components/notes/NotesPage.tsx`, samme formål
som S12) og Innsikt (`components/insights/InsightsPage.tsx`, ny — referansens
insightsside med nøkkeltall, dagssøyler og døgnrytme).

Konvensjoner i `replica/wireframes/`: koordinater normaliseres til 1000×1000
per skjerm/vindu; vindusrammen er utelatt (begge apper har et vindu); en rolle
med `"kind": "own"` finnes bare hos oss og er FlowStates eget krom — den
belønner aldri poengsummen.
