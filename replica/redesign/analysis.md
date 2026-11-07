# FlowState redesign: analyse og retning

Tre forskningsspor: anmeldelser, community-tråder, premium-UI. Dette er grunnlaget for retningen «Rolig».

## 1. Hva brukerne sier om Wispr Flow

### Ros (behold)
- Farten: «I speak three and a half times quicker than I can type.»
- Oppryddingen: «I absolutely love the way it cleans up your thoughts.»
- Flerspråklig kodebytte: «Dictation that actually gets my Hinglish right.»

### Klagene, sortert etter hvor ofte de dukker opp
| Tema | Hvor ofte | Hva de sier | Vårt svar |
|---|---|---|---|
| Nøyaktighet som faller | Vanligst, ~20 personer i 7 tråder | «It refuses to write what I actually say. It fixes my language and randomly inverts my meaning.» | Råtekst som egen modus, per notat |
| For aggressiv opprydding | Vanlig | En bruker med afasi fikk «I don't love» snudd til «I love». | Råtekst er standard, opprydding er opt-in |
| Personvern | Vanlig | Skjermbilder og tastetrykk til skyen: «I want a microphone, not a surveillance tool.» | Alt lokalt, sagt rett ut i grensesnittet |
| Perlen dekker knapper | 7 personer, 700+ i deres egen runde | «The little pill gets in the way of things frequently.» | Drabar, husker plass, skjuler seg for kontroller |
| Dashbord-tretthet | Vanlig | «I want invisible. I don't want to check a dashboard.» | Forsiden er et dokument, ikke statskort |
| Føles som Electron | Vanlig | «barely-disguised Electron app... the menus are slow.» | Native materialer, systemtema, ⌘K |
| Gjemte innstillinger | Gjentatt | Smart-formatering lå begravd; en bruker raste i avsnitt før støtte fant bryteren. | Alt gjennom ⌘K, snarveier trykt ved handlingen |
| Batteri og RAM | Vanlig | «about 10x» batteritap, 800 MB RAM | Lokal modell, ingen skjermbilder |
| Pris | Vanlig | 144 dollar i året, ingen livstid | Vi tar ikke abonnement |

### UI-gripes som er lette å fikse
- Ingen mørk modus: «tired of being blinded every time the app surfaces».
- Vinduet forsvinner: «hard to bring it back».
- Popups midt i arbeidet.
- Vanskelig å se om den er på eller av.

### Det folk ber om som ingen leverer
1. Strømming mens man snakker, ikke alt på slutten.
2. Perle som kan flyttes og husker plassen.
3. Helt lokal behandling.
4. Verbatim-modus: rene ord, ingen omskriving.
5. Fungerende mobiltastatur.

## 2. Hva premium-appene gjør
| Produkt | Grep | Hvorfor det virker | Hos oss |
|---|---|---|---|
| Raycast | Søkefeltet er vinduets helt | Hierarkiet lærer seg selv | ⌘K i toppen, samme idé |
| Raycast | Snarveier trykt ved hver handling | Læres ved bruk | Snarveier i paletten |
| Linear | ⌘K + enkelttaster | Musefri fart | ⌘K, V for ordbok, ⌘\ for sidestolpe |
| Things 3 | 95 % nøytralt, én aksent | Rolig leses som dyrt | Én pastellaksent, bare til opptak |
| Things 3 | 44 px rader, hårfine skiller | Rolig selv i lange lister | 27 px linjehøyde, hårfine skiller |
| Granola | SF Pro, ekte radius | Native føles i detaljene | Figtree for UI, EB Garamond for tekst |
| Superwhisper | Ingen skrivevindu; teksten lander i appen | Mål-appen er grensesnittet | Perlen + hurtigtast, huben er arkivet |
| Apple | Glass bare på kontrollaget | Innhold skal ikke være gjennomsiktig | Ingen glass i innholdet |

## 3. Designprinsipper
1. Dokumentet er produktet. Datoen er tittelen, transkripsjonen er innholdet.
2. Én aksent, og den betyr «tar opp». Ingenting annet får farge.
3. Hver påstand i grensesnittet må være sann: ingen bølge når den ikke tar opp.
4. Innstillinger er søkbare, handlinger viser snarveien sin.
5. Alt lokalt, og det står i bunnen av vinduet.
6. Hårfine skiller, ikke kort. Radene leses som tekst, ikke som widgets.
7. Mørk modus er førsteklasses, ikke et etterslep.

## 4. Retningen: «Rolig»
- Sidestolpe 210 px, native materiale, aktiv rad med aksentstripe.
- Dato i EB Garamond 31/38, oppsummering i 12,5 med dagens tall.
- Komponist: aksentkant når den tar opp, live tekst i serif, tidtaker, ⌥ Space.
- Notater: klokkeslett i tabelltall, tekst i serif 17,5/27, maks 58 tegn per linje.
- Råtekst/Ryddet er en bryter på notatet, ikke i innstillingene.
- Bunnlinje: lås + «Alt skjer på denne maskinen.»
- ⌘K-palett for alt: navigasjon, søk, tema, aksent, møte, rå/ryddet.

## 5. Hva som må endres i koden
- Nytt skall: sidestolpe med statusfot, toppstripe beholdes som 46 px.
- Forsiden skrives om fra kort til dokument.
- Ny `TranscriptRow` med Råtekst/Ryddet per notat, koblet til `post_processed_text`.
- Komponist med live tekst, i serif, fra eksisterende streaming-transkripsjon.
- ⌘K-palett: ny komponent, ruter til seksjoner og handlinger.
- Perlen: dragbar med lagret posisjon.
- Aksenten brukes bare på opptak og aktiv rad.

## 6. Bevisst ikke gjort
- Ingen kampanjekort, ingen «inviter teamet», ingen Pro-merke.
- Ingen chatbot over møter.
- Ingen deling på tvers av enheter; det er hele poenget.

## Kilder
- g2.com/products/wispr-flow/reviews
- producthunt.com/products/wisprflow/reviews
- apps.apple.com/us/app/wispr-flow-ai-voice-keyboard/id6497229487
- reddit.com/r/WisprFlow og r/ProductivityApps/comments/1owpzkr
- news.ycombinator.com/item?id=41696153
- digitaltrends.com/computing/wispr-flow-asked-its-haters-what-was-wrong-and-more-than-700-people-answered/
- podfeet.com/blog/2026/03/wispr-flow-scott-willsey/
- linear.app/now/how-we-redesigned-the-linear-ui
- granola.ai/updates/whats-new-2026-01-08
- raycast.com/blog/a-fresh-look-and-feel
- developer.apple.com/design/Human-Interface-Guidelines/materials


---

## Bygget (implementert i `src/`)

Siden analysen ble skrevet er retningen lagt inn i appen, ikke bare i mockupen:

| Mockup | Kode |
| --- | --- |
| Sidestolpen | `components/Sidebar.tsx` - 210 px, merkepille, statuslinje med modellnavn og temabryter, Innstillinger/Hjelp i bunnen |
| Dokumentflaten | `App.tsx` - rammeløst vindu, side med hairline mot railen, 44 px luft, ingen toppstripe |
| Dato og sammendrag | `components/home/DikteringPage.tsx` - EB Garamond 31 px, ordtelling, minutter spart |
| Komponisten | samme fil - `recording-state-event` og `stream-text-event` gir levende tekst, committed + tentative, aksentkant mens den tar opp |
| Transkripsjonen som bok | samme fil - råtekst/ryddet som segmentkontroll, kopier og slett på valgt rad |
| ⌘K | `components/command/CommandPalette.tsx` - navigasjon, tema, aksent, møte, hurtignotat |
| Bibliotek | `components/library/LibraryPage.tsx` - Ordbok, Snippets, Transforms, Stil bak én rad |
| Rolig innsikt | `components/insights/InsightsPage.tsx` - serif-tittel, hairline-kort, tynne stolper, dempet heatmap |
| Rolig notetaker | `components/notetaker/NotetakerPage.tsx` - I DAG uten fyll, aksentstolpe på valgt rad, serif tittel i panelet |
| Rolig notatblokk | `components/notes/NotesPage.tsx` - intro i stedet for banner, hairline-rader |

Perlen var allerede dragbar med lagret kant (`start_meeting_bar_drag`, `dock_meeting_bar_if_moved`), så den er ikke endret i denne runden.
