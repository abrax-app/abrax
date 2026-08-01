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

// ---------------------------------------------------------------------------
// Motor de retractacion HEREDADO — estacionado, no en el camino.
//
// Es la version que venia de la rama de Antonio. Se conserva entera como
// referencia, pero ya no se llama desde `correccion::procesar`: corria DESPUES
// del motor vivo (`audio_toolkit::autocorreccion`) y le daba vuelta el sentido
// a frases que estaban bien —«No es caro, mas bien, es carisimo» terminaba como
// «No es carisimo»— porque trata cualquier marcador como orden de borrar lo
// anterior, sin mirar si las dos partes son comparables.
//
// Lo util de aca ya esta adentro del motor vivo: sus 16 marcadores, las
// contracciones «al»/«del» y el cierre por puntuacion. Lo que no se trajo es la
// regla de anclaje, que exige que la correccion empiece por una palabra ya
// dicha antes del marcador — por eso «comprar pan, perdon, arroz» no le salia.
//
// Se deja compilando para poder contrastar comportamientos; si en unas semanas
// nadie la consulto, se borra.
// ---------------------------------------------------------------------------

/// Marcadores de autocorrección hablada (es-419), como secuencias de claves
/// normalizadas por [`build_match_key`]. Solo disparan delimitados por comas
/// (`, perdón,`): "te pido perdón" jamás activa la regla.
#[allow(dead_code)]
const MARCADORES: &[&[&str]] = &[
    &["perdon"],
    &["perdona"],
    &["disculpa"],
    &["digo"],
    &["mejor", "dicho"],
    &["mas", "bien"],
    &["quise", "decir"],
    &["quiero", "decir"],
    &["mentira"],
    &["miento"],
    &["me", "equivoque"],
    &["corrijo"],
];

/// Puntuación que puede colgar del final de un token ("miércoles.", "nueve,").
const PUNT_FINAL: &[char] = &['.', ',', ';', ':', '!', '?', '…', ')', ']', '»', '"', '\''];

/// Máximo de tokens que una autocorrección puede reemplazar o aportar. Más allá
/// de esto ya no es una autocorrección hablada, es otra frase.
#[allow(dead_code)]
const MAX_TOKENS_CORRECCION: usize = 8;

/// Rangos de bytes `(inicio, fin)` de cada token (separado por whitespace).
#[allow(dead_code)]
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
#[allow(dead_code)]
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
    for _ in 0..3 {
        match aplicar_una_correccion_tras_cierre(&actual) {
            Some(nuevo) => actual = nuevo,
            None => break,
        }
    }
    actual
}

/// Marcadores que anuncian corrección justo DESPUÉS de un cierre de oración.
/// El ASR real puntúa la pausa del que se corrige con un punto: «…terminar la
/// landing. No, mejor terminar el video.» — la forma con comas jamás llega.
/// Solo entran señales inequívocas de corrección; «no» a secas queda fuera.
#[allow(dead_code)]
const MARCADORES_TRAS_CIERRE: &[&[&str]] = &[
    &["no", "mejor"],
    &["bueno", "no"],
    &["mejor", "dicho"],
    &["mas", "bien"],
    &["quise", "decir"],
    &["quiero", "decir"],
    &["digo"],
    &["corrijo"],
    &["miento"],
    &["mentira"],
    &["me", "equivoque"],
];

/// Marcadores DÉBILES tras cierre: «Perdón, …» abre disculpas normales
/// («Perdón, no volverá a pasar»), así que solo disparan con un ancla de
/// contenido — jamás con una palabra función.
#[allow(dead_code)]
const MARCADORES_TRAS_CIERRE_DEBILES: &[&[&str]] = &[&["perdon"], &["perdona"], &["disculpa"]];

/// Palabras función: un ancla así tras un marcador débil es casi seguro una
/// frase nueva («Perdón, no te escuché»), no una corrección.
#[allow(dead_code)]
const ANCLAS_FUNCION: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "por",
    "para", "con", "sin", "no", "ni", "que", "se", "te", "le", "lo", "me", "mi", "tu", "su", "y",
    "o", "es",
];

/// ¿El token termina cerrando oración? El «?» queda fuera a propósito: tras
/// una pregunta, «No, …» es una RESPUESTA, no un falso comienzo.
#[allow(dead_code)]
fn cierra_para_correccion(token: &str) -> bool {
    token.ends_with(['.', '…', '!']) || token.ends_with("...")
}

/// Una pasada del falso comienzo tras cierre: «X…/. MARCADOR, corrección.» →
/// la corrección reemplaza desde su ancla hacia atrás, comiéndose el cierre
/// intermedio y el marcador. Mismo principio conservador que la forma con
/// comas: sin ancla no se toca nada.
#[allow(dead_code)]
fn aplicar_una_correccion_tras_cierre(texto: &str) -> Option<String> {
    let rangos = tokens_con_rango(texto);
    let toks: Vec<&str> = rangos.iter().map(|&(a, b)| &texto[a..b]).collect();
    let claves: Vec<String> = toks
        .iter()
        .map(|t| build_match_key(t.trim_end_matches(PUNT_FINAL)))
        .collect();

    for i in 1..toks.len() {
        if !cierra_para_correccion(toks[i - 1]) {
            continue;
        }
        // ¿Empieza aquí un marcador (fuerte o débil)?
        let casa = |tabla: &[&[&str]]| -> Option<usize> {
            tabla
                .iter()
                .filter(|m| i + m.len() <= toks.len())
                .find(|m| {
                    m.iter().zip(&claves[i..]).all(|(a, b)| a == b)
                        // El marcador no cruza otro cierre de oración.
                        && !toks[i..i + m.len() - 1].iter().any(|t| cierra_para_correccion(t))
                })
                .map(|m| m.len())
        };
        let (n, debil) = match casa(MARCADORES_TRAS_CIERRE) {
            Some(n) => (n, false),
            None => match casa(MARCADORES_TRAS_CIERRE_DEBILES) {
                Some(n) => (n, true),
                None => continue,
            },
        };

        // La corrección: tokens tras el marcador hasta el primer cierre
        // (incluido) o el final del texto, con el tope de siempre.
        let ini_corr = i + n;
        if ini_corr >= toks.len() {
            return None;
        }
        let mut fin_corr = ini_corr;
        while fin_corr < toks.len() {
            if fin_corr - ini_corr + 1 > MAX_TOKENS_CORRECCION {
                return None; // demasiado larga: ya no es una autocorrección
            }
            if cierra_para_correccion(toks[fin_corr]) || toks[fin_corr].ends_with(',') {
                break;
            }
            fin_corr += 1;
        }
        let fin_corr = fin_corr.min(toks.len() - 1);

        // Ancla: última aparición de la primera palabra de la corrección en
        // los tokens ANTERIORES al cierre.
        let clave_ancla = &claves[ini_corr];
        if clave_ancla.is_empty() {
            return None;
        }
        if debil && ANCLAS_FUNCION.contains(&clave_ancla.as_str()) {
            return None; // «Perdón, no…» es una disculpa, no una corrección
        }
        let ancla = (i.saturating_sub(MAX_TOKENS_CORRECCION)..i)
            .rev()
            .find(|&k| &claves[k] == clave_ancla)?;

        // Reconstrucción por tokens: prefijo + corrección (con su puntuación)
        // + lo que siga después de la corrección.
        let mut salida: Vec<&str> = Vec::with_capacity(toks.len());
        salida.extend(&toks[..ancla]);
        salida.extend(&toks[ini_corr..=fin_corr]);
        salida.extend(&toks[fin_corr + 1..]);
        return Some(salida.join(" "));
    }
    None
}

/// Una pasada: encuentra el primer `, marcador,` y lo resuelve. `None` si no
/// hay marcador o si no se pudo anclar el reemplazo (conservador).
#[allow(dead_code)]
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

/// Palabras cuya repetición inmediata suele ser deliberada: negación enfática
/// («no no, eso no»), énfasis («muy muy rápido») y dígitos dictados de a uno.
const NO_COLAPSAR: &[&str] = &[
    "no", "si", "ya", "muy", "cero", "uno", "dos", "tres", "cuatro", "cinco", "seis", "siete",
    "ocho", "nueve", "diez",
];

/// Colapsa la repetición inmediata accidental: «después después revisamos» →
/// «después revisamos». El ASR duplica palabras cuando quien dicta titubea.
///
/// Cuatro guardas, cada una nacida de un caso real que rompió el prototipo:
/// - [`NO_COLAPSAR`]: «no no», «muy muy» y los dígitos dictados se respetan.
/// - Mayúscula a MITAD de frase delata nombre propio («Baden Baden»); la del
///   token que abre el texto es ortotipografía y no cuenta.
/// - Tokens con dígitos jamás («22 22 33 44» es un teléfono).
/// - Vecinos de deletreo tampoco: en «hache te te pe ese» la doble «te» es
///   la doble T de «https».
pub fn colapsar_repeticiones(texto: &str) -> String {
    // Línea a línea: los saltos de línea del dictado sobreviven siempre.
    texto
        .split('\n')
        .map(colapsar_linea)
        .collect::<Vec<_>>()
        .join("\n")
}

fn colapsar_linea(texto: &str) -> String {
    let toks: Vec<&str> = texto.split_whitespace().collect();
    let mut out: Vec<&str> = Vec::new();
    for (i, t) in toks.iter().enumerate() {
        if let Some(&prev) = out.last() {
            let nucleo = prev.trim_end_matches(PUNT_FINAL);
            let clave_prev = build_match_key(nucleo);
            let clave_act = build_match_key(t.trim_end_matches(PUNT_FINAL));
            let mayuscula_media =
                out.len() > 1 && nucleo.chars().next().is_some_and(|c| c.is_uppercase());
            let numerico = nucleo.chars().any(|c| c.is_numeric());
            let es_deletreo =
                |w: &str| super::identificadores::es_clave_deletreo(&build_match_key(w));
            let vecino_deletreo = es_deletreo(nucleo)
                && (toks
                    .get(i + 1)
                    .is_some_and(|n| es_deletreo(n.trim_end_matches(PUNT_FINAL)))
                    || (i >= 2 && es_deletreo(toks[i - 2].trim_end_matches(PUNT_FINAL))));
            // El ASR real le pone COMAS al tartamudeo («después, después,»):
            // una coma pegada al primer token no es frontera de frase y no
            // impide el colapso. El punto y los demás cierres sí — esos
            // separan oraciones de verdad.
            let sufijo_prev = &prev[nucleo.len()..];
            let sufijo_no_bloquea = sufijo_prev.is_empty() || sufijo_prev == ",";
            if !clave_prev.is_empty()
                && clave_prev == clave_act
                && sufijo_no_bloquea
                && !NO_COLAPSAR.contains(&clave_prev.as_str())
                && !mayuscula_media
                && !numerico
                && !vecino_deletreo
            {
                // Conserva la puntuación del segundo token («tarda tarda,» → «tarda,»).
                *out.last_mut().unwrap() = t;
                continue;
            }
        }
        out.push(t);
    }
    // Sin colapso, el texto original queda byte a byte (espaciado incluido).
    let unido = out.join(" ");
    if unido == toks.join(" ") {
        texto.to_string()
    } else {
        unido
    }
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
            // ¿Token con separador interno de identificador? El punto de
            // «www.abrax.app», la arroba de «whisper@main.io» o los dos puntos
            // de «localhost:8080»: nada de eso es una palabra que capitalizar.
            let punto_interno = matches!(
                (indices.get(fin + 1), indices.get(fin + 2)),
                (Some(&(_, '.' | '@' | ':')), Some(&(_, sig))) if sig.is_alphanumeric()
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
        // La arroba y los dos puntos internos también delatan identificador:
        // un correo o un puerto abriendo oración se quedan como están.
        assert_eq!(
            capitalizar_oraciones("whisper@main.io es el remitente"),
            "whisper@main.io es el remitente"
        );
        assert_eq!(
            capitalizar_oraciones("localhost:8080 no responde. revisa el puerto"),
            "localhost:8080 no responde. Revisa el puerto"
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

    // ── repetición inmediata ──────────────────────────────────────────────

    #[test]
    fn colapsa_la_repeticion_accidental() {
        assert_eq!(
            colapsar_repeticiones("y después después revisamos el diseño"),
            "y después revisamos el diseño"
        );
        // La forma REAL del ASR: comas alrededor del tartamudeo. Cazada en el
        // E2E con micrófono — el corpus escrito nunca la produjo.
        assert_eq!(
            colapsar_repeticiones("Y después, después, revisar el diseño de Abrax."),
            "Y después, revisar el diseño de Abrax."
        );
        // El punto SÍ es frontera: dos oraciones que empiezan igual no se
        // tocan («Listo. listo del todo» sería otra frase).
        assert_eq!(
            colapsar_repeticiones("ya está listo. listo del todo"),
            "ya está listo. listo del todo"
        );
        // Conserva la puntuación del segundo token.
        assert_eq!(
            colapsar_repeticiones("la aplicación tarda tarda, mucho"),
            "la aplicación tarda, mucho"
        );
        assert_eq!(colapsar_repeticiones("es el el archivo"), "es el archivo");
    }

    #[test]
    fn repeticion_deliberada_se_respeta() {
        for t in [
            "no no, eso no",             // negación enfática
            "va a ser muy muy rápido",   // énfasis
            "el código es dos dos tres", // dígitos dictados de a uno
            "llama al 22 22 33 44",      // teléfono en cifras
            "vamos a Baden Baden",       // nombre propio a mitad de frase
        ] {
            assert_eq!(colapsar_repeticiones(t), t, "no debía tocar «{t}»");
        }
    }

    #[test]
    fn repeticion_en_deletreo_se_respeta() {
        // La doble «te» de «hache te te pe ese» es la doble T de «https».
        let t = "hache te te pe ese dos puntos barra barra";
        assert_eq!(colapsar_repeticiones(t), t);
    }

    #[test]
    fn mayuscula_que_abre_el_texto_no_es_nombre_propio() {
        // «Después, bueno, después» tras quitar la muletilla: la mayúscula del
        // primer token es ortotipografía, no evidencia de nombre propio. El
        // colapso conserva el SEGUNDO token (por su puntuación); la mayúscula
        // la restaura `capitalizar_oraciones` más adelante en la cadena.
        assert_eq!(
            colapsar_repeticiones("Después después hacemos las pruebas"),
            "después hacemos las pruebas"
        );
        assert_eq!(
            capitalizar_oraciones(&colapsar_repeticiones(
                "Después después hacemos las pruebas"
            )),
            "Después hacemos las pruebas"
        );
    }

    #[test]
    fn repeticion_preserva_saltos_de_linea() {
        assert_eq!(
            colapsar_repeticiones("primera línea línea\nsegunda intacta"),
            "primera línea\nsegunda intacta"
        );
        let t = "sin repetición\ncon  espacios  raros";
        assert_eq!(colapsar_repeticiones(t), t);
    }

    // ── marcadores nuevos de autocorrección ──────────────────────────────

    #[test]
    fn marcadores_nuevos_disparan_con_ancla() {
        assert_eq!(
            autocorreccion_hablada("debemos hacerlo el lunes, mentira, el martes"),
            "debemos hacerlo el martes"
        );
        assert_eq!(
            autocorreccion_hablada("lo vemos el viernes, corrijo, el sábado"),
            "lo vemos el sábado"
        );
        assert_eq!(
            autocorreccion_hablada("la reunión es en marzo, me equivoqué, en abril"),
            "la reunión es en abril"
        );
    }

    #[test]
    fn mentira_sin_estructura_de_marcador_no_dispara() {
        // «mentira» como sustantivo normal: sin comas alrededor no es señal.
        let t = "eso es mentira y lo sabes";
        assert_eq!(autocorreccion_hablada(t), t);
    }

    // ── falso comienzo tras cierre de oración (la forma real del ASR) ─────

    #[test]
    fn falso_comienzo_tras_punto_con_no_mejor() {
        // Whisper puntúa la pausa del que se corrige con un punto, no con
        // comas: esta es la frase estrella tal como salió del micrófono.
        assert_eq!(
            autocorreccion_hablada(
                "mañana tenemos que terminar la landing. No, mejor terminar el video. Y después revisamos."
            ),
            "mañana tenemos que terminar el video. Y después revisamos."
        );
    }

    #[test]
    fn redundancia_tras_punto_con_perdon() {
        // «…de Abrax. Perdón, Abrax.» — el diccionario ya corrigió las dos y
        // queda la reformulación redundante. El ancla la colapsa.
        assert_eq!(
            autocorreccion_hablada("y después revisamos el diseño de Abrax. Perdón, Abrax."),
            "y después revisamos el diseño de Abrax."
        );
    }

    #[test]
    fn digo_tras_punto_corrige_con_ancla() {
        assert_eq!(
            autocorreccion_hablada("Vamos el lunes. Digo, el martes."),
            "Vamos el martes."
        );
    }

    #[test]
    fn tras_interrogacion_no_es_falso_comienzo() {
        // Tras una pregunta, «No, …» es una respuesta. No se toca.
        let t = "¿Vamos el lunes? No, mejor el martes.";
        assert_eq!(autocorreccion_hablada(t), t);
    }

    #[test]
    fn perdon_tras_punto_como_disculpa_no_dispara() {
        // Marcador débil + ancla de palabra función = disculpa normal.
        for t in [
            "Pasé por tu casa. Perdón, por la demora.",
            "Llegué tarde. Perdón, no volverá a pasar.",
        ] {
            assert_eq!(autocorreccion_hablada(t), t, "no debía tocar «{t}»");
        }
    }

    #[test]
    fn falso_comienzo_sin_ancla_no_toca_nada() {
        let t = "La reunión terminó. Mentira, sigue en pie.";
        assert_eq!(autocorreccion_hablada(t), t);
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
