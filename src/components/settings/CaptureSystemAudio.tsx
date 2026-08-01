import React from "react";
import { useTranslation } from "react-i18next";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { useSettings } from "../../hooks/useSettings";

interface CaptureSystemAudioProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Capturar lo que SUENA en el equipo en vez del micrófono: una clase, un video,
 * una reunión.
 *
 * Vive en la pantalla «Streaming» y no en «Escucha» porque no es una variante
 * del dictado: pide un modelo que reconozca habla de reuniones, y la propia app
 * ya lo dice cuando falta (`errors.sistemaSinModeloApto` manda a descargar
 * Nemotron). Ponerlo junto a los modelos que lo permiten es lo que evita que
 * alguien lo encienda y no entienda por qué no sale nada.
 *
 * El ajuste ya existía y se accionaba desde un chip del inicio; esto solo le da
 * una fila con nombre y explicación, sin tocar el comportamiento.
 */
export const CaptureSystemAudio: React.FC<CaptureSystemAudioProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();

    const activo = getSetting("capture_system_audio") ?? false;

    return (
      <ToggleSwitch
        checked={activo}
        onChange={(enabled) => updateSetting("capture_system_audio", enabled)}
        isUpdating={isUpdating("capture_system_audio")}
        label={t("settings.streaming.systemAudio.label")}
        description={t("settings.streaming.systemAudio.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      />
    );
  },
);

CaptureSystemAudio.displayName = "CaptureSystemAudio";
