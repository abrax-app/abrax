//! Corrector de tildes determinista: restaura acentos que el dictado suele
//! comerse (codigo → código, perdon → perdón), sin ningún modelo.
//!
//! Regla de seguridad, y la razón de que esto sea fiable: **solo se añade la
//! tilde cuando la forma SIN tilde no es una palabra válida del español**. Así
//! `codigo` → `código` es seguro (no existe «codigo»), pero `esta`, `mas`,
//! `numero` o `publica` se dejan intactas porque su forma sin tilde también es
//! una palabra y decidir requeriría contexto. El mapa [`tildes_es.tsv`] se
//! generó offline con esa condición (diccionario RLA-ES como oráculo de
//! validez); aquí solo se consulta. Ver `scripts/gen_tildes.mjs`.
//!
//! Conservador como el resto del módulo: no toca cifras, marcadores
//! `__TIPO_n__`, identificadores (`useAuthStore`, `fetch_data`), URLs
//! (`www.abrax.app`) ni palabras en MAYÚSCULAS; ante cualquier duda, deja el
//! token igual. La ñ **no** se restaura nunca (ano/año, uno/uño son ambiguos y
//! el ASR ya acierta la ñ): solo se restauran tildes de vocales.

use std::collections::HashMap;
use std::sync::LazyLock;

/// El mapa se compila al binario. Cada línea de datos es
/// `<sin_tilde>\t<con_tilde>`; se ignoran comentarios (`#`) y líneas vacías.
static DATOS: &str = include_str!("tildes_es.tsv");

static MAPA: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    DATOS
        .lines()
        .filter_map(|linea| {
            let l = linea.trim_end_matches('\r');
            if l.is_empty() || l.starts_with('#') {
                return None;
            }
            l.split_once('\t')
        })
        .collect()
});

/// Forma ortográfica del núcleo de un token, para decidir si se puede tocar.
enum Caso {
    /// Todo en minúscula: `codigo`.
    Minuscula,
    /// Primera en mayúscula, resto en minúscula: `Codigo`.
    Titulo,
    /// MAYÚSCULAS, mayúscula interna (`useAuthStore`), etc.: no se toca.
    Otro,
}

fn forma_caso(core: &str) -> Caso {
    let mut it = core.chars();
    // `core` nunca está vacío cuando se llama aquí.
    let primera = match it.next() {
        Some(c) => c,
        None => return Caso::Otro,
    };
    let resto_minuscula = it.all(|c| c.is_lowercase());
    if primera.is_lowercase() && resto_minuscula {
        Caso::Minuscula
    } else if primera.is_uppercase() && resto_minuscula {
        Caso::Titulo
    } else {
        Caso::Otro
    }
}

fn capitalizar(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(primera) => primera.to_uppercase().chain(c).collect(),
        None => String::new(),
    }
}

/// Corrige un único token separado por espacios, conservando la puntuación de
/// apertura/cierre y las mayúsculas. Devuelve el token igual si no hay nada
/// seguro que hacer.
fn corregir_token(tok: &str) -> String {
    // Núcleo = tramo entre la primera y la última letra. Lo de fuera es
    // puntuación (`código.`, `¿qué`) y se conserva tal cual.
    let Some(inicio) = tok
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, _)| i)
    else {
        return tok.to_string(); // sin letras: cifras, «__NUM_1__», «15», «...»
    };
    let fin = tok
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(inicio);

    let pre = &tok[..inicio];
    let core = &tok[inicio..fin];
    let post = &tok[fin..];

    // Si el núcleo trae algo que no es letra (guion bajo, punto, dígito,
    // barra), es un identificador/URL/marcador: intocable.
    if !core.chars().all(|c| c.is_alphabetic()) {
        return tok.to_string();
    }

    let acentuada: Option<String> = match forma_caso(core) {
        Caso::Minuscula => MAPA.get(core).map(|v| v.to_string()),
        Caso::Titulo => MAPA
            .get(core.to_lowercase().as_str())
            .map(|v| capitalizar(v)),
        Caso::Otro => None,
    };

    match acentuada {
        Some(a) => format!("{pre}{a}{post}"),
        None => tok.to_string(),
    }
}

/// Restaura las tildes seguras de todo el texto, preservando exactamente los
/// espacios y saltos de línea originales.
pub fn restaurar_tildes(texto: &str) -> String {
    let mut out = String::with_capacity(texto.len());
    let mut token = String::new();
    for c in texto.chars() {
        if c.is_whitespace() {
            if !token.is_empty() {
                out.push_str(&corregir_token(&token));
                token.clear();
            }
            out.push(c);
        } else {
            token.push(c);
        }
    }
    if !token.is_empty() {
        out.push_str(&corregir_token(&token));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_mapa_carga_y_tiene_los_casos_base() {
        assert_eq!(MAPA.get("codigo").copied(), Some("código"));
        assert_eq!(MAPA.get("perdon").copied(), Some("perdón"));
        assert_eq!(MAPA.get("informacion").copied(), Some("información"));
        assert!(
            MAPA.len() > 10_000,
            "el mapa parece truncado: {}",
            MAPA.len()
        );
    }

    #[test]
    fn restaura_tildes_seguras() {
        assert_eq!(
            restaurar_tildes("mando la informacion tambien"),
            "mando la información también"
        );
        assert_eq!(restaurar_tildes("el codigo ya corre"), "el código ya corre");
    }

    #[test]
    fn conserva_puntuacion_pegada() {
        assert_eq!(restaurar_tildes("perdon, sigue"), "perdón, sigue");
        // Puntuación de apertura y cierre alrededor del núcleo.
        assert_eq!(restaurar_tildes("¡rapido!"), "¡rápido!");
        assert_eq!(restaurar_tildes("(informacion)"), "(información)");
    }

    #[test]
    fn interrogativas_ambiguas_no_se_tocan() {
        // que/qué, como/cómo, donde/dónde… son tildes diacríticas: su forma sin
        // tilde es palabra válida, así que se excluyen (necesitan contexto).
        assert_eq!(restaurar_tildes("¿que?"), "¿que?");
        assert_eq!(restaurar_tildes("¿como?"), "¿como?");
    }

    #[test]
    fn respeta_mayuscula_inicial() {
        assert_eq!(restaurar_tildes("Codigo limpio"), "Código limpio");
        assert_eq!(restaurar_tildes("Version nueva"), "Versión nueva");
    }

    #[test]
    fn no_toca_las_ambiguas() {
        // Su forma sin tilde también es palabra válida: decidir necesita
        // contexto que estas reglas no tienen. Se dejan intactas.
        for t in [
            "esta", "mas", "numero", "publica", "medico", "continuo", "como",
        ] {
            assert_eq!(restaurar_tildes(t), t, "no debía tocar «{t}»");
        }
    }

    #[test]
    fn no_restaura_la_ene() {
        // ano/año son ambiguas; la ñ no se restaura nunca.
        assert_eq!(restaurar_tildes("el ano pasado"), "el ano pasado");
        assert_eq!(restaurar_tildes("manana"), "manana");
    }

    #[test]
    fn jamas_toca_identificadores_marcadores_urls() {
        assert_eq!(restaurar_tildes("useAuthStore"), "useAuthStore");
        assert_eq!(restaurar_tildes("fetch_data"), "fetch_data");
        assert_eq!(restaurar_tildes("__FECHA_1__"), "__FECHA_1__");
        assert_eq!(
            restaurar_tildes("visita https://abrax.app/precios"),
            "visita https://abrax.app/precios"
        );
        assert_eq!(restaurar_tildes("la version v0.1.0"), "la versión v0.1.0");
        // MAYÚSCULAS: posible sigla, no se toca.
        assert_eq!(restaurar_tildes("CODIGO"), "CODIGO");
    }

    #[test]
    fn es_idempotente_sobre_texto_ya_correcto() {
        let ok = "el código ya está bien, gracias";
        assert_eq!(restaurar_tildes(ok), ok);
    }

    #[test]
    fn preserva_espacios_y_saltos() {
        assert_eq!(
            restaurar_tildes("codigo  con   espacios"),
            "código  con   espacios"
        );
        assert_eq!(restaurar_tildes("codigo\nnuevo"), "código\nnuevo");
        assert_eq!(restaurar_tildes(""), "");
        assert_eq!(restaurar_tildes("   "), "   ");
    }

    #[test]
    fn cifras_y_simbolos_intactos() {
        assert_eq!(restaurar_tildes("cuesta 15,50"), "cuesta 15,50");
        assert_eq!(restaurar_tildes("3.14"), "3.14");
    }
}
