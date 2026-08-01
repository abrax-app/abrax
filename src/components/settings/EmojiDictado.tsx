import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { useSettings } from "../../hooks/useSettings";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { EmojiDiccionario } from "./EmojiDiccionario";

interface EmojiDictadoProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Emoji dictado: «emoji cara feliz» → 🙂. Por tabla, sin ningún modelo de IA.
 *
 * Encendido de fábrica, al contrario que la autocorrección hablada: esta función
 * no puede dañar texto. Solo actúa detrás de la palabra «emoji» —que no aparece
 * por casualidad dictando prosa— y si no reconoce el nombre no toca nada. La
 * autocorrección va apagada porque BORRA; esto solo añade, y a petición.
 */
export const EmojiDictado: React.FC<EmojiDictadoProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const activo = getSetting("emoji_dictado") ?? true;
    const [verDiccionario, setVerDiccionario] = useState(false);

    return (
      <>
        <ToggleSwitch
          checked={activo}
          onChange={(value) => updateSetting("emoji_dictado", value)}
          isUpdating={isUpdating("emoji_dictado")}
          label={t("settings.advanced.emojiDictado.title")}
          description={t("settings.advanced.emojiDictado.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {activo && (
          <div
            className={`px-4 p-2 flex items-center justify-between gap-3 ${grouped ? "" : "rounded-lg border border-mid-gray/20"}`}
          >
            <p className="text-xs opacity-70">
              {t("settings.advanced.emojiDictado.ejemplo")}
            </p>
            {/* Sin esto la función era adivinanza: reconoce 1 531 nombres y no
                había forma de saber ninguno. */}
            <button
              type="button"
              onClick={() => setVerDiccionario(true)}
              className="text-xs underline underline-offset-2 opacity-80 hover:opacity-100 shrink-0"
            >
              {t("settings.advanced.emojiDictado.verDiccionario")}
            </button>
          </div>
        )}
        <EmojiDiccionario
          open={verDiccionario}
          onOpenChange={setVerDiccionario}
        />
      </>
    );
  },
);

EmojiDictado.displayName = "EmojiDictado";
