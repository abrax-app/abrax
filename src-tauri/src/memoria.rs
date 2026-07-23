//! Memoria de correcciones: ABRAX aprende de las ediciones del usuario.
//!
//! Cuando el usuario corrige una transcripción en el Historial (borró una o
//! dos palabras y escribió las correctas), se hace un diff por tokens entre el
//! texto original y el editado, y las sustituciones que pasan las puertas de
//! seguridad se guardan como pares `de → a`. Los dictados siguientes las
//! aplican como reemplazo EXACTO por frase (case-sensitive, sin fuzzy) en el
//! post-proceso, antes de las capas difusas.
//!
//! Puertas de seguridad — qué NO se aprende:
//! - Inserciones o borrados puros (agregar/quitar contenido no es corregir).
//! - Frases de más de [`MAX_TOKENS_LADO`] tokens por lado (edición de
//!   contenido, no corrección puntual).
//! - Palabras comunes es/en como origen (la stoplist del Diccionario): un
//!   par «de → que» corrompería todos los dictados futuros.
//! - Pares sin parecido (similitud Levenshtein sobre claves normalizadas):
//!   reescribir una idea no es corregir al motor de STT.
//!
//! Todo vive en los ajustes del usuario (local), con tope de [`MAX_PARES`]
//! pares: los más recientes al frente, lo excedente se descarta por la cola.

use serde::{Deserialize, Serialize};
use specta::Type;
use strsim::levenshtein;

use crate::audio_toolkit::{build_match_key, token_core};
use crate::dictionary::is_stopword;

/// Tope de pares aprendidos; lo excedente cae por la cola (los últimos
/// aprendidos van al frente).
pub const MAX_PARES: usize = 200;
/// Máximo de tokens por lado de una sustitución para considerarla corrección.
const MAX_TOKENS_LADO: usize = 3;
/// Un texto más largo que esto no se diffea (cota del costo O(n·m) del LCS).
const MAX_TOKENS_TEXTO: usize = 1200;
/// Largo mínimo de la clave normalizada del origen: claves de 1–2 caracteres
/// («a», «el») son demasiado ambiguas para reemplazo global.
const CLAVE_MIN: usize = 3;
/// Distancia Levenshtein normalizada máxima entre claves para aceptar el par.
const SIMILITUD_MAX: f64 = 0.75;

/// Un par aprendido: cuando el dictado produzca `de`, escribir `a`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Type)]
pub struct ParMemoria {
    pub de: String,
    pub a: String,
    /// Veces que el usuario confirmó esta corrección (re-aprendizajes).
    #[serde(default = "una")]
    pub veces: u32,
}

fn una() -> u32 {
    1
}

/// Diff por tokens entre la transcripción original y la editada; devuelve las
/// sustituciones que pasan las puertas de seguridad, como `(de, a)` con los
/// núcleos de token (sin puntuación adyacente) unidos por espacio.
pub fn aprender_de_edicion(original: &str, editado: &str) -> Vec<(String, String)> {
    let a: Vec<&str> = original
        .split_whitespace()
        .map(token_core)
        .filter(|w| !w.is_empty())
        .collect();
    let b: Vec<&str> = editado
        .split_whitespace()
        .map(token_core)
        .filter(|w| !w.is_empty())
        .collect();
    if a.is_empty() || b.is_empty() || a.len() > MAX_TOKENS_TEXTO || b.len() > MAX_TOKENS_TEXTO {
        return Vec::new();
    }

    // LCS clásico por programación dinámica (dp[i][j] = LCS de a[i..], b[j..]).
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u16; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] {
                dp[i + 1][j + 1] + 1
            } else {
                dp[i + 1][j].max(dp[i][j + 1])
            };
        }
    }

    // Recorrido completo del diff (colas incluidas): cada región contigua de
    // desacuerdo entre dos anclas de igualdad se agrupa en (borrados,
    // insertados) = candidato a sustitución.
    let mut pares = Vec::new();
    let (mut del, mut ins): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        let igual = i < n && j < m && a[i] == b[j];
        if igual || (i == n && j == m) {
            registrar_candidato(&mut pares, &del, &ins);
            del.clear();
            ins.clear();
            if i == n && j == m {
                break;
            }
            i += 1;
            j += 1;
        } else if j == m || (i < n && dp[i + 1][j] >= dp[i][j + 1]) {
            del.push(a[i]);
            i += 1;
        } else {
            ins.push(b[j]);
            j += 1;
        }
    }

    pares
}

fn registrar_candidato(pares: &mut Vec<(String, String)>, del: &[&str], ins: &[&str]) {
    if del.is_empty() || ins.is_empty() {
        return; // inserción o borrado puro: contenido, no corrección
    }
    if del.len() > MAX_TOKENS_LADO || ins.len() > MAX_TOKENS_LADO {
        return; // edición larga: contenido, no corrección
    }
    let de = del.join(" ");
    let a = ins.join(" ");
    if de == a {
        return;
    }
    let clave_de = build_match_key(&de);
    let clave_a = build_match_key(&a);
    if clave_de.chars().count() < CLAVE_MIN {
        return; // origen demasiado corto/ambiguo para reemplazo global
    }
    if del.len() == 1 && is_stopword(&build_match_key(del[0])) {
        return; // «de», «que», «the»…: reemplazarlas globalmente es veneno
    }
    if clave_de != clave_a {
        let distancia = levenshtein(&clave_de, &clave_a) as f64;
        let largo = clave_de.chars().count().max(clave_a.chars().count()) as f64;
        if largo == 0.0 || distancia / largo > SIMILITUD_MAX {
            return; // sin parecido: el usuario reescribió la idea, no corrigió
        }
    }
    pares.push((de, a));
}

/// Incorpora pares recién aprendidos a la lista persistida y devuelve los
/// pares resultantes que cambiaron (para mostrarlos al usuario).
///
/// Reglas: el mismo `de` se re-aprende (misma `a` suma `veces`; distinta `a`
/// la reemplaza — lo último que confirmó el usuario gana). Si el usuario
/// enseña `X → Y`, cualquier par viejo con `de == Y` se retira (acaba de
/// declarar lo contrario: evita el ping-pong). Los recientes van al frente y
/// la lista se trunca a [`MAX_PARES`].
pub fn incorporar(nuevos: Vec<(String, String)>, lista: &mut Vec<ParMemoria>) -> Vec<ParMemoria> {
    let mut tocados = Vec::new();
    for (de, a) in nuevos {
        lista.retain(|p| p.de != a);
        if let Some(pos) = lista.iter().position(|p| p.de == de) {
            let mut par = lista.remove(pos);
            if par.a == a {
                par.veces = par.veces.saturating_add(1);
            } else {
                par.a = a;
                par.veces = 1;
            }
            tocados.push(par.clone());
            lista.insert(0, par);
        } else {
            let par = ParMemoria { de, a, veces: 1 };
            tocados.push(par.clone());
            lista.insert(0, par);
        }
    }
    lista.truncate(MAX_PARES);
    tocados
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aprende_una_palabra_corregida() {
        let pares = aprender_de_edicion(
            "abre el proyecto en ábrax ahora",
            "abre el proyecto en Abrax ahora",
        );
        assert_eq!(pares, vec![("ábrax".to_string(), "Abrax".to_string())]);
    }

    #[test]
    fn aprende_frase_de_dos_palabras() {
        let pares = aprender_de_edicion(
            "usa el use auth store del proyecto",
            "usa el useAuthStore del proyecto",
        );
        assert_eq!(
            pares,
            vec![("use auth store".to_string(), "useAuthStore".to_string())]
        );
    }

    #[test]
    fn no_aprende_insercion_pura() {
        assert!(aprender_de_edicion("hola mundo", "hola querido mundo").is_empty());
    }

    #[test]
    fn no_aprende_borrado_puro() {
        assert!(aprender_de_edicion("hola querido mundo", "hola mundo").is_empty());
    }

    #[test]
    fn no_aprende_palabras_comunes() {
        // «que» está en la stoplist: reemplazarla globalmente sería veneno.
        assert!(aprender_de_edicion("dijo que vendría", "dijo cuando vendría").is_empty());
    }

    #[test]
    fn no_aprende_reescrituras_sin_parecido() {
        assert!(aprender_de_edicion(
            "manda el archivo al servidor",
            "manda el paquete al servidor"
        )
        .is_empty());
    }

    #[test]
    fn no_aprende_cambios_solo_de_puntuacion() {
        assert!(aprender_de_edicion("hola, mundo.", "hola mundo").is_empty());
    }

    #[test]
    fn aprende_cambio_de_mayusculas_y_tildes() {
        let pares = aprender_de_edicion("hablamos de github hoy", "hablamos de GitHub hoy");
        assert_eq!(pares, vec![("github".to_string(), "GitHub".to_string())]);
    }

    #[test]
    fn aprende_varias_correcciones_en_una_edicion() {
        let pares = aprender_de_edicion(
            "instala taury y corre el vite",
            "instala Tauri y corre el Vite",
        );
        assert_eq!(
            pares,
            vec![
                ("taury".to_string(), "Tauri".to_string()),
                ("vite".to_string(), "Vite".to_string())
            ]
        );
    }

    #[test]
    fn incorporar_suma_veces_al_reaprender() {
        let mut lista = Vec::new();
        incorporar(vec![("ábrax".into(), "Abrax".into())], &mut lista);
        incorporar(vec![("ábrax".into(), "Abrax".into())], &mut lista);
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].veces, 2);
    }

    #[test]
    fn incorporar_lo_ultimo_gana_con_a_distinta() {
        let mut lista = Vec::new();
        incorporar(vec![("taury".into(), "Taury".into())], &mut lista);
        incorporar(vec![("taury".into(), "Tauri".into())], &mut lista);
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].a, "Tauri");
        assert_eq!(lista[0].veces, 1);
    }

    #[test]
    fn incorporar_retira_el_par_inverso() {
        let mut lista = Vec::new();
        incorporar(vec![("Ruth".into(), "Root".into())], &mut lista);
        // El usuario ahora enseña lo contrario: el par viejo debe retirarse.
        incorporar(vec![("Root".into(), "Ruth".into())], &mut lista);
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].de, "Root");
        assert_eq!(lista[0].a, "Ruth");
    }

    #[test]
    fn incorporar_respeta_el_tope() {
        let mut lista = Vec::new();
        for i in 0..(MAX_PARES + 30) {
            incorporar(
                vec![(format!("palabra{i}"), format!("Palabra{i}"))],
                &mut lista,
            );
        }
        assert_eq!(lista.len(), MAX_PARES);
        // Los más recientes quedan al frente.
        assert_eq!(lista[0].de, format!("palabra{}", MAX_PARES + 29));
    }
}
