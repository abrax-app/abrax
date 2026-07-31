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
//! # Como funciona
//!
//! Tras la senal («no, perdon», «perdon», «mejor dicho», «digo», …) se busca
//! hacia atras un SEGMENTO PARALELO —misma preposicion y misma categoria (dia,
//! mes, hora, numero, nombre propio)— y se reemplaza por lo que vino despues de
//! la senal.
//!
//! # Hubo un segundo nivel, y se retiro
//!
//! Existio un «borrado explicito»: senales como «borra eso» u «olvida eso» que
//! eliminaban LA ORACION ANTERIOR COMPLETA. Se retiro el 31/07 por decision de
//! producto, y con razon: era la unica pieza de todo el dictado capaz de hacer
//! desaparecer texto, y sus cuatro senales de fabrica son espanol de todos los
//! dias. Medido con prosa normal, destrozaba seis de cada ocho frases:
//!
//! ```text
//! «Termina el informe y dejalo para el lunes»  ->  «para el lunes»
//! «Le dije que no, nada mas»                   ->  «mas»
//! ```
//!
//! Llego a tener una guarda que exigia que la senal cerrara el dictado, y con
//! ella dejaba de destrozar. Aun asi se fue: nadie la habia pedido, y una
//! funcion que borra parrafos enteros no se sostiene por si sola al lado de una
//! que solo sustituye una fecha.
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

/// Cuantas palabras puede variar el largo entre los dos tramos cuando el ancla
/// es la preposicion.
///
/// Exigir el mismo largo EXACTO dejaba fuera lo normal: quien se corrige suele
/// precisar, y precisar alarga. «vaya al dentista, perdon, al otorrino» pide dos
/// palabras donde habia una.
///
/// Uno es el numero justo, medido contra el corpus: deja pasar la precision y
/// sigue bloqueando la frase que continua —«por ti» frente a «por favor no te
/// enojes» son tres de diferencia—.
const TOLERANCIA_LARGO: usize = 1;

/// Señales de correccion (sustitución con paralelo) por defecto, en es-419.
///
/// **«perdón» sola SÍ entra**, desde el 31/07. Estuvo fuera por miedo a «perdón
/// por la demora», que es una disculpa y no una retractación — pero ese miedo
/// sobraba: la REGLA DE ORO ya lo cubre. Sin un segmento paralelo hacia atrás no
/// se toca nada, y una disculpa no tiene paralelo.
///
/// Medido antes de quitarla, con siete disculpas reales:
///
/// ```text
/// «Perdón por la demora, ya voy»              intacta
/// «te pido perdón por lo de ayer»             intacta
/// «perdón, no te escuché»                     intacta
/// «le pedí perdón a mi hermana el lunes»      intacta  ← lleva un día detrás
/// «perdón por llegar tarde el martes»         intacta  ← y ésta también
/// ```
///
/// Las dos últimas son la prueba que importa: tienen una fecha después de
/// «perdón» y aun así no se tocan, porque lo que falta es el paralelo hacia
/// ATRÁS. Excluirla costaba la corrección que más gente dice —Winston la dictó
/// tres veces seguidas el 31/07 y ninguna funcionó— a cambio de una seguridad
/// que ya daba otra regla.
pub const SUSTITUCION_POR_DEFECTO: &[&str] = &[
    "no, perdón",
    "perdón, quise decir",
    "perdón",
    // Traidas del motor de `correccion/reglas.rs` el 31/07, que las tenia y
    // este no. Es lo que hacia que en la maquina de Antonio «funcionara bien» y
    // aqui no: no era el codigo del emparejador, era que la mitad de las formas
    // de retractarse no estaban en la lista. «me equivoque» —la que mas se dice—
    // no figuraba.
    "me equivoqué",
    "perdona",
    "disculpa",
    "quise decir",
    "quiero decir",
    "más bien",
    "mentira",
    "miento",
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
        // Contracciones: «al» = a+el, «del» = de+el. Estaban solo en la lista de
        // palabras vacias, asi que «vaya AL dentista, perdon, AL otorrino» no
        // tenia ancla ninguna — ni preposicion ni cabeza. Dictado real del 31/07.
        "al", "del",
    ]
    .into_iter()
    .collect()
});

/// Palabras VACIAS: no sirven como ancla porque las dice todo el mundo todo el
/// rato. Que dos tramos empiecen los dos por «por» o por «que» no dice nada; que
/// empiecen los dos por «anda» o por «prueba», si.
///
/// Es la lista que separa una retractacion de una frase que sigue:
///
/// ```text
/// «anda a dormir, perdon, anda a preparar comida»   -> «anda» ancla    SI
/// «Vine por ti, perdon, por favor no te enojes»     -> «por» no ancla  NO
/// ```
static PALABRAS_VACIAS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        // preposiciones y determinantes van aparte, en sus propias listas
        "no", "si", "que", "y", "o", "pero", "ni", "mas", "menos", "muy", "ya", "tambien",
        "tampoco", "como", "cuando", "donde", "porque", "pues", "asi", "se", "le", "lo", "les",
        "me", "te", "nos",
        // COPULAS Y AUXILIARES. No son verbos con contenido: unen, no dicen. Que
        // dos tramos empiecen los dos por «es» pasa en media conversacion.
        //
        // Sin ellas, «No es caro, mas bien, es carisimo» salia «No es carisimo»
        // —el sentido INVERTIDO— porque «es» hacia de ancla. Es el mismo fallo
        // que tenia el motor de `reglas.rs` y por el que se desenchufo; al traer
        // sus marcadores volvio, y lo cazo el corpus de control.
        //
        // Los verbos CON contenido siguen anclando: «anda a dormir / anda a
        // preparar comida» corrige, que es de lo que se trata.
        "es", "era", "fue", "son", "eran", "fueron", "sera", "seria", "sea", "esta", "estaba",
        "estan", "estuvo", "hay", "habia", "ha", "han", "he", "hemos", "habra", "tiene", "tenia",
        "tengo", "puede", "podia",
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

/// Adverbios que señalan un DÍA sin nombrarlo. Ocupan la misma ranura que un
/// día de la semana —«nos vemos mañana» y «nos vemos el martes» dicen lo mismo—
/// y por eso comparten categoría con ellos.
///
/// Sin esto, la correccion que más gente intenta primero no funcionaba. Medido
/// el 31/07 con el dictado real de Winston: «que me ayudes con ABRAX mañana,
/// digo el martes» salía sin tocar, mientras «el lunes, digo el martes» sí se
/// corregía. Es la misma frase para cualquiera menos para el emparejador.
///
/// Solo las formas ADVERBIALES desnudas. «la mañana» (el rato del día) lleva
/// determinante, y el determinante es justo lo que las distingue.
static ADVERBIOS_DIA: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    // La «ñ» va literal: `fold_accent` pliega tildes pero NO la ñ, y hace bien
    // —es otra letra, no una n con adorno—. Escribir «manana» aqui dejaba la
    // lista muerta sin que nada fallara.
    //
    // «pasado» NO entra aunque exista «pasado mañana»: suelto es una palabra
    // corrientisima («el año pasado», «el mes pasado») y la daria por fecha.
    // «pasado mañana» se reconoce igual, por su segunda palabra.
    ["hoy", "mañana", "ayer", "anteayer", "anoche"]
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
    if DIAS.contains(token.clave.as_str()) || ADVERBIOS_DIA.contains(token.clave.as_str()) {
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

/// ¿Son paralelos dos segmentos? Igualdad de forma, con UNA excepción medida.
///
/// La regla general es exacta —misma preposición, mismo determinante, misma
/// categoría— y así debe seguir: es lo que impide pegar cosas que no se
/// corresponden.
///
/// La excepción son las FECHAS. En español el determinante de una fecha es
/// opcional y no cambia nada de lo que se dice: «mañana», «el martes», «este
/// jueves» ocupan la misma ranura. Exigir que coincida rompía justo la
/// corrección más natural («…mañana, digo el martes») mientras dejaba pasar la
/// menos frecuente («…el lunes, digo el martes»), y esa asimetría no la entiende
/// nadie. La preposición SÍ se sigue exigiendo: es la que separa «el martes» de
/// «hasta el martes».
fn son_paralelas(a: &Forma, b: &Forma) -> bool {
    if a.categoria != b.categoria || a.preposicion != b.preposicion {
        return false;
    }
    if matches!(a.categoria, Some(Categoria::DiaSemana)) {
        return true;
    }
    a.determinante == b.determinante
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

    // REGLA DE ORO: sin ancla no hay paralelo posible, y sin paralelo no se toca
    // nada. Aquí mueren «digo que sí» y «mejor dicho de otra manera».
    //
    // El ancla es una CATEGORÍA (fecha, mes, hora, número, nombre propio) o, en
    // su defecto, una PREPOSICIÓN. Las cinco categorías dejaban fuera la
    // corrección más natural que existe —«prueba de química, perdón, de
    // física»— porque «química» no es ninguna de ellas. Y ahí el paralelo está a
    // la vista: misma preposición y el mismo número de palabras detrás.
    //
    // Medido el 31/07 con dictados reales: se intentó cuatro veces seguidas con
    // «de X, perdón, de Y» y las cuatro salieron sin tocar.
    //
    // La preposición SOLA no basta —«Vine por ti, perdón, por favor no te
    // enojes» no es una corrección—, así que va con el candado del LARGO: los
    // dos segmentos deben medir lo mismo. Una retractación cambia una cosa por
    // otra del mismo tamaño; una frase que sigue, no.
    let ancla_preposicion = forma_r.categoria.is_none() && forma_r.preposicion.is_some();
    // TERCERA ANCLA: los dos tramos EMPIEZAN POR LA MISMA PALABRA, y esa palabra
    // tiene contenido. Es la señal que aparecia en todos los intentos reales y
    // que ninguna de las dos anclas anteriores veia:
    //
    //   «anda a dormir, perdon, anda a preparar comida mejor»
    //    ^^^^                   ^^^^
    //
    // Repetir la cabeza es como se retracta la gente al hablar: vuelve a
    // empezar la frase. Con una palabra VACIA no valdria —«por», «que», «no» las
    // dice cualquiera— y por eso la lista de exclusion es el candado.
    let cabeza_r = tokens.get(ini_r).map(|t| t.clave.clone()).filter(|c| {
        !c.is_empty()
            && !PALABRAS_VACIAS.contains(c.as_str())
            && !PREPOSICIONES.contains(c.as_str())
            && !DETERMINANTES.contains(c.as_str())
    });
    let ancla_cabeza = forma_r.categoria.is_none() && !ancla_preposicion && cabeza_r.is_some();
    if forma_r.categoria.is_none() && !ancla_preposicion && !ancla_cabeza {
        return None;
    }

    let piso = i.saturating_sub(VENTANA_ATRAS);
    let mut candidato = None;
    for p in (piso..i).rev() {
        let fin_c = fin_de_oracion(tokens, p, i);
        let forma_c = forma_de(tokens, p, fin_c);
        let casa = if ancla_cabeza {
            // Misma cabeza con contenido: el tramo candidato empieza por la
            // misma palabra que el recambio. No se exige el largo — quien se
            // retracta suele decir MAS la segunda vez.
            tokens.get(p).map(|t| &t.clave) == cabeza_r.as_ref()
        } else if ancla_preposicion {
            forma_c.categoria.is_none()
                && forma_c.preposicion == forma_r.preposicion
                && (fin_c - p).abs_diff(fin_r - ini_r) <= TOLERANCIA_LARGO
        } else {
            son_paralelas(&forma_c, &forma_r)
        };
        if casa {
            candidato = Some((p, fin_c));
            break;
        }
    }
    let (mut ini_c, fin_c) = candidato?;

    // En fechas el determinante ya no separa (ver `son_paralelas`), asi que la
    // busqueda hacia atras puede casar EMPEZANDO DESPUES del determinante del
    // candidato. Si eso pasa, el tramo a sustituir tiene que absorberlo, porque
    // el recambio trae el suyo o no trae ninguno. Los dos sentidos fallaban:
    //
    //   «el lunes, digo el martes»   ->  «el el martes»   (el recambio traia uno)
    //   «el jueves, digo mañana»     ->  «el mañana»      (el recambio no traia)
    //
    // Los dos los cazaron tests: dos que ya existian y uno escrito para esto.
    if matches!(forma_r.categoria, Some(Categoria::DiaSemana))
        && forma_de(tokens, ini_c, fin_c).determinante.is_none()
        && ini_c > 0
        && DETERMINANTES.contains(tokens[ini_c - 1].clave.as_str())
    {
        ini_c -= 1;
    }

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
    sustitucion: &[Vec<String>],
    sustitucion_elipsis: &[Vec<String>],
) -> Option<String> {
    let tokens = tokenizar(texto);
    let mut i = 0;
    while i < tokens.len() {
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
pub fn aplicar_autocorreccion(texto: &str, senales_sustitucion: &Option<Vec<String>>) -> String {
    let a_lista = |propias: &Option<Vec<String>>, defecto: &[&str]| -> Vec<Vec<String>> {
        match propias {
            Some(v) => compilar(v),
            None => compilar(&defecto.iter().map(|s| s.to_string()).collect::<Vec<_>>()),
        }
    };
    let sustitucion = a_lista(senales_sustitucion, SUSTITUCION_POR_DEFECTO);

    if sustitucion.is_empty() {
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
        match una_pasada(&actual, &sustitucion, &sustitucion_elipsis) {
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
        aplicar_autocorreccion(texto, &None)
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

    /// «perdón» sola YA es señal (31/07), y las disculpas siguen intactas: no
    /// por una lista de excepciones, sino porque no tienen paralelo hacia atrás.
    /// Ese es el control que permite que la señal exista.
    #[test]
    fn perdon_solo_es_senal_pero_no_toca_las_disculpas() {
        for texto in [
            "Perdón por la demora, ya voy.",
            "Te pido perdón por lo de ayer.",
            "Perdón, no te escuché.",
            // Las dos que importan: llevan una FECHA detrás de «perdón» y aun
            // así no se tocan, porque lo que falta está atrás.
            "Le pedí perdón a mi hermana el lunes.",
            "Perdón por llegar tarde el martes.",
        ] {
            assert_eq!(corrige(texto), texto, "tocó una disculpa: «{texto}»");
        }
    }

    /// El audio real de Winston del 31/07 a las 22:11, que fue el que dejó ver
    /// que la señal faltaba.
    #[test]
    fn el_audio_de_las_2211() {
        assert_eq!(
            corrige("me ayudas con ADAX mañana, perdón, el martes"),
            "me ayudas con ADAX el martes"
        );
        // Con punto entre medio, que es como lo transcribió el modelo.
        assert_eq!(
            corrige("me ayudas con ADAX mañana. Perdón, el martes mejor"),
            "me ayudas con ADAX el martes mejor."
        );
        assert_eq!(corrige("a las tres, perdón, a las cuatro"), "A las cuatro");
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
                aplicar_autocorreccion(texto, &vacio),
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
            aplicar_autocorreccion("Vamos el martes nel el miércoles", &propias),
            "Vamos el miércoles"
        );
        // …y las de fábrica dejan de aplicar, porque REEMPLAZAN, no se suman.
        let texto = "Vamos el martes, digo, el miércoles";
        assert_eq!(aplicar_autocorreccion(texto, &propias), texto);
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
        // La señal se compara por clave normalizada, así que ni las mayúsculas
        // ni las tildes que el dictado se come impiden reconocerla. (Se probaba
        // con «Déjalo», del nivel de borrado retirado el 31/07; ahora con una
        // señal de corrección, que es lo que queda.)
        assert_eq!(
            corrige("Nos vemos el lunes, PERDÓN, el martes."),
            "Nos vemos el martes."
        );
        assert_eq!(
            corrige("Nos vemos el lunes, perdon, el martes."),
            "Nos vemos el martes."
        );
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
        assert_eq!(aplicar_autocorreccion(t, &Some(vec![])), t);
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

    // ── Fechas: el determinante no debe separar lo que es lo mismo ──────────

    #[test]
    fn el_dictado_real_de_winston_del_31_07() {
        // Transcripcion literal de su prueba (21:55). Antes salia SIN TOCAR: el
        // emparejador no reconocia «mañana» como fecha, asi que la correccion
        // que cualquiera intenta primero no funcionaba.
        assert_eq!(
            corrige("quiero que me ayudes con ABRAX mañana, digo el martes"),
            "quiero que me ayudes con ABRAX el martes"
        );
    }

    #[test]
    fn los_adverbios_de_dia_valen_en_los_dos_sentidos() {
        assert_eq!(
            corrige("nos vemos hoy, digo el jueves"),
            "nos vemos el jueves"
        );
        assert_eq!(
            corrige("nos vemos el jueves, digo mañana"),
            "nos vemos mañana"
        );
        assert_eq!(corrige("lo dejamos ayer, digo hoy"), "lo dejamos hoy");
        // Y el caso que ya funcionaba sigue igual, sin determinante duplicado.
        assert_eq!(
            corrige("nos vemos el lunes, digo el martes"),
            "nos vemos el martes"
        );
    }

    #[test]
    fn la_fecha_se_sustituye_dejando_la_preposicion_en_pie() {
        // El paralelo puede empezar DESPUES de la preposicion, y debe: quien
        // dice «lo tengo hasta el lunes, digo el martes» quiere «hasta el
        // martes», no repetir la preposicion ni perderla. Comportamiento previo
        // a los adverbios de dia; se fija aqui para que no se rompa al tocarlo.
        assert_eq!(
            corrige("lo tengo hasta el lunes, digo el martes"),
            "lo tengo hasta el martes"
        );
    }

    #[test]
    fn la_mañana_con_determinante_no_es_una_fecha() {
        // «la mañana» es el rato del dia, no el dia siguiente. El determinante es
        // justo lo que las distingue, y por eso solo entran las formas desnudas.
        let t = "trabajo por la mañana, digo por la tarde";
        assert_eq!(
            corrige(t),
            t,
            "confundio el rato del dia con una fecha: {t}"
        );
    }

    // ── Ancla por preposicion: la correccion mas natural que existe ─────────

    #[test]
    fn el_dictado_real_del_31_07_de_la_prueba_de_quimica() {
        // Winston lo intento cuatro veces seguidas y las cuatro salieron sin
        // tocar: «quimica» no es ninguna de las cinco categorias. El paralelo
        // estaba a la vista igual — misma preposicion, mismo largo.
        assert_eq!(
            corrige("Oye, Antonio, acuérdate que el martes tenemos prueba de química, perdón, de física."),
            "Oye, Antonio, acuérdate que el martes tenemos prueba de física."
        );
        assert_eq!(
            corrige("lo dejamos en la mesa, digo, en la silla"),
            "lo dejamos en la silla"
        );
    }

    #[test]
    fn el_candado_del_largo_es_lo_que_hace_segura_la_preposicion() {
        // Corpus de control. Sin el candado del largo, la primera se comeria
        // «Vine por ti» y las demas destrozarian prosa corriente.
        for texto in [
            "Vine por ti, perdón, por favor no te enojes",
            "gracias por todo, perdón, por cierto te queria contar algo",
            "hablamos de esto, perdón, de verdad no era mi intencion molestarte",
            "Perdón por la demora, ya voy",
            "te pido perdón por lo de ayer",
            "digo que sí a la propuesta",
        ] {
            assert_eq!(corrige(texto), texto, "tocó: «{texto}»");
        }
    }

    // ── Tercera ancla: la misma cabeza con contenido ───────────────────────

    #[test]
    fn los_dictados_reales_del_31_07_de_madrugada() {
        // Winston los probo uno tras otro y ninguno corregia. Los dos tramos
        // empiezan por la misma palabra —«anda»— y eso ES un paralelo; el
        // emparejador solo miraba categorias y preposiciones.
        assert_eq!(
            corrige("Oye, Antonio, anda a dormir. Perdón, anda a preparar comida mejor."),
            "Oye, Antonio, anda a preparar comida mejor."
        );
        assert_eq!(
            corrige(
                "Oye, Antonio, anda a dormir, no mejor anda a preparar comida que tengo hambre."
            ),
            "Oye, Antonio, anda a preparar comida que tengo hambre."
        );
    }

    #[test]
    fn la_lista_de_palabras_vacias_es_lo_que_hace_segura_la_cabeza() {
        // Corpus de control. Que dos tramos empiecen los dos por «por», «de» o
        // «no» no dice NADA —las dice cualquiera—, y sin esa lista de exclusion
        // la primera se comeria «Vine por ti».
        for texto in [
            "Vine por ti, perdón, por favor no te enojes",
            "gracias por todo, perdón, por cierto te queria contar algo",
            "hablamos de esto, perdón, de verdad no era mi intencion",
            "Te llamo mañana, perdón, no te escuché bien",
            "Perdón por la demora, ya voy",
            "te pido perdón por lo de ayer",
            "digo que sí a la propuesta",
            "mejor dicho de otra manera, no me convence",
            "perdón, ¿me repites?",
        ] {
            assert_eq!(corrige(texto), texto, "tocó: «{texto}»");
        }
    }

    // ── Marcadores traidos del motor de reglas (31/07) ─────────────────────

    #[test]
    fn los_marcadores_que_faltaban_ya_corrigen() {
        // «me equivoque» es la que mas se dice y NO estaba en la lista: por eso
        // en la maquina de Antonio «funcionaba bien» y aqui no. No era el
        // emparejador, era el vocabulario.
        assert_eq!(
            corrige("nos vemos el lunes, me equivoqué, el martes"),
            "nos vemos el martes"
        );
        assert_eq!(
            corrige("la prueba es de química, me equivoqué, de física"),
            "la prueba es de física"
        );
        assert_eq!(
            corrige("anda a dormir, me equivoqué, anda a preparar comida"),
            "Anda a preparar comida"
        );
        assert_eq!(
            corrige("a las tres, disculpa, a las cuatro"),
            "A las cuatro"
        );
        assert_eq!(corrige("el lunes, perdona, el martes"), "El martes");
        assert_eq!(
            corrige("vamos el jueves, quise decir, el viernes"),
            "vamos el viernes"
        );
        assert_eq!(corrige("son doce, quiero decir, trece"), "son trece");
        assert_eq!(corrige("el martes, mentira, el miércoles"), "El miércoles");
    }

    #[test]
    fn las_copulas_no_anclan_o_se_invierte_el_sentido() {
        // Al traer los marcadores volvio el fallo que hizo desenchufar el motor
        // de `reglas.rs`: «es» y «fue» hacian de ancla y la frase salia AL
        // REVES. Este corpus es el que lo caza.
        for texto in [
            "No es caro, más bien, es carísimo.",
            "No fue rápido, mejor dicho, fue lentísimo.",
            "Eso no es lo que quiero. Quiero decir que necesito otra cosa.",
            // Y las formas de disculpa de los marcadores nuevos.
            "disculpa la demora, ya voy",
            "perdona, no te escuché",
            "eso es mentira y lo sabes",
            "te pido perdón por lo de ayer",
            "Vine por ti, perdón, por favor no te enojes",
            "digo que sí a la propuesta",
        ] {
            assert_eq!(corrige(texto), texto, "tocó: «{texto}»");
        }
    }

    #[test]
    fn el_dictado_del_dentista_31_07() {
        // «al» era invisible como ancla —solo estaba en palabras vacias— y ademas
        // el recambio tiene una palabra mas que lo corregido. Dos motivos para
        // no hacer nada ante un paralelo evidente.
        assert_eq!(
            corrige("voy a llamar a Antonio para que vaya al dentista, perdón, al otorrino"),
            "voy a llamar a Antonio para que vaya al otorrino"
        );
        // Quien se corrige suele PRECISAR, y precisar alarga: una palabra de
        // diferencia tiene que caber.
        assert_eq!(
            corrige("voy a llamar a Antonio para que vaya al dentista, perdón, al otorrino ladrincólogo"),
            "voy a llamar a Antonio para que vaya al otorrino ladrincólogo"
        );
    }

    #[test]
    fn la_tolerancia_de_largo_no_deja_pasar_una_frase_que_sigue() {
        // Control de la tolerancia: una palabra si, tres no. Sin este limite,
        // la primera se comeria «Vine por ti».
        for texto in [
            "Vine por ti, perdón, por favor no te enojes",
            "gracias por todo, perdón, por cierto te queria contar algo",
            "hablamos de esto, perdón, de verdad no era mi intencion",
            "Te llamo mañana, perdón, no te escuché bien",
        ] {
            assert_eq!(corrige(texto), texto, "tocó: «{texto}»");
        }
    }
}
