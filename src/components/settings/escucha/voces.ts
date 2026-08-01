// [ESCUCHA] Agrupación de voces para el selector de voz.
//
// El motor ONLINE (edge-tts) expone ~22 voces cuyo id es una etiqueta BCP-47
// completa (`es-CO-SalomeNeural`): se agrupan por idioma/país para que la lista
// larga sea navegable (no un dropdown plano de 20). Los motores LOCALES
// (sistema/Kokoro/Piper) usan ids sin locale (`em_alex`, tokens del SO) → se
// detecta y conservan su lista plana intacta (no se toca su presentación).
//
// El encabezado del grupo es «idioma · país»: el IDIOMA es una etiqueta de UI y
// se traduce vía `t()` (paridad en los 22 locales); el PAÍS es nombre propio y
// queda fijo (los nombres de voz y país no se traducen).
import type { VozEscucha } from "@/bindings";
import type { SelectOption, SelectOptionGroup } from "../../ui/Select";

// País por locale de voz (nombre propio: NO se traduce; fijo en los 22 locales).
const PAIS: Record<string, string> = {
  "es-MX": "México",
  "es-CL": "Chile",
  "es-CO": "Colombia",
  "es-AR": "Argentina",
  "es-PE": "Perú",
  "es-VE": "Venezuela",
  "es-CR": "Costa Rica",
  "es-US": "Estados Unidos",
  "es-ES": "España",
  "en-US": "Estados Unidos",
  "en-GB": "Reino Unido",
};

// Clave i18n del nombre del idioma por código (etiqueta de UI, traducida).
const CLAVE_IDIOMA: Record<string, string> = {
  es: "tts.voiceLangEs",
  en: "tts.voiceLangEn",
};

// Id con locale BCP-47 al inicio (`es-CO-…`, `en-GB-…`): distingue las voces del
// motor online de las locales.
const RE_LOCALE = /^([a-z]{2})-([A-Z]{2})-/;

/**
 * Convierte las voces del motor activo en opciones para `<Select>`:
 * - Agrupadas por idioma/país cuando TODAS traen locale (motor online).
 * - Planas (con sufijo de idioma) en cualquier otro caso (motores locales),
 *   preservando su presentación previa exacta.
 * El orden de los grupos respeta el orden de llegada de las voces (el catálogo
 * del motor decide el orden). `t` traduce el nombre del idioma del encabezado.
 */
export function opcionesDeVoces(
  voces: VozEscucha[],
  t: (clave: string) => string,
): SelectOption[] | SelectOptionGroup[] {
  const todasConLocale =
    voces.length > 0 && voces.every((v) => RE_LOCALE.test(v.id));
  if (!todasConLocale) {
    return voces.map((v) => ({
      value: v.id,
      label: `${v.nombre} (${v.idioma})`,
    }));
  }

  const grupos = new Map<string, SelectOptionGroup>(); // Map preserva inserción.
  for (const v of voces) {
    const m = RE_LOCALE.exec(v.id)!;
    const idioma = m[1]; // "es" | "en"
    const locale = `${m[1]}-${m[2]}`; // "es-MX", "en-GB", …
    let grupo = grupos.get(locale);
    if (!grupo) {
      const claveIdioma = CLAVE_IDIOMA[idioma];
      const nombreIdioma = claveIdioma ? t(claveIdioma) : idioma;
      const pais = PAIS[locale] ?? locale;
      grupo = { label: `${nombreIdioma} · ${pais}`, options: [] };
      grupos.set(locale, grupo);
    }
    grupo.options.push({ value: v.id, label: v.nombre });
  }
  return [...grupos.values()];
}
