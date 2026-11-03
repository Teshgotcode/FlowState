import type { Snippet } from "@/bindings";

/**
 * A shared dictionary bundle: one JSON file a student can hand to a classmate
 * ("kullet"). It carries the custom dictionary, the snippets and the ES
 * subject words, labelled with a name and an export date. Importing merges
 * without ever overwriting what the recipient already has.
 */
export const BUNDLE_FORMAT = "flowstate-dictionary-bundle";
export const BUNDLE_VERSION = 1;

export interface DictionaryBundle {
  format: typeof BUNDLE_FORMAT;
  version: number;
  /** Who/what this dictionary is, e.g. "Kullet 2025". */
  name: string;
  /** ISO timestamp of the export. */
  exported_at: string;
  /** The sender's custom words. */
  dictionary: string[];
  /** ES subject words carried along so the class shares them too. */
  es_terms: string[];
  /** The sender's snippets. */
  snippets: Snippet[];
}

export type BundleParseError =
  | { kind: "invalidJson" }
  | { kind: "notBundle" }
  | { kind: "unsupportedVersion"; version: number }
  | { kind: "empty" };

export interface MergeReport {
  wordsAdded: number;
  wordsSkipped: number;
  snippetsAdded: number;
  snippetsSkipped: number;
  /** Triggers that exist locally with a different expansion. */
  conflicts: string[];
  words: string[];
  snippets: Snippet[];
}

const normalizeWord = (word: unknown): string | null => {
  if (typeof word !== "string") return null;
  const normalized = word.replace(/\s+/g, " ").trim();
  return normalized || null;
};

const normalizeTrigger = (trigger: string) =>
  trigger.replace(/\s+/g, " ").trim();

/** Builds the bundle that gets written to disk. Nothing leaves the machine. */
export const buildDictionaryBundle = (
  name: string,
  customWords: string[],
  snippets: Snippet[],
  esTerms: string[],
): DictionaryBundle => ({
  format: BUNDLE_FORMAT,
  version: BUNDLE_VERSION,
  name: name.trim() || "FlowState",
  exported_at: new Date().toISOString(),
  dictionary: [...customWords],
  es_terms: [...esTerms],
  snippets: snippets.map((snippet) => ({ ...snippet })),
});

/**
 * Parses a bundle file. Returns a machine-readable error instead of throwing
 * so a bad file can never crash the settings window.
 */
export const parseDictionaryBundle = (
  raw: string,
): { bundle: DictionaryBundle | null; error?: BundleParseError } => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { bundle: null, error: { kind: "invalidJson" } };
  }
  const record = parsed as Record<string, unknown> | null;
  if (
    typeof record !== "object" ||
    record === null ||
    record.format !== BUNDLE_FORMAT
  ) {
    return { bundle: null, error: { kind: "notBundle" } };
  }
  if (
    typeof record.version !== "number" ||
    record.version > BUNDLE_VERSION ||
    record.version < 1
  ) {
    return {
      bundle: null,
      error: {
        kind: "unsupportedVersion",
        version: typeof record.version === "number" ? record.version : 0,
      },
    };
  }

  // Accept a bare array as the dictionary too, so hand-edited files work.
  const rawDictionary = Array.isArray(record.dictionary)
    ? record.dictionary
    : Array.isArray(record.words)
      ? record.words
      : [];
  const rawEsTerms = Array.isArray(record.es_terms) ? record.es_terms : [];
  const dictionary: string[] = [];
  for (const entry of rawDictionary) {
    const word = normalizeWord(entry);
    if (word) dictionary.push(word);
  }
  const esTerms: string[] = [];
  for (const entry of rawEsTerms) {
    const word = normalizeWord(entry);
    if (word) esTerms.push(word);
  }

  const rawSnippets = Array.isArray(record.snippets) ? record.snippets : [];
  const snippets: Snippet[] = [];
  for (const entry of rawSnippets) {
    const snippet = entry as Record<string, unknown> | null;
    if (typeof snippet?.trigger !== "string") continue;
    const trigger = normalizeTrigger(snippet.trigger);
    if (!trigger || typeof snippet.expansion !== "string") continue;
    snippets.push({
      trigger,
      expansion: snippet.expansion,
      case_sensitive: snippet.case_sensitive === true,
    });
  }

  if (
    dictionary.length === 0 &&
    esTerms.length === 0 &&
    snippets.length === 0
  ) {
    return { bundle: null, error: { kind: "empty" } };
  }

  return {
    bundle: {
      format: BUNDLE_FORMAT,
      version: record.version,
      name:
        typeof record.name === "string" && record.name.trim()
          ? record.name
          : "FlowState",
      exported_at:
        typeof record.exported_at === "string" ? record.exported_at : "",
      dictionary,
      es_terms: esTerms,
      snippets,
    },
  };
};

/**
 * Merges a bundle into the recipient's dictionary and snippets. Existing
 * entries always win: nothing of the user's own is overwritten, duplicates
 * are counted (both against existing entries and within the bundle itself),
 * and snippet triggers that collide with a different expansion are reported
 * as conflicts instead of being silently dropped.
 */
export const mergeDictionaryBundle = (
  existingWords: string[],
  existingSnippets: Snippet[],
  bundle: DictionaryBundle,
): MergeReport => {
  const words = [...existingWords];
  const seenWords = lowerSet(existingWords);
  let wordsAdded = 0;
  let wordsSkipped = 0;

  for (const word of [...bundle.dictionary, ...bundle.es_terms]) {
    const normalized = normalizeWord(word);
    if (!normalized) continue;
    const key = normalized.toLowerCase();
    if (seenWords.has(key)) {
      wordsSkipped += 1;
      continue;
    }
    seenWords.add(key);
    words.push(normalized);
    wordsAdded += 1;
  }

  const snippets = existingSnippets.map((snippet) => ({ ...snippet }));
  const snippetsByTrigger = new Map(
    snippets.map((snippet) => [snippet.trigger.toLowerCase(), snippet]),
  );
  let snippetsAdded = 0;
  let snippetsSkipped = 0;
  const conflicts: string[] = [];

  for (const snippet of bundle.snippets) {
    const trigger = normalizeTrigger(snippet.trigger);
    if (!trigger) continue;
    const key = trigger.toLowerCase();
    const existing = snippetsByTrigger.get(key);
    if (existing) {
      snippetsSkipped += 1;
      if (existing.expansion !== snippet.expansion) {
        conflicts.push(trigger);
      }
      continue;
    }
    snippetsByTrigger.set(key, snippet);
    snippets.push(snippet);
    snippetsAdded += 1;
  }

  return {
    wordsAdded,
    wordsSkipped,
    snippetsAdded,
    snippetsSkipped,
    conflicts,
    words,
    snippets,
  };
};

const lowerSet = (words: string[]) =>
  new Set(words.map((word) => word.toLowerCase()));
