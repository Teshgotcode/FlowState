import type { Snippet } from "@/bindings";
import {
  BUNDLE_FORMAT,
  BUNDLE_VERSION,
  mergeDictionaryBundle,
} from "@/lib/utils/dictionaryBundle";

/**
 * The built-in ES package: vocabulary an entrepreneurship ("ES") class needs
 * before anyone has typed a single word. It has three parts, matching how the
 * words are actually used: subject terms, (sample) lecturer names and
 * (sample) project names, plus a handful of snippets that expand common ES
 * abbreviations. Everything is plain data so it can travel inside a shared
 * dictionary bundle and be merged, not installed over anything.
 */
export const ES_PACKAGE_NAME = "ES-pakken";

/** Entrepreneurship subject terms ("fagord"). */
export const ES_TERMS: string[] = [
  "aksjekapital",
  "aksjonær",
  "balanse",
  "break-even",
  "bruttofortjeneste",
  "budsjett",
  "forretningsmodell",
  "forretningsplan",
  "ideutvikling",
  "inntekter",
  "innovasjon",
  "kjernepersoner",
  "konkurransefortrinn",
  "konseptbeskrivelse",
  "kostnadsstruktur",
  "kostnader",
  "kundemasse",
  "leverandør",
  "markedsanalyse",
  "markedsføring",
  "målgruppe",
  "omkostning",
  "omsetning",
  "organisasjonsform",
  "prising",
  "regnskap",
  "resultatregnskap",
  "revisor",
  "risikovurdering",
  "salgsstrategi",
  "selskapsform",
  "skatteetaten",
  "startkapital",
  "studentbedrift",
  "ungdomsbedrift",
  "verdiskaping",
];

/**
 * Sample lecturer names. Real classes replace these with their own after
 * enabling the package; the placeholders keep dictation from mangling a
 * Norwegian full name on day one.
 */
export const ES_LECTURERS: string[] = ["Kari Nordmann", "Ola Nordmann"];

/** Sample project names (student/Youth enterprise style). */
export const ES_PROJECT_NAMES: string[] = [
  "Eventyret",
  "Fjordlys",
  "Gnist",
  "Spiret",
  "Tindra",
];

/** Every custom word the package contributes. */
export const ES_WORDS: string[] = [
  ...ES_TERMS,
  ...ES_LECTURERS,
  ...ES_PROJECT_NAMES,
];

/** Snippets the package contributes (short spoken forms to full terms). */
export const ES_SNIPPETS: Snippet[] = [
  { trigger: "es", expansion: "entreprenørskap", case_sensitive: false },
  { trigger: "ue", expansion: "Ungt Entreprenørskap", case_sensitive: false },
  { trigger: "ub", expansion: "ungdomsbedrift", case_sensitive: false },
  { trigger: "sb", expansion: "studentbedrift", case_sensitive: false },
];

const lowerSet = (words: string[]) =>
  new Set(words.map((word) => word.toLowerCase()));

const hasAllWords = (installed: string[], wanted: string[]) => {
  const present = lowerSet(installed);
  return wanted.every((word) => present.has(word.toLowerCase()));
};

/**
 * Whether the package is installed: every ES word is already in the custom
 * dictionary and every ES snippet trigger already exists. Derived from the
 * settings themselves, so it cannot drift out of sync with what the
 * transcription backend actually uses.
 */
export const isEsPackageInstalled = (
  customWords: string[],
  snippets: Snippet[],
): boolean =>
  hasAllWords(customWords, ES_WORDS) &&
  ES_SNIPPETS.every((snippet) =>
    snippets.some(
      (existing) =>
        existing.trigger.toLowerCase() === snippet.trigger.toLowerCase(),
    ),
  );

/**
 * Installs the ES package into a dictionary and snippet list: same merge
 * rules as a shared bundle — nothing the user already has is overwritten.
 * Returns the next lists plus the usual report, so the one-click buttons
 * (onboarding, settings) can tell the user what was added.
 */
export const installEsPackage = (customWords: string[], snippets: Snippet[]) =>
  mergeDictionaryBundle(customWords, snippets, {
    format: BUNDLE_FORMAT,
    version: BUNDLE_VERSION,
    name: ES_PACKAGE_NAME,
    exported_at: "",
    dictionary: [],
    es_terms: ES_WORDS,
    snippets: ES_SNIPPETS,
  });
