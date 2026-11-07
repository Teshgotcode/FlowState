export interface Note {
  id: string;
  title: string;
  body: string;
  createdAt: number;
  updatedAt: number;
  pinned?: boolean;
}

const STORAGE_KEY = "flowstate.notes.v1";

// localStorage is the smallest store that survives restarts and never leaves the machine.
let corruptLogged = false;

export function parseNotes(raw: string | null): Note[] {
  if (!raw) return [];
  try {
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) return [];
    return parsed
      .filter(
        (n): n is Record<string, unknown> =>
          typeof n === "object" && n !== null,
      )
      .map((n) => ({
        id: String(n.id ?? ""),
        title: String(n.title ?? ""),
        body: String(n.body ?? ""),
        createdAt: Number(n.createdAt ?? 0),
        updatedAt: Number(n.updatedAt ?? 0),
        pinned: Boolean(n.pinned ?? false),
      }))
      .filter((n) => n.id)
      .sort((a, b) => b.updatedAt - a.updatedAt);
  } catch {
    if (!corruptLogged) {
      corruptLogged = true;
      console.warn("flowstate.notes.v1: corrupt JSON, starting fresh");
    }
    return [];
  }
}

function load(): Note[] {
  return parseNotes(localStorage.getItem(STORAGE_KEY));
}

function persist(notes: Note[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(notes));
}

export function listNotes(): Note[] {
  return load();
}

export function saveNote(note: Note): void {
  const notes = load().filter((n) => n.id !== note.id);
  notes.unshift(note);
  persist(notes);
}

export function toggleNotePin(id: string): void {
  const notes = load();
  const note = notes.find((n) => n.id === id);
  if (!note) return;
  note.pinned = !note.pinned;
  persist(notes);
}

export function deleteNote(id: string): void {
  persist(load().filter((n) => n.id !== id));
}
