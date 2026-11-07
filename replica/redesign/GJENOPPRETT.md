# Gjenopprett «Rolig»-temaet

Denne filen er fasiten for designretningen som ble bygget 7. november 2026, og
oppskriften for å hente tilbake det som ble fjernet. Alt nedenfor er sannhet om
koden slik den står, ikke et ønske.

## Temaet i én setning

Et rolig dokument, ikke et dashbord: datoen er tittelen, transkripsjonen er
innholdet, og én aksent betyr «tar opp».

## Reglene

1. Dokumentet er produktet. Forsiden er dagens transkripsjon, ikke statskort.
2. Én aksent (pastell), og den betyr opptak eller aktiv rad. Ingenting annet får farge.
3. Hver påstand i grensesnittet må være sann: ingen bølge når den ikke tar opp.
4. Hårfine skiller, ikke kort. Radene leses som tekst.
5. Mørk modus er førsteklasses. Lys/mørk følger systemet til brukeren velger.
6. Alt lokalt, og det står i bunnlinjen: «Ingenting forlater denne maskinen.»
7. Innstillinger er søkbare gjennom ⌘K. Handlinger viser snarveien sin.

## Tokenene

| Ting | Verdi |
| --- | --- |
| UI-font | Figtree (`--flow-font-ui`), bundlet lokalt, SIL OFL |
| Lesefont | EB Garamond (`--flow-font-display`) |
| Rail | 210 px, `bg-surface-soft`, `border-e border-line` |
| Side | `bg-surface`, `px-[44px] pb-[40px] pt-[34px]`, `border-s border-line` |
| Tittel | `font-display text-[27px] font-semibold leading-[34px] tracking-[-0.01em]` |
| Dato på forsiden | `font-display text-[31px] leading-[38px]` |
| Brødtekst | serif 17,5/27, maks 58 tegn per linje |
| Metarad | 11,5 til 12,5 px, `text-ink-3` |
| Seksjonslabel | 10,5 til 11 px, `font-semibold uppercase tracking-[0.08em]`, `text-ink-3` |
| Rad | 30 til 36 px, gap 2 til 4 px, ikon 15 px, `strokeWidth 1.8` |
| Skiller | `border-line`, aldri skygge på innhold |
| Aksent | `--flow-accent-*` fra `src/lib/utils/accent.ts`, fem pasteller + egen fargetone |
| Kontrast | Mørk tekst på pastell (`--color-on-accent`), aksenttekst bare i dyp tone. Testet i `accent.test.ts` |

## Filene som eier temaet

- `src/styles/theme.css` - `--flow-*`-tokens, `@font-face`, `data-theme`-overstyringer.
- `src/App.css` - `@theme`, rotfont 16 px.
- `src/lib/utils/accent.ts` - palettene, `applyAccent`, `contrast`.
- `src/components/hub/Primitives.tsx` - `Page`, `PageHeader`, `Card`, `SectionLabel`.
- `src/App.tsx` - skallet: rail, side, ⌘K, ⌘\.
- `src/components/Sidebar.tsx` - railen, modellnavn, temabryter.

## Det som ble fjernet, og hvordan det kommer tilbake

Ingenting er slettet fra disk. To filer er bare ikke rutet lenger:
`src/components/home/DictationHome.tsx` og `src/components/TopBar.tsx`.

### 1. Banneret på forsiden

Vil du ha det tilbake, lim denne blokken inn i `DikteringPage.tsx` rett over
`<div className="mt-[10px]">` som starter transkripsjonen. Den bruker tokens som
fortsatt finnes, og dismiss-nøkkelen er den samme som før.

```tsx
const [bannerHidden, setBannerHidden] = useState(
  () => localStorage.getItem("flowstate.homeBannerDismissed") === "1",
);
```

```tsx
{!bannerHidden && (
  <section className="relative mt-[20px] overflow-hidden rounded-[12px] border border-line bg-surface-soft px-[26px] py-[22px]">
    <button
      type="button"
      onClick={() => {
        setBannerHidden(true);
        try {
          localStorage.setItem("flowstate.homeBannerDismissed", "1");
        } catch {
          // banneret kommer tilbake neste gang
        }
      }}
      aria-label={t("home.dismiss")}
      className="absolute end-[12px] top-[12px] flex h-[24px] w-[24px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 hover:bg-wash hover:text-text"
    >
      <X size={14} />
    </button>
    <h2 className="max-w-[24ch] font-display text-[24px] font-semibold leading-[30px] tracking-[-0.01em] text-text">
      {t("home.bannerTitle")}
    </h2>
    <p className="mt-[7px] max-w-[58ch] text-[13px] leading-[20px] text-ink-2">
      {t("home.bannerBody")}
    </p>
    <button
      type="button"
      onClick={() => onNavigate("general")}
      className="mt-[12px] inline-flex h-[30px] items-center rounded-[7px] border border-logo-primary/40 bg-background-ui px-[11px] text-[12px] font-[600] text-on-accent"
    >
      {t("home.bannerCta")}
    </button>
  </section>
)}
```

Teksten ligger i `home.bannerTitle`, `home.bannerBody` og `home.bannerCta` i alle
26 språk. Norsk: «Snakk i stedet for å skrive», «Hold snarveien, si setningen,
slipp. Teksten havner der markøren står, i hvilken som helst app.» og «Vis meg
hvordan».

### 2. Høyrekolonnen (250 px)

Statistikken (ord totalt, dikteringer, dager på rad, «Stemmeprofilen din») er den
mest dashbord-aktige flaten i appen, og derfor droppet. Vil du ha den tilbake, er
den urørt i `DictationHome.tsx` og kan flyttes over som en `<aside
className="w-[250px] shrink-0">` ved siden av transkripsjonen. Da bør
`w-[1044px]`-kolonnene i den gamle filen gjenoppstå også.

### 3. Resten

| Fjernet | Hvor det ligger |
| --- | --- |
| Toppstripe 44 px (tema, konto, panelbryter) | `src/components/TopBar.tsx`, urørt |
| Søkefelt på forsiden | `DictationHome.tsx` rundt linje 360 |
| Fire radhandlinger (Play, Kopier, Kommentar, Mer) | `DictationHome.tsx` rundt linje 140 til 175 |
| «Kom i gang»-kortet | `DictationHome.tsx` og `Sidebar.tsx`-historikk, søk på `flowstate.sidebarPromoDismissed` |
| Fire rail-rader (Ordbok, Snippets, Stil, Transforms) | lever som faner i `src/components/library/LibraryPage.tsx` |

## Slik ser du at temaet fortsatt holder

```bash
cd ~/Documents/ES/flowstate
bunx tsc --noEmit && bun run lint && bun run check:translations
bun test src/lib/utils/accent.test.ts
bun ~/.hermes/cache/scratch/sweep.mjs   # skjermbilder i 1350x850
```
