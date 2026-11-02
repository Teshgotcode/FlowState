// Splits text around every case-insensitive match of a query so the UI can
// wrap the hits in a <mark>. This mirrors the backend's LIKE search: a plain
// substring that matches in either case.

export interface HighlightPart {
  text: string;
  match: boolean;
}

export function splitHighlight(text: string, query: string): HighlightPart[] {
  const needle = query.trim();
  if (!needle) return [{ text, match: false }];

  const haystack = text.toLowerCase();
  const target = needle.toLowerCase();
  const parts: HighlightPart[] = [];
  let cursor = 0;

  while (cursor < text.length) {
    const hit = haystack.indexOf(target, cursor);
    if (hit === -1) {
      parts.push({ text: text.slice(cursor), match: false });
      break;
    }
    if (hit > cursor) {
      parts.push({ text: text.slice(cursor, hit), match: false });
    }
    parts.push({ text: text.slice(hit, hit + target.length), match: true });
    cursor = hit + target.length;
  }

  return parts;
}
