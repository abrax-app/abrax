import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { useSettings } from "@/hooks/useSettings";
import type { CorreccionModo, CorreccionMotor } from "@/bindings";

interface CorreccionSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

const MOTOR_OPTIONS: CorreccionMotor[] = ["solo_reglas", "desactivado"];
const MODO_OPTIONS: CorreccionModo[] = ["literal", "limpio"];

/**
 * Ajustes del módulo de corrección local (`correccion_motor` +
 * `correccion_modo`). El motor nace `desactivado` (passthrough exacto).
 *
 * Tuvo dos motores más —Auto y Modelo local— y un tercer modo, Pulido, que
 * mandaban el texto a un LLM local (sidecar propio u Ollama en loopback). El
 * «Pulido con IA» se retiró el 29/07 por decisión de producto: no se alcanzó a
 * probar en condiciones. Con él se fue la línea de estado de Ollama y el gestor
 * de modelos de Pulido, así que este componente ya no consulta nada ni tiene
 * estado propio: dos desplegables y nada más.
 */
export const CorreccionSettings: React.FC<CorreccionSettingsProps> = React.memo(
  ({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { settings, updateSetting } = useSettings();

    const motor: CorreccionMotor = settings?.correccion_motor ?? "desactivado";
    const modo: CorreccionModo = settings?.correccion_modo ?? "literal";

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
      </>
    );
  },
);

CorreccionSettings.displayName = "CorreccionSettings";
