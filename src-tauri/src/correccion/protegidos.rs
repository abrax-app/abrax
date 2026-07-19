//! Términos protegidos: placeholders restaurables alrededor de la llamada al
//! modelo local. Antes de enviar el texto al fraseador, [`proteger`] sustituye
//! los fragmentos que un modelo pequeño más daña — negaciones, números,
//! fechas, horas, URLs y correos — por marcadores opacos (`__NEG_1__`,
//! `__NUM_1__`…); al volver la respuesta, [`restaurar`] los reemplaza por el
//! texto original byte a byte. Este módulo se usa SOLO alrededor del modelo:
//! las reglas deterministas ([`super::reglas`]) no pasan por aquí.
//!
//! Filosofía conservadora, como todo el módulo: si CUALQUIER marcador se
//! perdió, se duplicó o el modelo inventó uno con el mismo formato,
//! [`restaurar`] devuelve `None` y el caller hace fallback al texto previo.
//! La promesa de nunca dejar al usuario sin texto se sostiene aquí con un
//! `Option`, no con heurísticas de reparación.
//!
//! Pasadas de detección en orden de prioridad — un tramo ya reclamado no se
//! re-captura: URLs → correos → fechas → horas → números → negaciones. Así el
//! «240» de `https://abrax.app/v1/240` viaja dentro del marcador de URL y
//! jamás genera un `__NUM_n__` propio.
//!
//! **Fuera de esta fase, a propósito: nombres propios.** Detectarlos por
//! mayúscula inicial produce demasiados falsos positivos en dictado es-419
//! (toda palabra que abre oración calificaría). Quedará para cuando exista
//! una señal mejor que la ortografía.
//!
//! Limitaciones asumidas: los signos pegados a una cifra (`-15`, `15%`)
//! quedan fuera del marcador — solo se protege la cifra; y si el dictado
//! original ya contiene un literal con formato `__X_n__`, [`restaurar`] lo
//! tratará como marcador desconocido y abortará — degradación segura, nunca
//! pérdida de texto.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;

/// Qué clase de fragmento protege un marcador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoProtegido {
    Negacion,
    Numero,
    Fecha,
    Hora,
    Url,
    Correo,
}

impl TipoProtegido {
    /// Prefijo del marcador: `__NEG_1__`, `__NUM_1__`, `__FECHA_1__`…
    fn prefijo(self) -> &'static str {
        match self {
            TipoProtegido::Negacion => "NEG",
            TipoProtegido::Numero => "NUM",
            TipoProtegido::Fecha => "FECHA",
            TipoProtegido::Hora => "HORA",
            TipoProtegido::Url => "URL",
            TipoProtegido::Correo => "CORREO",
        }
    }

    /// Posición fija (0..6) para llevar un contador por tipo.
    fn indice(self) -> usize {
        match self {
            TipoProtegido::Negacion => 0,
            TipoProtegido::Numero => 1,
            TipoProtegido::Fecha => 2,
            TipoProtegido::Hora => 3,
            TipoProtegido::Url => 4,
            TipoProtegido::Correo => 5,
        }
    }
}

/// Un fragmento sustituido: el marcador que lo representa en el texto que ve
/// el modelo, y el original exacto con el que se restaura.
#[derive(Debug, Clone, PartialEq)]
pub struct TerminoProtegido {
    pub marcador: String,
    pub original: String,
    pub tipo: TipoProtegido,
}

/// Resultado de [`proteger`]: el texto con marcadores y la lista de términos
/// necesaria para deshacerlos con [`restaurar`].
#[derive(Debug, Clone, PartialEq)]
pub struct TextoProtegido {
    pub texto: String,
    pub terminos: Vec<TerminoProtegido>,
}

/// Compila un patrón literal. `None` degrada: ese detector simplemente no
/// dispara (y sin el patrón de marcadores, [`restaurar`] devuelve `None` y el
/// caller hace fallback). El test `todos_los_patrones_compilan` garantiza que
/// en la práctica nunca pasa.
fn compilar(patron: &str) -> Option<Regex> {
    Regex::new(patron).ok()
}

static RE_URL: LazyLock<Option<Regex>> =
    LazyLock::new(|| compilar(r"(?i)\b(?:https?://|www\.)\S+"));
static RE_CORREO: LazyLock<Option<Regex>> =
    LazyLock::new(|| compilar(r"\b[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}\b"));
static RE_FECHA_BARRAS: LazyLock<Option<Regex>> =
    LazyLock::new(|| compilar(r"\b\d{1,2}/\d{1,2}(?:/\d{4})?\b"));
static RE_FECHA_TEXTO: LazyLock<Option<Regex>> = LazyLock::new(|| {
    compilar(
        r"\b(\d{1,2}) de (?:enero|febrero|marzo|abril|mayo|junio|julio|agosto|septiembre|setiembre|octubre|noviembre|diciembre)\b",
    )
});
static RE_HORA: LazyLock<Option<Regex>> =
    LazyLock::new(|| compilar(r"\b(?:[01]?\d|2[0-3]):[0-5]\d\b"));
static RE_NUMERO: LazyLock<Option<Regex>> = LazyLock::new(|| compilar(r"\b\d+(?:[.,]\d+)*\b"));
static RE_NEGACION: LazyLock<Option<Regex>> =
    LazyLock::new(|| compilar(r"(?i)\b(?:no|nunca|jam[aá]s|tampoco|ni)\b"));
/// Formato general de marcador — sirve tanto para los propios como para
/// detectar que el modelo inventó uno (`__FOO_9__`).
static RE_MARCADOR: LazyLock<Option<Regex>> = LazyLock::new(|| compilar(r"__[A-Z]+_\d+__"));

/// Puntuación de cierre que puede quedar pegada al final de una URL dictada
/// («visita abrax.app.») — no forma parte de la URL y no se protege.
const PUNT_CIERRE: &[char] = &['.', ',', ';', ':', '!', '?', ')', ']', '…', '»', '"', '\''];

/// Un tramo detectado sobre el texto ORIGINAL (offsets en bytes).
struct Deteccion {
    inicio: usize,
    fin: usize,
    tipo: TipoProtegido,
}

fn solapa(dets: &[Deteccion], inicio: usize, fin: usize) -> bool {
    dets.iter().any(|d| inicio < d.fin && d.inicio < fin)
}

/// Agrega la detección solo si no pisa un tramo ya reclamado por una pasada
/// anterior — así un número dentro de una URL o fecha nunca se re-captura.
fn reclamar(dets: &mut Vec<Deteccion>, inicio: usize, fin: usize, tipo: TipoProtegido) {
    if inicio < fin && !solapa(dets, inicio, fin) {
        dets.push(Deteccion { inicio, fin, tipo });
    }
}

/// Validación aproximada de `dd/mm(/aaaa)`: día 1-31, mes 1-12. Un «45/99» no
/// es fecha — sus cifras quedarán para la pasada de números.
fn fecha_barras_valida(s: &str) -> bool {
    let mut partes = s.split('/');
    let (Some(dia), Some(mes)) = (partes.next(), partes.next()) else {
        return false;
    };
    matches!(dia.parse::<u32>(), Ok(d) if (1..=31).contains(&d))
        && matches!(mes.parse::<u32>(), Ok(m) if (1..=12).contains(&m))
}

/// Sustituye los fragmentos frágiles por marcadores `__TIPO_n__` (contador
/// por tipo, desde 1, en orden de aparición). Sin nada que proteger devuelve
/// el texto igual y cero términos. Nunca falla: ante cualquier duda, el
/// fragmento se queda como está y viaja al modelo sin proteger.
pub fn proteger(texto: &str) -> TextoProtegido {
    let mut dets: Vec<Deteccion> = Vec::new();

    // 1) URLs — recortando puntuación de cierre pegada.
    if let Some(re) = RE_URL.as_ref() {
        for m in re.find_iter(texto) {
            let recortado = m.as_str().trim_end_matches(PUNT_CIERRE);
            reclamar(
                &mut dets,
                m.start(),
                m.start() + recortado.len(),
                TipoProtegido::Url,
            );
        }
    }

    // 2) Correos.
    if let Some(re) = RE_CORREO.as_ref() {
        for m in re.find_iter(texto) {
            reclamar(&mut dets, m.start(), m.end(), TipoProtegido::Correo);
        }
    }

    // 3) Fechas dd/mm y dd/mm/aaaa, con validación aproximada de rangos.
    if let Some(re) = RE_FECHA_BARRAS.as_ref() {
        for m in re.find_iter(texto) {
            if fecha_barras_valida(m.as_str()) {
                reclamar(&mut dets, m.start(), m.end(), TipoProtegido::Fecha);
            }
        }
    }

    // 4) Fechas «N de <mes>» (meses en minúscula, como los emite el ASR).
    if let Some(re) = RE_FECHA_TEXTO.as_ref() {
        for c in re.captures_iter(texto) {
            let (Some(total), Some(dia)) = (c.get(0), c.get(1)) else {
                continue;
            };
            if matches!(dia.as_str().parse::<u32>(), Ok(d) if (1..=31).contains(&d)) {
                reclamar(&mut dets, total.start(), total.end(), TipoProtegido::Fecha);
            }
        }
    }

    // 5) Horas hh:mm — el rango 0-23:0-59 (aproximado) va en el propio patrón.
    if let Some(re) = RE_HORA.as_ref() {
        for m in re.find_iter(texto) {
            reclamar(&mut dets, m.start(), m.end(), TipoProtegido::Hora);
        }
    }

    // 6) Números: enteros, decimales y miles («15», «1,5», «1.240,50»). No se
    //    interpreta el formato — se protege el literal tal cual.
    if let Some(re) = RE_NUMERO.as_ref() {
        for m in re.find_iter(texto) {
            reclamar(&mut dets, m.start(), m.end(), TipoProtegido::Numero);
        }
    }

    // 7) Negaciones como palabras completas («ni» jamás dispara dentro de
    //    «nido» — el \b de ambos lados lo impide).
    if let Some(re) = RE_NEGACION.as_ref() {
        for m in re.find_iter(texto) {
            reclamar(&mut dets, m.start(), m.end(), TipoProtegido::Negacion);
        }
    }

    dets.sort_unstable_by_key(|d| d.inicio);

    let mut contadores = [0usize; 6];
    let mut out = String::with_capacity(texto.len());
    let mut terminos = Vec::with_capacity(dets.len());
    let mut ultimo = 0;
    for d in &dets {
        contadores[d.tipo.indice()] += 1;
        let marcador = format!("__{}_{}__", d.tipo.prefijo(), contadores[d.tipo.indice()]);
        out.push_str(&texto[ultimo..d.inicio]);
        out.push_str(&marcador);
        terminos.push(TerminoProtegido {
            marcador,
            original: texto[d.inicio..d.fin].to_string(),
            tipo: d.tipo,
        });
        ultimo = d.fin;
    }
    out.push_str(&texto[ultimo..]);

    TextoProtegido {
        texto: out,
        terminos,
    }
}

/// Deshace los marcadores en el texto que devolvió el modelo. Cada marcador
/// debe aparecer EXACTAMENTE una vez; si alguno falta, sobra, o aparece un
/// marcador con formato `__X_n__` que no emitimos, devuelve `None` y el
/// caller hace fallback al texto previo a la llamada. Al log van solo
/// conteos — jamás el contenido (regla S3).
pub fn restaurar(texto: &str, terminos: &[TerminoProtegido]) -> Option<String> {
    // Sin el patrón de marcadores no podemos garantizar nada: fallback.
    let re = RE_MARCADOR.as_ref()?;

    let mut mapa: HashMap<&str, &str> = HashMap::with_capacity(terminos.len());
    for t in terminos {
        if mapa
            .insert(t.marcador.as_str(), t.original.as_str())
            .is_some()
        {
            // Marcador repetido en la propia lista: estado inconsistente.
            log::debug!("correccion/protegidos: lista de términos con marcador repetido");
            return None;
        }
    }

    let mut conteo: HashMap<&str, usize> = mapa.keys().map(|k| (*k, 0usize)).collect();
    for m in re.find_iter(texto) {
        match conteo.get_mut(m.as_str()) {
            Some(c) => *c += 1,
            None => {
                log::debug!("correccion/protegidos: marcador desconocido, restauración abortada");
                return None;
            }
        }
    }
    if conteo.values().any(|&c| c != 1) {
        log::debug!(
            "correccion/protegidos: {} de {} marcadores no aparecen exactamente una vez",
            conteo.values().filter(|&&c| c != 1).count(),
            terminos.len()
        );
        return None;
    }

    let mut out = String::with_capacity(texto.len());
    let mut ultimo = 0;
    for m in re.find_iter(texto) {
        out.push_str(&texto[ultimo..m.start()]);
        if let Some(original) = mapa.get(m.as_str()) {
            out.push_str(original);
        }
        ultimo = m.end();
    }
    out.push_str(&texto[ultimo..]);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn todos_los_patrones_compilan() {
        let patrones: [(&str, &Option<Regex>); 8] = [
            ("url", &*RE_URL),
            ("correo", &*RE_CORREO),
            ("fecha_barras", &*RE_FECHA_BARRAS),
            ("fecha_texto", &*RE_FECHA_TEXTO),
            ("hora", &*RE_HORA),
            ("numero", &*RE_NUMERO),
            ("negacion", &*RE_NEGACION),
            ("marcador", &*RE_MARCADOR),
        ];
        for (nombre, re) in patrones {
            assert!(re.is_some(), "el patrón de {nombre} no compila");
        }
    }

    // ── ida y vuelta ──────────────────────────────────────────────────────

    #[test]
    fn ida_y_vuelta_es_identidad() {
        // Todos los tipos a la vez: si el modelo devuelve el texto sin tocar,
        // restaurar reconstruye el original byte a byte.
        let original = "no vayas el 12/07/2026 a las 15:30: escribe a soporte@abrax.app, \
                        visita https://abrax.app/precios y nunca pagues 1.240,50 antes del 5 de julio";
        let p = proteger(original);
        assert!(!p.terminos.is_empty());
        assert_eq!(restaurar(&p.texto, &p.terminos).as_deref(), Some(original));
    }

    #[test]
    fn texto_sin_nada_que_proteger_queda_igual() {
        let p = proteger("hola qué tal todo bien");
        assert!(p.terminos.is_empty());
        assert_eq!(p.texto, "hola qué tal todo bien");
        assert_eq!(
            restaurar(&p.texto, &p.terminos).as_deref(),
            Some("hola qué tal todo bien")
        );
    }

    // ── detección por tipo ────────────────────────────────────────────────

    #[test]
    fn detecta_negaciones_como_palabras_completas() {
        let p = proteger("no lo hagas nunca, tampoco mañana ni jamás");
        assert_eq!(p.terminos.len(), 5);
        assert!(p.terminos.iter().all(|t| t.tipo == TipoProtegido::Negacion));
        assert_eq!(p.terminos[0].marcador, "__NEG_1__");
        assert_eq!(p.terminos[4].marcador, "__NEG_5__");
        assert_eq!(p.terminos[4].original, "jamás");
    }

    #[test]
    fn ni_dentro_de_palabra_no_dispara() {
        let t = "el nido en la nieve parece un espejismo animado";
        let p = proteger(t);
        assert!(p.terminos.is_empty());
        assert_eq!(p.texto, t);
    }

    #[test]
    fn detecta_numeros_con_decimales_y_miles() {
        let p = proteger("cuesta 1.240,50 hoy y 15 mañana");
        assert_eq!(p.terminos.len(), 2);
        assert_eq!(p.terminos[0].marcador, "__NUM_1__");
        assert_eq!(p.terminos[0].original, "1.240,50");
        assert_eq!(p.terminos[1].marcador, "__NUM_2__");
        assert_eq!(p.terminos[1].original, "15");
        assert_eq!(p.texto, "cuesta __NUM_1__ hoy y __NUM_2__ mañana");
    }

    #[test]
    fn detecta_fechas_con_barras_y_en_texto() {
        let p = proteger("del 12/07 al 3/1/2026 y luego el 5 de julio");
        assert_eq!(p.terminos.len(), 3);
        assert!(p.terminos.iter().all(|t| t.tipo == TipoProtegido::Fecha));
        let originales: Vec<&str> = p.terminos.iter().map(|t| t.original.as_str()).collect();
        assert_eq!(originales, vec!["12/07", "3/1/2026", "5 de julio"]);
        assert_eq!(p.terminos[2].marcador, "__FECHA_3__");
    }

    #[test]
    fn fecha_invalida_cae_a_numeros() {
        // «45/99» no pasa la validación de rangos: quedan dos números sueltos
        // y la negación — conservador, pero nada se pierde.
        let p = proteger("la razón 45/99 no es fecha");
        let vista: Vec<(TipoProtegido, &str)> = p
            .terminos
            .iter()
            .map(|t| (t.tipo, t.original.as_str()))
            .collect();
        assert_eq!(
            vista,
            vec![
                (TipoProtegido::Numero, "45"),
                (TipoProtegido::Numero, "99"),
                (TipoProtegido::Negacion, "no"),
            ]
        );
    }

    #[test]
    fn detecta_horas_validas() {
        let p = proteger("de 9:05 a 23:59");
        assert_eq!(p.terminos.len(), 2);
        assert!(p.terminos.iter().all(|t| t.tipo == TipoProtegido::Hora));
        assert_eq!(p.terminos[0].original, "9:05");
        assert_eq!(p.terminos[1].original, "23:59");
        assert_eq!(p.texto, "de __HORA_1__ a __HORA_2__");
    }

    #[test]
    fn detecta_urls_sin_arrastrar_puntuacion() {
        let p = proteger("visita https://abrax.app/precios. luego www.abrax.app, gracias");
        assert_eq!(p.terminos.len(), 2);
        assert!(p.terminos.iter().all(|t| t.tipo == TipoProtegido::Url));
        assert_eq!(p.terminos[0].original, "https://abrax.app/precios");
        assert_eq!(p.terminos[1].original, "www.abrax.app");
        assert_eq!(p.texto, "visita __URL_1__. luego __URL_2__, gracias");
    }

    #[test]
    fn detecta_correos() {
        let p = proteger("escribe a soporte@abrax.app cuando puedas");
        assert_eq!(p.terminos.len(), 1);
        assert_eq!(p.terminos[0].tipo, TipoProtegido::Correo);
        assert_eq!(p.terminos[0].original, "soporte@abrax.app");
        assert_eq!(p.texto, "escribe a __CORREO_1__ cuando puedas");
    }

    #[test]
    fn numero_dentro_de_url_no_se_recaptura() {
        let p = proteger("mira https://abrax.app/v1/240 y paga 15");
        let urls: Vec<&str> = p
            .terminos
            .iter()
            .filter(|t| t.tipo == TipoProtegido::Url)
            .map(|t| t.original.as_str())
            .collect();
        let numeros: Vec<&str> = p
            .terminos
            .iter()
            .filter(|t| t.tipo == TipoProtegido::Numero)
            .map(|t| t.original.as_str())
            .collect();
        assert_eq!(urls, vec!["https://abrax.app/v1/240"]);
        assert_eq!(numeros, vec!["15"]);
        assert_eq!(p.texto, "mira __URL_1__ y paga __NUM_1__");
    }

    // ── restaurar ─────────────────────────────────────────────────────────

    #[test]
    fn restaurar_respeta_marcadores_reubicados() {
        // El modelo puede reordenar la frase: mientras cada marcador aparezca
        // exactamente una vez, la restauración funciona.
        let p = proteger("no vengas el 15");
        let movido = "el __NUM_1__ __NEG_1__ es para venir";
        assert_eq!(
            restaurar(movido, &p.terminos).as_deref(),
            Some("el 15 no es para venir")
        );
    }

    #[test]
    fn marcador_perdido_devuelve_none() {
        let p = proteger("no vengas mañana");
        assert_eq!(p.terminos.len(), 1);
        // El modelo se comió el marcador: fallback obligatorio.
        assert_eq!(restaurar("vengas mañana", &p.terminos), None);
    }

    #[test]
    fn marcador_duplicado_devuelve_none() {
        let p = proteger("no vengas");
        assert_eq!(restaurar("__NEG_1__ y __NEG_1__ vengas", &p.terminos), None);
    }

    #[test]
    fn marcador_desconocido_devuelve_none() {
        let p = proteger("no vengas");
        assert_eq!(restaurar("__NEG_1__ vengas __FOO_7__", &p.terminos), None);
    }
}
