//! Numerales españoles dictados → dígitos, con patrones compuestos.
//!
//! Reconoce, en este orden y en cada posición del texto:
//!
//! 1. **hora**    — «a las diecisiete treinta» → `17:30` · «a las ocho y media» → `8:30`
//! 2. **versión** — «cero punto nueve punto dos» → `0.9.2` (también IPs, ≥3 partes)
//! 3. **decimal** — «uno coma cinco» → `1,5`
//! 4. **numeral** — «doce mil ochocientos» → `12800` · «nueve ocho siete seis» → `9876`
//!
//! Principio del módulo: **si un patrón no encaja del todo, se abstiene** y las
//! palabras quedan tal cual. Convertir a medias es peor que no convertir.
//!
//! Guardas aprendidas de un corpus generado de >30 000 casos de ida y vuelta:
//! - «un/una/uno» SUELTOS jamás se convierten: son artículos («una idea»).
//! - «ciento» tras «por» es el modismo del porcentaje, no la cifra
//!   («cien por ciento» debe seguir siendo texto hasta que `simbolos` lo haga `%`).
//! - «dos puntos»/«tres puntos» nombran un signo, no cifras
//!   («localhost dos puntos ocho mil ochenta» → los convierte `identificadores`).
//! - Los sumandos de un numeral español son **estrictamente decrecientes**:
//!   «ochocientos veinte» (800>20) es un número; «diecisiete treinta» (17<30)
//!   son DOS números seguidos — casi siempre una hora.

use crate::audio_toolkit::build_match_key;

/// Valor de un numeral simple. `None` si el token no es numeral.
fn unidad(k: &str) -> Option<u64> {
    Some(match k {
        "cero" => 0,
        "uno" | "un" | "una" => 1,
        "dos" => 2,
        "tres" => 3,
        "cuatro" => 4,
        "cinco" => 5,
        "seis" => 6,
        "siete" => 7,
        "ocho" => 8,
        "nueve" => 9,
        "diez" => 10,
        "once" => 11,
        "doce" => 12,
        "trece" => 13,
        "catorce" => 14,
        "quince" => 15,
        "dieciseis" => 16,
        "diecisiete" => 17,
        "dieciocho" => 18,
        "diecinueve" => 19,
        "veinte" => 20,
        "veintiuno" | "veintiun" | "veintiuna" => 21,
        "veintidos" => 22,
        "veintitres" => 23,
        "veinticuatro" => 24,
        "veinticinco" => 25,
        "veintiseis" => 26,
        "veintisiete" => 27,
        "veintiocho" => 28,
        "veintinueve" => 29,
        "treinta" => 30,
        "cuarenta" => 40,
        "cincuenta" => 50,
        "sesenta" => 60,
        "setenta" => 70,
        "ochenta" => 80,
        "noventa" => 90,
        "cien" | "ciento" => 100,
        "doscientos" | "doscientas" => 200,
        "trescientos" | "trescientas" => 300,
        "cuatrocientos" | "cuatrocientas" => 400,
        "quinientos" | "quinientas" => 500,
        "seiscientos" | "seiscientas" => 600,
        "setecientos" | "setecientas" => 700,
        "ochocientos" | "ochocientas" => 800,
        "novecientos" | "novecientas" => 900,
        _ => return None,
    })
}

/// Multiplicadores de escala.
fn multiplicador(k: &str) -> Option<u64> {
    Some(match k {
        "mil" => 1_000,
        "millon" | "millones" => 1_000_000,
        _ => return None,
    })
}

fn es_numeral(k: &str) -> bool {
    unidad(k).is_some() || multiplicador(k).is_some()
}

/// Artículo indefinido: nunca se convierte suelto.
fn es_articulo(k: &str) -> bool {
    matches!(k, "un" | "una" | "uno")
}

/// Convierte una secuencia de claves numerales en su valor. `None` si la
/// secuencia no es un numeral español legal (sumandos no decrecientes).
fn acumular(claves: &[String]) -> Option<u64> {
    let (mut total, mut cur) = (0u64, 0u64);
    let (mut visto, mut ultimo) = (false, u64::MAX);
    for k in claves {
        if k == "y" {
            continue;
        }
        if let Some(v) = unidad(k) {
            if v >= ultimo {
                return None;
            }
            cur += v;
            ultimo = v;
            visto = true;
        } else if let Some(m) = multiplicador(k) {
            if cur == 0 {
                cur = 1;
            }
            total += cur * m;
            cur = 0;
            ultimo = u64::MAX; // tras una escala empieza un grupo nuevo
            visto = true;
        } else {
            return None;
        }
    }
    if visto {
        Some(total + cur)
    } else {
        None
    }
}

/// Dictado cifra a cifra («nueve ocho siete seis…» → «9876»). Mínimo 4 dígitos
/// sueltos para no confundirlo con «dos tres» hablado.
fn como_secuencia(claves: &[String]) -> Option<String> {
    if claves.len() < 4 {
        return None;
    }
    let mut s = String::new();
    for k in claves {
        match unidad(k) {
            Some(v) if v <= 9 => s.push_str(&v.to_string()),
            _ => return None,
        }
    }
    Some(s)
}

const PUNT: &[char] = &['.', ',', ';', ':', '!', '?', '…', ')', ']', '»', '"', '\''];

/// Reescribe los numerales del texto. Trabaja línea a línea (los saltos de
/// línea del dictado sobreviven siempre) y una línea sin conversiones se
/// devuelve **byte a byte** — el espaciado original solo se renormaliza en las
/// líneas donde de verdad hubo una conversión.
pub fn normalizar_numeros(texto: &str) -> String {
    texto
        .split('\n')
        .map(normalizar_linea)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalizar_linea(linea: &str) -> String {
    let convertida = convertir_tokens(linea);
    // Sin conversión semántica, la línea original queda intacta (espacios
    // dobles incluidos): comparar contra la mera re-unión de tokens detecta
    // si `convertir_tokens` cambió algo más que el whitespace.
    let sin_cambios = convertida == linea.split_whitespace().collect::<Vec<_>>().join(" ");
    if sin_cambios {
        linea.to_string()
    } else {
        convertida
    }
}

fn convertir_tokens(texto: &str) -> String {
    let toks: Vec<&str> = texto.split_whitespace().collect();
    let clave = |t: &str| build_match_key(t.trim_end_matches(PUNT).trim_start_matches('¿'));
    let sufijo = |t: &str| {
        let n = t.trim_end_matches(PUNT);
        t[n.len()..].to_string()
    };
    // Numerales que aquí NO son cifras porque nombran otra cosa (ver cabecera).
    let cifra = |idx: usize| -> bool {
        let k = clave(toks[idx]);
        if k == "ciento" && idx > 0 && clave(toks[idx - 1]) == "por" {
            return false;
        }
        if (k == "dos" || k == "tres") && idx + 1 < toks.len() && clave(toks[idx + 1]) == "puntos" {
            return false;
        }
        es_numeral(&k)
    };
    // Prefijo VÁLIDO MÁS LARGO desde `i`: (valor, tokens consumidos). Se encoge
    // por la derecha hasta que la secuencia sea un numeral legal — así
    // «diecisiete treinta» entrega el 17 y deja el 30 para el patrón de hora.
    let numeral_en = |i: usize| -> Option<(u64, usize)> {
        if i >= toks.len() || !cifra(i) {
            return None;
        }
        let mut j = i;
        while j < toks.len() {
            if cifra(j) {
                if toks[j].ends_with(PUNT) {
                    j += 1;
                    break;
                }
                j += 1;
            } else if clave(toks[j]) == "y" && j + 1 < toks.len() && cifra(j + 1) {
                j += 1;
            } else {
                break;
            }
        }
        while j > i {
            let claves: Vec<String> = toks[i..j].iter().map(|t| clave(t)).collect();
            if claves.last().is_some_and(|k| k == "y") {
                j -= 1;
                continue;
            }
            if let Some(v) = acumular(&claves) {
                return Some((v, j - i));
            }
            j -= 1;
        }
        None
    };

    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < toks.len() {
        if !cifra(i) {
            out.push(toks[i].to_string());
            i += 1;
            continue;
        }
        let previa = if i > 0 {
            clave(toks[i - 1])
        } else {
            String::new()
        };
        let contexto_hora = previa == "las" || previa == "la";

        // ── 1. HORA ──────────────────────────────────────────────────────
        if contexto_hora {
            if let Some((h, n)) = numeral_en(i) {
                if h <= 23 {
                    // «las ocho y media» / «y cuarto»
                    if i + n + 1 < toks.len() && clave(toks[i + n]) == "y" {
                        let m = clave(toks[i + n + 1]);
                        let min = match m.as_str() {
                            "media" => Some(30),
                            "cuarto" => Some(15),
                            _ => None,
                        };
                        if let Some(min) = min {
                            out.push(format!("{h}:{min:02}{}", sufijo(toks[i + n + 1])));
                            i += n + 2;
                            continue;
                        }
                    }
                    // «las diecisiete treinta»: dos numerales que no forman uno
                    if let Some((mm, n2)) = numeral_en(i + n) {
                        if mm <= 59 {
                            out.push(format!("{h}:{mm:02}{}", sufijo(toks[i + n + n2 - 1])));
                            i += n + n2;
                            continue;
                        }
                    }
                }
            }
        }

        // ── 2. VERSIÓN / IP: N punto N (punto N)+ — mínimo 3 partes ──────
        if let Some((primera, n)) = numeral_en(i) {
            let mut partes = vec![primera.to_string()];
            let mut j = i + n;
            while j + 1 < toks.len() && clave(toks[j]) == "punto" {
                match numeral_en(j + 1) {
                    Some((v, m)) => {
                        partes.push(v.to_string());
                        j += 1 + m;
                    }
                    None => break,
                }
            }
            if partes.len() >= 3 {
                out.push(format!("{}{}", partes.join("."), sufijo(toks[j - 1])));
                i = j;
                continue;
            }
        }

        // ── 3. DECIMAL: N coma N ─────────────────────────────────────────
        if let Some((ent, n)) = numeral_en(i) {
            if i + n < toks.len() && clave(toks[i + n]) == "coma" {
                if let Some((dec, m)) = numeral_en(i + n + 1) {
                    out.push(format!("{ent},{dec}{}", sufijo(toks[i + n + m])));
                    i += n + 1 + m;
                    continue;
                }
            }
        }

        // ── 4. NUMERAL SUELTO ────────────────────────────────────────────
        let mut j = i;
        while j < toks.len() {
            if cifra(j) {
                if toks[j].ends_with(PUNT) {
                    j += 1;
                    break;
                }
                j += 1;
            } else if clave(toks[j]) == "y" && j + 1 < toks.len() && cifra(j + 1) {
                j += 1;
            } else {
                break;
            }
        }
        let claves: Vec<String> = toks[i..j].iter().map(|t| clave(t)).collect();
        let solo_articulo = claves.len() == 1 && es_articulo(&claves[0]);
        let valor = if solo_articulo {
            None
        } else {
            como_secuencia(&claves).or_else(|| acumular(&claves).map(|n| n.to_string()))
        };
        match valor {
            Some(v) => out.push(format!("{v}{}", sufijo(toks[j - 1]))),
            None => {
                for t in &toks[i..j] {
                    out.push(t.to_string());
                }
            }
        }
        i = j;
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── generador con verdad conocida: la INVERSA del parser ───────────────
    // Si sabemos que 12800 se dicta «doce mil ochocientos», el parser debe
    // devolver exactamente 12800. Miles de casos sin sesgo del autor.

    const UNIDADES: &[&str] = &[
        "cero",
        "uno",
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
        "dieciséis",
        "diecisiete",
        "dieciocho",
        "diecinueve",
        "veinte",
        "veintiuno",
        "veintidós",
        "veintitrés",
        "veinticuatro",
        "veinticinco",
        "veintiséis",
        "veintisiete",
        "veintiocho",
        "veintinueve",
    ];
    const DECENAS: &[&str] = &[
        "",
        "",
        "",
        "treinta",
        "cuarenta",
        "cincuenta",
        "sesenta",
        "setenta",
        "ochenta",
        "noventa",
    ];
    const CENTENAS: &[&str] = &[
        "",
        "ciento",
        "doscientos",
        "trescientos",
        "cuatrocientos",
        "quinientos",
        "seiscientos",
        "setecientos",
        "ochocientos",
        "novecientos",
    ];

    fn numero_a_palabras(n: u64) -> String {
        if n < 30 {
            return UNIDADES[n as usize].to_string();
        }
        if n < 100 {
            let (d, u) = ((n / 10) as usize, n % 10);
            return if u == 0 {
                DECENAS[d].to_string()
            } else {
                format!("{} y {}", DECENAS[d], UNIDADES[u as usize])
            };
        }
        if n == 100 {
            return "cien".to_string();
        }
        if n < 1000 {
            let (c, r) = ((n / 100) as usize, n % 100);
            return if r == 0 {
                CENTENAS[c].to_string()
            } else {
                format!("{} {}", CENTENAS[c], numero_a_palabras(r))
            };
        }
        let (m, r) = (n / 1000, n % 1000);
        let miles = if m == 1 {
            "mil".to_string()
        } else {
            format!("{} mil", numero_a_palabras(m))
        };
        if r == 0 {
            miles
        } else {
            format!("{} {}", miles, numero_a_palabras(r))
        }
    }

    /// La forma idiomática real: «veintiún mil», no «veintiuno mil». Romper la
    /// complicidad generador↔parser probando también la forma apocopada.
    fn apocopar(dictado: &str) -> String {
        dictado
            .replace("veintiuno mil", "veintiún mil")
            .replace("y uno mil", "y un mil")
            .replace("uno mil", "un mil")
    }

    /// xorshift64*: pseudoaleatorio determinista, reproducible con la semilla.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545F4914F6CDD1D)
        }
    }

    #[test]
    fn ida_y_vuelta_exhaustiva_hasta_3000() {
        for n in 2u64..=3000 {
            let d = numero_a_palabras(n);
            assert_eq!(normalizar_numeros(&d), n.to_string(), "dictado: {d}");
        }
    }

    #[test]
    fn ida_y_vuelta_muestreada_hasta_un_millon() {
        let mut rng = Rng(0x9E3779B97F4A7C15);
        for _ in 0..5000 {
            let n = rng.next() % 1_000_000;
            if n < 2 {
                continue; // «uno»/«cero» sueltos: ver test del artículo
            }
            let d = numero_a_palabras(n);
            assert_eq!(normalizar_numeros(&d), n.to_string(), "dictado: {d}");
            let a = apocopar(&d);
            assert_eq!(normalizar_numeros(&a), n.to_string(), "apocopado: {a}");
        }
    }

    #[test]
    fn dentro_de_frase_y_con_puntuacion() {
        assert_eq!(
            normalizar_numeros("necesitamos procesar doscientas cincuenta palabras"),
            "necesitamos procesar 250 palabras"
        );
        assert_eq!(
            normalizar_numeros("configura el puerto doce mil ochocientos."),
            "configura el puerto 12800."
        );
        assert_eq!(
            normalizar_numeros("la fecha límite es el treinta y uno de agosto"),
            "la fecha límite es el 31 de agosto"
        );
    }

    #[test]
    fn horas() {
        assert_eq!(
            normalizar_numeros("la prueba empieza a las diecisiete treinta"),
            "la prueba empieza a las 17:30"
        );
        assert_eq!(
            normalizar_numeros("la reunión es a las ocho y media."),
            "la reunión es a las 8:30."
        );
    }

    #[test]
    fn decimales_y_versiones() {
        assert_eq!(
            normalizar_numeros("el archivo pesa uno coma cinco gigas"),
            "el archivo pesa 1,5 gigas"
        );
        assert_eq!(
            normalizar_numeros("la versión actual es la cero punto nueve punto dos."),
            "la versión actual es la 0.9.2."
        );
        assert_eq!(
            normalizar_numeros("escuchar en ciento veintisiete punto cero punto cero punto uno"),
            "escuchar en 127.0.0.1"
        );
    }

    #[test]
    fn secuencia_de_digitos_dictada() {
        assert_eq!(
            normalizar_numeros("mi número es nueve ocho siete seis cinco"),
            "mi número es 98765"
        );
        // Menos de 4 dígitos sueltos NO son secuencia, y «dos tres» tampoco
        // acumula (no decrece): la corrida entera se abstiene.
        let t = "el código es dos tres";
        assert_eq!(normalizar_numeros(t), t);
    }

    #[test]
    fn el_articulo_jamas_se_convierte() {
        for t in [
            "quiero una idea nueva",
            "un poco de calma",
            "uno de los archivos",
            "dame una mano",
            "es un problema",
        ] {
            assert_eq!(normalizar_numeros(t), t, "no debía tocar «{t}»");
        }
    }

    #[test]
    fn ciento_tras_por_es_porcentaje_no_cifra() {
        // «ciento» queda para que `simbolos` arme el «%»; «cien» sí es cifra.
        assert_eq!(
            normalizar_numeros("cien por ciento seguro"),
            "100 por ciento seguro"
        );
    }

    #[test]
    fn dos_puntos_nombra_al_signo_no_al_numero() {
        assert_eq!(
            normalizar_numeros("la ruta es C dos puntos barra usuarios"),
            "la ruta es C dos puntos barra usuarios"
        );
        // …pero «dos» sin «puntos» detrás sigue siendo cifra normal.
        assert_eq!(
            normalizar_numeros("el video no puede superar los dos minutos"),
            "el video no puede superar los 2 minutos"
        );
    }

    #[test]
    fn no_decreciente_se_abstiene_en_vez_de_sumar() {
        // 17 + 30 NO es 47. Fuera del contexto de hora («a las…») la corrida
        // entera se abstiene: convertir a medias es peor que no convertir.
        let t = "dijo diecisiete treinta sin más contexto";
        assert_eq!(normalizar_numeros(t), t);
    }

    #[test]
    fn idempotente() {
        for t in [
            "necesitamos procesar doscientas cincuenta palabras",
            "la prueba empieza a las diecisiete treinta",
            "el archivo pesa uno coma cinco gigas",
        ] {
            let una = normalizar_numeros(t);
            assert_eq!(normalizar_numeros(&una), una);
        }
    }

    #[test]
    fn preserva_saltos_de_linea_y_espaciado_ajeno() {
        // Los saltos de línea del dictado sobreviven; la línea sin conversión
        // queda byte a byte, espacios dobles incluidos.
        assert_eq!(
            normalizar_numeros("compra  esto hoy\nnecesito quince pesos"),
            "compra  esto hoy\nnecesito 15 pesos"
        );
        let intocada = "texto  con   espacios raros\nsin nada que convertir";
        assert_eq!(normalizar_numeros(intocada), intocada);
    }
}
