//! Reglas deterministas del módulo de corrección: transformaciones rápidas y
//! predecibles sobre la transcripción, sin ningún modelo.
//!
//! Diseño: cada regla es una función pura `&str -> String`, idempotente en la
//! práctica (aplicarla dos veces no cambia el resultado) y **conservadora**:
//! ante cualquier ambigüedad devuelve el texto tal cual. Las capas de
//! vocabulario (reemplazos exactos, custom words, Diccionario Vivo, muletillas)
//! ya corrieron antes en `post_process_transcription_text` — aquí solo entra lo
//! que aquellas no cubren: autocorrecciones habladas, espacios alrededor de
//! puntuación y mayúsculas de oración.

use crate::audio_toolkit::build_match_key;

/// Marcadores de autocorrección hablada (es-419), como secuencias de claves
/// normalizadas por [`build_match_key`]. Solo disparan delimitados por comas
/// (`, perdón,`): "te pido perdón" jamás activa la regla.
const MARCADORES: &[&[&str]] = &[
    &["perdon"],
    &["digo"],
    &["mejor", "dicho"],
    &["mas", "bien"],
    &["quise", "decir"],
    &["quiero", "decir"],
];

/// Puntuación que puede colgar del final de un token ("miércoles.", "nueve,").
const PUNT_FINAL: &[char] = &['.', ',', ';', ':', '!', '?', '…', ')', ']', '»', '"', '\''];

/// Máximo de tokens que una autocorrección puede reemplazar o aportar. Más allá
/// de esto ya no es una autocorrección hablada, es otra frase.
const MAX_TOKENS_CORRECCION: usize = 8;

/// Rangos de bytes `(inicio, fin)` de cada token (separado por whitespace).
fn tokens_con_rango(s: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut inicio: Option<usize> = None;
    for (i, c) in s.char_indices() {
        if c.is_whitespace() {
            if let Some(a) = inicio.take() {
                out.push((a, i));
            }
        } else if inicio.is_none() {
            inicio = Some(i);
        }
    }
    if let Some(a) = inicio {
        out.push((a, s.len()));
    }
    out
}

/// Resuelve autocorrecciones habladas: «lo entregamos el martes, perdón, el
/// miércoles» → «lo entregamos el miércoles». La frase tras el marcador
/// reemplaza desde la última aparición (por clave normalizada) de su primera
/// palabra hacia atrás — el ancla. **Sin ancla no se toca nada**: preferimos
/// dejar el marcador visible antes que adivinar cuánto texto reemplazar.
pub fn autocorreccion_hablada(texto: &str) -> String {
    let mut actual = texto.to_string();
    // Varias autocorrecciones en un mismo dictado son raras; 3 pasadas bastan
    // y evitan cualquier riesgo de bucle.
    for _ in 0..3 {
        match aplicar_una_autocorreccion(&actual) {
            Some(nuevo) => actual = nuevo,
            None => break,
        }
    }
    actual
}

/// Una pasada: encuentra el primer `, marcador,` y lo resuelve. `None` si no
/// hay marcador o si no se pudo anclar el reemplazo (conservador).
fn aplicar_una_autocorreccion(texto: &str) -> Option<String> {
    let comas: Vec<usize> = texto
        .char_indices()
        .filter(|(_, c)| *c == ',')
        .map(|(i, _)| i)
        .collect();

    for (idx, &i) in comas.iter().enumerate() {
        let Some(&j) = comas.get(idx + 1) else { break };
        let segmento = &texto[i + 1..j];
        let claves: Vec<String> = segmento
            .split_whitespace()
            .map(build_match_key)
            .filter(|k| !k.is_empty())
            .collect();
        let es_marcador = MARCADORES
            .iter()
            .any(|m| m.len() == claves.len() && m.iter().zip(&claves).all(|(a, b)| a == b));
        if !es_marcador {
            continue;
        }

        let pre = &texto[..i];
        let post = &texto[j + 1..];

        // La corrección: tokens de POST hasta la primera puntuación de cierre.
        let rangos_post = tokens_con_rango(post);
        let mut correccion: Vec<&str> = Vec::new();
        let mut resto_desde = post.len();
        for &(a, b) in rangos_post.iter().take(MAX_TOKENS_CORRECCION) {
            let token = &post[a..b];
            let nucleo = token.trim_end_matches(PUNT_FINAL);
            if nucleo.is_empty() {
                resto_desde = a;
                break;
            }
            correccion.push(nucleo);
            if nucleo.len() < token.len() {
                // El token traía puntuación de cierre: la corrección termina aquí.
                resto_desde = a + nucleo.len();
                break;
            }
            resto_desde = b;
        }
        if correccion.is_empty() {
            return None;
        }

        // Ancla: última aparición en PRE (mirando hasta 8 tokens atrás) de la
        // primera palabra de la corrección.
        let clave_ancla = build_match_key(correccion[0]);
        if clave_ancla.is_empty() {
            return None;
        }
        let rangos_pre = tokens_con_rango(pre);
        let &(ancla_inicio, _) = rangos_pre
            .iter()
            .rev()
            .take(MAX_TOKENS_CORRECCION)
            .find(|&&(a, b)| build_match_key(&pre[a..b]) == clave_ancla)?;

        let mut nuevo = String::with_capacity(texto.len());
        nuevo.push_str(&pre[..ancla_inicio]);
        nuevo.push_str(&correccion.join(" "));
        nuevo.push_str(&post[resto_desde..]);
        return Some(nuevo);
    }
    None
}

/// Normaliza espacios alrededor de puntuación: quita los espacios ANTES de
/// `, . ; : ! ? …` («hola , mundo» → «hola, mundo») y asegura uno DESPUÉS de
/// `,` y `;` cuando sigue una letra («uno,dos» → «uno, dos»). No toca `.` ni
/// `:` hacia adelante — romperían URLs, decimales («3.14») y horas («15:30»);
/// y una coma seguida de dígito es un decimal es-419 («1,5»), tampoco se toca.
pub fn normalizar_espacios(texto: &str) -> String {
    let mut out = String::with_capacity(texto.len());
    let mut chars = texto.chars().peekable();
    while let Some(c) = chars.next() {
        if matches!(c, ',' | '.' | ';' | ':' | '!' | '?' | '…') {
            while out.ends_with(' ') {
                out.pop();
            }
            out.push(c);
            if matches!(c, ',' | ';') {
                if let Some(&sig) = chars.peek() {
                    if sig.is_alphabetic() {
                        out.push(' ');
                    }
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Mayúscula al inicio del texto y tras `. ! ? …`. Solo toca palabras
/// **enteramente en minúscula y sin dígitos ni guiones bajos** — jamás
/// convierte `useAuthStore` en `UseAuthStore` — y salta tokens con punto
/// interno (`www.abrax.app`, `archivo.rs`). Un punto pegado a la siguiente
/// palabra («3.14», «v0.1.0») no es fin de oración y no activa nada.
///
/// Tampoco capitaliza tras `/`, `\` o `@`: esos caracteres abren una ruta o un
/// identificador, no una oración («/usr/bin», `@antonio`).
pub fn capitalizar_oraciones(texto: &str) -> String {
    let mut out = String::with_capacity(texto.len());
    let mut esperar_mayuscula = true;
    let indices: Vec<(usize, char)> = texto.char_indices().collect();
    let mut i = 0;
    while i < indices.len() {
        let (byte_i, c) = indices[i];
        if matches!(c, '.' | '!' | '?' | '…') {
            out.push(c);
            // Solo es fin de oración si sigue espacio (o el texto termina).
            esperar_mayuscula = match indices.get(i + 1) {
                Some(&(_, sig)) => sig.is_whitespace(),
                None => false,
            };
            i += 1;
            continue;
        }
        // Ruta o identificador abriendo oración: `/`, `\` y `@` no anteceden a
        // una mayúscula. Sin esto «/usr/bin y algo» salía «/Usr/bin y algo».
        if esperar_mayuscula && matches!(c, '/' | '\\' | '@') {
            esperar_mayuscula = false;
            out.push(c);
            i += 1;
            continue;
        }
        if esperar_mayuscula && c.is_alphabetic() {
            // Extiende la palabra completa para aplicar los guardas.
            let mut fin = i;
            while let Some(&(_, w)) = indices.get(fin + 1) {
                if w.is_alphanumeric() || w == '_' {
                    fin += 1;
                } else {
                    break;
                }
            }
            let fin_byte = indices.get(fin + 1).map(|&(b, _)| b).unwrap_or(texto.len());
            let palabra = &texto[byte_i..fin_byte];
            let es_llana = palabra
                .chars()
                .all(|w| w.is_alphabetic() && w.is_lowercase());
            // ¿Token con punto interno? («www» en «www.abrax.app»)
            let punto_interno = matches!(
                (indices.get(fin + 1), indices.get(fin + 2)),
                (Some(&(_, '.')), Some(&(_, sig))) if sig.is_alphanumeric()
            );
            if es_llana && !punto_interno {
                let mut cs = palabra.chars();
                if let Some(primera) = cs.next() {
                    out.extend(primera.to_uppercase());
                    out.push_str(cs.as_str());
                }
            } else {
                out.push_str(palabra);
            }
            esperar_mayuscula = false;
            i = fin + 1;
            continue;
        }
        if esperar_mayuscula && c.is_numeric() {
            // Una cifra abre la oración («15 dólares...»): nada que capitalizar.
            esperar_mayuscula = false;
        }
        out.push(c);
        i += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── autocorrección hablada ────────────────────────────────────────────

    #[test]
    fn autocorreccion_del_ejemplo_de_diseno() {
        assert_eq!(
            autocorreccion_hablada("Lo entregamos el martes, perdón, el miércoles."),
            "Lo entregamos el miércoles."
        );
    }

    #[test]
    fn autocorreccion_con_ancla_lejana_y_texto_posterior() {
        assert_eq!(
            autocorreccion_hablada("nos vemos a las ocho, digo, a las nueve, en el café"),
            "nos vemos a las nueve, en el café"
        );
    }

    #[test]
    fn autocorreccion_respeta_marcadores_multitoken() {
        assert_eq!(
            autocorreccion_hablada("va para el lunes, mejor dicho, el jueves"),
            "va para el jueves"
        );
    }

    #[test]
    fn sin_comas_no_dispara() {
        let t = "te pido perdón por llegar tarde";
        assert_eq!(autocorreccion_hablada(t), t);
    }

    #[test]
    fn sin_ancla_no_toca_nada() {
        // «mía» no aparece antes del marcador: adivinar sería peor que dejarlo.
        let t = "fue su culpa, perdón, mía";
        assert_eq!(autocorreccion_hablada(t), t);
    }

    #[test]
    fn dos_autocorrecciones_en_un_dictado() {
        assert_eq!(
            autocorreccion_hablada(
                "llega el lunes, digo, el martes y cuesta diez, perdón, cuesta veinte"
            ),
            "llega el martes y cuesta veinte"
        );
    }

    #[test]
    fn marcador_capitalizado_o_con_tilde_tambien_dispara() {
        assert_eq!(
            autocorreccion_hablada("el plazo es en marzo, Perdón, en abril"),
            "el plazo es en abril"
        );
    }

    // ── espacios ──────────────────────────────────────────────────────────

    #[test]
    fn quita_espacio_antes_de_puntuacion() {
        assert_eq!(normalizar_espacios("hola , mundo ."), "hola, mundo.");
    }

    #[test]
    fn agrega_espacio_tras_coma_pegada() {
        assert_eq!(normalizar_espacios("uno,dos"), "uno, dos");
    }

    #[test]
    fn no_toca_decimales_urls_ni_horas() {
        assert_eq!(
            normalizar_espacios("vale 1,5 y pi es 3.14"),
            "vale 1,5 y pi es 3.14"
        );
        assert_eq!(
            normalizar_espacios("visita https://abrax.app a las 15:30"),
            "visita https://abrax.app a las 15:30"
        );
    }

    // ── mayúsculas ────────────────────────────────────────────────────────

    #[test]
    fn capitaliza_inicio_y_tras_punto() {
        assert_eq!(
            capitalizar_oraciones("hola. qué tal? todo bien"),
            "Hola. Qué tal? Todo bien"
        );
    }

    #[test]
    fn jamas_toca_identificadores() {
        // «useAuthStore» abre el texto y NO se capitaliza (mayúscula interna);
        // «usa» es palabra llana abriendo oración y SÍ.
        assert_eq!(
            capitalizar_oraciones("useAuthStore es el store. usa fetch_data aquí"),
            "useAuthStore es el store. Usa fetch_data aquí"
        );
        // Identificadores abriendo oración: guión bajo y mayúscula interna
        // los protegen aunque estén tras un punto.
        assert_eq!(
            capitalizar_oraciones("listo. fetch_data falla. useAuthStore también"),
            "Listo. fetch_data falla. useAuthStore también"
        );
    }

    #[test]
    fn no_capitaliza_tokens_con_punto_interno() {
        assert_eq!(
            capitalizar_oraciones("visita www.abrax.app. gracias"),
            "Visita www.abrax.app. Gracias"
        );
    }

    #[test]
    fn punto_pegado_no_es_fin_de_oracion() {
        assert_eq!(
            capitalizar_oraciones("la versión v0.1.0 salió"),
            "La versión v0.1.0 salió"
        );
    }

    #[test]
    fn signos_de_apertura_no_bloquean_la_mayuscula() {
        assert_eq!(capitalizar_oraciones("¿qué hora es?"), "¿Qué hora es?");
    }

    #[test]
    fn rutas_e_identificadores_no_se_capitalizan() {
        // Abriendo el texto y tras punto: una ruta no es una oración.
        assert_eq!(capitalizar_oraciones("/usr/bin y algo"), "/usr/bin y algo");
        assert_eq!(
            capitalizar_oraciones("listo. /home/antonio falla"),
            "Listo. /home/antonio falla"
        );
        // Ruta UNC tras punto: la barra invertida tampoco abre oración.
        assert_eq!(
            capitalizar_oraciones("listo. \\\\servidor\\ruta"),
            "Listo. \\\\servidor\\ruta"
        );
        // La letra de unidad SÍ se capitaliza, y así debe ser: «c:» → «C:».
        assert_eq!(
            capitalizar_oraciones("c:\\usuarios\\antonio"),
            "C:\\usuarios\\antonio"
        );
        assert_eq!(
            capitalizar_oraciones("@antonio revisa esto"),
            "@antonio revisa esto"
        );
        // Y la oración normal siguiente SÍ se capitaliza: el arreglo no
        // desactiva la regla, solo la salta en el token del identificador.
        assert_eq!(
            capitalizar_oraciones("/tmp/x existe. ahora sigue"),
            "/tmp/x existe. Ahora sigue"
        );
    }
}
