import assert from "node:assert/strict";
import { newFolder, parseAssignments, parseFolders } from "./folders";

assert.deepEqual(parseFolders(null), []);
assert.deepEqual(parseFolders("not json"), []);
assert.deepEqual(parseFolders('{"a":1}'), []);

const folders = parseFolders(
  JSON.stringify([
    { id: "f1", name: "Solan" },
    { id: "f2", name: "DNB" },
    { id: "", name: "broken" },
  ]),
);
assert.equal(folders.length, 2);
assert.equal(folders[0].name, "Solan");

// assignments pointing at a folder that no longer exists must not survive
assert.deepEqual(parseAssignments(null, folders), {});
assert.deepEqual(
  parseAssignments(JSON.stringify({ "12": "f1", "13": "gone" }), folders),
  { "12": "f1" },
);
assert.deepEqual(parseAssignments("[1,2]", folders), {});

const made = newFolder("  Møter  ");
assert.equal(made.name, "Møter");
assert.ok(made.id.startsWith("f"));

console.log("notetaker folders: all assertions passed");
