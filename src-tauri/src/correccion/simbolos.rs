//! Normalización de símbolos dictados INEQUÍVOCOS: rinde como símbolo aquello
//! que, dictado, solo puede ser un símbolo. Conservador por diseño: solo
//! dispara con disparadores que no son habla normal y con un número a cada
//! lado. Deliberadamente NO toca «más», «menos», «por» ni «igual» sueltos —
//! son palabras corrientes («quiero más café», «gracias por todo», «me da
//! igual») y convertirlas rompería texto normal.
//!
//! Patrones soportados (N = dígitos o un número-palabra de un solo token):
//! - `N slash N`                    → `N/M`
//! - `N dividido|partido por|entre N` → `N/M`
//! - `N por ciento`                 → `N%`
//!
//! Los números de varias palabras («mil doscientos») quedan fuera a propósito:
//! convertir todo número a dígito es otra decisión de estilo, no de esta capa.

/// Número-palabra de un solo token → dígitos. Incluye variantes con y sin
/// tilde tal como las emite el ASR. `None` si el token no es un número simple.
fn palabra_a_digito(w: &str) -> Option<&'static str> {
    Some(match w {
        "cero" => "0",
        "uno" | "una" => "1",
        "dos" => "2",
        "tres" => "3",
        "cuatro" => "4",
        "cinco" => "5",
        "seis" => "6",
        "siete" => "7",
        "ocho" => "8",
        "nueve" => "9",
        "diez" => "10",
        "once" => "11",
        "doce" => "12",
        "trece" => "13",
        "catorce" => "14",
        "quince" => "15",
        "dieciséis" | "dieciseis" => "16",
        "diecisiete" => "17",
        "dieciocho" => "18",
        "diecinueve" => "19",
        "veinte" => "20",
        "veintiuno" => "21",
        "veintidós" | "veintidos" => "22",
        "veintitrés" | "veintitres" => "23",
        "veinticuatro" => "24",
        "veinticinco" => "25",
        "veintiséis" | "veintiseis" => "26",
        "veintisiete" => "27",
        "veintiocho" => "28",
        "veintinueve" => "29",
        "treinta" => "30",
        "cuarenta" => "40",
        "cincuenta" => "50",
        "sesenta" => "60",
        "setenta" => "70",
        "ochenta" => "80",
        "noventa" => "90",
        "cien" | "ciento" => "100",
        "mil" => "1000",
        _ => return None,
    })
}

/// Interpreta un token como número (dígitos puros o número-palabra) y devuelve
/// su forma en dígitos. Requiere el token LIMPIO (sin puntuación pegada).
fn como_digitos(tok: &str) -> Option<String> {
    if !tok.is_empty() && tok.bytes().all(|b| b.is_ascii_digit()) {
        return Some(tok.to_string());
    }
    palabra_a_digito(&tok.to_lowercase()).map(|d| d.to_string())
}

/// Rangos `(inicio, fin)` en bytes de cada token separado por whitespace.
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

const PUNT_FINAL: &[char] = &['.', ',', ';', ':', '!', '?', '…', ')', ']', '»', '"', '\''];

/// Intenta casar un patrón que empieza en el token `i`. Devuelve
/// `(tokens_consumidos, símbolo, sufijo_de_puntuación)` o `None`.
/// El último número del patrón puede llevar puntuación de cierre pegada
/// («dos slash tres.»); se conserva tras el símbolo.
fn intentar(texto: &str, toks: &[(usize, usize)], i: usize) -> Option<(usize, String, String)> {
    let palabra = |k: usize| -> Option<&str> { toks.get(k).map(|&(a, b)| &texto[a..b]) };
    // Número con posible puntuación de cierre; devuelve (digitos, sufijo).
    let numero_con_sufijo = |k: usize| -> Option<(String, String)> {
        let t = palabra(k)?;
        let nucleo = t.trim_end_matches(PUNT_FINAL);
        let sufijo = &t[nucleo.len()..];
        como_digitos(nucleo).map(|d| (d, sufijo.to_string()))
    };
    let numero_limpio = |k: usize| -> Option<String> {
        let t = palabra(k)?;
        if t.ends_with(PUNT_FINAL) {
            return None;
        }
        como_digitos(t)
    };

    let n1 = numero_limpio(i)?;
    let baja = |k: usize| palabra(k).map(|s| s.to_lowercase());

    // N slash N
    if baja(i + 1).as_deref() == Some("slash") {
        let (n2, suf) = numero_con_sufijo(i + 2)?;
        return Some((3, format!("{n1}/{n2}"), suf));
    }
    // N (dividido|partido) (por|entre) N
    if matches!(baja(i + 1).as_deref(), Some("dividido") | Some("partido"))
        && matches!(baja(i + 2).as_deref(), Some("por") | Some("entre"))
    {
        let (n2, suf) = numero_con_sufijo(i + 3)?;
        return Some((4, format!("{n1}/{n2}"), suf));
    }
    // N por ciento
    if baja(i + 1).as_deref() == Some("por") {
        // «ciento» puede traer puntuación de cierre pegada.
        let t3 = palabra(i + 2)?;
        let nucleo = t3.trim_end_matches(PUNT_FINAL);
        if nucleo.eq_ignore_ascii_case("ciento") {
            let suf = t3[nucleo.len()..].to_string();
            return Some((3, format!("{n1}%"), suf));
        }
    }
    None
}

/// Reescribe los patrones de símbolos inequívocos, preservando exactamente el
/// texto y los espacios que quedan fuera de cada coincidencia.
pub fn normalizar_simbolos(texto: &str) -> String {
    let toks = tokens_con_rango(texto);
    let mut out = String::with_capacity(texto.len());
    let mut ultimo = 0usize;
    let mut i = 0usize;
    while i < toks.len() {
        if let Some((consumidos, simbolo, sufijo)) = intentar(texto, &toks, i) {
            // Todo lo previo (incluidos tokens no casados y su espaciado).
            out.push_str(&texto[ultimo..toks[i].0]);
            out.push_str(&simbolo);
            out.push_str(&sufijo);
            ultimo = toks[i + consumidos - 1].1;
            i += consumidos;
        } else {
            i += 1;
        }
    }
    out.push_str(&texto[ultimo..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fracciones_con_slash() {
        assert_eq!(normalizar_simbolos("dos slash tres"), "2/3");
        assert_eq!(
            normalizar_simbolos("la proporción es 16 slash 9"),
            "la proporción es 16/9"
        );
    }

    #[test]
    fn division_explicita() {
        assert_eq!(normalizar_simbolos("diez dividido por dos"), "10/2");
        assert_eq!(normalizar_simbolos("cien partido entre cuatro"), "100/4");
    }

    #[test]
    fn porcentaje() {
        assert_eq!(normalizar_simbolos("subió cinco por ciento"), "subió 5%");
        assert_eq!(normalizar_simbolos("cien por ciento"), "100%");
    }

    #[test]
    fn conserva_puntuacion_final() {
        assert_eq!(normalizar_simbolos("es dos slash tres."), "es 2/3.");
        assert_eq!(
            normalizar_simbolos("subió cinco por ciento, ojo"),
            "subió 5%, ojo"
        );
    }

    #[test]
    fn no_toca_habla_normal() {
        // La razón de ser del alcance conservador: estas NO cambian.
        for t in [
            "quiero más café",
            "gracias por todo",
            "me da igual",
            "trabajamos por turnos",
            "el partido fue entre dos equipos",
            "dividido en secciones",
        ] {
            assert_eq!(normalizar_simbolos(t), t, "no debía tocar «{t}»");
        }
    }

    #[test]
    fn sin_numero_a_los_lados_no_dispara() {
        assert_eq!(
            normalizar_simbolos("algo slash otra cosa"),
            "algo slash otra cosa"
        );
        assert_eq!(normalizar_simbolos("dos slash mesa"), "dos slash mesa");
    }

    #[test]
    fn preserva_espacios_alrededor() {
        assert_eq!(
            normalizar_simbolos("antes  dos slash tres  después"),
            "antes  2/3  después"
        );
        assert_eq!(
            normalizar_simbolos("vale dos slash tres\ny sigue"),
            "vale 2/3\ny sigue"
        );
    }

    #[test]
    fn digitos_ya_puestos_tambien_valen() {
        assert_eq!(normalizar_simbolos("5 slash 8"), "5/8");
        assert_eq!(normalizar_simbolos("20 por ciento"), "20%");
    }

    #[test]
    fn texto_sin_patrones_queda_igual() {
        let t = "esto es una frase normal sin símbolos";
        assert_eq!(normalizar_simbolos(t), t);
        assert_eq!(normalizar_simbolos(""), "");
    }
}
