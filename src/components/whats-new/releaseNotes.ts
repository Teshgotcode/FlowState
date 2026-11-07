export interface ReleaseNote {
  version: string;
  markdown: string;
}

interface ReleaseNoteRecord {
  version: string;
  /** Language the note was written in; English is the fallback. */
  locale: string;
  markdown: string;
}

interface FindReleaseNoteOptions {
  currentVersion: string;
  lastSeenVersion: string;
  /** UI language: its translation is used when one exists. */
  locale?: string;
}

const releaseNoteModules = import.meta.glob<string>(
  "../../content/release-notes/*.md",
  {
    eager: true,
    import: "default",
    query: "?raw",
  },
);

const releaseNotesByVersion = new Map<string, ReleaseNoteRecord>();

const parseVersion = (version: string): [number, number, number] | null => {
  const normalized = version.trim().replace(/^v/i, "");
  const match = normalized.match(/^(\d+)\.(\d+)\.(\d+)/);
  if (!match) return null;

  return [Number(match[1]), Number(match[2]), Number(match[3])];
};

for (const [path, markdown] of Object.entries(releaseNoteModules)) {
  // `<version>.md` is English; `<version>.<locale>.md` is a translation.
  const match = path.match(/release-notes\/([^/]+?)(?:\.([a-z]{2}))?\.md$/);
  if (!match) continue;

  const version = match[1];
  const locale = match[2] ?? "en";

  if (!parseVersion(version)) continue;

  const key = `${version}:${locale}`;
  if (releaseNotesByVersion.has(key)) continue;

  releaseNotesByVersion.set(key, {
    version,
    locale,
    markdown,
  });
}

const compareVersions = (a: string, b: string) => {
  const parsedA = parseVersion(a);
  const parsedB = parseVersion(b);

  if (!parsedA || !parsedB) return 0;

  for (let index = 0; index < parsedA.length; index += 1) {
    if (parsedA[index] !== parsedB[index]) {
      return parsedA[index] - parsedB[index];
    }
  }

  return 0;
};

const forLocale = (note: ReleaseNoteRecord, locale: string): ReleaseNote => {
  const translated = releaseNotesByVersion.get(`${note.version}:${locale}`);
  const chosen = translated ?? note;
  return { version: chosen.version, markdown: chosen.markdown };
};

export const findReleaseNoteToShow = ({
  currentVersion,
  lastSeenVersion,
  locale = "en",
}: FindReleaseNoteOptions): ReleaseNote | null => {
  if (!parseVersion(currentVersion)) return null;

  const hasValidLastSeenVersion = parseVersion(lastSeenVersion) !== null;
  const candidate = Array.from(releaseNotesByVersion.values())
    .filter((note) => compareVersions(note.version, currentVersion) <= 0)
    .filter(
      (note) =>
        !hasValidLastSeenVersion ||
        compareVersions(note.version, lastSeenVersion) > 0,
    )
    .sort((a, b) => compareVersions(b.version, a.version))[0];

  if (!candidate) return null;

  return forLocale(candidate, locale);
};

export const findLatestReleaseNote = (locale = "en"): ReleaseNote | null => {
  const candidate = Array.from(releaseNotesByVersion.values()).sort((a, b) =>
    compareVersions(b.version, a.version),
  )[0];

  if (!candidate) return null;

  return forLocale(candidate, locale);
};
