/**
 * Screenshots of the hub preview, one per daily screen, for visual review.
 * Usage: bun scripts/shoot-hub.mjs [--theme light|dark]
 */
import { chromium } from "playwright";
import { mkdirSync } from "node:fs";

const theme = process.argv.includes("--theme")
  ? process.argv[process.argv.indexOf("--theme") + 1]
  : "light";
const outDir = "/Users/tesh/.hermes/cache/scratch/flow-shots";
mkdirSync(outDir, { recursive: true });

// The playwright browser download is not installed here; the desktop Chrome is.
const browser = await chromium.launch({ channel: "chrome" });
const page = await browser.newPage({
  viewport: { width: 1350, height: 850 },
  deviceScaleFactor: 2,
  colorScheme: theme,
});

page.on("console", (message) => {
  if (message.type() === "error") console.log("[console]", message.text());
});
page.on("pageerror", (error) => console.log("[pageerror]", error.message));

await page.goto("http://localhost:1420/src/preview/index.html", {
  waitUntil: "networkidle",
});
await page.waitForTimeout(900);
// The mock settings pin the theme, so the OS colour scheme never reaches CSS.
await page.evaluate((value) => {
  document.documentElement.dataset.theme = value;
}, theme);

const shots = [
  { name: "home", label: "Diktering" },
  { name: "notes", label: "Notatblokk" },
  { name: "insights", label: "Innsikt" },
];

for (const shot of shots) {
  await page.getByRole("button", { name: shot.label, exact: true }).click();
  await page.waitForTimeout(700);
  await page.screenshot({
    path: `${outDir}/${shot.name}-${theme}.png`,
    fullPage: false,
  });
  console.log("shot", shot.name);
}

await browser.close();
