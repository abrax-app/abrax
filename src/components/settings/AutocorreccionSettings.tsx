import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { toast } from "sonner";
import { X } from "lucide-react";
import { commands } from "@/bindings";
import { useSettings } from "../../hooks/useSettings";
import { Input } from "../ui/Input";
import { Button } from "../ui/Button";
import { ToggleSwitch } from "../ui/ToggleSwitch";
import { SettingContainer } from "../ui/SettingContainer";

// Espejo de las señales de fábrica de `audio_toolkit/autocorreccion.rs`. Se
// duplican aquí (como el preset es-419 de las muletillas) solo para poder
// MOSTRARLAS: el botón «usar las de fábrica» las materializa en la lista para
// que se vean y se puedan editar una por una. Quien manda al corregir es Rust.
// Las senales de fabrica NO se escriben aqui: se piden al backend
// (`listar_senales_de_fabrica`), que es donde viven de verdad. Estaban copiadas
// a mano en este archivo y coincidian por suerte: el dia que alguien anadiera
// una en Rust, esta pantalla habria seguido ensenando la lista vieja.
const SIN_SENALES: string[] = [];

type ClaveSenales =
  | "autocorreccion_senales_borrado"
  | "autocorreccion_senales_sustitucion";

interface EditorSenalesProps {
  clave: ClaveSenales;
  /** Donde se RECUERDA la lista propia aunque esten activas las de fabrica. */
  propiasClave:
    | "autocorreccion_propias_borrado"
    | "autocorreccion_propias_sustitucion";
  titulo: string;
  descripcion: string;
  deFabrica: string[];
  descriptionMode: "inline" | "tooltip";
  grouped: boolean;
  disabled: boolean;
}

/**
 * Editor de una lista de señales.
 *
 * TRES MODOS, y los tres SIEMPRE a la vista. Antes eran tres botones que
 * aparecían y desaparecían segun el estado, y asi se llega a un sitio sin
 * salida: Winston apago un nivel y no encontro como volver a encenderlo.
 * Un control del que no se puede salir es peor que uno que no existe.
 *
 * El valor guardado sigue teniendo tres estados —`null` = las de fabrica,
 * `[]` = nivel apagado, lista = las propias— pero eso es asunto del backend; en
 * pantalla son tres opciones que se ven todas y se pueden pulsar todas.
 *
 * Las PROPIAS se recuerdan aparte (`propiasClave`), asi que ir a «de fabrica» y
 * volver ya no borra el trabajo de nadie.
 */
const EditorSenales: React.FC<EditorSenalesProps> = ({
  clave,
  propiasClave,
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
  const propias = getSetting(propiasClave) ?? [];
  const modo =
    senales === null
      ? "defaults"
      : senales.length === 0
        ? "disabled"
        : "custom";
  // En «de fabrica» se muestran LAS DE FABRICA, no una lista vacia: son las que
  // estan gobernando el comportamiento, y pintarlas vacias hacia creer que no
  // habia ninguna.
  const actuales =
    modo === "custom" ? senales! : modo === "defaults" ? deFabrica : [];
  const editable = modo === "custom";
  const ocupado = isUpdating(clave) || disabled;

  /** Guarda una lista propia y la recuerda, para poder ir y volver. */
  const guardarPropias = (lista: string[]) => {
    updateSetting(clave, lista);
    updateSetting(propiasClave, lista);
  };

  const agregar = () => {
    const limpia = nueva
      .trim()
      .replace(/[<>"']/g, "")
      .replace(/\s+/g, " ");
    if (!limpia || limpia.length > 50) return;
    // Agregar desde «de fabrica» arranca la lista propia CON las de fabrica
    // dentro: nadie quiere perder las siete que ya funcionaban por sumar una.
    const base = modo === "custom" ? actuales : deFabrica;
    if (base.includes(limpia)) {
      toast.error(
        t("settings.advanced.autocorreccion.duplicate", { senal: limpia }),
      );
      return;
    }
    guardarPropias([...base, limpia]);
    setNueva("");
  };

  const OPCIONES: { id: string; onClick: () => void }[] = [
    { id: "defaults", onClick: () => updateSetting(clave, null) },
    {
      id: "custom",
      // Si nunca hubo propias, se siembran con las de fabrica: asi la opcion
      // hace algo visible desde el primer clic en vez de dejar una lista vacia.
      onClick: () => guardarPropias(propias.length ? propias : [...deFabrica]),
    },
    { id: "disabled", onClick: () => updateSetting(clave, []) },
  ];

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
            disabled={ocupado || modo === "disabled"}
          />
          <Button
            onClick={agregar}
            disabled={
              ocupado ||
              modo === "disabled" ||
              !nueva.trim() ||
              nueva.trim().length > 50
            }
            variant="primary"
            size="md"
          >
            {t("settings.advanced.autocorreccion.add")}
          </Button>
        </div>
      </SettingContainer>

      {/* Los tres modos, siempre los tres. El activo va marcado. */}
      <div
        className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap items-center gap-2`}
        role="radiogroup"
        aria-label={titulo}
      >
        {OPCIONES.map((o) => (
          <Button
            key={o.id}
            onClick={o.onClick}
            disabled={ocupado}
            variant={modo === o.id ? "primary" : "secondary"}
            size="sm"
            role="radio"
            aria-checked={modo === o.id}
          >
            {t(`settings.advanced.autocorreccion.mode.${o.id}`)}
          </Button>
        ))}
      </div>

      {actuales.length > 0 && (
        <div
          className={`px-4 p-2 ${grouped ? "" : "rounded-lg border border-mid-gray/20"} flex flex-wrap gap-1`}
        >
          {actuales.map((senal) =>
            editable ? (
              <Button
                key={senal}
                onClick={() =>
                  guardarPropias(actuales.filter((s) => s !== senal))
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
            ) : (
              // En «de fabrica» las señales se ven pero no se borran: son la
              // lista del producto. Para quitar una, pasas a «Las mias» —que se
              // siembra con estas— y ahi mandas tu.
              <span
                key={senal}
                className="px-2 py-1 text-sm rounded-md bg-mid-gray/10 text-text/70"
              >
                {senal}
              </span>
            ),
          )}
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
 * ENCENDIDA de fábrica desde el 30/07. Estuvo apagada porque su nivel 1 BORRA
 * texto y disparaba en mitad de una frase corriente; ahora ese nivel exige que
 * la señal CIERRE el dictado, y con eso deja de destrozar prosa. Las dos listas
 * solo se muestran con la función encendida, para no ofrecer controles muertos.
 */
export const AutocorreccionSettings: React.FC<AutocorreccionSettingsProps> =
  React.memo(({ descriptionMode = "tooltip", grouped = false }) => {
    const { t } = useTranslation();
    const { getSetting, updateSetting, isUpdating } = useSettings();
    const activa = getSetting("autocorreccion_activa") ?? true;
    // Las de fábrica se piden al backend: es donde viven, y así esta pantalla no
    // puede quedarse enseñando una lista vieja.
    const [deFabrica, setDeFabrica] = useState<{
      borrado: string[];
      sustitucion: string[];
    }>({ borrado: [], sustitucion: [] });
    useEffect(() => {
      let vivo = true;
      commands
        .listarSenalesDeFabrica()
        .then((r) => {
          if (vivo) setDeFabrica(r);
        })
        .catch(() => {});
      return () => {
        vivo = false;
      };
    }, []);

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
              propiasClave="autocorreccion_propias_sustitucion"
              deFabrica={deFabrica.sustitucion}
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
              propiasClave="autocorreccion_propias_borrado"
              deFabrica={deFabrica.borrado}
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
