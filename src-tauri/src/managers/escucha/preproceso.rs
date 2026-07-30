//! [ESCUCHA] Preprocesador de lectura: convierte markdown/código en una cola
//! de oraciones hablables. Este módulo es el corazón de Escucha — el motor TTS
//! es intercambiable; entender qué se lee y cómo se pronuncia vive aquí.
//!
//! Módulo puro (sin Tauri, sin TTS): recibe texto, devuelve
//! `Vec<OracionHablable>`. La prosa va cruda al TTS (la prosodia es del motor);
//! el código se verbaliza símbolo a símbolo en es-419 con una tabla
//! configurable, y los identificadores camelCase/snake_case se parten para que
//! la voz los pronuncie ("useAuthStore" → "use Auth Store").

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::{Deserialize, Serialize};
use specta::Type;

/// Con qué voz debe leerse un trozo: la de prosa (natural) o la técnica.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum VozTrozo {
    Prosa,
    Codigo,
}

/// Una unidad de lectura: el panel habla `texto_hablable` con la voz `voz` y
/// resalta las líneas `linea_inicio..=linea_fin` (1-based) del documento.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct OracionHablable {
    pub texto_hablable: String,
    pub voz: VozTrozo,
    pub linea_inicio: u32,
    pub linea_fin: u32,
}

/// Cuánto símbolo se pronuncia al leer código. `Natural` calla los cierres de
/// paréntesis/llaves/corchetes (menos ruido al oído); `Literal` pronuncia
/// "abre"/"cierra" en cada uno (fidelidad total, útil para dictar de vuelta).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum VerbosidadSimbolos {
    #[default]
    Natural,
    Literal,
}

/// Cómo interpretar el contenido a leer.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum ModoLectura {
    /// Segmentación markdown: prosa/encabezados con voz natural, bloques de
    /// código con voz técnica.
    Markdown,
    /// Todo el contenido es código: se lee línea a línea verbalizado.
    Codigo,
    /// Heurística: si el texto "parece código" se lee como código; si no,
    /// como markdown (que degrada bien a prosa plana).
    Auto,
}

/// Tabla de verbalización de símbolos es-419. Los multi-carácter se prueban
/// primero (más largos ganan). Configurable: Fase 2 la expone en settings.
pub struct TablaSimbolos {
    pub multi: Vec<(&'static str, &'static str)>,
    pub simples: Vec<(char, &'static str)>,
}

impl TablaSimbolos {
    pub fn es_419() -> Self {
        Self {
            // Orden: más largos primero — "&&" debe ganar antes que "&".
            multi: vec![
                ("&&", "y y"),
                ("||", "o o"),
                ("=>", "flecha"),
                ("->", "flecha"),
                ("==", "igual igual"),
                ("!=", "distinto"),
                (">=", "mayor o igual"),
                ("<=", "menor o igual"),
            ],
            simples: vec![
                ('/', "slash"),
                ('_', "guión bajo"),
                ('-', "guión"),
                ('"', "comillas"),
                ('.', "punto"),
                ('#', "gato"),
                ('$', "peso"),
                ('=', "igual"),
                (':', "dos puntos"),
                (';', "punto y coma"),
                (',', "coma"),
                ('+', "más"),
                ('*', "asterisco"),
                ('<', "menor que"),
                ('>', "mayor que"),
                ('&', "ampersand"),
                ('|', "pipe"),
                ('!', "exclamación"),
                ('?', "interrogación"),
                ('@', "arroba"),
                ('%', "por ciento"),
                ('\\', "backslash"),
                ('`', "backtick"),
                ('\'', "comilla simple"),
                ('~', "tilde"),
                ('^', "caret"),
            ],
        }
    }

    fn palabra_simple(&self, c: char) -> Option<&'static str> {
        self.simples
            .iter()
            .find(|(sym, _)| *sym == c)
            .map(|(_, palabra)| *palabra)
    }
}

/// Paréntesis/llaves/corchetes: (palabra base, es apertura).
fn bracket_de(c: char) -> Option<(&'static str, bool)> {
    match c {
        '(' => Some(("paréntesis", true)),
        ')' => Some(("paréntesis", false)),
        '{' => Some(("llaves", true)),
        '}' => Some(("llaves", false)),
        '[' => Some(("corchetes", true)),
        ']' => Some(("corchetes", false)),
        _ => None,
    }
}

fn digito_es(c: char) -> &'static str {
    match c {
        '0' => "cero",
        '1' => "uno",
        '2' => "dos",
        '3' => "tres",
        '4' => "cuatro",
        '5' => "cinco",
        '6' => "seis",
        '7' => "siete",
        '8' => "ocho",
        '9' => "nueve",
        _ => "",
    }
}

/// Parte un run alfanumérico en sub-palabras pronunciables: límites camelCase
/// ("useAuthStore" → use/Auth/Store), siglas seguidas de palabra ("HTTPServer"
/// → HTTP/Server) y fronteras letra↔dígito ("v0" → v/0).
fn partir_alfanumerico(run: &str) -> Vec<String> {
    let chars: Vec<char> = run.chars().collect();
    let mut tokens = Vec::new();
    let mut actual = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if !actual.is_empty() {
            let prev = chars[i - 1];
            let limite = (prev.is_lowercase() && c.is_uppercase())
                || (prev.is_alphabetic() && c.is_ascii_digit())
                || (prev.is_ascii_digit() && c.is_alphabetic())
                || (prev.is_uppercase()
                    && c.is_uppercase()
                    && chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
            if limite {
                tokens.push(std::mem::take(&mut actual));
            }
        }
        actual.push(c);
    }
    if !actual.is_empty() {
        tokens.push(actual);
    }
    tokens
}

/// Verbaliza un identificador: separa camelCase/siglas/dígitos y pronuncia los
/// dígitos sueltos en español ("v0" → "v cero"). Los runs de varios dígitos se
/// dejan tal cual — el TTS ya lee "404" bien en español.
pub fn verbalizar_identificador(run: &str) -> String {
    partir_alfanumerico(run)
        .into_iter()
        .map(|t| {
            let mut chars = t.chars();
            match (chars.next(), chars.next()) {
                (Some(c), None) if c.is_ascii_digit() => digito_es(c).to_string(),
                _ => t,
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Verbaliza una línea de código en texto hablable es-419.
pub fn verbalizar_codigo(
    linea: &str,
    tabla: &TablaSimbolos,
    verbosidad: VerbosidadSimbolos,
) -> String {
    let chars: Vec<char> = linea.chars().collect();
    let mut palabras: Vec<String> = Vec::new();
    let mut i = 0;
    'exterior: while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        for (simbolo, palabra) in &tabla.multi {
            let sym: Vec<char> = simbolo.chars().collect();
            if chars[i..].starts_with(&sym) {
                palabras.push((*palabra).to_string());
                i += sym.len();
                continue 'exterior;
            }
        }
        if c.is_alphanumeric() {
            let mut j = i;
            while j < chars.len() && chars[j].is_alphanumeric() {
                j += 1;
            }
            let run: String = chars[i..j].iter().collect();
            palabras.push(verbalizar_identificador(&run));
            i = j;
            continue;
        }
        if let Some((palabra, abre)) = bracket_de(c) {
            match verbosidad {
                VerbosidadSimbolos::Literal => {
                    let prefijo = if abre { "abre" } else { "cierra" };
                    palabras.push(format!("{prefijo} {palabra}"));
                }
                VerbosidadSimbolos::Natural => {
                    if abre {
                        palabras.push(palabra.to_string());
                    }
                    // Los cierres se callan: el oído no los necesita.
                }
            }
            i += 1;
            continue;
        }
        if let Some(palabra) = tabla.palabra_simple(c) {
            palabras.push(palabra.to_string());
        }
        // Símbolo desconocido: se omite en silencio (mejor que deletrear ruido).
        i += 1;
    }
    palabras.join(" ")
}

/// Divide prosa en oraciones: corta tras `.?!…` seguido de espacio o fin.
/// "v0.1.0" no corta (el punto va seguido de dígito). Devuelve rangos de bytes.
fn partir_oraciones(texto: &str) -> Vec<(usize, usize)> {
    let mut cortes = Vec::new();
    let mut inicio = 0usize;
    let mut iter = texto.char_indices().peekable();
    while let Some((i, c)) = iter.next() {
        if matches!(c, '.' | '?' | '!' | '…') {
            let fin = i + c.len_utf8();
            let siguiente = iter.peek().map(|(_, n)| *n);
            if siguiente.is_none() || siguiente.is_some_and(|n| n.is_whitespace()) {
                cortes.push((inicio, fin));
                inicio = fin;
            }
        }
    }
    if inicio < texto.len() {
        cortes.push((inicio, texto.len()));
    }
    cortes
}

/// Tabla de inicios de línea para convertir offsets de byte → línea 1-based.
struct Lineas {
    inicios: Vec<usize>,
}

impl Lineas {
    fn new(fuente: &str) -> Self {
        let mut inicios = vec![0usize];
        for (i, b) in fuente.bytes().enumerate() {
            if b == b'\n' {
                inicios.push(i + 1);
            }
        }
        Self { inicios }
    }

    fn linea_de(&self, offset: usize) -> u32 {
        match self.inicios.binary_search(&offset) {
            Ok(idx) => idx as u32 + 1,
            Err(idx) => idx as u32,
        }
    }
}

/// Buffer de prosa en construcción: texto acumulado + puntos de control
/// (offset de byte en el buffer → línea del documento) para asignarle línea a
/// cada oración resultante.
#[derive(Default)]
struct BufferProsa {
    texto: String,
    checkpoints: Vec<(usize, u32)>,
}

impl BufferProsa {
    fn push(&mut self, trozo: &str, linea: u32) {
        self.checkpoints.push((self.texto.len(), linea));
        self.texto.push_str(trozo);
    }

    fn linea_en(&self, pos: usize) -> u32 {
        self.checkpoints
            .iter()
            .rev()
            .find(|(idx, _)| *idx <= pos)
            .map(|(_, linea)| *linea)
            .unwrap_or(1)
    }

    fn descargar(&mut self, salida: &mut Vec<OracionHablable>) {
        if self.texto.trim().is_empty() {
            self.texto.clear();
            self.checkpoints.clear();
            return;
        }
        for (inicio, fin) in partir_oraciones(&self.texto) {
            let crudo = &self.texto[inicio..fin];
            let oracion = crudo.trim();
            if oracion.is_empty() {
                continue;
            }
            let inicio_real = inicio + (crudo.len() - crudo.trim_start().len());
            let fin_real = inicio_real + oracion.len() - 1;
            salida.push(OracionHablable {
                texto_hablable: oracion.to_string(),
                voz: VozTrozo::Prosa,
                linea_inicio: self.linea_en(inicio_real),
                linea_fin: self.linea_en(fin_real),
            });
        }
        self.texto.clear();
        self.checkpoints.clear();
    }
}

/// Segmenta markdown en la cola de oraciones hablables.
pub fn preprocesar_markdown(fuente: &str, verbosidad: VerbosidadSimbolos) -> Vec<OracionHablable> {
    let tabla = TablaSimbolos::es_419();
    let lineas = Lineas::new(fuente);
    let mut salida = Vec::new();

    let mut prosa = BufferProsa::default();
    // Encabezado en construcción: (nivel, texto).
    let mut encabezado: Option<(u32, String)> = None;
    // Bloque de código en construcción: (texto, línea del primer Text).
    let mut codigo: Option<(String, u32)> = None;

    let opciones = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH;
    for (evento, rango) in Parser::new_ext(fuente, opciones).into_offset_iter() {
        let linea = lineas.linea_de(rango.start);
        match evento {
            Event::Start(Tag::Heading { level, .. }) => {
                prosa.descargar(&mut salida);
                encabezado = Some((nivel_de(level), String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((nivel, texto)) = encabezado.take() {
                    let texto = texto.trim();
                    if !texto.is_empty() {
                        let prefijo = match nivel {
                            1 => "Sección",
                            2 => "Subsección",
                            _ => "Apartado",
                        };
                        // El encabezado termina en la línea donde cierra el tag.
                        let linea_fin = lineas.linea_de(rango.end.saturating_sub(1));
                        salida.push(OracionHablable {
                            texto_hablable: format!("{prefijo}: {texto}"),
                            voz: VozTrozo::Prosa,
                            linea_inicio: lineas.linea_de(rango.start),
                            linea_fin,
                        });
                    }
                }
            }
            Event::Start(Tag::CodeBlock(_)) => {
                prosa.descargar(&mut salida);
                // La línea real del código la fija el primer Text del bloque
                // (rango.start apunta a la línea del ```).
                codigo = Some((String::new(), 0));
            }
            Event::End(TagEnd::CodeBlock) => {
                if let Some((texto, primera_linea)) = codigo.take() {
                    for (idx, linea_codigo) in texto.lines().enumerate() {
                        if linea_codigo.trim().is_empty() {
                            continue;
                        }
                        let hablado = verbalizar_codigo(linea_codigo, &tabla, verbosidad);
                        if hablado.is_empty() {
                            continue;
                        }
                        let n = primera_linea.max(1) + idx as u32;
                        salida.push(OracionHablable {
                            texto_hablable: hablado,
                            voz: VozTrozo::Codigo,
                            linea_inicio: n,
                            linea_fin: n,
                        });
                    }
                }
            }
            Event::End(TagEnd::Paragraph) | Event::End(TagEnd::Item) => {
                prosa.descargar(&mut salida);
            }
            Event::Text(t) => {
                if let Some((_, texto)) = encabezado.as_mut() {
                    texto.push_str(&t);
                } else if let Some((texto, primera_linea)) = codigo.as_mut() {
                    if *primera_linea == 0 {
                        *primera_linea = linea;
                    }
                    texto.push_str(&t);
                } else {
                    prosa.push(&t, linea);
                }
            }
            Event::Code(t) => {
                // Código inline dentro de prosa/encabezado: se verbaliza pero
                // sigue formando parte de la oración (misma voz de prosa).
                let hablado = verbalizar_codigo(&t, &tabla, verbosidad);
                if let Some((_, texto)) = encabezado.as_mut() {
                    texto.push_str(&hablado);
                } else {
                    prosa.push(&hablado, linea);
                }
            }
            Event::SoftBreak | Event::HardBreak => {
                if let Some((_, texto)) = encabezado.as_mut() {
                    texto.push(' ');
                } else if codigo.is_none() {
                    prosa.push(" ", linea);
                }
            }
            _ => {}
        }
    }
    prosa.descargar(&mut salida);
    salida
}

/// Lee un archivo de código: cada línea no vacía es una oración con voz técnica.
pub fn preprocesar_codigo_plano(
    fuente: &str,
    verbosidad: VerbosidadSimbolos,
) -> Vec<OracionHablable> {
    let tabla = TablaSimbolos::es_419();
    fuente
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .filter_map(|(idx, l)| {
            let hablado = verbalizar_codigo(l, &tabla, verbosidad);
            if hablado.is_empty() {
                return None;
            }
            let n = idx as u32 + 1;
            Some(OracionHablable {
                texto_hablable: hablado,
                voz: VozTrozo::Codigo,
                linea_inicio: n,
                linea_fin: n,
            })
        })
        .collect()
}

/// Heurística para "Leer portapapeles": ¿esto parece código?
pub fn parece_codigo(texto: &str) -> bool {
    let lineas: Vec<&str> = texto.lines().filter(|l| !l.trim().is_empty()).collect();
    if lineas.is_empty() {
        return false;
    }
    let con_pinta = lineas
        .iter()
        .filter(|l| {
            let t = l.trim();
            t.ends_with('{')
                || t.ends_with('}')
                || t.ends_with(';')
                || t.contains("=>")
                || t.contains("::")
                || t.contains("()")
                || t.contains(" = ")
                || t.starts_with("import ")
                || t.starts_with("use ")
                || t.starts_with("fn ")
                || t.starts_with("def ")
                || t.starts_with("const ")
                || t.starts_with("let ")
                || t.starts_with("$ ")
                || t.starts_with("//")
        })
        .count();
    con_pinta * 100 / lineas.len() >= 35
}

/// Deshace el corte de líneas de un texto copiado, para que se lea de corrido.
///
/// # El problema
///
/// Un PDF no guarda párrafos: guarda líneas colocadas en la página. Al copiar,
/// cada línea VISUAL llega con su propio salto, y el motor de voz lee cada salto
/// como fin de frase — una pausa a media oración, cada seis palabras. Medido el
/// 30/07 leyendo una ley en PDF. Word no sufre esto porque copia el párrafo
/// entero en una línea, y por eso ahí sonaba bien.
///
/// # Qué hace
///
/// Une las líneas que están cortadas a media frase y **respeta** las que son
/// cortes de verdad:
///
/// ```text
/// «…planificación urbana, urbanización\ny construcción…»  → una sola frase
/// «…dicte el Presidente de la República,\nregirán en…»    → una sola frase
/// «Fin del artículo.\nArtículo 2°.-»                      → dos, se conserva
/// «construc-\nción»                                        → «construcción»
/// «\n\n»  (línea en blanco)                                → párrafo, se conserva
/// «• primero\n• segundo»                                   → lista, se conserva
/// ```
///
/// Es conservador por diseño: ante la duda **conserva** el salto. Unir dos frases
/// que no debían unirse suena peor que una pausa de más.
pub fn desenvolver_lineas(texto: &str) -> String {
    /// ¿La línea termina una oración de verdad?
    fn cierra_oracion(l: &str) -> bool {
        l.chars()
            .rev()
            .find(|c| !c.is_whitespace())
            .is_some_and(|c| matches!(c, '.' | '!' | '?' | ':' | ';' | '»' | '"' | '…' | '”'))
    }

    /// ¿La línea empieza algo que NO debe pegarse a la anterior? Viñetas,
    /// numeración, encabezados markdown y artículos de ley.
    fn empieza_bloque(l: &str) -> bool {
        let t = l.trim_start();
        if t.is_empty() {
            return true;
        }
        let primera = t.chars().next().unwrap_or(' ');
        if matches!(primera, '•' | '-' | '*' | '#' | '>' | '|' | '–' | '—') {
            return true;
        }
        // «1.» «1)» «a)» «IV.» — numeración de lista al abrir línea.
        //
        // Las MINÚSCULAS cuentan, y no es un detalle: «a) primero» es un marcador
        // tan válido como «1.», y dejarlas fuera se comía el salto de las listas
        // alfabéticas (lo cazó el test `respeta_las_listas`). Pero una minúscula
        // solo cuenta con PARÉNTESIS: «a.» podría ser el final de una frase, y
        // unir dos oraciones suena peor que una pausa de más.
        let cabeza: String = t.chars().take(6).collect();
        if let Some(p) = cabeza.split(['.', ')']).next() {
            let corto = !p.is_empty() && p.chars().count() <= 4;
            let digitos_o_mayusculas = p
                .chars()
                .all(|c| c.is_ascii_digit() || (c.is_alphabetic() && c.is_uppercase()));
            let letra_con_parentesis = p.chars().count() == 1
                && p.chars().next().is_some_and(|c| c.is_alphabetic())
                && cabeza[p.len()..].starts_with(')');
            if corto
                && cabeza.contains(['.', ')'])
                && (digitos_o_mayusculas || letra_con_parentesis)
            {
                return true;
            }
        }
        false
    }

    let normalizado = texto.replace("\r\n", "\n").replace('\r', "\n");
    let lineas: Vec<&str> = normalizado.split('\n').collect();
    let mut salida = String::with_capacity(normalizado.len());

    for (i, linea) in lineas.iter().enumerate() {
        let actual = linea.trim_end();
        let ultima = i + 1 == lineas.len();

        // Guion de partición de palabra: «construc-» + «ción» → «construcción».
        // Solo si detrás hay letra y la línea siguiente abre en minúscula; un
        // guion de lista o un compuesto («norte-» / «Sur») no se toca.
        let siguiente = lineas.get(i + 1).map(|s| s.trim_start()).unwrap_or("");
        let parte_palabra = actual.ends_with('-')
            && actual
                .chars()
                .rev()
                .nth(1)
                .is_some_and(|c| c.is_alphabetic())
            && siguiente
                .chars()
                .next()
                .is_some_and(|c| c.is_lowercase() && c.is_alphabetic());

        if parte_palabra {
            salida.push_str(actual.trim_end_matches('-'));
            continue; // sin espacio ni salto: la palabra sigue
        }

        salida.push_str(actual);

        if ultima {
            continue;
        }

        // ¿Salto de verdad o corte de página?
        let conservar = actual.trim().is_empty()
            || cierra_oracion(actual)
            || empieza_bloque(siguiente)
            || siguiente.is_empty();

        if conservar {
            salida.push('\n');
        } else if !actual.is_empty() {
            salida.push(' ');
        }
    }

    salida
}

/// Punto de entrada del preprocesador.
pub fn preprocesar(
    contenido: &str,
    modo: ModoLectura,
    verbosidad: VerbosidadSimbolos,
) -> Vec<OracionHablable> {
    match modo {
        ModoLectura::Markdown => preprocesar_markdown(contenido, verbosidad),
        ModoLectura::Codigo => preprocesar_codigo_plano(contenido, verbosidad),
        ModoLectura::Auto => {
            if parece_codigo(contenido) {
                preprocesar_codigo_plano(contenido, verbosidad)
            } else {
                preprocesar_markdown(contenido, verbosidad)
            }
        }
    }
}

fn nivel_de(level: HeadingLevel) -> u32 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

#[cfg(test)]
mod tests_desenvolver {
    use super::desenvolver_lineas;

    /// EL CASO REPORTADO, con el texto exacto de la captura: una ley en PDF donde
    /// cada línea visual traía su salto y el motor pausaba a media frase.
    #[test]
    fn el_pdf_de_la_ley_se_lee_de_corrido() {
        let pdf = concat!(
            "Artículo 1°.-   Las disposiciones de la presente ley, relativas a ",
            "planificación urbana, urbanización\n",
            "y construcción, y las de la Ordenanza que sobre la materia dicte el ",
            "Presidente de la República,\n",
            "regirán en todo el territorio nacional."
        );
        let r = desenvolver_lineas(pdf);
        assert!(!r.contains('\n'), "quedó un salto a media frase: {r}");
        assert!(r.contains("urbanización y construcción"));
        assert!(r.contains("República, regirán"));
    }

    /// Lo que NO debe unir: dos oraciones distintas.
    #[test]
    fn respeta_el_fin_de_oracion() {
        let r = desenvolver_lineas("Fin del artículo.\nArtículo 2°.- Empieza otro.");
        assert_eq!(r, "Fin del artículo.\nArtículo 2°.- Empieza otro.");
    }

    #[test]
    fn respeta_los_parrafos() {
        let r = desenvolver_lineas("Primer párrafo cortado\naquí.\n\nSegundo párrafo.");
        assert_eq!(r, "Primer párrafo cortado aquí.\n\nSegundo párrafo.");
    }

    #[test]
    fn respeta_las_listas() {
        for lista in [
            "Los requisitos son\n• primero\n• segundo",
            "Los requisitos son\n- primero\n- segundo",
            "Los requisitos son\n1. primero\n2. segundo",
            "Los requisitos son\na) primero\nb) segundo",
        ] {
            let r = desenvolver_lineas(lista);
            assert_eq!(
                r.lines().count(),
                3,
                "se comió un salto de lista en «{lista}» -> «{r}»"
            );
        }
    }

    /// Los PDF parten palabras con guion al final de línea.
    #[test]
    fn une_las_palabras_partidas_por_guion() {
        assert_eq!(
            desenvolver_lineas("la construc-\nción de la obra"),
            "la construcción de la obra"
        );
    }

    /// Pero un guion que NO parte palabra se respeta: viñeta, o compuesto con
    /// mayúscula detrás.
    #[test]
    fn el_guion_que_no_parte_palabra_se_respeta() {
        let r = desenvolver_lineas("eje norte-\nSur del terreno");
        assert!(r.contains("norte-"), "se comió el guion: {r}");
        let lista = desenvolver_lineas("Requisitos\n- uno\n- dos");
        assert_eq!(lista.lines().count(), 3);
    }

    #[test]
    fn el_texto_de_word_no_se_toca() {
        // Word copia el párrafo entero en una línea: no hay nada que desenvolver.
        let word = "Las disposiciones de la presente ley regirán en todo el territorio nacional.";
        assert_eq!(desenvolver_lineas(word), word);
    }

    #[test]
    fn casos_degenerados() {
        assert_eq!(desenvolver_lineas(""), "");
        assert_eq!(desenvolver_lineas("una línea"), "una línea");
        assert_eq!(desenvolver_lineas("\n\n\n"), "\n\n\n");
        // CRLF de Windows.
        assert_eq!(desenvolver_lineas("cortado\r\naquí"), "cortado aquí");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn natural(linea: &str) -> String {
        verbalizar_codigo(linea, &TablaSimbolos::es_419(), VerbosidadSimbolos::Natural)
    }

    #[test]
    fn camel_case_se_parte() {
        assert_eq!(verbalizar_identificador("useAuthStore"), "use Auth Store");
    }

    #[test]
    fn siglas_seguidas_de_palabra() {
        assert_eq!(verbalizar_identificador("HTTPServer"), "HTTP Server");
    }

    #[test]
    fn rama_git_kebab() {
        assert_eq!(
            natural("hotfix/token-expiry"),
            "hotfix slash token guión expiry"
        );
    }

    #[test]
    fn version_con_digitos() {
        assert_eq!(natural("v0.1.0"), "v cero punto uno punto cero");
    }

    #[test]
    fn digitos_multiples_se_conservan() {
        assert_eq!(natural("error404"), "error 404");
    }

    #[test]
    fn operadores_logicos_y_flechas() {
        assert_eq!(natural("a && b || c"), "a y y b o o c");
        assert_eq!(natural("x => y == z"), "x flecha y igual igual z");
    }

    #[test]
    fn comando_de_la_demo() {
        assert_eq!(
            natural("bun install && bun run dev"),
            "bun install y y bun run dev"
        );
    }

    #[test]
    fn snake_case_pronuncia_guion_bajo() {
        assert_eq!(natural("user_id"), "user guión bajo id");
    }

    #[test]
    fn verbosidad_natural_calla_cierres() {
        assert_eq!(natural("f(x)"), "f paréntesis x");
    }

    #[test]
    fn verbosidad_literal_pronuncia_todo() {
        let literal = verbalizar_codigo(
            "f(x)",
            &TablaSimbolos::es_419(),
            VerbosidadSimbolos::Literal,
        );
        assert_eq!(literal, "f abre paréntesis x cierra paréntesis");
    }

    #[test]
    fn oraciones_no_cortan_versiones() {
        let cortes = partir_oraciones("Instala v0.1.0 hoy. Es rápido.");
        assert_eq!(cortes.len(), 2);
    }

    #[test]
    fn markdown_mixto_completo() {
        let doc = "# Instalación\n\nAbrax es dictado local. Corre en tu equipo.\n\n```bash\nbun install && bun run dev\n```\n\nFin de la guía.\n";
        let oraciones = preprocesar_markdown(doc, VerbosidadSimbolos::Natural);

        let textos: Vec<&str> = oraciones
            .iter()
            .map(|o| o.texto_hablable.as_str())
            .collect();
        assert_eq!(
            textos,
            vec![
                "Sección: Instalación",
                "Abrax es dictado local.",
                "Corre en tu equipo.",
                "bun install y y bun run dev",
                "Fin de la guía.",
            ]
        );

        let voces: Vec<VozTrozo> = oraciones.iter().map(|o| o.voz).collect();
        assert_eq!(
            voces,
            vec![
                VozTrozo::Prosa,
                VozTrozo::Prosa,
                VozTrozo::Prosa,
                VozTrozo::Codigo,
                VozTrozo::Prosa,
            ]
        );

        // Líneas 1-based: el encabezado en la 1, la prosa en la 3, el código
        // dentro del fence en la 6, el cierre en la 9.
        assert_eq!(oraciones[0].linea_inicio, 1);
        assert_eq!(oraciones[1].linea_inicio, 3);
        assert_eq!(oraciones[3].linea_inicio, 6);
        assert_eq!(oraciones[3].linea_fin, 6);
        assert_eq!(oraciones[4].linea_inicio, 9);
    }

    #[test]
    fn codigo_inline_se_verbaliza_dentro_de_la_prosa() {
        let doc = "Usa `useAuthStore` para el estado.\n";
        let oraciones = preprocesar_markdown(doc, VerbosidadSimbolos::Natural);
        assert_eq!(oraciones.len(), 1);
        assert_eq!(oraciones[0].voz, VozTrozo::Prosa);
        assert_eq!(
            oraciones[0].texto_hablable,
            "Usa use Auth Store para el estado."
        );
    }

    #[test]
    fn archivo_de_codigo_linea_a_linea() {
        let fuente = "let x = 1;\n\nconst y = x && 2;\n";
        let oraciones = preprocesar_codigo_plano(fuente, VerbosidadSimbolos::Natural);
        assert_eq!(oraciones.len(), 2);
        assert_eq!(oraciones[0].texto_hablable, "let x igual uno punto y coma");
        assert_eq!(oraciones[0].linea_inicio, 1);
        assert_eq!(oraciones[1].linea_inicio, 3);
    }

    #[test]
    fn heuristica_de_portapapeles() {
        assert!(parece_codigo(
            "const a = 1;\nfunction foo() {\n  return a;\n}"
        ));
        assert!(!parece_codigo(
            "Hola equipo, mañana revisamos el informe de avance y cerramos el plan."
        ));
    }

    #[test]
    fn lista_markdown_lee_cada_item() {
        let doc = "- Primero instala.\n- Luego corre.\n";
        let oraciones = preprocesar_markdown(doc, VerbosidadSimbolos::Natural);
        assert_eq!(oraciones.len(), 2);
        assert_eq!(oraciones[0].texto_hablable, "Primero instala.");
        assert_eq!(oraciones[1].linea_inicio, 2);
    }
}
