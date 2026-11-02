import React, { useEffect, useState } from "react";
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
          setSuggestions(result.data);
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
        toast.error(t("settings.advanced.customWords.suggestionError", { word }));
        return;
      }
      // The backend retires the suggestion; the word itself goes through
      // the same path as a typed one, so it is used by the next
      // transcription.
      if (!customWords.some((existing) => existing.toLowerCase() === word.toLowerCase())) {
        updateSetting("custom_words", [...customWords, word]);
      }
      setSuggestions((prev) => prev.filter((s) => s.word !== word));
    };

    const handleIgnoreSuggestion = async (word: string) => {
      const result = await commands.ignoreWordSuggestion(word);
      if (result.status === "error") {
        toast.error(t("settings.advanced.customWords.suggestionError", { word }));
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
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-line"} flex flex-wrap gap-1`}
          >
            {customWords.map((word) => (
              <Button
                key={word}
                onClick={() => handleRemoveWord(word)}
                disabled={isUpdating("custom_words")}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.advanced.customWords.remove", { word })}
              >
                <span>{word}</span>
                <svg
                  className="w-3 h-3"
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
              </Button>
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
                        onClick={() => void handleAcceptSuggestion(suggestion.word)}
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
                        onClick={() => void handleIgnoreSuggestion(suggestion.word)}
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
