//! Autocorrección hablada — 100% local, por REGLAS, sin ningún modelo de IA.
//!
//! Cuando quien dicta se corrige a sí mismo en voz alta, el texto sale ya
//! corregido:
//!
//! ```text
//! «Agenda la reunión para el martes… no, perdón, para el miércoles»
//!   → «Agenda la reunión para el miércoles.»
//! ```
//!
//! Vive en el mismo lugar y con el mismo contrato que el filtro de muletillas
//! ([`super::text::filter_transcription_output`]): corrige al instante, sin red
//! y sin Post Proceso/BYOK. Refuerza el pilar «100% local» en vez de abrir una
//! excepción.
//!
//! # Los dos niveles
//!
//! - **Nivel 1 — borrado explícito.** Señales inequívocas («borra eso»,
//!   «olvida eso», «no, nada», «déjalo») borran la ORACIÓN ANTERIOR COMPLETA
//!   más la señal. Se prueba primero justamente por ser inequívoco.
//! - **Nivel 2 — sustitución con paralelo.** Tras la señal («no, perdón»,
//!   «mejor dicho», «digo», …) se busca hacia atrás un SEGMENTO PARALELO —misma
//!   preposición, mismo determinante y misma categoría (día, mes, hora, número,
//!   nombre propio)— y se reemplaza por lo que vino después de la señal.
//!
//! # REGLA DE ORO
//!
//! **Si no hay paralelo claro, NO SE TOCA NADA.** Ante la duda el texto queda
//! como se dictó. Esa cobardía deliberada es lo que hace la función segura: un
//! borrado indebido destruye la confianza en la app entera, mientras que una
//! corrección no aplicada solo deja al usuario donde ya estaba.
//!
//! Es la regla, y no una lista negra de excepciones, la que resuelve los falsos
//! positivos que importan: «perdón por la demora» y «te pido perdón» ni siquiera
//! son señales (la señal es «no, perdón», no «perdón» suelto), y «digo que sí» o
//! «mejor dicho de otra manera» sí lo son pero **no tienen categoría** después
//! de la señal, así que no se toca nada.
//!
//! # Lo que NO intenta
//!
//! Correcciones semánticas sin marcador, reescritura de estilo, resumen o cambio
//! de tono. Para eso haría falta un LLM, y este módulo existe precisamente para
//! no necesitarlo.

use std::collections::HashSet;

use once_cell::sync::Lazy;

use super::text::{build_match_key, extract_punctuation, token_core};

/// Pasadas máximas sobre un mismo dictado. Cada pasada aplica UNA corrección y
/// vuelve a empezar, así que el tope acota un dictado con varias correcciones
/// encadenadas sin permitir un bucle si alguna regla futura fuera inestable.
const MAX_PASADAS: usize = 8;

/// Tokens hacia atrás en los que se busca el segmento paralelo. Más allá de esto
/// ya no es «lo que acabo de decir», es otro párrafo.
const VENTANA_ATRAS: usize = 25;

/// Tokens del segmento en los que se busca la categoría. La categoría vive en la
/// cabeza del segmento; buscarla hasta el final haría paralelos a dos frases que
/// solo comparten una palabra lejana.
const CABEZA_SEGMENTO: usize = 4;

/// Señales de nivel 1 (borrado explícito) por defecto, en es-419.
pub const BORRADO_POR_DEFECTO: &[&str] = &["borra eso", "olvida eso", "no, nada", "déjalo"];

/// Señales de nivel 2 (sustitución con paralelo) por defecto, en es-419.
///
/// «perdón» NO está sola a propósito: «perdón por la demora» es una disculpa,
/// no una retractación. Solo entran las formas que anuncian corrección.
pub const SUSTITUCION_POR_DEFECTO: &[&str] = &[
    "no, perdón",
    "perdón, quise decir",
    "mejor dicho",
    "o sea, no",
    "corrijo",
    "no, mejor",
    "digo",
];

/// Señales de sustitución que SOLO valen justo después de puntos suspensivos.
///
/// «no» delimitado por comas es casi siempre una negación de verdad («vamos
/// mañana, no, pasado mañana» la contiene y no debe tocarse), así que jamás
/// puede ser señal general. Pero tras «…» la prosodia cambia: «a las ocho…
/// no, a las nueve» es un falso comienzo, la forma más común de corregirse al
/// dictar. La elipsis la escribe el ASR cuando la voz se corta a media frase.
///
/// Doble candado: además del contexto de elipsis, estas señales pasan por el
/// mismo nivel 2 que las demás — sin segmento paralelo no se toca nada.
/// No son configurables por diseño: su seguridad depende del contexto, no de
/// la lista, y siguen la suerte del nivel de sustitución (apagado = apagadas).
const SUSTITUCION_TRAS_ELIPSIS: &[&str] = &["no", "bueno, no", "mejor"];

static PREPOSICIONES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "a", "ante", "bajo", "con", "contra", "de", "desde", "durante", "en", "entre", "hacia",
        "hasta", "mediante", "para", "por", "segun", "sin", "sobre", "tras",
    ]
    .into_iter()
    .collect()
});

static DETERMINANTES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "el", "la", "los", "las", "un", "una", "unos", "unas", "mi", "mis", "tu", "tus", "su",
        "sus", "este", "esta", "estos", "estas", "ese", "esa", "esos", "esas", "aquel", "aquella",
    ]
    .into_iter()
    .collect()
});

static DIAS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "lunes",
        "martes",
        "miercoles",
        "jueves",
        "viernes",
        "sabado",
        "domingo",
    ]
    .into_iter()
    .collect()
});

static MESES: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "enero",
        "febrero",
        "marzo",
        "abril",
        "mayo",
        "junio",
        "julio",
        "agosto",
        "septiembre",
        "setiembre",
        "octubre",
        "noviembre",
        "diciembre",
    ]
    .into_iter()
    .collect()
});

static NUMEROS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "cero",
        "uno",
        "una",
        "dos",
        "tres",
        "cuatro",
        "cinco",
        "seis",
        "siete",
        "ocho",
        "nueve",
        "diez",
        "once",
        "doce",
        "trece",
        "catorce",
        "quince",
        "dieciseis",
        "diecisiete",
        "dieciocho",
        "diecinueve",
        "veinte",
        "veintiuno",
        "veintidos",
        "veintitres",
        "veinticuatro",
        "veinticinco",
        "veintiseis",
        "veintisiete",
        "veintiocho",
        "veintinueve",
        "treinta",
        "cuarenta",
        "cincuenta",
        "sesenta",
        "setenta",
        "ochenta",
        "noventa",
        "cien",
        "ciento",
        "mil",
        "millon",
    ]
    .into_iter()
    .collect()
});

/// Determinantes que convierten un número en una hora: «las tres» es una hora,
/// «tres» a secas es un número.
static DETERMINANTES_HORA: Lazy<HashSet<&'static str>> =
    Lazy::new(|| ["la", "las"].into_iter().collect());

/// La categoría de un segmento. Dos segmentos son paralelos solo si comparten
/// categoría (además de preposición y determinante).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Categoria {
    DiaSemana,
    Mes,
    Hora,
    Numero,
    NombrePropio,
}

/// La «forma» de un segmento: lo que se compara para decidir si dos segmentos
/// son paralelos.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Forma {
    preposicion: Option<String>,
    determinante: Option<String>,
    categoria: Option<Categoria>,
}

struct Token<'a> {
    crudo: &'a str,
    clave: String,
    nucleo: &'a str,
    /// El token cierra oración (lleva `.`, `!`, `?` o puntos suspensivos).
    cierra: bool,
    /// El token abre oración (es el primero, o el anterior cerró).
    inicia: bool,
}

/// Puntuación que cierra una oración. El punto y coma no entra: no cierra una
/// oración, y para el nivel 1 («la oración anterior completa») esa distinción es
/// exactamente la que decide cuánto se borra.
fn cierra_oracion(sufijo: &str) -> bool {
    sufijo.chars().any(|c| matches!(c, '.' | '!' | '?' | '…'))
}

/// Los puntos suspensivos son marca de duda al dictar, no puntuación real: al
/// heredar el cierre de un segmento sustituido se normalizan a punto, que es lo
/// que el usuario habría escrito.
fn normalizar_cierre(sufijo: &str) -> String {
    let limpio = sufijo.replace('…', ".");
    let mut salida = String::new();
    let mut puntos = 0;
    for c in limpio.chars() {
        if c == '.' {
            puntos += 1;
            if puntos > 1 {
                continue;
            }
        }
        salida.push(c);
    }
    salida
}

fn tokenizar(texto: &str) -> Vec<Token<'_>> {
    let crudos: Vec<&str> = texto.split_whitespace().collect();
    let mut tokens: Vec<Token> = Vec::with_capacity(crudos.len());
    for (idx, crudo) in crudos.iter().enumerate() {
        let (_, sufijo) = extract_punctuation(crudo);
        let inicia = idx == 0 || tokens.last().is_some_and(|t: &Token| t.cierra);
        tokens.push(Token {
            crudo,
            clave: build_match_key(crudo),
            nucleo: token_core(crudo),
            cierra: cierra_oracion(sufijo),
            inicia,
        });
    }
    tokens
}

/// Señales compiladas a secuencias de claves normalizadas, más largas primero
/// (matching codicioso: «no, perdón» gana a «no, nada» si ambas empataran).
fn compilar(senales: &[String]) -> Vec<Vec<String>> {
    let mut compiladas: Vec<Vec<String>> = senales
        .iter()
        .map(|s| {
            s.split_whitespace()
                .map(build_match_key)
                .filter(|k| !k.is_empty())
                .collect::<Vec<String>>()
        })
        .filter(|k: &Vec<String>| !k.is_empty())
        .collect();
    compiladas.sort_by_key(|k| std::cmp::Reverse(k.len()));
    compiladas.dedup();
    compiladas
}

/// ¿El token termina en puntos suspensivos? Acepta el carácter «…» y la forma
/// de tres puntos sueltos que emiten algunos ASR («gigas...»).
fn termina_en_elipsis(token: &Token) -> bool {
    let (_, sufijo) = extract_punctuation(token.crudo);
    sufijo.contains('…') || sufijo.contains("...")
}

/// ¿Empieza en `i` alguna de estas señales? Devuelve su largo en tokens.
///
/// Una señal multi-token no cruza un cierre de oración en su interior, igual que
/// las muletillas multi-palabra: «no. Perdón» no es la señal «no, perdón».
fn casa_senal(tokens: &[Token], i: usize, senales: &[Vec<String>]) -> Option<usize> {
    for claves in senales {
        let n = claves.len();
        if i + n > tokens.len() {
            continue;
        }
        if (0..n.saturating_sub(1)).any(|j| tokens[i + j].cierra) {
            continue;
        }
        if (0..n).all(|j| tokens[i + j].clave == claves[j]) {
            return Some(n);
        }
    }
    None
}

fn categoria_de(token: &Token) -> Option<Categoria> {
    if DIAS.contains(token.clave.as_str()) {
        return Some(Categoria::DiaSemana);
    }
    if MESES.contains(token.clave.as_str()) {
        return Some(Categoria::Mes);
    }
    if matches!(token.clave.as_str(), "mediodia" | "medianoche") {
        return Some(Categoria::Hora);
    }
    // «15:30» / «15.30» — el núcleo conserva la puntuación interior.
    let nucleo = token.nucleo;
    if let Some((h, m)) = nucleo.split_once([':', '.']) {
        if !h.is_empty()
            && h.len() <= 2
            && h.chars().all(|c| c.is_ascii_digit())
            && m.len() == 2
            && m.chars().all(|c| c.is_ascii_digit())
        {
            return Some(Categoria::Hora);
        }
    }
    if !token.clave.is_empty() && token.clave.chars().all(|c| c.is_ascii_digit()) {
        return Some(Categoria::Numero);
    }
    if NUMEROS.contains(token.clave.as_str()) {
        return Some(Categoria::Numero);
    }
    // Nombre propio: mayúscula inicial que NO se explica por abrir oración.
    // Se pide largo >= 3 para no confundir siglas de una o dos letras con un
    // nombre, y se excluyen las categorías anteriores (ya resueltas arriba).
    if !token.inicia
        && token.nucleo.chars().count() >= 3
        && token
            .nucleo
            .chars()
            .next()
            .is_some_and(|c| c.is_uppercase())
        && !PREPOSICIONES.contains(token.clave.as_str())
        && !DETERMINANTES.contains(token.clave.as_str())
    {
        return Some(Categoria::NombrePropio);
    }
    None
}

/// La forma del segmento `tokens[desde..hasta)`.
fn forma_de(tokens: &[Token], desde: usize, hasta: usize) -> Forma {
    let mut idx = desde;
    let mut preposicion = None;
    let mut determinante = None;

    if idx < hasta && PREPOSICIONES.contains(tokens[idx].clave.as_str()) {
        preposicion = Some(tokens[idx].clave.clone());
        idx += 1;
    }
    if idx < hasta && DETERMINANTES.contains(tokens[idx].clave.as_str()) {
        determinante = Some(tokens[idx].clave.clone());
        idx += 1;
    }

    let tope = hasta.min(idx + CABEZA_SEGMENTO);
    let mut categoria = None;
    for t in tokens.iter().take(tope).skip(idx) {
        if let Some(c) = categoria_de(t) {
            categoria = Some(c);
            break;
        }
    }

    // «las tres» es una hora; «tres» a secas es un número. El determinante es
    // lo único que los distingue sin entender la frase.
    if categoria == Some(Categoria::Numero)
        && determinante
            .as_deref()
            .is_some_and(|d| DETERMINANTES_HORA.contains(d))
    {
        categoria = Some(Categoria::Hora);
    }

    Forma {
        preposicion,
        determinante,
        categoria,
    }
}

/// Fin (exclusivo) de la oración que empieza en `desde`, sin pasar de `tope`.
fn fin_de_oracion(tokens: &[Token], desde: usize, tope: usize) -> usize {
    for (k, t) in tokens.iter().enumerate().take(tope).skip(desde) {
        if t.cierra {
            return k + 1;
        }
    }
    tope
}

/// Nivel 1: borra la oración anterior completa y la señal.
/// ¿La señal de borrado CIERRA el dictado? Es lo que separa una orden de una
/// frase que casualmente contiene esas palabras.
///
/// Cierra si no queda nada detrás, o si la propia señal termina en puntuación de
/// cierre («… borra eso. Y ahora otra cosa»: ahí sí fue una orden y lo que sigue
/// es dictado nuevo).
fn borrado_cierra_el_dictado(tokens: &[Token], i: usize, n: usize) -> bool {
    i + n >= tokens.len() || tokens[i + n - 1].cierra
}

fn aplicar_borrado(tokens: &[Token], i: usize, n: usize) -> String {
    let mut inicio = i;
    if i > 0 {
        // El token anterior es el final de la oración a borrar; se retrocede
        // hasta justo después del cierre anterior.
        inicio = i - 1;
        while inicio > 0 && !tokens[inicio - 1].cierra {
            inicio -= 1;
        }
    }
    let mut salida: Vec<&str> = Vec::with_capacity(tokens.len());
    salida.extend(tokens[..inicio].iter().map(|t| t.crudo));
    salida.extend(tokens[i + n..].iter().map(|t| t.crudo));
    salida.join(" ")
}

/// Nivel 2: sustitución con paralelo. Devuelve `None` —y por tanto NO toca
/// nada— si no hay categoría o no aparece un segmento paralelo hacia atrás.
fn aplicar_sustitucion(tokens: &[Token], i: usize, n: usize) -> Option<String> {
    let ini_r = i + n;
    if ini_r >= tokens.len() {
        return None;
    }
    let fin_r = fin_de_oracion(tokens, ini_r, tokens.len());
    let forma_r = forma_de(tokens, ini_r, fin_r);

    // REGLA DE ORO: sin categoría no hay paralelo posible, y sin paralelo no se
    // toca nada. Aquí mueren «digo que sí» y «mejor dicho de otra manera».
    forma_r.categoria?;

    let piso = i.saturating_sub(VENTANA_ATRAS);
    let mut candidato = None;
    for p in (piso..i).rev() {
        let fin_c = fin_de_oracion(tokens, p, i);
        if forma_de(tokens, p, fin_c) == forma_r {
            candidato = Some((p, fin_c));
            break;
        }
    }
    let (ini_c, fin_c) = candidato?;

    // El reemplazo hereda el cierre de oración del segmento sustituido cuando él
    // no trae uno propio: el «…» de la duda se convierte en el punto final.
    let mut reemplazo: Vec<String> = tokens[ini_r..fin_r]
        .iter()
        .map(|t| t.crudo.to_string())
        .collect();
    let cierre_candidato = extract_punctuation(tokens[fin_c - 1].crudo).1;
    if !tokens[fin_r - 1].cierra && cierra_oracion(cierre_candidato) {
        let ultimo = reemplazo.len() - 1;
        reemplazo[ultimo].push_str(&normalizar_cierre(cierre_candidato));
    }
    // Si el segmento sustituido abría la oración, el reemplazo hereda la
    // mayúscula: no puede quedar una oración empezando en minúscula.
    if tokens[ini_c].inicia {
        let primero = &mut reemplazo[0];
        let mut chars = primero.chars();
        if let Some(c) = chars.next() {
            if c.is_lowercase() {
                *primero = c.to_uppercase().collect::<String>() + chars.as_str();
            }
        }
    }

    let mut salida: Vec<String> = Vec::with_capacity(tokens.len());
    salida.extend(tokens[..ini_c].iter().map(|t| t.crudo.to_string()));
    salida.extend(reemplazo);
    salida.extend(tokens[fin_c..i].iter().map(|t| t.crudo.to_string()));
    salida.extend(tokens[fin_r..].iter().map(|t| t.crudo.to_string()));
    Some(salida.join(" "))
}

/// Una pasada: aplica como mucho UNA corrección. `None` = no se tocó nada.
fn una_pasada(
    texto: &str,
    borrado: &[Vec<String>],
    sustitucion: &[Vec<String>],
    sustitucion_elipsis: &[Vec<String>],
) -> Option<String> {
    let tokens = tokenizar(texto);
    let mut i = 0;
    while i < tokens.len() {
        // Nivel 1 primero: es inequívoco… PERO SOLO SI CIERRA EL DICTADO.
        //
        // Sin esa condición no era inequívoco en absoluto: las cuatro señales de
        // fábrica son español corriente en mitad de una frase, y como el nivel 1
        // borra LA ORACIÓN ANTERIOR COMPLETA, el destrozo era total. Medido el
        // 30/07 sobre prosa normal, con las señales de fábrica:
        //
        //   «Termina el informe y déjalo para el lunes»  → «para el lunes»
        //   «El archivo está en el escritorio, déjalo ahí» → «ahí»
        //   «Le dije que no, nada más»                   → «más»
        //   «Borra eso del pizarrón por favor»           → «del pizarrón por favor»
        //
        // Seis de ocho frases mutiladas. Por eso la función venía apagada de
        // fábrica, y por eso no se podía encender sin esto.
        //
        // La condición sale de cómo se usa de verdad: «borra eso» es una ORDEN,
        // y una orden se dicta al final —dices el texto, te arrepientes, dices
        // «borra eso» y callas—. Si detrás sigue habiendo frase, no era una
        // orden: era la frase.
        if let Some(n) = casa_senal(&tokens, i, borrado) {
            if borrado_cierra_el_dictado(&tokens, i, n) {
                return Some(aplicar_borrado(&tokens, i, n));
            }
            // No era orden: se sigue buscando, igual que hace el nivel 2 cuando
            // una señal suya no encuentra paralelo.
            i += n;
            continue;
        }
        if let Some(n) = casa_senal(&tokens, i, sustitucion) {
            if let Some(nuevo) = aplicar_sustitucion(&tokens, i, n) {
                return Some(nuevo);
            }
            // Señal sin paralelo: no se toca nada y se sigue buscando más
            // adelante, por si el dictado trae otra corrección que sí aplica.
            i += n;
            continue;
        }
        // Señales que solo existen justo después de «…»: mismo nivel 2, con el
        // candado extra del contexto. «no» entre comas jamás llega aquí.
        if i > 0 && termina_en_elipsis(&tokens[i - 1]) {
            if let Some(n) = casa_senal(&tokens, i, sustitucion_elipsis) {
                if let Some(nuevo) = aplicar_sustitucion(&tokens, i, n) {
                    return Some(nuevo);
                }
                i += n;
                continue;
            }
        }
        i += 1;
    }
    None
}

/// Aplica la autocorrección hablada.
///
/// Mismo contrato que las muletillas para las dos listas de señales:
/// `None` usa las señales por defecto, `Some(vec![])` desactiva ese nivel y
/// `Some(lista)` reemplaza las de fábrica — cada quien tiene sus propias
/// muletillas de corrección.
///
/// Con ambas listas vacías la función es un no-op exacto: devuelve el texto sin
/// tocar ni un espacio, que es lo que debe pasar con el ajuste apagado.
pub fn aplicar_autocorreccion(
    texto: &str,
    senales_borrado: &Option<Vec<String>>,
    senales_sustitucion: &Option<Vec<String>>,
) -> String {
    let a_lista = |propias: &Option<Vec<String>>, defecto: &[&str]| -> Vec<Vec<String>> {
        match propias {
            Some(v) => compilar(v),
            None => compilar(&defecto.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        }
    };
    let borrado = a_lista(senales_borrado, BORRADO_POR_DEFECTO);
    let sustitucion = a_lista(senales_sustitucion, SUSTITUCION_POR_DEFECTO);

    if borrado.is_empty() && sustitucion.is_empty() {
        return texto.to_string();
    }

    // Las señales de elipsis siguen la suerte del nivel de sustitución: con el
    // nivel apagado (`Some(vec![])`) tampoco existen.
    let sustitucion_elipsis: Vec<Vec<String>> = if sustitucion.is_empty() {
        Vec::new()
    } else {
        compilar(
            &SUSTITUCION_TRAS_ELIPSIS
                .iter()
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
        )
    };

    let mut actual = texto.to_string();
    for _ in 0..MAX_PASADAS {
        match una_pasada(&actual, &borrado, &sustitucion, &sustitucion_elipsis) {
            Some(nuevo) => actual = nuevo,
            None => return actual,
        }
    }
    actual
}

#[cfg(test)]
mod tests {
    use super::*;

    fn corrige(texto: &str) -> String {
        aplicar_autocorreccion(texto, &None, &None)
    }

    // ───────────────────────── Nivel 2 — el caso principal ─────────────────

    #[test]
    fn nivel2_el_ejemplo_de_la_orden() {
        assert_eq!(
            corrige("Agenda la reunión para el martes… no, perdón, para el miércoles"),
            "Agenda la reunión para el miércoles."
        );
    }

    #[test]
    fn nivel2_sustituye_dia_sin_preposicion() {
        assert_eq!(
            corrige("Nos vemos el jueves, digo, el viernes."),
            "Nos vemos el viernes."
        );
    }

    #[test]
    fn nivel2_sustituye_mes() {
        assert_eq!(
            corrige("El pago vence en marzo, mejor dicho en abril."),
            "El pago vence en abril."
        );
    }

    #[test]
    fn nivel2_sustituye_hora() {
        assert_eq!(
            corrige("La llamada es a las tres, corrijo a las cuatro."),
            "La llamada es a las cuatro."
        );
    }

    #[test]
    fn nivel2_sustituye_numero() {
        assert_eq!(
            corrige("Pide veinte cajas, no, mejor treinta cajas."),
            "Pide treinta cajas."
        );
    }

    #[test]
    fn nivel2_sustituye_nombre_propio() {
        assert_eq!(
            corrige("Avísale a Pedro, perdón, quise decir a Juan."),
            "Avísale a Juan."
        );
    }

    #[test]
    fn nivel2_toma_el_paralelo_mas_cercano() {
        // Hay dos jueves; manda el que está justo antes de la señal.
        assert_eq!(
            corrige("El jueves cerramos y el jueves abrimos, digo, el viernes abrimos."),
            "El jueves cerramos y el viernes abrimos."
        );
    }

    #[test]
    fn nivel2_hereda_la_mayuscula_de_la_oracion() {
        assert_eq!(
            corrige("Martes es el plazo. Mejor dicho miércoles es el plazo."),
            "Miércoles es el plazo."
        );
    }

    #[test]
    fn nivel2_exige_la_misma_preposicion() {
        // «para el miércoles» no es paralelo de «desde el martes»: distinta
        // preposición, distinto sentido. No se toca nada.
        let texto = "Trabajo desde el martes, digo, para el miércoles";
        assert_eq!(corrige(texto), texto);
    }

    // ───────────────────────── Nivel 1 — borrado explícito ─────────────────

    #[test]
    fn nivel1_borra_la_oracion_anterior_completa() {
        assert_eq!(
            corrige("Hola equipo. Manda el informe hoy. Borra eso."),
            "Hola equipo."
        );
    }

    #[test]
    fn nivel1_olvida_eso() {
        assert_eq!(
            corrige("Compra pan. Olvida eso. Compra leche."),
            "Compra leche."
        );
    }

    #[test]
    fn nivel1_no_nada() {
        assert_eq!(corrige("Llama a soporte. No, nada."), "");
    }

    #[test]
    fn nivel1_dejalo() {
        assert_eq!(corrige("Cancela el pedido. Déjalo."), "");
    }

    #[test]
    fn nivel1_no_cruza_el_punto_hacia_atras() {
        // Solo cae la última oración; la primera sobrevive intacta.
        assert_eq!(
            corrige("Primera oración. Segunda oración. Borra eso."),
            "Primera oración."
        );
    }

    // ─────────────── Falsos positivos OBLIGATORIOS de la orden ─────────────
    //
    // Importan MÁS que los casos que funcionan: un borrado indebido destruye la
    // confianza en la app entera.

    #[test]
    fn falso_positivo_perdon_por_la_demora() {
        let texto = "Perdón por la demora, ya te mando el archivo.";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_te_pido_perdon() {
        let texto = "Te pido perdón por lo de ayer.";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_digo_que_si() {
        // «digo» SÍ es señal, pero «que sí» no tiene categoría: regla de oro.
        let texto = "Yo digo que sí.";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_mejor_dicho_sin_paralelo() {
        let texto = "Explícalo mejor dicho de otra manera.";
        assert_eq!(corrige(texto), texto);
    }

    // ───────────────────── Más falsos positivos, por seguridad ─────────────

    #[test]
    fn falso_positivo_perdon_solo_no_es_senal() {
        let texto = "Perdón, el martes no puedo.";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_digo_sin_nada_detras() {
        let texto = "Lo que digo";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_senal_partida_por_un_punto() {
        // «no. Perdón» no es la señal «no, perdón»: el punto cierra la oración.
        let texto = "Te dije que no. Perdón el martes reviso.";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_categoria_sin_paralelo_hacia_atras() {
        // Hay categoría después de la señal, pero nada paralelo antes.
        let texto = "Hablemos, digo, el martes";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn falso_positivo_paralelo_demasiado_lejos() {
        let relleno = "palabra ".repeat(VENTANA_ATRAS + 5);
        let texto = format!("El martes {relleno}digo, el miércoles");
        assert_eq!(corrige(&texto), texto);
    }

    // ───────────────────────── Apagado y contratos ─────────────────────────

    #[test]
    fn apagado_es_identidad_exacta() {
        // Con ambas listas vacías no se toca ni un espacio: el comportamiento
        // debe ser IDÉNTICO al de antes de existir esta función.
        let vacio = Some(vec![]);
        for texto in [
            "Agenda la reunión para el martes… no, perdón, para el miércoles",
            "Hola equipo. Manda el informe hoy. Borra eso.",
            "  espacios   raros  y  tabs\ty  saltos\n",
            "",
        ] {
            assert_eq!(
                aplicar_autocorreccion(texto, &vacio, &vacio),
                texto,
                "el ajuste apagado no puede tocar el texto"
            );
        }
    }

    #[test]
    fn senales_propias_reemplazan_las_de_fabrica() {
        // Quien usa «nel» en vez de «no, perdón» lo configura y funciona…
        let propias = Some(vec!["nel".to_string()]);
        assert_eq!(
            aplicar_autocorreccion("Vamos el martes nel el miércoles", &None, &propias),
            "Vamos el miércoles"
        );
        // …y las de fábrica dejan de aplicar, porque REEMPLAZAN, no se suman.
        let texto = "Vamos el martes, digo, el miércoles";
        assert_eq!(aplicar_autocorreccion(texto, &None, &propias), texto);
    }

    #[test]
    fn texto_sin_ninguna_senal_queda_igual() {
        let texto = "Agenda la reunión para el miércoles a las cuatro con Juan.";
        assert_eq!(corrige(texto), texto);
    }

    #[test]
    fn dos_correcciones_en_un_mismo_dictado() {
        assert_eq!(
            corrige("Nos vemos el martes, digo, el miércoles a las tres, corrijo a las cuatro."),
            "Nos vemos el miércoles a las cuatro."
        );
    }

    #[test]
    fn no_entra_en_bucle_con_texto_degenerado() {
        // Solo debe terminar; el contenido exacto no es contrato.
        let texto = "digo digo digo el martes digo el miércoles digo";
        let _ = corrige(texto);
    }

    #[test]
    fn tildes_y_mayusculas_no_impiden_reconocer_la_senal() {
        // La señal se compara por clave normalizada: «Dejalo» sin tilde cae.
        assert_eq!(corrige("Manda el correo. Dejalo."), "");
        assert_eq!(corrige("Manda el correo. DÉJALO."), "");
    }

    // ──────────────── «no» tras puntos suspensivos (falso comienzo) ────────

    #[test]
    fn elipsis_no_suelto_corrige_hora() {
        assert_eq!(
            corrige("Mañana voy a llegar a las ocho… no, a las nueve."),
            "Mañana voy a llegar a las nueve."
        );
    }

    #[test]
    fn elipsis_no_suelto_corrige_dia() {
        assert_eq!(
            corrige("Lo dejamos para el viernes… no, el jueves."),
            "Lo dejamos para el jueves."
        );
    }

    #[test]
    fn elipsis_tres_puntos_ascii_tambien_cuenta() {
        // Algunos ASR escriben «...» en vez de «…».
        assert_eq!(
            corrige("La reunión es a las cuatro... no, a las cinco."),
            "La reunión es a las cinco."
        );
    }

    #[test]
    fn elipsis_mejor_corrige_con_paralelo() {
        assert_eq!(
            corrige("La demo es a las cuatro… mejor, a las seis."),
            "La demo es a las seis."
        );
    }

    #[test]
    fn elipsis_sin_categoria_conocida_se_abstiene() {
        // «violeta» no es día, mes, hora, número ni nombre propio: sin
        // categoría no hay paralelo, y sin paralelo no se toca nada. El
        // marcador queda visible — mejor eso que adivinar cuánto reemplazar.
        let t = "Tenemos que usar el color verde… no, mejor el violeta.";
        assert_eq!(corrige(t), t);
    }

    #[test]
    fn no_entre_comas_sigue_siendo_negacion() {
        // Sin elipsis delante, «no» jamás es señal: esta frase contiene una
        // negación de verdad y debe quedar exactamente como se dictó.
        let t = "vamos mañana, no, pasado mañana";
        assert_eq!(corrige(t), t);
    }

    #[test]
    fn no_tras_punto_no_es_senal() {
        // El punto es un cierre deliberado, no una duda: no activa la señal.
        let t = "Ya lo revisé. No, no hace falta repetirlo.";
        assert_eq!(corrige(t), t);
    }

    #[test]
    fn elipsis_sin_paralelo_no_toca_nada() {
        // La regla de oro sobrevive al contexto de elipsis: «espera» no tiene
        // categoría, así que no hay paralelo posible y no se toca nada.
        let t = "Primero tenemos que… no, espera.";
        assert_eq!(corrige(t), t);
    }

    #[test]
    fn elipsis_con_nivel_sustitucion_apagado_no_existe() {
        // Las señales de elipsis siguen la suerte del nivel 2: apagado el
        // nivel, apagadas ellas — identidad exacta.
        let t = "Voy a las ocho… no, a las nueve.";
        assert_eq!(aplicar_autocorreccion(t, &None, &Some(vec![])), t);
    }

    // ── Nivel 1: orden sí, frase no ────────────────────────────────────────
    //
    // Este bloque es la razón por la que la función se puede encender de
    // fábrica. Antes de la guarda, las seis primeras salían mutiladas.

    #[test]
    fn nivel1_no_toca_prosa_donde_la_senal_va_en_medio() {
        for texto in [
            "Termina el informe y déjalo para el lunes",
            "El archivo está en el escritorio, déjalo ahí",
            "Ya revisé el contrato. Déjalo como está",
            "No, nada que ver con eso",
            "Le dije que no, nada más",
            "Borra eso del pizarrón por favor",
            "Olvida eso que te conté y sigamos",
        ] {
            assert_eq!(corrige(texto), texto, "mutiló: «{texto}»");
        }
    }

    #[test]
    fn nivel1_sigue_borrando_cuando_es_una_orden_de_verdad() {
        // Dictas una frase, te arrepientes y dices la orden: la frase se va
        // entera y no queda nada. Es lo que se pidió.
        assert_eq!(corrige("Agenda la reunión para el martes. Borra eso"), "");
        assert_eq!(corrige("Nos juntamos el lunes, borra eso"), "");
        // Se borra SOLO la oración anterior, no todo lo dictado antes.
        assert_eq!(
            corrige("Primero esto. Segundo lo otro. Borra eso"),
            "Primero esto."
        );
        // Con la señal cerrada por punto y dictado nuevo detrás.
        assert_eq!(
            corrige("Confirmo el envío. Olvida eso. Mando el correo mañana"),
            "Mando el correo mañana"
        );
    }

    #[test]
    fn los_valores_de_fabrica_corrigen_sin_destrozar() {
        // Control POSITIVO: lo que la función promete, con los ajustes de
        // fábrica exactos (ambas listas en `None`).
        assert_eq!(
            corrige("Llegamos a las tres, digo, a las cuatro"),
            "Llegamos a las cuatro"
        );
        assert_eq!(
            corrige("Manda el correo a Pedro, mejor dicho, a Ana"),
            "Manda el correo a Ana"
        );
        // Control NEGATIVO: prosa que usa las mismas palabras sin corregir nada.
        for texto in [
            "Perdón por la demora, ya voy.",
            "Digo que sí a la propuesta.",
            "Te pido perdón por lo de ayer.",
        ] {
            assert_eq!(corrige(texto), texto, "tocó: «{texto}»");
        }
    }
}
