// The Rust app compiles in a copy of @handy-computer/models' catalog.json
// (src-tauri can't depend on node_modules: cargo-only builds never run bun
// install). This keeps the copy identical to the version pinned in
// package.json — after applying the fork's local catalog patches below.
//
//   bun run sync:catalog    copy the patched catalog.json into src-tauri
//   bun run check:catalog   fail if the copy differs (CI)
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

// Local corrections to the upstream @handy-computer/models metadata. Applied
// on top of the package's catalog.json so `sync` and `check` stay
// deterministic; both fail loudly if upstream moves away from a patch, so
// the list gets revisited instead of silently diverging.
type CatalogPatch = {
  model: string;
  field: string;
  from: string;
  to: string;
  reason: string;
};

const patches: CatalogPatch[] = [
  {
    // HF model card: license "other", license_name "nvidia-open-model-license"
    // (NVIDIA Open Model License, which requires the NOTICE attribution; see
    // the NOTICE file shipped with the app).
    model: "diar_streaming_sortformer_4spk-v2.1",
    field: "license",
    from: "other",
    to: "nvidia-open-model-license",
    reason: "NVIDIA Open Model License per the upstream model card",
  },
];

function applyPatches(catalog: string): string {
  for (const patch of patches) {
    const id = `"id": ${JSON.stringify(patch.model)}`;
    const start = catalog.indexOf(id);
    if (start === -1) {
      throw new Error(`catalog patch: model "${patch.model}" not found`);
    }
    // The model object ends where the next model's "id" begins (or EOF).
    const end = catalog.indexOf(`"id":`, start + id.length);
    const block = catalog.slice(start, end === -1 ? undefined : end);
    const field = new RegExp(`("${patch.field}":\\s*)"([^"]*)"`);
    const match = field.exec(block);
    if (!match) {
      throw new Error(
        `catalog patch: field "${patch.field}" not found on "${patch.model}"`,
      );
    }
    if (match[2] !== patch.from) {
      throw new Error(
        `catalog patch: "${patch.model}.${patch.field}" is "${match[2]}", expected "${patch.from}"; update patches in scripts/sync-catalog.ts`,
      );
    }
    catalog =
      catalog.slice(0, start) +
      block.replace(field, `$1${JSON.stringify(patch.to)}`) +
      (end === -1 ? "" : catalog.slice(end));
  }
  return catalog;
}

const require = createRequire(import.meta.url);
const source = require.resolve("@handy-computer/models/catalog.json");
const target = fileURLToPath(
  new URL("../src-tauri/src/catalog/catalog.json", import.meta.url),
);
// The package's exports don't expose package.json; it sits beside the catalog.
const { version } = JSON.parse(
  readFileSync(join(dirname(source), "package.json"), "utf8"),
) as { version: string };

const expected = applyPatches(readFileSync(source, "utf8"));
// Only a missing copy counts as "differs"; any other read error is rethrown
// rather than overwriting a file that couldn't be read.
let current: string | undefined;
try {
  current = readFileSync(target, "utf8");
} catch (e) {
  if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
}

if (current === expected) {
  console.log(
    `catalog.json matches @handy-computer/models@${version} + local patches`,
  );
} else if (process.argv.includes("--check")) {
  console.error(
    `src-tauri/src/catalog/catalog.json differs from @handy-computer/models@${version} + local patches; run \`bun run sync:catalog\``,
  );
  process.exit(1);
} else {
  writeFileSync(target, expected);
  console.log(
    `catalog.json synced from @handy-computer/models@${version} + local patches`,
  );
}
