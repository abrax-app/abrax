//! Emoji dictado: «emoji cara feliz» → «🙂». Por tabla, sin ningún modelo.
//!
//! # La palabra «emoji» es toda la seguridad
//!
//! La tabla trae 1 531 nombres de CLDR, y muchos son palabras corrientes:
//! «bandera», «ojos», «uno», «diez», «estrella», «fuego», «beso». Sin un
//! disparador, «tengo dos banderas» o «se me fue el uno» saldrían llenos de
//! pictogramas y la función destruiría texto normal.
//!
//! Por eso **solo actúa detrás de la palabra «emoji»**, que no aparece por
//! casualidad al dictar prosa. Mismo principio que
//! [`super::simbolos`]: dispara con lo que no es habla normal.
//!
//! # Qué reconoce
//!
//! ```text
//! «emoji cara feliz»            → 🙂
//! «emoji, corazón»              → ❤          (la coma se ignora)
//! «pon un emoji de fuego»       → «pon un 🔥» (de/el/la/un… se saltan)
//! «mándame un emoji»            → sin cambios (no hay nombre detrás)
//! «no uso ningún emoji en esto» → sin cambios («en» no es un nombre)
//! ```
//!
//! **Coincidencia más larga primero**: «emoji cara feliz» da 🙂 y no la 🙂 de
//! «cara» seguida de «feliz» suelto. Si detrás de «emoji» no hay ningún nombre
//! conocido, **no se toca nada** — ni se borra la palabra «emoji».

use std::collections::HashMap;

use once_cell::sync::Lazy;

use crate::audio_toolkit::build_match_key;

/// Nombre en español → emoji. Generada offline por `scripts/gen_emoji_es.mjs`
/// desde las anotaciones de CLDR de Unicode, más un puñado de alias curados a
/// mano («cara feliz», «me gusta») porque los nombres canónicos de CLDR son
/// precisos pero nadie los dicta así.
const TABLA: &str = include_str!("emoji_es.tsv");

/// Palabras que pueden ir entre «emoji» y el nombre sin romper la frase:
/// «pon un emoji **de** fuego», «el emoji **de la** bandera».
const PUENTE: &[&str] = &["de", "del", "el", "la", "los", "las", "un", "una"];

/// Tokens máximos que puede ocupar un nombre. El más largo de la tabla ronda
/// los 6; el tope acota la búsqueda sin recortar nada real.
const MAX_TOKENS_NOMBRE: usize = 6;

static MAPA: Lazy<HashMap<String, &'static str>> = Lazy::new(|| {
    TABLA
        .lines()
        .filter_map(|l| {
            let (nombre, cp) = l.split_once('\t')?;
            let clave = nombre
                .split_whitespace()
                .map(build_match_key)
                .collect::<Vec<_>>()
                .join(" ");
            if clave.is_empty() {
                return None;
            }
            Some((clave, cp))
        })
        .collect()
});

/// ¿El token es la palabra disparadora? Se compara por clave normalizada, así
/// «Emoji», «emojis» no (plural distinto) y «emoji,» con coma sí caen.
fn es_disparador(token: &str) -> bool {
    build_match_key(token) == "emoji"
}

/// Sustituye los emoji dictados. Sin la palabra «emoji» devuelve el texto tal
/// cual, byte a byte.
pub fn aplicar_emoji_dictado(texto: &str) -> String {
    let tokens: Vec<&str> = texto.split_whitespace().collect();
    if !tokens.iter().any(|t| es_disparador(t)) {
        return texto.to_string();
    }

    let claves: Vec<String> = tokens.iter().map(|t| build_match_key(t)).collect();
    let mut salida: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        if !es_disparador(tokens[i]) {
            salida.push(tokens[i].to_string());
            i += 1;
            continue;
        }

        // Tras «emoji» se saltan los puentes («de», «la», …) para localizar
        // dónde empieza el nombre.
        let mut inicio = i + 1;
        while inicio < tokens.len() && PUENTE.contains(&claves[inicio].as_str()) {
            inicio += 1;
        }

        // Coincidencia MÁS LARGA primero.
        let tope = tokens.len().min(inicio + MAX_TOKENS_NOMBRE);
        let mut encontrado: Option<(usize, &'static str)> = None;
        let mut n = tope;
        while n > inicio {
            let clave = claves[inicio..n].join(" ");
            if let Some(cp) = MAPA.get(&clave) {
                encontrado = Some((n, cp));
                break;
            }
            n -= 1;
        }

        match encontrado {
            Some((fin, cp)) => {
                // El emoji hereda la puntuación de cierre del último token del
                // nombre: «pon un emoji de fuego.» → «pon un 🔥.»
                let (_, sufijo) =
                    super::super::audio_toolkit::text::extract_punctuation(tokens[fin - 1]);
                salida.push(format!("{cp}{sufijo}"));
                i = fin;
            }
            None => {
                // Sin nombre detrás no se toca NADA: se conserva «emoji» y lo
                // que venga después. Es la cobardía deliberada de siempre.
                salida.push(tokens[i].to_string());
                i += 1;
            }
        }
    }

    salida.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─────────────────────────── lo que debe convertir ──────────────────────

    #[test]
    fn el_ejemplo_pedido() {
        assert_eq!(aplicar_emoji_dictado("emoji cara feliz"), "🙂");
    }

    #[test]
    fn en_medio_de_una_frase() {
        assert_eq!(
            aplicar_emoji_dictado("te mando un emoji cara feliz y listo"),
            "te mando un 🙂 y listo"
        );
    }

    #[test]
    fn con_puente_de() {
        assert_eq!(aplicar_emoji_dictado("pon un emoji de fuego"), "pon un 🔥");
    }

    #[test]
    fn con_puente_de_la() {
        assert_eq!(aplicar_emoji_dictado("el emoji de la cara feliz"), "el 🙂");
    }

    #[test]
    fn la_coma_del_asr_no_estorba() {
        // El ASR suele meter coma tras la palabra suelta: «emoji, corazón».
        assert_eq!(aplicar_emoji_dictado("emoji, corazón"), "❤");
    }

    #[test]
    fn hereda_la_puntuacion_de_cierre() {
        assert_eq!(
            aplicar_emoji_dictado("me encantó, emoji de fuego."),
            "me encantó, 🔥."
        );
    }

    #[test]
    fn coincidencia_mas_larga_primero() {
        // «cara llorando de risa» existe y «cara llorando» también: gana la larga.
        assert_eq!(aplicar_emoji_dictado("emoji cara llorando de risa"), "😂");
        assert_eq!(aplicar_emoji_dictado("emoji cara llorando"), "😢");
    }

    #[test]
    fn varios_en_una_frase() {
        assert_eq!(
            aplicar_emoji_dictado("emoji de fuego y emoji cohete"),
            "🔥 y 🚀"
        );
    }

    #[test]
    fn los_alias_curados_funcionan() {
        for (dicho, esperado) in [
            ("emoji me gusta", "👍"),
            ("emoji aplausos", "👏"),
            ("emoji pensando", "🤔"),
            ("emoji llorando de risa", "😂"),
        ] {
            assert_eq!(
                aplicar_emoji_dictado(dicho),
                esperado,
                "fallo con «{dicho}»"
            );
        }
    }

    // ───────────────── lo que NO debe tocar (importa más) ───────────────────

    #[test]
    fn sin_la_palabra_emoji_no_toca_nada() {
        // Todos estos son nombres REALES de la tabla y deben quedar intactos.
        for texto in [
            "tengo dos banderas en la mano",
            "se me fue el uno y el diez",
            "mira esa estrella",
            "prendí el fuego de la parrilla",
            "le di un beso",
            "cierra los ojos",
        ] {
            assert_eq!(aplicar_emoji_dictado(texto), texto, "tocó «{texto}»");
        }
    }

    #[test]
    fn emoji_sin_nombre_detras_no_cambia_nada() {
        let t = "mándame un emoji";
        assert_eq!(aplicar_emoji_dictado(t), t);
    }

    #[test]
    fn emoji_seguido_de_algo_desconocido_no_cambia_nada() {
        let t = "no uso ningún emoji en mis mensajes";
        assert_eq!(aplicar_emoji_dictado(t), t);
    }

    #[test]
    fn emoji_solo_con_puentes_detras_no_cambia_nada() {
        // «emoji de la» sin nombre: no debe comerse los puentes.
        let t = "hablemos del emoji de la";
        assert_eq!(aplicar_emoji_dictado(t), t);
    }

    #[test]
    fn texto_vacio_y_sin_disparador_son_identidad() {
        assert_eq!(aplicar_emoji_dictado(""), "");
        let t = "una frase cualquiera sin nada especial";
        assert_eq!(aplicar_emoji_dictado(t), t);
    }

    #[test]
    fn el_plural_no_dispara() {
        // «emojis» es otra palabra: hablar DE los emoji no debe convertir nada.
        let t = "los emojis de WhatsApp son distintos";
        assert_eq!(aplicar_emoji_dictado(t), t);
    }

    // ───────────────────────────── la tabla en sí ──────────────────────────

    #[test]
    fn la_tabla_carga_y_tiene_volumen() {
        assert!(
            MAPA.len() > 1_000,
            "la tabla debería traer más de mil nombres, trae {}",
            MAPA.len()
        );
    }

    #[test]
    fn las_claves_estan_normalizadas() {
        // Si una clave llevara tilde o mayúscula, nunca casaría: `build_match_key`
        // normaliza el texto dictado y la búsqueda sería siempre fallida.
        for clave in MAPA.keys().take(200) {
            assert_eq!(
                *clave,
                clave
                    .split_whitespace()
                    .map(build_match_key)
                    .collect::<Vec<_>>()
                    .join(" "),
                "clave sin normalizar: {clave}"
            );
        }
    }
}
