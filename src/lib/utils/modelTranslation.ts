import type { TFunction } from "i18next";
import type { ModelInfo } from "@/bindings";

/**
 * Clave i18n de un modelo, a partir de su id.
 *
 * Dos trampas, y por eso esto existe en vez de usar `model.id` a secas:
 *
 *  1. i18next parte las claves por PUNTOS. Media docena de ids los llevan
 *     (`nemotron-3.5-…`, `parakeet-tdt-0.6b-v2`), así que la ruta se partía por
 *     dentro del id, no encontraba nada y la UI caía a la descripción en inglés
 *     del catálogo. Las traducciones existían y no se veían.
 *  2. El id del descriptor es `org/repo/archivo.gguf` — incluye el quant. Si la
 *     clave lo incluyera, cambiar el quant por defecto dejaría la traducción
 *     huérfana en silencio.
 *
 * Se toma el repo (`org/repo`, o el id entero si es uno corto heredado) y se
 * aplanan puntos y barras a `_`.
 */
export const claveDeModelo = (id: string): string => {
  const partes = id.split("/");
  const repo = partes.length > 1 ? partes.slice(0, 2).join("/") : id;
  return repo.replace(/[./]/g, "_");
};

/**
 * Get the translated name for a model
 * @param model - The model info object
 * @param t - The translation function from useTranslation
 * @returns The translated model name, or the original name if no translation exists
 */
export function getTranslatedModelName(model: ModelInfo, t: TFunction): string {
  const translationKey = `onboarding.models.${claveDeModelo(model.id)}.name`;
  const translated = t(translationKey, { defaultValue: "" });
  return translated !== "" ? translated : model.name;
}

/**
 * Get the translated description for a model
 * @param model - The model info object
 * @param t - The translation function from useTranslation
 * @returns The translated model description, or the original description if no translation exists
 */
export function getTranslatedModelDescription(
  model: ModelInfo,
  t: TFunction,
): string {
  // Custom models use a generic translation key
  if (model.is_custom) {
    return t("onboarding.customModelDescription");
  }
  const translationKey = `onboarding.models.${claveDeModelo(model.id)}.description`;
  const translated = t(translationKey, { defaultValue: "" });
  return translated !== "" ? translated : model.description;
}
