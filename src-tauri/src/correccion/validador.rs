//! Validador de significado — la red de seguridad del modo con modelo: compara
//! la transcripción literal con la versión procesada y produce alertas cuando
//! el «arreglo» pudo cambiar lo que la persona dijo. Vacío = confiable.
//!
//! Todo es determinista y 100% local: nada de red, nada de estado, nada de
//! modelo. El caller decide qué hacer con las alertas (degradar al literal,
//! avisar, registrar); este módulo solo observa. Conservador en su propio
//! sentido: solo alerta ante señales verificables (multiconjuntos que no
//! cuadran, proporciones fuera de rango), jamás ante una sospecha estilística.
//!
//! Regla S3 del fork: el `detalle` de una alerta lleva **solo conteos,
//! posiciones y porcentajes** («2 negaciones → 1») — el contenido del dictado
//! jamás sale por aquí, ni a logs ni a eventos.
//!
//! Fuera de esta fase (honesto):
//! - **Persona gramatical**: «Voy a terminar el proyecto» → «Dile que
//!   terminaré el proyecto» NO dispara ninguna alerta. Detectarlo exige
//!   análisis morfológico de la conjugación española (terminar/terminaré/
//!   terminarás comparten raíz y solo la desinencia cambia la persona), y
//!   cualquier heurística de terminaciones (-é, -ás…) daría falsos positivos
//!   constantes (llegué/llegue, hablo/habló). Antes que una regla frágil,
//!   declaramos el hueco; hay un test-canario que lo documenta.
//! - **Años**: el contrato cubre `dd/mm` y «N de mes»; en `dd/mm/aaaa` el año
//!   se compara como número suelto (consistente en ambos lados).
//! - **«creó»/«creo»** comparten clave por el fold de acentos de
//!   [`build_match_key`]: un «creó» puede contar como marcador de certeza.
//!   Como se comparan totales, solo molesta si el modelo lo reescribe.
//! - **«1.500»** se lee como mil quinientos (grupo de 3 dígitos = separador de
//!   miles); `1,5` y `1.5` son el mismo valor, `15` y `50` no.

use crate::audio_toolkit::build_match_key;
use std::collections::{HashMap, HashSet};

/// Qué pudo alterar el modelo. Cada variante corresponde a una comprobación
/// determinista de [`validar`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TipoAlerta {
    NegacionAlterada,
    NumeroAlterado,
    FechaAlterada,
    UrlAlterada,
    CorreoAlterado,
    EntidadNueva,
    LongitudSospechosa,
    CertezaAlterada,
}

/// Una alerta del validador. `detalle` NUNCA contiene contenido del usuario:
/// solo conteos, posiciones y porcentajes (regla S3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alerta {
    pub tipo: TipoAlerta,
    pub detalle: String,
}

/// Palabras de negación (como claves de [`build_match_key`]): perder O agregar
/// una cambia el sentido de la frase entera.
const NEGACIONES: &[&str] = &["no", "nunca", "jamas", "tampoco", "ni"];

/// Marcadores de certeza/duda (secuencias de claves). Si el original duda y el
/// procesado afirma — o al revés — el modelo cambió el compromiso del hablante.
const CERTEZA: &[&[&str]] = &[
    &["tal", "vez"],
    &["puede", "ser"],
    &["a", "lo", "mejor"],
    &["quizas"],
    &["quiza"],
    &["creo"],
    &["ojala"],
];

/// Puntuación que puede envolver un token («(05/03)», «Marcela,», «¿Quizás»).
const PUNT_BORDES: &[char] = &[
    '.', ',', ';', ':', '!', '?', '…', '(', ')', '[', ']', '{', '}', '«', '»', '"', '\'', '¿', '¡',
    '—',
];

/// Cierres que pueden colgar tras el punto final de una oración («app.»»).
const CIERRES: &[char] = &['"', '\'', ')', ']', '»', '}'];

/// Fin de oración: solo estos habilitan mayúscula «legítima» en el siguiente
/// token (mismo criterio que `reglas::capitalizar_oraciones`).
const FIN_ORACION: &[char] = &['.', '!', '?', '…'];

/// Piso de la comprobación de longitud: con originales de menos de 30 chars
/// las proporciones no significan nada («sí» → «de acuerdo» es 500%). El piso
/// NO ampara al procesado vacío — borrar el dictado entero siempre alerta.
const PISO_LONGITUD: usize = 30;

/// Compara la versión literal con la procesada. Devuelve las alertas de todas
/// las comprobaciones que no cuadren; vacío significa que el procesado es
/// confiable. No loguea nada — eso es del caller (y sin contenido, regla S3).
pub fn validar(original: &str, procesado: &str) -> Vec<Alerta> {
    let mut alertas = Vec::new();
    if original == procesado {
        return alertas;
    }

    let toks_o: Vec<&str> = original.split_whitespace().collect();
    let toks_p: Vec<&str> = procesado.split_whitespace().collect();
    let claves_o: Vec<String> = toks_o.iter().map(|t| build_match_key(t)).collect();
    let claves_p: Vec<String> = toks_p.iter().map(|t| build_match_key(t)).collect();

    // 1. Negaciones: multiconjunto exacto por palabra.
    let neg_o = contar_negaciones(&claves_o);
    let neg_p = contar_negaciones(&claves_p);
    if neg_o != neg_p {
        let (a, b) = (suma(&neg_o), suma(&neg_p));
        alertas.push(Alerta {
            tipo: TipoAlerta::NegacionAlterada,
            detalle: detalle_conteo("negaciones", a, b),
        });
    }

    // 2 y 3. Números y fechas se extraen juntos: los dígitos que forman parte
    // de una fecha no cuentan como números, para que reformatear «05/03» como
    // «5 de marzo» no dispare NumeroAlterado.
    let (fechas_o, nums_o) = extraer_fechas_y_numeros(&toks_o, &claves_o);
    let (fechas_p, nums_p) = extraer_fechas_y_numeros(&toks_p, &claves_p);
    if nums_o != nums_p {
        let (a, b) = (suma(&nums_o), suma(&nums_p));
        alertas.push(Alerta {
            tipo: TipoAlerta::NumeroAlterado,
            detalle: detalle_conteo("números", a, b),
        });
    }
    if fechas_o != fechas_p {
        alertas.push(Alerta {
            tipo: TipoAlerta::FechaAlterada,
            detalle: detalle_conteo("fechas", fechas_o.len(), fechas_p.len()),
        });
    }

    // 4. URLs y correos: conjuntos exactos.
    let (urls_o, correos_o) = extraer_urls_y_correos(&toks_o);
    let (urls_p, correos_p) = extraer_urls_y_correos(&toks_p);
    if urls_o != urls_p {
        alertas.push(Alerta {
            tipo: TipoAlerta::UrlAlterada,
            detalle: detalle_conteo("URLs", urls_o.len(), urls_p.len()),
        });
    }
    if correos_o != correos_p {
        alertas.push(Alerta {
            tipo: TipoAlerta::CorreoAlterado,
            detalle: detalle_conteo("correos", correos_o.len(), correos_p.len()),
        });
    }

    // 5. Entidades nuevas: palabra capitalizada del procesado, fuera de inicio
    // de oración, cuya clave no existe en el original.
    let nuevas = entidades_nuevas(&toks_p, &claves_o);
    if nuevas > 0 {
        let detalle = if nuevas == 1 {
            "1 palabra capitalizada sin antecedente en el original".to_string()
        } else {
            format!("{nuevas} palabras capitalizadas sin antecedente en el original")
        };
        alertas.push(Alerta {
            tipo: TipoAlerta::EntidadNueva,
            detalle,
        });
    }

    // 6. Longitud: proporción fuera de [40%, 250%], con piso. El piso NO
    // ampara al procesado vacío (o solo espacios) con original con contenido:
    // ningún «arreglo» legítimo borra el dictado entero, y la promesa del
    // módulo es no dejar nunca al usuario sin texto.
    let chars_o = original.chars().count();
    let chars_p = procesado.chars().count();
    let vaciado = procesado.trim().is_empty() && !original.trim().is_empty();
    let fuera_de_rango =
        chars_o >= PISO_LONGITUD && (chars_p * 100 < chars_o * 40 || chars_p * 100 > chars_o * 250);
    if vaciado || fuera_de_rango {
        let pct = chars_p * 100 / chars_o.max(1);
        alertas.push(Alerta {
            tipo: TipoAlerta::LongitudSospechosa,
            detalle: format!("{chars_o} chars → {chars_p} ({pct}%)"),
        });
    }

    // 7. Certeza: total de marcadores de duda. Cambiar «quizás» por «tal vez»
    // conserva la duda y no alerta; perderla (o inventarla) sí.
    let cert_o = contar_certeza(&claves_o);
    let cert_p = contar_certeza(&claves_p);
    if cert_o != cert_p {
        alertas.push(Alerta {
            tipo: TipoAlerta::CertezaAlterada,
            detalle: detalle_conteo("marcadores de certeza", cert_o, cert_p),
        });
    }

    alertas
}

// ── helpers ──────────────────────────────────────────────────────────────────

/// Detalle sin contenido (regla S3): «2 negaciones → 1», o «mismo total (2) de
/// negaciones pero no coinciden» cuando cambió el contenido y no el conteo.
fn detalle_conteo(nombre: &str, a: usize, b: usize) -> String {
    if a != b {
        format!("{a} {nombre} → {b}")
    } else {
        format!("mismo total ({a}) de {nombre} pero no coinciden")
    }
}

fn suma<K>(conteo: &HashMap<K, usize>) -> usize {
    conteo.values().sum()
}

/// Núcleo de un token sin la puntuación que lo envuelve.
fn nucleo(token: &str) -> &str {
    token.trim_matches(PUNT_BORDES)
}

/// Multiconjunto de negaciones por palabra normalizada.
fn contar_negaciones(claves: &[String]) -> HashMap<&'static str, usize> {
    let mut conteo = HashMap::new();
    for clave in claves {
        if let Some(&neg) = NEGACIONES.iter().find(|&&n| n == clave.as_str()) {
            *conteo.entry(neg).or_insert(0usize) += 1;
        }
    }
    conteo
}

/// Total de marcadores de certeza (los multi-token cuentan como uno).
fn contar_certeza(claves: &[String]) -> usize {
    let mut total = 0;
    let mut i = 0;
    while i < claves.len() {
        let marcador = CERTEZA.iter().find(|marcador| {
            marcador.len() <= claves.len() - i
                && marcador
                    .iter()
                    .zip(&claves[i..])
                    .all(|(esperado, clave)| *esperado == clave.as_str())
        });
        match marcador {
            Some(m) => {
                total += 1;
                i += m.len();
            }
            None => i += 1,
        }
    }
    total
}

/// «05» → «5» (para que «05/03» y «5 de marzo» compartan clave). «00» → «0».
fn sin_ceros(s: &str) -> &str {
    let sin = s.trim_start_matches('0');
    if sin.is_empty() {
        "0"
    } else {
        sin
    }
}

/// ¿Núcleo de token que puede ser el día de «N de mes»? (1–2 dígitos ASCII).
fn es_dia(core: &str) -> bool {
    !core.is_empty() && core.len() <= 2 && core.bytes().all(|c| c.is_ascii_digit())
}

fn numero_de_mes(clave: &str) -> Option<u32> {
    let mes = match clave {
        "enero" => 1,
        "febrero" => 2,
        "marzo" => 3,
        "abril" => 4,
        "mayo" => 5,
        "junio" => 6,
        "julio" => 7,
        "agosto" => 8,
        "septiembre" | "setiembre" => 9,
        "octubre" => 10,
        "noviembre" => 11,
        "diciembre" => 12,
        _ => return None,
    };
    Some(mes)
}

/// Si el núcleo empieza con `dd/mm` (1–2 dígitos cada uno, seguido de fin de
/// token o de otro `/` como en `dd/mm/aaaa`), devuelve la clave normalizada
/// («5/3») y el resto del núcleo (p. ej. «/2026», cuyo año contará como
/// número suelto — igual en ambos lados).
fn fecha_ddmm(core: &str) -> Option<(String, &str)> {
    let b = core.as_bytes();
    let n1 = b.iter().take_while(|c| c.is_ascii_digit()).count();
    if n1 == 0 || n1 > 2 || b.get(n1) != Some(&b'/') {
        return None;
    }
    let n2 = b[n1 + 1..]
        .iter()
        .take_while(|c| c.is_ascii_digit())
        .count();
    if n2 == 0 || n2 > 2 {
        return None;
    }
    let fin = n1 + 1 + n2;
    match b.get(fin) {
        None | Some(&b'/') => {}
        _ => return None,
    }
    let clave = format!(
        "{}/{}",
        sin_ceros(&core[..n1]),
        sin_ceros(&core[n1 + 1..fin])
    );
    Some((clave, &core[fin..]))
}

/// Fechas (conjunto de claves «d/m») y números (multiconjunto de valores
/// normalizados) en una sola pasada por los tokens. Los dígitos consumidos por
/// una fecha no vuelven a contarse como números.
fn extraer_fechas_y_numeros(
    tokens: &[&str],
    claves: &[String],
) -> (HashSet<String>, HashMap<String, usize>) {
    let mut fechas = HashSet::new();
    let mut numeros: HashMap<String, usize> = HashMap::new();
    let mut i = 0;
    while i < tokens.len() {
        let core = nucleo(tokens[i]);

        // «5 de marzo» → fecha 5/3, y el «5» no cuenta como número.
        if es_dia(core) && claves.get(i + 1).map(String::as_str) == Some("de") {
            if let Some(mes) = claves.get(i + 2).and_then(|k| numero_de_mes(k)) {
                fechas.insert(format!("{}/{}", sin_ceros(core), mes));
                i += 3;
                continue;
            }
        }

        // «05/03» → fecha 5/3; en «05/03/2026» el año sigue como número.
        if let Some((clave, resto)) = fecha_ddmm(core) {
            fechas.insert(clave);
            extraer_numeros_en(resto, &mut numeros);
            i += 1;
            continue;
        }

        extraer_numeros_en(core, &mut numeros);
        i += 1;
    }
    (fechas, numeros)
}

/// Suma al multiconjunto las secuencias de dígitos de `s`, normalizadas.
fn extraer_numeros_en(s: &str, numeros: &mut HashMap<String, usize>) {
    let cs: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < cs.len() {
        if !cs[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        let mut grupos: Vec<String> = Vec::new();
        let mut seps: Vec<char> = Vec::new();
        loop {
            let mut grupo = String::new();
            while i < cs.len() && cs[i].is_ascii_digit() {
                grupo.push(cs[i]);
                i += 1;
            }
            grupos.push(grupo);
            let es_sep =
                i + 1 < cs.len() && (cs[i] == '.' || cs[i] == ',') && cs[i + 1].is_ascii_digit();
            if es_sep {
                seps.push(cs[i]);
                i += 1;
            } else {
                break;
            }
        }
        for clave in normalizar_numero(&grupos, &seps) {
            *numeros.entry(clave).or_insert(0) += 1;
        }
    }
}

/// Normaliza un número con separadores a su valor: quita separadores de miles
/// (grupos de exactamente 3 dígitos) y canoniza el decimal a punto, de modo
/// que «1,5» ≡ «1.5» y «1.500» ≡ «1,500» ≡ mil quinientos. Secuencias que no
/// encajan («0.1.0») se devuelven como números sueltos, igual en ambos lados.
fn normalizar_numero(grupos: &[String], seps: &[char]) -> Vec<String> {
    let Some((primero, resto)) = grupos.split_first() else {
        return Vec::new();
    };
    if seps.is_empty() {
        return vec![primero.clone()];
    }
    let hay_punto = seps.contains(&'.');
    let hay_coma = seps.contains(&',');
    if hay_punto && hay_coma {
        // «1.234,56»: el último separador es el decimal, el resto son miles.
        if let Some((dec, enteros)) = grupos.split_last() {
            return vec![format!("{}.{}", enteros.concat(), dec)];
        }
    } else if seps.len() == 1 {
        if let Some(segundo) = resto.first() {
            if segundo.len() == 3 {
                return vec![format!("{primero}{segundo}")]; // 1.500 → 1500
            }
            return vec![format!("{primero}.{segundo}")]; // 1,5 → 1.5
        }
    } else if resto.iter().all(|g| g.len() == 3) {
        return vec![grupos.concat()]; // 1.234.567 → 1234567
    } else {
        return grupos.to_vec(); // 0.1.0 → [0, 1, 0]
    }
    grupos.to_vec()
}

/// URLs (exactas) y correos (local sensible a mayúsculas, dominio no).
fn extraer_urls_y_correos(tokens: &[&str]) -> (HashSet<String>, HashSet<String>) {
    let mut urls = HashSet::new();
    let mut correos = HashSet::new();
    for token in tokens {
        let core = nucleo(token);
        if core.is_empty() {
            continue;
        }
        let minus = core.to_lowercase();
        if minus.starts_with("http://")
            || minus.starts_with("https://")
            || minus.starts_with("www.")
        {
            urls.insert(core.to_string());
            continue;
        }
        if let Some(arroba) = core.find('@') {
            let local = &core[..arroba];
            let dominio = &core[arroba + 1..];
            if !local.is_empty() && dominio.contains('.') && !dominio.contains('@') {
                correos.insert(format!("{}@{}", local, dominio.to_lowercase()));
            }
        }
    }
    (urls, correos)
}

/// Cuenta palabras capitalizadas del procesado (fuera de inicio de oración,
/// mínimo 2 letras, ni URL ni correo) cuya clave no aparece en ningún token
/// del original. El fold de acentos juega a favor: «Él» no es entidad nueva
/// si el original decía «el».
fn entidades_nuevas(toks_p: &[&str], claves_o: &[String]) -> usize {
    let conocidas: HashSet<&str> = claves_o
        .iter()
        .map(String::as_str)
        .filter(|k| !k.is_empty())
        .collect();
    let mut nuevas = 0;
    for (i, token) in toks_p.iter().enumerate() {
        let core = nucleo(token);
        let Some(primera) = core.chars().next() else {
            continue;
        };
        if !(primera.is_alphabetic() && primera.is_uppercase()) || core.chars().count() < 2 {
            continue;
        }
        if core.contains('@') || core.contains("://") {
            continue;
        }
        let inicio_de_oracion = i == 0
            || toks_p[i - 1]
                .trim_end_matches(CIERRES)
                .ends_with(FIN_ORACION);
        if inicio_de_oracion {
            continue;
        }
        let clave = build_match_key(core);
        if !clave.is_empty() && !conocidas.contains(clave.as_str()) {
            nuevas += 1;
        }
    }
    nuevas
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tipos(alertas: &[Alerta]) -> Vec<TipoAlerta> {
        alertas.iter().map(|a| a.tipo).collect()
    }

    // ── ediciones inocuas: nada dispara ───────────────────────────────────

    #[test]
    fn texto_identico_no_dispara() {
        let t = "No subo nada hasta el 05/03; escribe a ana@abrax.app o visita https://abrax.app";
        assert!(validar(t, t).is_empty());
    }

    #[test]
    fn mayuscula_inicial_y_espacios_no_disparan() {
        assert!(validar("hola , mundo", "Hola, mundo").is_empty());
    }

    #[test]
    fn quitar_una_muletilla_no_dispara() {
        assert!(validar(
            "bueno, este, no subo los cambios hasta las 15:30 de hoy",
            "No subo los cambios hasta las 15:30 de hoy."
        )
        .is_empty());
    }

    #[test]
    fn reformatear_la_fecha_no_dispara() {
        // «05/03» y «5 de marzo» comparten clave: cambiar el formato no es
        // cambiar la fecha, y el día tampoco reaparece como número suelto.
        assert!(validar(
            "nos vemos el 05/03 en la oficina",
            "nos vemos el 5 de marzo en la oficina"
        )
        .is_empty());
    }

    #[test]
    fn separadores_de_miles_y_decimales_no_disparan() {
        assert!(validar("cuesta 1.500 pesos en total", "cuesta 1,500 pesos en total").is_empty());
        assert!(validar("mide 1,5 metros de alto", "mide 1.5 metros de alto").is_empty());
    }

    #[test]
    fn textos_cortos_no_disparan_longitud() {
        // 2 chars → 20 chars sería 1000%, pero el piso de 30 chars protege.
        assert!(validar("sí", "de acuerdo, lo vemos").is_empty());
    }

    #[test]
    fn procesado_vacio_dispara_aunque_el_original_sea_corto() {
        // El piso ampara expansiones legítimas, no el borrado total: sin este
        // caso el validador bendeciría dejar al usuario sin texto — justo lo
        // que el módulo promete que nunca pasa.
        let alertas = validar("hola amigo", "");
        assert_eq!(tipos(&alertas), vec![TipoAlerta::LongitudSospechosa]);
        // Solo espacios cuenta como vacío; la negación perdida también salta.
        let alertas = validar("no vengo", "   ");
        assert_eq!(
            tipos(&alertas),
            vec![TipoAlerta::NegacionAlterada, TipoAlerta::LongitudSospechosa]
        );
    }

    // ── ejemplos del diseño: deben fallar la validación ───────────────────

    #[test]
    fn negacion_perdida_dispara() {
        let alertas = validar("No puedo ir el martes", "Puedo ir el martes");
        assert_eq!(tipos(&alertas), vec![TipoAlerta::NegacionAlterada]);
    }

    #[test]
    fn certeza_perdida_dispara() {
        let alertas = validar("Quizás llegue a las ocho", "Llegaré a las ocho");
        assert_eq!(tipos(&alertas), vec![TipoAlerta::CertezaAlterada]);
    }

    #[test]
    fn numero_alterado_dispara() {
        let alertas = validar("Cuesta 15 dólares", "Cuesta 50 dólares");
        assert_eq!(tipos(&alertas), vec![TipoAlerta::NumeroAlterado]);
    }

    /// Honesto: el cambio de persona gramatical NO lo caza ninguna regla de
    /// esta fase — ver el encabezado del módulo (exigiría morfología de la
    /// conjugación; una heurística de terminaciones sería frágil). «Dile»
    /// tampoco cuenta como entidad porque abre la oración. Este test es un
    /// canario: si algún día algo lo detecta, que falle y obligue a revisar.
    #[test]
    fn cambio_de_persona_queda_fuera_de_esta_fase() {
        assert!(validar(
            "Voy a terminar el proyecto",
            "Dile que terminaré el proyecto"
        )
        .is_empty());
    }

    // ── resto de comprobaciones ───────────────────────────────────────────

    #[test]
    fn negacion_agregada_tambien_dispara() {
        let alertas = validar(
            "puedo ir el martes a la oficina",
            "no puedo ir el martes a la oficina",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::NegacionAlterada]);
    }

    #[test]
    fn detalle_de_negaciones_solo_lleva_conteos() {
        let alertas = validar(
            "no puedo hoy y no puedo mañana",
            "no puedo hoy y puedo mañana",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::NegacionAlterada]);
        assert_eq!(alertas[0].detalle, "2 negaciones → 1");
        // Regla S3: ni una palabra del dictado en el detalle.
        assert!(!alertas[0].detalle.contains("puedo"));
        assert!(!alertas[0].detalle.contains("mañana"));
    }

    #[test]
    fn fecha_alterada_dispara() {
        let alertas = validar(
            "la reunión queda para el 5 de marzo",
            "la reunión queda para el 5 de mayo",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::FechaAlterada]);
    }

    #[test]
    fn url_alterada_dispara() {
        let alertas = validar(
            "descarga el modelo desde https://abrax.app/modelos ahora",
            "descarga el modelo desde https://abrax.app/descargas ahora",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::UrlAlterada]);
    }

    #[test]
    fn correo_distingue_local_pero_no_dominio() {
        // El dominio es insensible a mayúsculas: esto es inocuo.
        assert!(validar(
            "manda el reporte a Ana.Perez@Abrax.App por favor",
            "manda el reporte a Ana.Perez@abrax.app por favor"
        )
        .is_empty());
        // La parte local NO lo es: cambiarla dispara.
        let alertas = validar(
            "manda el reporte a Ana.Perez@Abrax.App por favor",
            "manda el reporte a ana.perez@abrax.app por favor",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::CorreoAlterado]);
    }

    #[test]
    fn entidad_nueva_dispara() {
        let alertas = validar(
            "voy a mandar el informe mañana temprano",
            "voy a mandarle el informe a Marcela mañana temprano",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::EntidadNueva]);
        // Regla S3 y concordancia: conteo en singular, jamás el nombre.
        assert_eq!(
            alertas[0].detalle,
            "1 palabra capitalizada sin antecedente en el original"
        );
    }

    #[test]
    fn longitud_sospechosa_dispara() {
        let alertas = validar(
            "el informe completo del proyecto queda listo mañana por la tarde",
            "listo",
        );
        assert_eq!(tipos(&alertas), vec![TipoAlerta::LongitudSospechosa]);
    }

    #[test]
    fn certeza_agregada_tambien_dispara() {
        let alertas = validar("llego a las ocho", "creo que llego a las ocho");
        assert_eq!(tipos(&alertas), vec![TipoAlerta::CertezaAlterada]);
    }

    #[test]
    fn certeza_multitoken_tambien_cuenta() {
        let alertas = validar("tal vez vaya al cine", "voy al cine");
        assert_eq!(tipos(&alertas), vec![TipoAlerta::CertezaAlterada]);
    }
}
