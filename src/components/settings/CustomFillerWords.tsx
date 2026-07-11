import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

// Preset conservador de muletillas frecuentes en español latinoamericano.
// Solo entradas que casi nunca son texto intencional al dictar; es editable:
// el usuario puede quitar cualquiera (p. ej. "o sea" si la usa con sentido).
const ES_419_PRESET = [
  "eh",
  "ehm",
  "eeh",
  "em",
  "emm",
  "mmm",
  "hmm",
  "hm",
  "ajá",
  "o sea",
];

interface CustomFillerWordsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

export const CustomFillerWords: React.FC<CustomFillerWordsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [newWord, setNewWord] = useState("");
    // null/undefined = lista por defecto del idioma · [] = filtro apagado · [..] = lista propia
    const fillerWords = getSetting("custom_filler_words") ?? null;
    const mode =
      fillerWords === null
        ? "defaults"
        : fillerWords.length === 0
          ? "disabled"
          : "custom";
    const currentWords = fillerWords ?? [];

    const handleAddWord = () => {
      const sanitizedWord = newWord
        .trim()
        .replace(/[<>"']/g, "")
        .replace(/\s+/g, " ");
      if (sanitizedWord && sanitizedWord.length <= 50) {
        if (currentWords.includes(sanitizedWord)) {
          toast.error(
            t("settings.advanced.customFillerWords.duplicate", {
              word: sanitizedWord,
            }),
          );
          return;
        }
        updateSetting("custom_filler_words", [...currentWords, sanitizedWord]);
        setNewWord("");
      }
    };

    const handleRemoveWord = (wordToRemove: string) => {
      updateSetting(
        "custom_filler_words",
        currentWords.filter((word) => word !== wordToRemove),
      );
    };

    const handleApplyPreset = () => {
      const merged = [
        ...currentWords,
        ...ES_419_PRESET.filter((word) => !currentWords.includes(word)),
      ];
      updateSetting("custom_filler_words", merged);
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAddWord();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.customFillerWords.title")}
          description={t("settings.advanced.customFillerWords.description")}
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
              placeholder={t("settings.advanced.customFillerWords.placeholder")}
              variant="compact"
              disabled={isUpdating("custom_filler_words")}
            />
            <Button
              onClick={handleAddWord}
              disabled={
                !newWord.trim() ||
                newWord.trim().length > 50 ||
                isUpdating("custom_filler_words")
              }
              variant="primary"
              size="md"
            >
              {t("settings.advanced.customFillerWords.add")}
            </Button>
          </div>
        </SettingContainer>
        <div
          className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap items-center gap-2`}
        >
          <span className="text-xs opacity-70">
            {t(`settings.advanced.customFillerWords.mode.${mode}`)}
          </span>
          <Button
            onClick={handleApplyPreset}
            disabled={isUpdating("custom_filler_words")}
            variant="secondary"
            size="sm"
          >
            {t("settings.advanced.customFillerWords.applyPreset")}
          </Button>
          {mode !== "defaults" && (
            <Button
              onClick={() => updateSetting("custom_filler_words", null)}
              disabled={isUpdating("custom_filler_words")}
              variant="secondary"
              size="sm"
            >
              {t("settings.advanced.customFillerWords.resetDefaults")}
            </Button>
          )}
          {mode !== "disabled" && (
            <Button
              onClick={() => updateSetting("custom_filler_words", [])}
              disabled={isUpdating("custom_filler_words")}
              variant="secondary"
              size="sm"
            >
              {t("settings.advanced.customFillerWords.disable")}
            </Button>
          )}
        </div>
        {currentWords.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
          >
            {currentWords.map((word) => (
              <Button
                key={word}
                onClick={() => handleRemoveWord(word)}
                disabled={isUpdating("custom_filler_words")}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.advanced.customFillerWords.remove", {
                  word,
                })}
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
      </>
    );
  },
);
