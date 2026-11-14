import { open, save } from "@tauri-apps/plugin-dialog";
import { readFile } from "@tauri-apps/plugin-fs";
import type { Snippet } from "@/bindings";
import { commands } from "@/bindings";

/** Triggers are matched as whole words, so inner runs of space collapse. */
export const normalizeTrigger = (trigger: string) =>
  trigger.replace(/\s+/g, " ").trim();

/**
 * Parses a snippets JSON file. Returns a machine-readable error instead of
 * throwing so an invalid file never crashes the window; the caller turns it
 * into a clear, localized message.
 */
export type SnippetsParseError =
  | { kind: "invalidJson" }
  | { kind: "notSnippets" }
  | { kind: "badEntry"; index: number; reason: string };

export const parseSnippetsFile = (
  raw: string,
): { snippets: Snippet[]; error?: SnippetsParseError } => {
  let parsed: unknown;
  try {
    parsed = JSON.parse(raw);
  } catch {
    return { snippets: [], error: { kind: "invalidJson" } };
  }

  // Accept both a bare array and a `{ "snippets": [...] }` wrapper.
  const list = Array.isArray(parsed)
    ? parsed
    : typeof parsed === "object" &&
        parsed !== null &&
        Array.isArray((parsed as { snippets?: unknown }).snippets)
      ? (parsed as { snippets: unknown[] }).snippets
      : null;
  if (!list) {
    return { snippets: [], error: { kind: "notSnippets" } };
  }

  const snippets: Snippet[] = [];
  for (const [index, entry] of list.entries()) {
    const record = entry as Record<string, unknown> | null;
    if (
      typeof record?.trigger !== "string" ||
      !normalizeTrigger(record.trigger)
    ) {
      return {
        snippets: [],
        error: { kind: "badEntry", index: index + 1, reason: "trigger" },
      };
    }
    if (typeof record.expansion !== "string") {
      return {
        snippets: [],
        error: { kind: "badEntry", index: index + 1, reason: "expansion" },
      };
    }
    snippets.push({
      trigger: normalizeTrigger(record.trigger),
      expansion: record.expansion,
      case_sensitive: record.case_sensitive === true,
    });
  }
  return { snippets };
};

export const parseSnippetsError = (
  error: SnippetsParseError,
  t: (key: string, options?: Record<string, unknown>) => string,
): string => {
  switch (error.kind) {
    case "invalidJson":
      return t("settings.advanced.snippets.invalidJson");
    case "notSnippets":
      return t("settings.advanced.snippets.notSnippets");
    case "badEntry":
      return t("settings.advanced.snippets.badEntry", {
        index: error.index,
        field: error.reason,
      });
  }
};

/** The user picks the file; nothing leaves the machine on its own. */
export const pickSnippetsFile = async (): Promise<string | null> =>
  (await open({
    multiple: false,
    directory: false,
    filters: [{ name: "JSON", extensions: ["json"] }],
  })) as string | null;

export const pickSnippetsExportPath = async (): Promise<string | null> =>
  (await save({
    defaultPath: "flowstate-snippets.json",
    filters: [{ name: "JSON", extensions: ["json"] }],
  })) as string | null;

export const readSnippetsFile = async (path: string): Promise<string> => {
  const data = await readFile(path);
  return new TextDecoder("utf-8").decode(new Uint8Array(data));
};

/** True when the file was written. */
export const writeSnippetsFile = async (
  path: string,
  snippets: Snippet[],
): Promise<boolean> => {
  const result = await commands.exportTextFile(
    path,
    JSON.stringify(snippets, null, 2),
  );
  return result.status === "ok";
};
