import React, { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { commands, type WordSuggestion } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";

interface CustomWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const normalizeCustomWord = (word: string) =>
  word
    .replace(/[<>"']/g, "")
    .replace(/\s+/g, " ")
    .trim();

export const CustomWords: React.FC<CustomWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { settings, getSetting, updateSetting, isUpdating } = useSettings();
    const [newWord, setNewWord] = useState("");
    const [suggestions, setSuggestions] = useState<WordSuggestion[]>([]);
    const customWords = getSetting("custom_words") || [];
    const normalizedWord = normalizeCustomWord(newWord);
    // Learning is on unless the loaded settings say otherwise.
    const learnFromCorrections = getSetting("learn_from_corrections") ?? true;
    const autoAddLearned = getSetting("add_learned_words_automatically") ?? false;
    const starred = getSetting("starred_words") || [];
    const learned = getSetting("learned_words") || [];
    const sort = getSetting("dictionary_sort") || "az";

    // Starred words hold the top of the list whatever the sort is; the sort
    // only decides the order inside each half.
    const ordered = useMemo(() => {
      const base =
        sort === "newest"
          ? [...customWords].reverse()
          : [...customWords].sort((a, b) => a.localeCompare(b, "nb"));
      return base.sort(
        (a, b) => Number(starred.includes(b)) - Number(starred.includes(a)),
      );
    }, [customWords, sort, starred]);

    const toggleStar = (word: string) => {
      const next = starred.includes(word)
        ? starred.filter((w) => w !== word)
        : [...starred, word];
      void updateSetting("starred_words", next);
    };

    const handleAddWord = () => {
      if (normalizedWord && normalizedWord.length <= 50) {
        if (customWords.includes(normalizedWord)) {
          toast.error(
            t("settings.advanced.customWords.duplicate", {
              word: normalizedWord,
            }),
          );
          return;
        }
        updateSetting("custom_words", [...customWords, normalizedWord]);
        setNewWord("");
      }
    };

    const handleRemoveWord = (wordToRemove: string) => {
      updateSetting(
        "custom_words",
        customWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    // Suggestions live in the backend's local tally; they depend on the
    // dictionary, so they are refetched whenever the words change.
    useEffect(() => {
      let cancelled = false;
      const load = async () => {
        const result = await commands.getWordSuggestions();
        if (!cancelled && result.status === "ok") {
          setSuggestions(result.data ?? []);
        }
      };
      void load();
      return () => {
        cancelled = true;
      };
    }, [settings?.custom_words, learnFromCorrections]);

    const handleAcceptSuggestion = async (word: string) => {
      const result = await commands.acceptWordSuggestion(word);
      if (result.status === "error") {
        toast.error(
          t("settings.advanced.customWords.suggestionError", { word }),
        );
        return;
      }
      // The backend retires the suggestion; the word itself goes through
      // the same path as a typed one, so it is used by the next
      // transcription.
      if (
        !customWords.some(
          (existing) => existing.toLowerCase() === word.toLowerCase(),
        )
      ) {
        updateSetting("custom_words", [...customWords, word]);
        if (!learned.includes(word)) {
          updateSetting("learned_words", [...learned, word]);
        }
      }
      setSuggestions((prev) => prev.filter((s) => s.word !== word));
    };

    const handleIgnoreSuggestion = async (word: string) => {
      const result = await commands.ignoreWordSuggestion(word);
      if (result.status === "error") {
        toast.error(
          t("settings.advanced.customWords.suggestionError", { word }),
        );
        return;
      }
      setSuggestions((prev) => prev.filter((s) => s.word !== word));
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.customWords.title")}
          description={t("settings.advanced.customWords.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-40"
              value={newWord}
              onChange={(e) => setNewWord(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t("settings.advanced.customWords.placeholder")}
              variant="compact"
              disabled={isUpdating("custom_words")}
            />
            <Button
              onClick={handleAddWord}
              disabled={
                !normalizedWord ||
                normalizedWord.length > 50 ||
                isUpdating("custom_words")
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.customWords.add")}
            </Button>
          </div>
        </SettingContainer>
        {customWords.length > 0 && (
          <div
            className={`overflow-hidden ${grouped ? "" : "rounded-lg border border-line"}`}
          >
            <div className="flex h-[36px] items-center justify-between border-b border-line px-4">
              <span className="text-[12px] text-ink-3">
                {customWords.length}{" "}
                {t("settings.advanced.customWords.countLabel")}
              </span>
              <button
                type="button"
                onClick={() =>
                  updateSetting(
                    "dictionary_sort",
                    sort === "az" ? "newest" : "az",
                  )
                }
                className="cursor-pointer rounded-[6px] px-[7px] py-[3px] text-[12px] font-[550] text-ink-2 transition-colors hover:bg-wash hover:text-text"
              >
                {sort === "az"
                  ? t("settings.advanced.customWords.sortAz")
                  : t("settings.advanced.customWords.sortNewest")}
              </button>
            </div>
            {ordered.map((word, index) => (
              <div
                key={word}
                className={`flex h-[52px] items-center justify-between px-4 ${
                  index > 0 ? "border-t border-line" : ""
                }`}
              >
                <span className="flex min-w-0 items-center gap-[8px]">
                  <button
                    type="button"
                    onClick={() => toggleStar(word)}
                    aria-pressed={starred.includes(word)}
                    aria-label={t(
                      starred.includes(word)
                        ? "settings.advanced.customWords.unstar"
                        : "settings.advanced.customWords.star",
                      { word },
                    )}
                    className={`flex h-[24px] w-[24px] shrink-0 cursor-pointer items-center justify-center rounded-[6px] transition-colors hover:bg-wash ${
                      starred.includes(word) ? "text-logo-primary" : "text-ink-3"
                    }`}
                  >
                    <svg
                      viewBox="0 0 24 24"
                      className="h-[13px] w-[13px]"
                      fill={starred.includes(word) ? "currentColor" : "none"}
                      stroke="currentColor"
                      strokeWidth={1.8}
                      aria-hidden="true"
                    >
                      <path
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        d="m12 3.6 2.6 5.3 5.8.8-4.2 4.1 1 5.8-5.2-2.8-5.2 2.8 1-5.8L3.6 9.7l5.8-.8z"
                      />
                    </svg>
                  </button>
                  <span className="text-[15px] font-[550] text-text">{word}</span>
                  {learned.includes(word) && (
                    <span
                      title={t("settings.advanced.customWords.learned")}
                      className="rounded-[5px] bg-wash px-[5px] py-[1px] text-[10.5px] font-[550] text-ink-3"
                    >
                      {t("settings.advanced.customWords.learned")}
                    </span>
                  )}
                </span>
                <button
                  type="button"
                  onClick={() => handleRemoveWord(word)}
                  disabled={isUpdating("custom_words")}
                  aria-label={t("settings.advanced.customWords.remove", { word })}
                  className="flex h-[28px] w-[28px] cursor-pointer items-center justify-center rounded-[6px] text-ink-3 transition-colors hover:bg-wash hover:text-text disabled:cursor-not-allowed disabled:opacity-40"
                >
                  <svg
                    className="h-3.5 w-3.5"
                    fill="none"
                    stroke="currentColor"
                    viewBox="0 0 24 24"
                  >
                    <path
                      strokeLinecap="round"
                      strokeLinejoin="round"
                      strokeWidth={2}
                      d="M6 18L18 6M6 6l12 12"
                    />
                  </svg>
                </button>
              </div>
            ))}
          </div>
        )}
        <ToggleSwitch
          checked={learnFromCorrections}
          onChange={(value) =>
            updateSetting("learn_from_corrections", value as boolean)
          }
          isUpdating={isUpdating("learn_from_corrections")}
          label={t("settings.advanced.customWords.learnTitle")}
          description={t("settings.advanced.customWords.learnDescription")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {learnFromCorrections && (
          <ToggleSwitch
            checked={autoAddLearned}
            onChange={(value) =>
              updateSetting(
                "add_learned_words_automatically",
                value as boolean,
              )
            }
            isUpdating={isUpdating("add_learned_words_automatically")}
            label={t("settings.advanced.customWords.autoAddTitle")}
            description={t("settings.advanced.customWords.autoAddDescription")}
            descriptionMode={descriptionMode}
            grouped={grouped}
          />
        )}
        {/* With auto-add on, threshold-reaching words never queue as
            suggestions, so the list (whose copy says "nothing is added
            automatically") would only ever show its empty state. */}
        {learnFromCorrections && !autoAddLearned && (
          <SettingContainer
            title={t("settings.advanced.customWords.suggestionsTitle")}
            description={t(
              "settings.advanced.customWords.suggestionsDescription",
            )}
            descriptionMode={descriptionMode}
            grouped={grouped}
          >
            {suggestions.length === 0 ? (
              <p className="text-sm text-ink-3">
                {t("settings.advanced.customWords.suggestionsEmpty")}
              </p>
            ) : (
              <ul className="flex flex-col gap-2">
                {suggestions.map((suggestion) => (
                  <li
                    key={suggestion.word}
                    className="flex items-center justify-between gap-3"
                  >
                    <span className="flex min-w-0 items-center gap-2 text-sm">
                      <span className="truncate font-medium">
                        {suggestion.word}
                      </span>
                      <span className="shrink-0 text-xs text-ink-3">
                        {t("settings.advanced.customWords.suggestionCount", {
                          count: suggestion.count,
                        })}
                      </span>
                    </span>
                    <span className="flex shrink-0 items-center gap-1">
                      <Button
                        onClick={() =>
                          void handleAcceptSuggestion(suggestion.word)
                        }
                        disabled={isUpdating("custom_words")}
                        variant="primary"
                        size="sm"
                        aria-label={t(
                          "settings.advanced.customWords.suggestionAdd",
                          { word: suggestion.word },
                        )}
                      >
                        {t("settings.advanced.customWords.suggestionAdd", {
                          word: suggestion.word,
                        })}
                      </Button>
                      <Button
                        onClick={() =>
                          void handleIgnoreSuggestion(suggestion.word)
                        }
                        variant="secondary"
                        size="sm"
                        aria-label={t(
                          "settings.advanced.customWords.suggestionIgnore",
                          { word: suggestion.word },
                        )}
                      >
                        {t("settings.advanced.customWords.suggestionIgnore", {
                          word: suggestion.word,
                        })}
                      </Button>
                    </span>
                  </li>
                ))}
              </ul>
            )}
          </SettingContainer>
        )}
      </>
    );
  },
);
