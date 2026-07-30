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

/// Esqueleto fonético de un token, para reconocer «emoji» como lo oye el ASR.
///
/// **Por qué hace falta.** «Emoji» es un préstamo del japonés y ningún modelo lo
/// transcribe fiablemente. Medido el 29/07 con Canary dictando «emoji, cara
/// feliz»: salieron **«Hemohi»**, **«emoyi»** y **«emogi»**. Con coincidencia
/// exacta la función no se activaba nunca, así que estaba muerta en la práctica.
///
/// Los tres errores son fonéticos y del mismo tipo: en español el sonido /x/ se
/// escribe `j`, `g` (ante e/i) o se oye como `h` aspirada, la `y` se confunde con
/// `j`, y la `h` inicial es muda. Así que:
///
/// 1. se cae la `h` inicial, que no suena;
/// 2. `j`, `g`, `h`, `x` e `y` colapsan en un solo símbolo.
///
/// ```text
/// «emoji»  → emoji      «hemohi» → emohi → emoji
/// «emoyi»  → emoji      «emogi»  → emoji
/// ```
///
/// No pretende ser fonética del español: es una reducción mínima dirigida a
/// ESTE problema. Colapsa cosas absurdas («gato» → «jato») y da igual, porque lo
/// único con lo que se compara es «emoji».
fn esqueleto_fonetico(token: &str) -> String {
    let clave = build_match_key(token);
    let sin_h = clave.strip_prefix('h').unwrap_or(&clave);
    sin_h
        .chars()
        .map(|c| match c {
            'j' | 'g' | 'h' | 'x' | 'y' => 'j',
            otro => otro,
        })
        .collect()
}

/// Distancia de edición acotada: en cuanto pasa de `tope` se corta. No hace falta
/// el valor exacto, solo saber si cabe en el presupuesto.
fn distancia_hasta(a: &str, b: &str, tope: usize) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > tope {
        return tope + 1;
    }
    let mut fila: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.iter().enumerate() {
        let mut previa = fila[0];
        fila[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let coste = usize::from(ca != cb);
            let actual = fila[j + 1];
            fila[j + 1] = (fila[j] + 1).min(actual + 1).min(previa + coste);
            previa = actual;
        }
        if fila.iter().min().copied().unwrap_or(0) > tope {
            return tope + 1;
        }
    }
    fila[b.len()]
}

const DISPARADOR: &str = "emoji";
/// El plural NO dispara: hablar **de** los emoji no debe convertir nada
/// («los emojis de fuego son populares» tiene que quedarse igual). Se excluye por
/// esqueleto, así que «emoyis» y «emogis» caen con él.
const DISPARADOR_PLURAL: &str = "emojis";

/// ¿El esqueleto es el disparador? Exacto, o a una edición de distancia para
/// aguantar errores de vocal de otros modelos («imoji», «emji»).
///
/// Puede permitirse ser generoso porque **hay una segunda puerta**: si detrás no
/// viene un nombre de emoji conocido, no se toca nada. Un falso disparo sin
/// nombre válido detrás es un no-op, no un destrozo.
fn es_esqueleto_disparador(esq: &str) -> bool {
    if esq == DISPARADOR_PLURAL {
        return false;
    }
    esq == DISPARADOR || (esq.chars().count() >= 4 && distancia_hasta(esq, DISPARADOR, 1) <= 1)
}

/// ¿Empieza el disparador en `i`? Devuelve cuántos tokens ocupa (1 o 2).
///
/// Los 2 tokens son para cuando el ASR PARTE la palabra («emo yi», «e moji»),
/// que con un préstamo raro pasa. Se prueba primero suelto y luego unido, así el
/// caso normal no paga nada.
///
/// **El caso partido exige coincidencia EXACTA, sin el margen de una edición.**
/// Juntar dos reglas permisivas multiplica los falsos positivos, y aquí hubo uno
/// real: la `y` de la conjunción colapsa a `j` en el esqueleto, así que «y emoji»
/// unido daba «jemoji», que está a UNA edición de «emoji» — y «emoji de fuego y
/// emoji cohete» se comía la «y». Lo cazó un test que ya existía.
fn disparador_en(esqueletos: &[String], i: usize) -> Option<usize> {
    if es_esqueleto_disparador(&esqueletos[i]) {
        return Some(1);
    }
    let siguiente = esqueletos.get(i + 1)?;
    if !esqueletos[i].is_empty()
        && !siguiente.is_empty()
        && format!("{}{}", esqueletos[i], siguiente) == DISPARADOR
    {
        return Some(2);
    }
    None
}

/// Sustituye los emoji dictados. Sin la palabra «emoji» devuelve el texto tal
/// cual, byte a byte.
pub fn aplicar_emoji_dictado(texto: &str) -> String {
    let tokens: Vec<&str> = texto.split_whitespace().collect();
    if tokens.is_empty() {
        return texto.to_string();
    }

    let esqueletos: Vec<String> = tokens.iter().map(|t| esqueleto_fonetico(t)).collect();
    if !(0..tokens.len()).any(|i| disparador_en(&esqueletos, i).is_some()) {
        return texto.to_string();
    }

    let claves: Vec<String> = tokens.iter().map(|t| build_match_key(t)).collect();
    let mut salida: Vec<String> = Vec::with_capacity(tokens.len());
    let mut i = 0;

    while i < tokens.len() {
        let Some(ancho) = disparador_en(&esqueletos, i) else {
            salida.push(tokens[i].to_string());
            i += 1;
            continue;
        };

        // Tras «emoji» se saltan los puentes («de», «la», …) para localizar
        // dónde empieza el nombre. `ancho` puede ser 2 si el ASR partió la
        // palabra en dos tokens.
        let mut inicio = i + ancho;
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

    // ───────── el ASR no dice «emoji»: los casos REALES medidos ─────────────

    #[test]
    fn los_tres_errores_reales_de_canary() {
        // Medidos el 29/07 dictando «emoji, cara feliz» con Canary 180M. Con
        // coincidencia exacta la función no se activaba NUNCA: estaba muerta.
        for dicho in [
            "Hemohi cara feliz",
            "emoyi cara feliz",
            "emogi cara feliz",
            "emoji cara feliz",
        ] {
            assert_eq!(
                aplicar_emoji_dictado(dicho),
                "🙂",
                "no reconoció el disparador en «{dicho}»"
            );
        }
    }

    #[test]
    fn el_esqueleto_colapsa_las_variantes_del_sonido_x() {
        for v in [
            "emoji", "emoyi", "emogi", "emohi", "hemohi", "Hemoji", "emoxi",
        ] {
            assert_eq!(esqueleto_fonetico(v), "emoji", "falló con «{v}»");
        }
    }

    #[test]
    fn aguanta_errores_de_vocal_de_otros_modelos() {
        // «independiente del modelo»: una edición de margen cubre confusiones de
        // vocal y letras comidas que otros modelos producen.
        for dicho in ["imoji cara feliz", "emojo cara feliz", "emji cara feliz"] {
            assert_eq!(aplicar_emoji_dictado(dicho), "🙂", "falló con «{dicho}»");
        }
    }

    #[test]
    fn si_el_asr_parte_la_palabra_en_dos() {
        assert_eq!(aplicar_emoji_dictado("emo yi cara feliz"), "🙂");
        assert_eq!(aplicar_emoji_dictado("e moji de fuego"), "🔥");
    }

    #[test]
    fn la_conjuncion_y_no_se_come_al_unir_tokens() {
        // REGRESIÓN REAL. La `y` colapsa a `j` en el esqueleto, así que «y emoji»
        // unido da «jemoji», a UNA edición de «emoji». Con margen difuso en el
        // caso partido, «y» desaparecía. Por eso la unión exige coincidencia
        // exacta. Lo cazó `varios_en_una_frase`, que ya existía.
        assert_eq!(
            aplicar_emoji_dictado("emoji de fuego y emoji cohete"),
            "🔥 y 🚀"
        );
        assert_eq!(aplicar_emoji_dictado("fuego y agua"), "fuego y agua");
        assert!(disparador_en(&["j".to_string(), "emoji".to_string()], 0).is_none());
    }

    #[test]
    fn el_margen_no_se_come_palabras_reales() {
        // LA TRAMPA: «enojo» está a dos ediciones de «emoji» y ES una palabra
        // española. Con margen 2 se habría convertido «el enojo de fuego» en
        // «el 🔥». Por eso el margen es 1 y no 2.
        assert_eq!(esqueleto_fonetico("enojo"), "enojo");
        assert!(!es_esqueleto_disparador("enojo"));
        for t in [
            "el enojo de fuego le duró poco",
            "un mojito de fuego no existe",
            "el remojo de fuego",
            "no seas majo",
        ] {
            assert_eq!(aplicar_emoji_dictado(t), t, "tocó «{t}»");
        }
    }

    #[test]
    fn el_plural_no_dispara_ni_deformado() {
        // «emojis» queda fuera, y con él sus variantes fonéticas: hablar DE los
        // emoji no puede convertir nada.
        for t in [
            "los emojis de fuego son populares",
            "los emoyis de fuego son populares",
            "los emogis de fuego son populares",
        ] {
            assert_eq!(aplicar_emoji_dictado(t), t, "tocó «{t}»");
        }
    }

    #[test]
    fn la_distancia_acotada_se_comporta() {
        assert_eq!(distancia_hasta("emoji", "emoji", 1), 0);
        assert_eq!(distancia_hasta("emoji", "emojo", 1), 1);
        assert_eq!(distancia_hasta("emoji", "emoj", 1), 1);
        assert_eq!(distancia_hasta("emoji", "emojis", 1), 1);
        // Se corta al pasar del tope: solo importa que NO quepa.
        assert!(distancia_hasta("emoji", "enojo", 1) > 1);
        assert!(distancia_hasta("emoji", "cualquiera", 1) > 1);
        assert!(distancia_hasta("", "emoji", 1) > 1);
    }

    #[test]
    fn el_disparador_deformado_sin_nombre_detras_no_toca_nada() {
        // La segunda puerta sigue en pie: sin nombre válido detrás, un disparo
        // (aunque sea un falso positivo) es un no-op.
        for t in ["mándame un emoyi", "no uso ningún emogi en esto"] {
            assert_eq!(aplicar_emoji_dictado(t), t, "tocó «{t}»");
        }
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
