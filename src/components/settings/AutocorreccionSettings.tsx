import React, { useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { X } from "lucide-react";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer } from "../ui/SettingContainer";

// Espejo de las señales de fábrica de `audio_toolkit/autocorreccion.rs`. Se
// duplican aquí (como el preset es-419 de las muletillas) solo para poder
// MOSTRARLAS: el botón «usar las de fábrica» las materializa en la lista para
// que se vean y se puedan editar una por una. Quien manda al corregir es Rust.
const BORRADO_DE_FABRICA = ["borra eso", "olvida eso", "no, nada", "déjalo"];
const SUSTITUCION_DE_FABRICA = [
  "no, perdón",
  "perdón, quise decir",
  "mejor dicho",
  "o sea, no",
  "corrijo",
  "no, mejor",
  "digo",
];

type ClaveSenales =
  | "autocorreccion_senales_borrado"
  | "autocorreccion_senales_sustitucion";

interface EditorSenalesProps {
  clave: ClaveSenales;
  titulo: string;
  descripcion: string;
  deFabrica: string[];
  descriptionMode: "inline" | "tooltip";
  grouped: boolean;
  disabled: boolean;
}

/** Editor de una lista de señales. Mismo contrato que las muletillas:
 *  `null` = las de fábrica · `[]` = ese nivel apagado · lista = la del usuario. */
const EditorSenales: React.FC<EditorSenalesProps> = ({
  clave,
  titulo,
  descripcion,
  deFabrica,
  descriptionMode,
  grouped,
  disabled,
}) => {
  const { t } = useTranslation();
  const { getSetting, updateSetting, isUpdating } = useSettings();
  const [nueva, setNueva] = useState("");

  const senales = getSetting(clave) ?? null;
  const modo =
    senales === null
      ? "defaults"
      : senales.length === 0
        ? "disabled"
        : "custom";
  const actuales = senales ?? [];
  const ocupado = isUpdating(clave) || disabled;

  const agregar = () => {
    const limpia = nueva
      .trim()
      .replace(/[<>"']/g, "")
      .replace(/\s+/g, " ");
    if (!limpia || limpia.length > 50) return;
    if (actuales.includes(limpia)) {
      toast.error(
        t("settings.advanced.autocorreccion.duplicate", { senal: limpia }),
      );
      return;
    }
    updateSetting(clave, [...actuales, limpia]);
    setNueva("");
  };

  return (
    <>
      <SettingContainer
        title={titulo}
        description={descripcion}
        descriptionMode={descriptionMode}
        grouped={grouped}
      >
        <div className="flex items-center gap-2">
          <Input
            type="text"
            className="max-w-40"
            value={nueva}
            onChange={(e) => setNueva(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                agregar();
              }
            }}
            placeholder={t("settings.advanced.autocorreccion.placeholder")}
            variant="compact"
            disabled={ocupado}
          />
          <Button
            onClick={agregar}
            disabled={ocupado || !nueva.trim() || nueva.trim().length > 50}
            variant="primary"
            size="md"
          >
            {t("settings.advanced.autocorreccion.add")}
          </Button>
        </div>
      </SettingContainer>
      <div
        className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap items-center gap-2`}
      >
        <span className="text-xs opacity-70">
          {t(`settings.advanced.autocorreccion.mode.${modo}`)}
        </span>
        {modo !== "custom" && (
          <Button
            onClick={() => updateSetting(clave, [...deFabrica])}
            disabled={ocupado}
            variant="secondary"
            size="sm"
          >
            {t("settings.advanced.autocorreccion.showDefaults")}
          </Button>
        )}
        {modo !== "defaults" && (
          <Button
            onClick={() => updateSetting(clave, null)}
            disabled={ocupado}
            variant="secondary"
            size="sm"
          >
            {t("settings.advanced.autocorreccion.resetDefaults")}
          </Button>
        )}
        {modo !== "disabled" && (
          <Button
            onClick={() => updateSetting(clave, [])}
            disabled={ocupado}
            variant="secondary"
            size="sm"
          >
            {t("settings.advanced.autocorreccion.disable")}
          </Button>
        )}
      </div>
      {actuales.length > 0 && (
        <div
          className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
        >
          {actuales.map((senal) => (
            <Button
              key={senal}
              onClick={() =>
                updateSetting(
                  clave,
                  actuales.filter((s) => s !== senal),
                )
              }
              disabled={ocupado}
              variant="secondary"
              size="sm"
              className="inline-flex items-center gap-1 cursor-pointer"
              aria-label={t("settings.advanced.autocorreccion.remove", {
                senal,
              })}
            >
              <span>{senal}</span>
              <X className="w-3 h-3" />
            </Button>
          ))}
        </div>
      )}
    </>
  );
};

interface AutocorreccionSettingsProps {
  descriptionMode?: "inline" | "tooltip";
  grouped?: boolean;
}

/**
 * Autocorrección hablada: si quien dicta se corrige a sí mismo en voz alta
 * («…el martes, no, perdón, el miércoles»), el texto sale ya corregido.
 *
 * Por reglas y 100% local — no usa Post Proceso ni ningún modelo de IA.
 * Apagada de fábrica a propósito: la función BORRA texto, y eso se enciende a
 * conciencia, no por sorpresa. Las dos listas de señales solo se muestran con
 * la función encendida, para no ofrecer controles que no hacen nada.
 */
export const AutocorreccionSettings: React.FC<AutocorreccionSettingsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const activa = getSetting("autocorreccion_activa") ?? false;

    return (
      <>
        <ToggleSwitch
          checked={activa}
          onChange={(value) => updateSetting("autocorreccion_activa", value)}
          isUpdating={isUpdating("autocorreccion_activa")}
          label={t("settings.advanced.autocorreccion.title")}
          description={t("settings.advanced.autocorreccion.description")}
          descriptionMode={descriptionMode}
          grouped={grouped}
        />
        {activa && (
          <>
            <div
              className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"}`}
            >
              <p className="text-xs opacity-70">
                {t("settings.advanced.autocorreccion.ejemplo")}
              </p>
            </div>
            <EditorSenales
              clave="autocorreccion_senales_sustitucion"
              titulo={t("settings.advanced.autocorreccion.sustitucionTitle")}
              descripcion={t(
                "settings.advanced.autocorreccion.sustitucionDescription",
              )}
              deFabrica={SUSTITUCION_DE_FABRICA}
              descriptionMode={descriptionMode}
              grouped={grouped}
              disabled={!activa}
            />
            <EditorSenales
              clave="autocorreccion_senales_borrado"
              titulo={t("settings.advanced.autocorreccion.borradoTitle")}
              descripcion={t(
                "settings.advanced.autocorreccion.borradoDescription",
              )}
              deFabrica={BORRADO_DE_FABRICA}
              descriptionMode={descriptionMode}
              grouped={grouped}
              disabled={!activa}
            />
          </>
        )}
      </>
    );
  });

AutocorreccionSettings.displayName = "AutocorreccionSettings";
