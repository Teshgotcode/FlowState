import assert from "node:assert/strict";
import { splitHighlight } from "./searchHighlight";

// Case-insensitive matches are marked wherever they appear.
assert.deepEqual(splitHighlight("Hello world, hello again", "hello"), [
  { text: "Hello", match: true },
  { text: " world, ", match: false },
  { text: "hello", match: true },
  { text: " again", match: false },
]);

// An empty query marks nothing; the text comes back as one plain part.
assert.deepEqual(splitHighlight("plain text", ""), [
  { text: "plain text", match: false },
]);

// A query with no hits keeps the whole text in one part.
assert.deepEqual(splitHighlight("no match", "zzz"), [
  { text: "no match", match: false },
]);

// Adjacent matches stay separate instead of merging into one.
assert.deepEqual(splitHighlight("abab", "ab"), [
  { text: "ab", match: true },
  { text: "ab", match: true },
]);

// Empty text yields no parts, matching how the backend returns no entry.
assert.deepEqual(splitHighlight("", "query"), []);

// A query longer than the text simply doesn't match.
assert.deepEqual(splitHighlight("hi", "high"), [{ text: "hi", match: false }]);

console.log("searchHighlight: all assertions passed");
