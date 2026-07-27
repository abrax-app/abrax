import { ask } from "@tauri-apps/plugin-dialog";
import type { TFunction } from "i18next";
import type { ModelInfo } from "@/bindings";
import { formatModelSize } from "./format";
import {
  getTranslatedModelDescription,
  getTranslatedModelName,
} from "./modelTranslation";

/**
 * Diálogos de confirmación de la pantalla de modelos.
 *
 * Viven aquí y no dentro de un componente porque los usan las DOS pantallas que
 * muestran modelos (el onboarding y Ajustes → Modelos), y hasta ahora el de
 * borrado estaba escrito solo en Ajustes: el onboarding no dejaba borrar nada.
 */

/**
 * Confirmación previa a descargar.
 *
 * Antes, un clic en la tarjeta empezaba a bajar cientos de MB sin preguntar ni
 * decir cuánto pesaba. Se detectó midiendo la instalación en un Windows limpio
 * (26/07). El diálogo repite nombre, tamaño real y para qué sirve el modelo, y
 * deja salir sin descargar.
 */
export const confirmarDescarga = async (
  model: ModelInfo,
  t: TFunction,
): Promise<boolean> =>
  ask(
    t("modelSelector.confirmDownload.body", {
      modelName: getTranslatedModelName(model, t),
      size: formatModelSize(Number(model.size_mb)),
      description: getTranslatedModelDescription(model, t),
    }),
    {
      title: t("modelSelector.confirmDownload.title"),
      kind: "info",
      okLabel: t("modelSelector.confirmDownload.ok"),
      cancelLabel: t("modelSelector.cancel"),
    },
  );

/**
 * Confirmación previa a borrar. Avisa distinto si el modelo es el activo,
 * porque borrarlo deja el dictado sin motor hasta elegir otro.
 */
export const confirmarBorrado = async (
  model: ModelInfo,
  esActivo: boolean,
  t: TFunction,
): Promise<boolean> => {
  const modelName = getTranslatedModelName(model, t);
  return ask(
    esActivo
      ? t("settings.models.deleteActiveConfirm", { modelName })
      : t("settings.models.deleteConfirm", { modelName }),
    {
      title: t("settings.models.deleteTitle"),
      kind: "warning",
    },
  );
};
