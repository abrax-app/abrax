import React, { useCallback, useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import { commands } from "@/bindings";
import type { CorreccionModo, CorreccionMotor } from "@/bindings";

interface CorreccionSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const MOTOR_OPTIONS: CorreccionMotor[] = [
  "auto",
  "solo_reglas",
  "modelo",
  "desactivado",
];
const MODO_OPTIONS: CorreccionModo[] = ["literal", "limpio", "pulido"];

/** Estado de la detección de Ollama para la línea informativa. */
type EstadoOllama =
  | { clase: "cargando" }
  | { clase: "no_detectado" }
  | { clase: "sin_modelos" }
  | { clase: "detectado"; modelo: string };

/**
 * Ajustes del módulo de corrección local (`correccion_motor` +
 * `correccion_modo`). El motor nace `desactivado` (passthrough exacto); al
 * elegir Auto o Modelo local se consulta el estado de Ollama en 127.0.0.1 y se
 * explica en una línea qué va a pasar de verdad — si no hay modelo, la app usa
 * las reglas deterministas y nunca deja al usuario sin texto.
 */
export const CorreccionSettings: React.FC<CorreccionSettingsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { settings, updateSetting } = useSettings();

    const motor: CorreccionMotor = settings?.correccion_motor ?? "desactivado";
    const modo: CorreccionModo = settings?.correccion_modo ?? "literal";
    const usaModelo = motor === "auto" || motor === "modelo";

    const [estado, setEstado] = useState<EstadoOllama>({ clase: "cargando" });

    const consultarOllama = useCallback(() => {
      setEstado({ clase: "cargando" });
      void commands.detectarCorreccionOllama().then((res) => {
        if (res.status !== "ok" || res.data === null) {
          setEstado({ clase: "no_detectado" });
        } else if (res.data.length === 0) {
          setEstado({ clase: "sin_modelos" });
        } else {
          setEstado({ clase: "detectado", modelo: res.data[0] });
        }
      });
    }, []);

    useEffect(() => {
      if (usaModelo) {
        consultarOllama();
      }
    }, [usaModelo, consultarOllama]);

    const lineaEstado = () => {
      switch (estado.clase) {
        case "cargando":
          return "…";
        case "detectado":
          return t("correccion.estado.detectado", { modelo: estado.modelo });
        case "sin_modelos":
          return t("correccion.estado.sinModelos");
        case "no_detectado":
          return t("correccion.estado.noDetectado");
      }
    };

    return (
      <>
        <SettingContainer
          title={t("correccion.motor.title")}
          description={t("correccion.motor.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        >
          <Dropdown
            options={MOTOR_OPTIONS.map((value) => ({
              value,
              label: t(`correccion.motor.options.${value}`),
            }))}
            selectedValue={motor}
            onSelect={(value) =>
              updateSetting("correccion_motor", value as CorreccionMotor)
            }
          />
        </SettingContainer>

        <SettingContainer
          title={t("correccion.modo.title")}
          description={t("correccion.modo.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          disabled={motor === "desactivado"}
        >
          <Dropdown
            options={MODO_OPTIONS.map((value) => ({
              value,
              label: t(`correccion.modo.options.${value}`),
            }))}
            selectedValue={modo}
            onSelect={(value) =>
              updateSetting("correccion_modo", value as CorreccionModo)
            }
            disabled={motor === "desactivado"}
          />
        </SettingContainer>

        {usaModelo && (
          <div className="px-4 py-2 text-xs opacity-70" aria-live="polite">
            {lineaEstado()}
          </div>
        )}
      </>
    );
  },
);

CorreccionSettings.displayName = "CorreccionSettings";
