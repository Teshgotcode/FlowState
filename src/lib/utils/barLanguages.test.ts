import assert from "node:assert/strict";
import { barLanguageOptions } from "./barLanguages";

// Configured languages are offered in order.
assert.deepEqual(barLanguageOptions(["no", "en"], ["de"]), ["no", "en"]);

// No configured languages: the suggestions stand in.
assert.deepEqual(barLanguageOptions([], ["nb", "en"]), ["nb", "en"]);
assert.deepEqual(barLanguageOptions(undefined, ["ja"]), ["ja"]);

// Duplicates collapse; "auto" is filtered (it is the menu's first item).
assert.deepEqual(barLanguageOptions(["no", "no", "en"], []), ["no", "en"]);
assert.deepEqual(barLanguageOptions(["auto", "en"], []), ["en"]);

// Whitespace-only entries are dropped.
assert.deepEqual(barLanguageOptions(["  ", "en"], []), ["en"]);
assert.deepEqual(barLanguageOptions(["", "fr"], []), ["fr"]);

// Order of the first occurrence is kept.
assert.deepEqual(barLanguageOptions(["b", "a", "b"], []), ["b", "a"]);

console.log("barLanguages: all assertions passed");
