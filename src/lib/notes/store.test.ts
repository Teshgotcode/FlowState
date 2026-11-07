import assert from "node:assert/strict";
import { parseNotes, type Note } from "./store";

assert.deepEqual(parseNotes(null), []);
assert.deepEqual(parseNotes(""), []);
assert.deepEqual(parseNotes("not json"), []);

const full: Note = {
  id: "a1",
  title: "Hello",
  body: "World",
  createdAt: 1000,
  updatedAt: 2000,
};

const parsed = parseNotes(JSON.stringify([full]));
assert.equal(parsed.length, 1);
assert.equal(parsed[0].id, "a1");
assert.equal(parsed[0].title, "Hello");
assert.equal(parsed[0].body, "World");
assert.equal(parsed[0].createdAt, 1000);
assert.equal(parsed[0].updatedAt, 2000);

const sparse = parseNotes(JSON.stringify([{ id: "b2" }]));
assert.equal(sparse.length, 1);
assert.equal(sparse[0].id, "b2");
assert.equal(sparse[0].title, "");
assert.equal(sparse[0].body, "");
assert.equal(sparse[0].createdAt, 0);
assert.equal(sparse[0].updatedAt, 0);

const older: Note = {
  id: "old",
  title: "",
  body: "",
  createdAt: 1,
  updatedAt: 1,
};
const newer: Note = {
  id: "new",
  title: "",
  body: "",
  createdAt: 2,
  updatedAt: 5,
};
const ordered = parseNotes(JSON.stringify([older, newer]));
assert.deepEqual(
  ordered.map((n) => n.id),
  ["new", "old"],
);

assert.deepEqual(parseNotes(JSON.stringify({})), []);

const valid: Note = {
  id: "c3",
  title: "T",
  body: "B",
  createdAt: 3,
  updatedAt: 3,
  pinned: false,
};
assert.deepEqual(parseNotes(JSON.stringify([null, "x", valid])), [valid]);

// A note written before pinning existed still parses, defaulted to unpinned.
const legacy = parseNotes(
  JSON.stringify([
    { id: "d4", title: "T", body: "B", createdAt: 4, updatedAt: 4 },
  ]),
);
assert.equal(legacy[0].pinned, false);

assert.deepEqual(parseNotes(JSON.stringify([{ title: "no id" }])), []);

console.log("notes store: all assertions passed");
