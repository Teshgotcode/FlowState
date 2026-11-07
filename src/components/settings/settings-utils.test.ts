// Self-check for isExternalHost: the external-send notice must fire for
// every off-machine provider URL and for nothing that stays local. The
// rules are the approved design's host table, so they are the contract.
import assert from "node:assert/strict";
import { isExternalHost } from "./settings-utils";

const cases: [string, boolean][] = [
  ["https://api.openai.com/v1", true],
  ["http://localhost:11434", false],
  ["http://127.0.0.1:8080", false],
  ["http://[::1]:9000", false],
  ["apple-intelligence://local", false],
  ["", true],
  ["not a url", true],
];

for (const [baseUrl, expected] of cases) {
  assert.equal(
    isExternalHost(baseUrl),
    expected,
    `isExternalHost(${JSON.stringify(baseUrl)}) should be ${expected}`,
  );
}

console.log("isExternalHost: all assertions passed");
