import { suggestedLanguages } from "@/lib/utils/languageSuggestions";

/**
 * Which languages the overlay bar's quick picker offers (Wispr parity:
 * switch the dictation language without opening Settings).
 *
 * The user's configured spoken languages come first; when none are
 * configured, the system-derived suggestions stand in. "auto" is never
 * offered as a pinned language (it is always the first menu item instead).
 */
export function barLanguageOptions(
  spoken: string[] | undefined,
  suggested: string[] = suggestedLanguages(),
): string[] {
  const source = spoken?.length ? spoken : suggested;
  const seen = new Set<string>();
  const options: string[] = [];
  for (const code of source) {
    const normalized = code?.trim();
    if (!normalized || normalized === "auto" || seen.has(normalized)) continue;
    seen.add(normalized);
    options.push(normalized);
  }
  return options;
}
