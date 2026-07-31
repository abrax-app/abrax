import React from "react";
import { useTranslation } from "react-i18next";
import { Dropdown } from "../ui/Dropdown";
import { SettingContainer } from "../ui/SettingContainer";
import { ToggleSwitch } from "../ui/ToggleSwitch";
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

    const motor: CorreccionMotor = settings?.correccion_motor ?? "solo_reglas";
    const modo: CorreccionModo = settings?.correccion_modo ?? "limpio";
    const numeros: boolean = settings?.correccion_numeros ?? true;

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

        {/* El conversor de numerales, con llave propia. Es la unica capa del
            paquete que cambia el ESTILO del texto y no solo su forma —convierte
            TODO numeral, no solo los tecnicos—, asi que quien quiera tildes y
            simbolos sin que «los dos minutos» se vuelva «los 2 minutos» puede
            apagar esto sin renunciar al resto. */}
        <ToggleSwitch
          checked={numeros}
          onChange={(value) => updateSetting("correccion_numeros", value)}
          label={t("correccion.numeros.title")}
          description={t("correccion.numeros.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
          disabled={motor === "desactivado" || modo === "literal"}
        />
      </>
    );
  },
);

CorreccionSettings.displayName = "CorreccionSettings";
