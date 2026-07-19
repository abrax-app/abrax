import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { convertFileSrc } from "@tauri-apps/api/core";
import { Button } from "../ui/Button";
import { SettingContainer } from "../ui/SettingContainer";
import { AudioPlayer } from "../ui/AudioPlayer";
import { commands } from "@/bindings";
import type { PruebaMicrofono as ResultadoPrueba } from "@/bindings";

interface PruebaMicrofonoProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const DURACION_MS = 3000;

type Estado =
  | { fase: "idle" }
  | { fase: "grabando" }
  | { fase: "error"; mensaje: string }
  | { fase: "resultado"; r: ResultadoPrueba };

/**
 * Prueba de micrófono: graba 3 segundos de la MISMA señal que recibiría el
 * modelo (16 kHz mono, post-captura, sin VAD), la reproduce, y da un veredicto
 * con números (RMS/pico en dBFS). Además revela qué dispositivo real hay
 * detrás de «Default» — en Windows el default de captura (rol Console) puede
 * ser un micrófono distinto del que usan las apps de llamadas.
 */
export const PruebaMicrofono: React.FC<PruebaMicrofonoProps> = ({
  descriptionMode = "tooltip",
  grouped = false,
}) => {
  const { t } = useTranslation();
  const [estado, setEstado] = useState<Estado>({ fase: "idle" });

  const grabar = () => {
    setEstado({ fase: "grabando" });
    void commands.probarMicrofono(DURACION_MS).then((res) => {
      if (res.status === "ok") {
        setEstado({ fase: "resultado", r: res.data });
      } else {
        setEstado({ fase: "error", mensaje: res.error });
      }
    });
  };

  const colorVeredicto = (v: ResultadoPrueba["veredicto"]) =>
    v === "sano"
      ? "text-green-500"
      : v === "bajo"
        ? "text-yellow-500"
        : "text-red-500";

  return (
    <>
      <SettingContainer
        title={t("pruebaMic.title")}
        description={t("pruebaMic.description")}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <Button
          variant="secondary"
          size="sm"
          onClick={grabar}
          disabled={estado.fase === "grabando"}
        >
          {estado.fase === "grabando"
            ? t("pruebaMic.grabando")
            : t("pruebaMic.boton")}
        </Button>
      </SettingContainer>

      {estado.fase === "error" && (
        <div className="px-4 py-2 text-xs text-red-500">{estado.mensaje}</div>
      )}

      {estado.fase === "resultado" && (
        <div className="px-4 py-3 space-y-2 text-xs" aria-live="polite">
          <div className={`font-medium ${colorVeredicto(estado.r.veredicto)}`}>
            {t(`pruebaMic.veredicto.${estado.r.veredicto}`)}
          </div>
          <div className="opacity-70">
            {t("pruebaMic.dispositivo", {
              nombre: estado.r.dispositivo,
              contexto: estado.r.es_default
                ? t("pruebaMic.esElDefault")
                : t("pruebaMic.esElegido"),
            })}
          </div>
          <div className="opacity-70 font-mono">
            {t("pruebaMic.niveles", {
              rms: estado.r.rms_db.toFixed(1),
              pico: estado.r.pico_db.toFixed(1),
            })}
            {estado.r.clip_pct > 0.05
              ? t("pruebaMic.nivelClip", {
                  clip: estado.r.clip_pct.toFixed(1),
                })
              : ""}
          </div>
          <AudioPlayer src={convertFileSrc(estado.r.wav, "asset")} />
        </div>
      )}
    </>
  );
};
