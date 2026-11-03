import assert from "node:assert/strict";
import type { Snippet } from "@/bindings";
import {
  buildDictionaryBundle,
  mergeDictionaryBundle,
  parseDictionaryBundle,
} from "./dictionaryBundle";

const snippet = (
  trigger: string,
  expansion: string,
  case_sensitive = false,
): Snippet => ({ trigger, expansion, case_sensitive });

// Export → import round trip: a second profile ends up with the same
// dictionary the first one exported.
const profileA_words = ["FlowState", "aksjekapital", "Kari Nordmann"];
const profileA_snippets = [snippet("es", "entreprenørskap")];
const bundle = buildDictionaryBundle(
  "Kullet 2025",
  profileA_words,
  profileA_snippets,
  ["aksjekapital", "ungdomsbedrift"],
);
assert.equal(bundle.format, "flowstate-dictionary-bundle");
assert.equal(bundle.version, 1);
assert.equal(bundle.name, "Kullet 2025");
assert.ok(bundle.exported_at.length > 0, "bundle has an export date");

const raw = JSON.stringify(bundle);
const parsed = parseDictionaryBundle(raw);
assert.ok(parsed.bundle, "round-trip parses");
const merged = mergeDictionaryBundle([], [], parsed.bundle!);
assert.deepEqual(merged.words, [
  "FlowState",
  "aksjekapital",
  "Kari Nordmann",
  "ungdomsbedrift",
]);
assert.deepEqual(merged.snippets, [snippet("es", "entreprenørskap")]);
assert.equal(merged.wordsAdded, 4);
// "aksjekapital" ships in both dictionary and es_terms: one in-bundle duplicate.
assert.equal(merged.wordsSkipped, 1);
assert.equal(merged.snippetsAdded, 1);

// Re-importing the same bundle changes nothing and reports every duplicate.
const again = mergeDictionaryBundle(
  merged.words,
  merged.snippets,
  parsed.bundle!,
);
assert.deepEqual(again.words, merged.words);
assert.deepEqual(again.snippets, merged.snippets);
assert.equal(again.wordsAdded, 0);
assert.equal(again.wordsSkipped, 5);
assert.equal(again.snippetsAdded, 0);
assert.equal(again.snippetsSkipped, 1);

// Duplicates inside the bundle itself are double-checked too.
const selfDup = mergeDictionaryBundle([], [], {
  ...parsed.bundle!,
  dictionary: ["omsetning", "Omsetning"],
  es_terms: ["omsetning"],
  snippets: [snippet("ub", "ungdomsbedrift"), snippet("UB", "ungdomsbedrift")],
});
assert.deepEqual(selfDup.words, ["omsetning"]);
assert.equal(selfDup.wordsAdded, 1);
assert.equal(selfDup.wordsSkipped, 2);
assert.deepEqual(selfDup.snippets, [snippet("ub", "ungdomsbedrift")]);
assert.equal(selfDup.snippetsSkipped, 1);

// A conflicting snippet trigger is skipped, kept as the user wrote it, and
// reported instead of overwritten.
const conflict = mergeDictionaryBundle(
  ["aksjekapital"],
  [snippet("es", "min egen utvidelse")],
  {
    ...parsed.bundle!,
    dictionary: ["aksjekapital"],
    snippets: [snippet("ES", "noe annet")],
  },
);
assert.deepEqual(conflict.snippets, [snippet("es", "min egen utvidelse")]);
assert.deepEqual(conflict.conflicts, ["ES"]);
assert.equal(conflict.snippetsAdded, 0);
assert.equal(conflict.conflicts.length, 1);

// Bad files fail cleanly, never throw.
assert.deepEqual(parseDictionaryBundle("not json").error, {
  kind: "invalidJson",
});
assert.deepEqual(parseDictionaryBundle('{"a":1}').error, { kind: "notBundle" });
assert.deepEqual(
  parseDictionaryBundle('{"format":"flowstate-dictionary-bundle"}').error,
  { kind: "unsupportedVersion", version: 0 },
);
assert.deepEqual(
  parseDictionaryBundle(
    '{"format":"flowstate-dictionary-bundle","version":99,"dictionary":[]}',
  ).error,
  { kind: "unsupportedVersion", version: 99 },
);
assert.deepEqual(
  parseDictionaryBundle(
    '{"format":"flowstate-dictionary-bundle","version":1,"dictionary":[]}',
  ).error,
  { kind: "empty" },
);

// Malformed entries are tolerated; the valid ones still import.
const partial = parseDictionaryBundle(
  JSON.stringify({
    format: "flowstate-dictionary-bundle",
    version: 1,
    dictionary: ["budsjett", 42, "  "],
    snippets: [
      { trigger: "ue", expansion: "Ungt Entreprenørskap" },
      { bad: 1 },
    ],
  }),
);
assert.ok(partial.bundle);
assert.deepEqual(partial.bundle.dictionary, ["budsjett"]);
assert.equal(partial.bundle.snippets.length, 1);

console.log("dictionaryBundle: all assertions passed");
