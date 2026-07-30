//! Puntuación dictada dentro de identificadores, rutas, dominios y correos.
//!
//! El problema: «punto», «barra», «coma» y «guion» son palabras corrientes del
//! español («ponle un punto final», «la barra del bar», «el guion de la
//! película»). No se pueden convertir por su cuenta. Por eso aquí **nada se
//! une salvo que haya EVIDENCIA** de que el tramo es un identificador:
//!
//! - **E1 correo**   — «arroba» + un punto con TLD/extensión de la lista cerrada
//! - **E2 archivo**  — el último tramo es una extensión o TLD de la lista cerrada
//! - **E3 ruta**     — dos o más «barra»/«slash»
//! - **E4 préstamo** — «underscore» o «slash»: no son palabras del español
//! - **E5 guion**    — solo si el vecino previo NO es palabra función
//!   (mata «el guion de la película», une «Micro guion Phraser»)
//! - **E6 prefijo**  — «numeral X» → `#X` · «arroba X» → `@X`
//! - **deletreo**    — 3+ nombres de letra seguidos se concatenan
//!   («hache te te pe ese» → `https`)
//! - **dos puntos**  — pega `:` a lo YA emitido y funde el puerto o la ruta
//!   («localhost dos puntos 8080» → `localhost:8080`)
//!
//! Sin evidencia, se abstiene y las palabras quedan tal cual.
//!
//! El TLD `.es` NO está en la lista a propósito: choca con el verbo «es», de
//! las palabras más comunes del idioma. Preferimos no soportar dominios `.es`
//! dictados antes que romper «el archivo es grande».

use crate::audio_toolkit::build_match_key;

/// Extensiones y TLD: lista CERRADA. Es lo que hace segura la regla del punto.
const EXT: &[&str] = &[
    "rs", "cpp", "c", "h", "py", "ts", "tsx", "js", "jsx", "json", "md", "txt", "toml", "yaml",
    "yml", "html", "css", "sh", "rb", "go", "java", "exe", "dll", "pdf", "png", "jpg", "jpeg",
    "webp", "svg", "mp3", "mp4", "wav", "zip", "gguf", "onnx", "lock", "cfg", "ini", "log", "com",
    "app", "org", "net", "io", "dev", "cl", "ai", "co", "me", "gg",
];

/// Palabras función: si una toca al conector, no es un identificador.
const FUNCION: &[&str] = &[
    "el", "la", "los", "las", "un", "una", "unos", "unas", "de", "del", "al", "a", "en", "y", "o",
    "que", "con", "por", "para", "su", "sus", "mi", "mis", "tu", "tus", "es", "son", "este",
    "esta", "ese", "esa", "lo", "le", "se", "no", "si", "como", "mas", "muy", "ya", "hay", "fue",
    "ser",
];

/// Nombres de letra deletreados (es-419).
fn letra(k: &str) -> Option<&'static str> {
    Some(match k {
        "a" => "a",
        "be" => "b",
        "ce" => "c",
        "de" => "d",
        "e" => "e",
        "efe" => "f",
        "ge" => "g",
        "hache" => "h",
        "i" => "i",
        "jota" => "j",
        "ka" => "k",
        "ele" => "l",
        "eme" => "m",
        "ene" => "n",
        "o" => "o",
        "pe" => "p",
        "cu" => "q",
        "erre" => "r",
        "ese" => "s",
        "te" => "t",
        "u" => "u",
        "uve" => "v",
        "equis" => "x",
        "ye" => "y",
        "zeta" => "z",
        _ => return None,
    })
}

fn conector(k: &str) -> Option<&'static str> {
    Some(match k {
        "punto" => ".",
        "barra" => "/",
        "slash" => "/",
        "guion" => "-",
        "underscore" => "_",
        "arroba" => "@",
        _ => return None,
    })
}

fn es_funcion(k: &str) -> bool {
    FUNCION.contains(&k)
}

/// ¿La clave nombra una pieza de deletreo? La usa también `reglas` para no
/// colapsar «te te» dentro de «hache te te pe ese» como si fuera repetición.
pub(crate) fn es_clave_deletreo(k: &str) -> bool {
    letra(k).is_some()
        || matches!(
            k,
            "barra"
                | "punto"
                | "coma"
                | "guion"
                | "arroba"
                | "numeral"
                | "underscore"
                | "slash"
                | "raya"
                | "puntos"
                | "doble"
        )
}

const PUNT: &[char] = &['.', ',', ';', ':', '!', '?', '…', ')', ']', '»', '"'];

fn nucleo(t: &str) -> &str {
    t.trim_end_matches(PUNT)
}

fn entrelazar(segs: &[String], cons: &[&str]) -> String {
    let mut s = segs[0].clone();
    for (n, c) in cons.iter().enumerate() {
        s.push_str(c);
        s.push_str(&segs[n + 1]);
    }
    s
}

/// Reescribe la puntuación dictada de identificadores. Conservador: cada rama
/// exige su evidencia y ante la duda no toca nada. Trabaja línea a línea (los
/// saltos de línea del dictado sobreviven) y una línea sin conversiones se
/// devuelve byte a byte.
pub fn normalizar_identificadores(texto: &str) -> String {
    texto
        .split('\n')
        .map(normalizar_linea)
        .collect::<Vec<_>>()
        .join("\n")
}

fn normalizar_linea(linea: &str) -> String {
    let convertida = convertir_tokens(linea);
    let sin_cambios = convertida == linea.split_whitespace().collect::<Vec<_>>().join(" ");
    if sin_cambios {
        linea.to_string()
    } else {
        convertida
    }
}

fn convertir_tokens(texto: &str) -> String {
    let toks: Vec<&str> = texto.split_whitespace().collect();
    let clave = |t: &str| build_match_key(t.trim_end_matches(PUNT));
    let sufijo = |t: &str| {
        let n = t.trim_end_matches(PUNT);
        t[n.len()..].to_string()
    };

    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < toks.len() {
        let k = clave(toks[i]);

        // ── deletreo: 3+ nombres de letra seguidos → se concatenan ───────
        if letra(&k).is_some() {
            let mut j = i;
            let mut s = String::new();
            while j < toks.len() {
                match letra(&clave(toks[j])) {
                    Some(l) => {
                        s.push_str(l);
                        j += 1;
                    }
                    None => break,
                }
                if toks[j - 1].ends_with(PUNT) {
                    break;
                }
            }
            if j - i >= 3 {
                out.push(format!("{s}{}", sufijo(toks[j - 1])));
                i = j;
                continue;
            }
        }

        // ── prefijos: «numeral X» → #X · «arroba X» → @X ──────────────────
        if (k == "numeral" || k == "arroba") && i + 1 < toks.len() {
            let sig = clave(toks[i + 1]);
            if !sig.is_empty() && !es_funcion(&sig) && conector(&sig).is_none() {
                let simbolo = if k == "numeral" { "#" } else { "@" };
                // «arroba» solo es prefijo si NO viene pegada a un identificador
                // por detrás («soporte arroba abrax» es un correo, no una mención).
                let pegado_atras = i > 0
                    && !es_funcion(&clave(toks[i - 1]))
                    && conector(&clave(toks[i - 1])).is_none();
                if k == "numeral" || !pegado_atras {
                    out.push(format!(
                        "{simbolo}{}{}",
                        nucleo(toks[i + 1]),
                        sufijo(toks[i + 1])
                    ));
                    i += 2;
                    continue;
                }
            }
        }

        // ── «dos puntos» → «:» pegado a lo YA emitido ─────────────────────
        // Mira `out`, no `toks`: el deletreo pudo haberse comido el token
        // anterior («hache te te pe ese» ya salió como «https»).
        if k == "dos" && i + 1 < toks.len() && clave(toks[i + 1]) == "puntos" {
            let anterior_util = out
                .last()
                .map(|p| {
                    let kp = build_match_key(nucleo(p));
                    !kp.is_empty() && !es_funcion(&kp)
                })
                .unwrap_or(false);
            if anterior_util {
                let suf = sufijo(toks[i + 1]);
                let prev = out.last_mut().unwrap();
                *prev = format!("{}:", nucleo(prev));
                // «://» — dos barras seguidas tras los dos puntos
                if i + 3 < toks.len()
                    && matches!(clave(toks[i + 2]).as_str(), "barra" | "slash")
                    && matches!(clave(toks[i + 3]).as_str(), "barra" | "slash")
                {
                    prev.push_str("//");
                    prev.push_str(&sufijo(toks[i + 3]));
                    i += 4;
                    continue;
                }
                // «: barra barra» al FINAL del texto también cierra el «://».
                if i + 3 == toks.len() && matches!(clave(toks[i + 2]).as_str(), "barra" | "slash") {
                    // solo una barra suelta al final: no es «://», se deja.
                }
                prev.push_str(&suf);
                i += 2;
                continue;
            }
        }

        // ── cadena word (conector word)+ ──────────────────────────────────
        if !k.is_empty() && conector(&k).is_none() && !es_funcion(&k) {
            let mut segs: Vec<String> = vec![nucleo(toks[i]).to_string()];
            let mut cons: Vec<&'static str> = Vec::new();
            let mut claves_con: Vec<String> = Vec::new();
            let mut j = i;
            let mut fin = i;
            loop {
                if j + 2 >= toks.len() + 1 || j + 1 >= toks.len() {
                    break;
                }
                let kc = clave(toks[j + 1]);
                let Some(sim) = conector(&kc) else { break };
                if j + 2 >= toks.len() {
                    break;
                }
                let ks = clave(toks[j + 2]);
                if ks.is_empty() || es_funcion(&ks) || conector(&ks).is_some() {
                    break;
                }
                cons.push(sim);
                claves_con.push(kc);
                segs.push(nucleo(toks[j + 2]).to_string());
                fin = j + 2;
                if toks[j + 2].ends_with(PUNT) {
                    break;
                }
                j += 2;
            }

            if !cons.is_empty() {
                // El ASR a veces entrega segmentos YA unidos («gmail.com»
                // como un token): la evidencia de extensión/TLD se busca en el
                // último componente tras el punto, no en el token entero.
                let ultimo_seg = &segs[segs.len() - 1];
                let ultimo = clave(ultimo_seg.rsplit('.').next().unwrap_or(ultimo_seg));
                let hay_arroba = claves_con.iter().any(|c| c == "arroba");
                let barras = claves_con
                    .iter()
                    .filter(|c| *c == "barra" || *c == "slash")
                    .count();
                let prestamo = claves_con.iter().any(|c| c == "underscore" || c == "slash");
                let solo_guiones = claves_con.iter().all(|c| c == "guion");
                let previo_funcion = i > 0 && es_funcion(&clave(toks[i - 1]));

                let e2 = EXT.contains(&ultimo.as_str());
                let e1 = hay_arroba && e2;
                let e3 = barras >= 2;
                let e5 = solo_guiones && !previo_funcion;

                if e1 || e2 || e3 || prestamo || e5 {
                    out.push(format!("{}{}", entrelazar(&segs, &cons), sufijo(toks[fin])));
                    i = fin + 1;
                    continue;
                }
            }
        }

        // ── ruta que ABRE con barra: «barra usuarios barra Antonio…» ──────
        if (k == "barra" || k == "slash") && i + 1 < toks.len() {
            let mut segs: Vec<String> = Vec::new();
            let mut j = i;
            while j + 1 < toks.len() && matches!(clave(toks[j]).as_str(), "barra" | "slash") {
                let ks = clave(toks[j + 1]);
                if ks.is_empty() || es_funcion(&ks) || conector(&ks).is_some() {
                    break;
                }
                segs.push(nucleo(toks[j + 1]).to_string());
                if toks[j + 1].ends_with(PUNT) {
                    j += 2;
                    break;
                }
                j += 2;
            }
            if segs.len() >= 2 {
                out.push(format!("/{}{}", segs.join("/"), sufijo(toks[j - 1])));
                i = j;
                continue;
            }
        }

        out.push(toks[i].to_string());
        i += 1;
    }

    // Lo que sigue a «X:» va pegado: una ruta («C:/…») o un puerto
    // («localhost:8080»). Sin esto quedaría «localhost: 8080».
    let mut fusion: Vec<String> = Vec::new();
    for t in out {
        if let Some(prev) = fusion.last_mut() {
            let pega = prev.ends_with(':')
                && (t.starts_with('/') || t.chars().next().is_some_and(|c| c.is_numeric()));
            if pega {
                prev.push_str(&t);
                continue;
            }
        }
        fusion.push(t);
    }
    fusion.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn archivos_y_dominios() {
        assert_eq!(
            normalizar_identificadores("revisaremos ABRAX, Handy y llama punto cpp."),
            "revisaremos ABRAX, Handy y llama.cpp."
        );
        assert_eq!(
            normalizar_identificadores("el archivo se llama ProcessingEngine punto rs."),
            "el archivo se llama ProcessingEngine.rs."
        );
        assert_eq!(
            normalizar_identificadores("la página es abrax punto app."),
            "la página es abrax.app."
        );
    }

    #[test]
    fn correos() {
        assert_eq!(
            normalizar_identificadores("mi correo es antonio punto prueba arroba gmail punto com."),
            "mi correo es antonio.prueba@gmail.com."
        );
        assert_eq!(
            normalizar_identificadores("mándalo a soporte arroba abrax punto app."),
            "mándalo a soporte@abrax.app."
        );
    }

    #[test]
    fn rutas() {
        assert_eq!(
            normalizar_identificadores("está en barra usuarios barra Antonio barra documentos."),
            "está en /usuarios/Antonio/documentos."
        );
        assert_eq!(
            normalizar_identificadores(
                "la ruta es C dos puntos barra usuarios barra Antonio barra descargas."
            ),
            "la ruta es C:/usuarios/Antonio/descargas."
        );
        assert_eq!(
            normalizar_identificadores("visita github punto com slash cjpais slash handy."),
            "visita github.com/cjpais/handy."
        );
    }

    #[test]
    fn snake_case_guiones_y_prefijos() {
        assert_eq!(
            normalizar_identificadores("el nombre es abrax underscore demo underscore final."),
            "el nombre es abrax_demo_final."
        );
        assert_eq!(
            normalizar_identificadores("abre la carpeta src guion tauri guion src."),
            "abre la carpeta src-tauri-src."
        );
        assert_eq!(
            normalizar_identificadores("añade el término Micro guion Phraser al diccionario."),
            "añade el término Micro-Phraser al diccionario."
        );
        assert_eq!(
            normalizar_identificadores("el hashtag es numeral ABRAX."),
            "el hashtag es #ABRAX."
        );
        assert_eq!(
            normalizar_identificadores("menciona a arroba Antonio en el mensaje."),
            "menciona a @Antonio en el mensaje."
        );
    }

    #[test]
    fn deletreo_y_dos_puntos() {
        assert_eq!(
            normalizar_identificadores(
                "el enlace comienza con hache te te pe ese dos puntos barra barra."
            ),
            "el enlace comienza con https://."
        );
        assert_eq!(
            normalizar_identificadores("el endpoint local es localhost dos puntos 8080."),
            "el endpoint local es localhost:8080."
        );
    }

    #[test]
    fn habla_normal_jamas_se_toca() {
        for t in [
            "ponle un punto final",
            "la barra del bar",
            "el punto de partida",
            "escribió el guion de la película",
            "un guion largo",
            "punto y aparte",
            "de barra en barra",
            "el numeral es otro",
            "estaba en coma",
            "hasta cierto punto",
            "la barra de progreso",
            "dos puntos de vista",
            "de punto a punto",
            "subimos por la barra",
        ] {
            assert_eq!(normalizar_identificadores(t), t, "no debía tocar «{t}»");
        }
    }

    #[test]
    fn segmentos_ya_unidos_por_el_asr() {
        // Whisper a veces entrega «gmail.com» como un solo token: la arroba
        // dictada debe unirse igual.
        assert_eq!(
            normalizar_identificadores("mi correo es antonio.prueba arroba gmail.com"),
            "mi correo es antonio.prueba@gmail.com"
        );
    }

    #[test]
    fn punto_es_no_es_dominio() {
        // «.es» quedó fuera de la lista a propósito: «es» es el verbo.
        let t = "el archivo es grande y la página es lenta";
        assert_eq!(normalizar_identificadores(t), t);
    }

    #[test]
    fn idempotente() {
        for t in [
            "mi correo es antonio punto prueba arroba gmail punto com.",
            "la ruta es C dos puntos barra usuarios barra Antonio barra descargas.",
            "el enlace comienza con hache te te pe ese dos puntos barra barra.",
        ] {
            let una = normalizar_identificadores(t);
            assert_eq!(normalizar_identificadores(&una), una);
        }
    }
}
