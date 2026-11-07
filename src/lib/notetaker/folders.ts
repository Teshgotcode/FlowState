export interface Folder {
  id: string;
  name: string;
}

const FOLDERS_KEY = "flowstate.folders.v1";
const ASSIGN_KEY = "flowstate.notefolders.v1";

export type FolderStore = {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
};

export function parseFolders(raw: string | null): Folder[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter((f): f is Record<string, unknown> => typeof f === "object" && f !== null)
      .map((f) => ({ id: String(f.id ?? ""), name: String(f.name ?? "") }))
      .filter((f) => f.id && f.name);
  } catch {
    return [];
  }
}

/** noteId -> folderId. Unknown ids are dropped, so a deleted note leaves nothing behind. */
export function parseAssignments(
  raw: string | null,
  folders: Folder[],
): Record<string, string> {
  if (!raw) return {};
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    const known = new Set(folders.map((f) => f.id));
    const out: Record<string, string> = {};
    for (const [noteId, folderId] of Object.entries(parsed as Record<string, unknown>)) {
      const id = String(folderId);
      if (noteId && known.has(id)) out[noteId] = id;
    }
    return out;
  } catch {
    return {};
  }
}

const store = (): FolderStore | null => {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
};

export function loadFolders(): Folder[] {
  return parseFolders(store()?.getItem(FOLDERS_KEY) ?? null);
}

export function saveFolders(folders: Folder[]): void {
  store()?.setItem(FOLDERS_KEY, JSON.stringify(folders));
}

export function loadAssignments(): Record<string, string> {
  const raw = store()?.getItem(ASSIGN_KEY) ?? null;
  return parseAssignments(raw, loadFolders());
}

export function saveAssignments(map: Record<string, string>): void {
  store()?.setItem(ASSIGN_KEY, JSON.stringify(map));
}

export function newFolder(name: string): Folder {
  return { id: `f${Date.now().toString(36)}`, name: name.trim() };
}
