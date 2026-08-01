import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { ArrowRight } from "lucide-react";
import type { CustomReplacement } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";

interface CustomReplacementsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Reemplazos exactos del Diccionario Vivo (F5.1): "Ruth" → "rut" dispara solo
 * con el token exacto (case-sensitive); "ruta" es otro token — colisión
 * imposible por diseño. Dos campos (de → a), calcado de las muletillas.
 */
export const CustomReplacements: React.FC<CustomReplacementsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const [from, setFrom] = useState("");
    const [to, setTo] = useState("");
    const replacements: CustomReplacement[] =
      getSetting("custom_replacements") || [];

    const sanitize = (value: string) =>
      value
        .trim()
        .replace(/[<>"']/g, "")
        .replace(/\s+/g, "");

    const fromClean = sanitize(from);
    const toClean = to.trim().replace(/[<>"']/g, "");
    const canAdd =
      fromClean.length > 0 &&
      fromClean.length <= 50 &&
      toClean.length > 0 &&
      toClean.length <= 50 &&
      !isUpdating("custom_replacements");

    const handleAdd = () => {
      if (!canAdd) return;
      if (replacements.some((r) => r.from === fromClean)) {
        toast.error(
          t("settings.advanced.customReplacements.duplicate", {
            word: fromClean,
          }),
        );
        return;
      }
      updateSetting("custom_replacements", [
        ...replacements,
        { from: fromClean, to: toClean },
      ]);
      setFrom("");
      setTo("");
    };

    const handleRemove = (target: CustomReplacement) => {
      updateSetting(
        "custom_replacements",
        replacements.filter((r) => r.from !== target.from),
      );
    };

    const handleKeyPress = (e: React.KeyboardEvent) => {
      if (e.key === "Enter") {
        e.preventDefault();
        handleAdd();
      }
    };

    return (
      <>
        <SettingContainer
          title={t("settings.advanced.customReplacements.title")}
          description={t("settings.advanced.customReplacements.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <div className="flex items-center gap-2">
            <Input
              type="text"
              className="max-w-28"
              value={from}
              onChange={(e) => setFrom(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t(
                "settings.advanced.customReplacements.fromPlaceholder",
              )}
              variant="compact"
              disabled={isUpdating("custom_replacements")}
            />
            <ArrowRight className="w-4 h-4 shrink-0 text-text/40" />
            <Input
              type="text"
              className="max-w-28"
              value={to}
              onChange={(e) => setTo(e.target.value)}
              onKeyDown={handleKeyPress}
              placeholder={t(
                "settings.advanced.customReplacements.toPlaceholder",
              )}
              variant="compact"
              disabled={isUpdating("custom_replacements")}
            />
            <Button
              onClick={handleAdd}
              disabled={!canAdd}
              variant="primary"
              size="md"
            >
              {t("settings.advanced.customReplacements.add")}
            </Button>
          </div>
        </SettingContainer>
        {replacements.length > 0 && (
          <div
            className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
          >
            {replacements.map((replacement) => (
              <Button
                key={replacement.from}
                onClick={() => handleRemove(replacement)}
                disabled={isUpdating("custom_replacements")}
                variant="secondary"
                size="sm"
                className="inline-flex items-center gap-1 cursor-pointer"
                aria-label={t("settings.advanced.customReplacements.remove", {
                  word: replacement.from,
                })}
              >
                <span>
                  {replacement.from} → {replacement.to}
                </span>
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
