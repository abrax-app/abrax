import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "../../hooks/useSettings";
import type { ClipboardHandling, PasteMethod } from "@/bindings";

interface ClipboardHandlingProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Métodos que pegan pasando por el portapapeles. Con ellos el texto acaba ahí
 * de todas formas, así que el manejo del portapapeles queda fijado en «copiar
 * al portapapeles» y el desplegable se bloquea — el backend aplica la misma
 * regla en `effective_clipboard_handling` (src-tauri/src/clipboard.rs).
 */
const METODOS_VIA_PORTAPAPELES: PasteMethod[] = [
  "ctrl_v",
  "ctrl_shift_v",
  "shift_insert",
];

export const ClipboardHandlingSetting: React.FC<ClipboardHandlingProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const clipboardHandlingOptions = [
      {
        value: "dont_modify",
        label: t("settings.advanced.clipboardHandling.options.dontModify"),
      },
      {
        value: "copy_to_clipboard",
        label: t("settings.advanced.clipboardHandling.options.copyToClipboard"),
      },
    ];

    const pasteMethod = (getSetting("paste_method") || "ctrl_v") as PasteMethod;
    const fijadoPorPegado = METODOS_VIA_PORTAPAPELES.includes(pasteMethod);

    const selectedHandling = fijadoPorPegado
      ? ("copy_to_clipboard" as ClipboardHandling)
      : ((getSetting("clipboard_handling") ||
          "dont_modify") as ClipboardHandling);

    return (
      <SettingContainer
        title={t("settings.advanced.clipboardHandling.title")}
        description={
          fijadoPorPegado
            ? t("settings.advanced.clipboardHandling.lockedByPasteMethod")
            : t("settings.advanced.clipboardHandling.description")
        }
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Dropdown
          options={clipboardHandlingOptions}
          selectedValue={selectedHandling}
          onSelect={(value) =>
            updateSetting("clipboard_handling", value as ClipboardHandling)
          }
          disabled={fijadoPorPegado || isUpdating("clipboard_handling")}
        />
      </SettingContainer>
    );
  });
