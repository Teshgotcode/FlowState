// The root NOTICE file is the single source of truth for third-party
// attribution (currently the NVIDIA Open Model License for the Sortformer
// diarization model). Tauri bundles everything under src-tauri/resources/
// with every app bundle (`resources/**/*` in tauri.conf.json), so the NOTICE
// is copied there to ship with the distribution, not just with the repo.
//
//   bun run sync:notice    copy the root NOTICE into src-tauri/resources/
//   bun run check:notice   fail if the bundled copy differs (CI)
import { readFileSync, writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const source = fileURLToPath(new URL("../NOTICE", import.meta.url));
const target = fileURLToPath(
  new URL("../src-tauri/resources/NOTICE", import.meta.url),
);

const expected = readFileSync(source, "utf8");

// Only a missing copy counts as "differs"; any other read error is rethrown
// rather than overwriting a file that couldn't be read.
let current: string | undefined;
try {
  current = readFileSync(target, "utf8");
} catch (e) {
  if ((e as NodeJS.ErrnoException).code !== "ENOENT") throw e;
}

if (current === expected) {
  console.log("src-tauri/resources/NOTICE matches the root NOTICE file");
} else if (process.argv.includes("--check")) {
  console.error(
    "src-tauri/resources/NOTICE differs from the root NOTICE file; run `bun run sync:notice`",
  );
  process.exit(1);
} else {
  writeFileSync(target, expected);
  console.log("src-tauri/resources/NOTICE synced from the root NOTICE file");
}
