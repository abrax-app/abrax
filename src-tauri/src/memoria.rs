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
/// 0.5 admite las correcciones reales de ortografía/oído («taury»→«Tauri»,
/// «portavapeles»→«portapapeles») y rechaza sustituciones de contenido entre
/// palabras distintas («lunes»→«martes» da 0.67) — hallado en revisión.
const SIMILITUD_MAX: f64 = 0.5;

/// Un par aprendido: cuando el dictado produzca `de`, escribir `a`.
#[derive(Serialize, Deserialize, Clone, PartialEq, Type)]
pub struct ParMemoria {
    pub de: String,
    pub a: String,
    /// Veces que el usuario confirmó esta corrección (re-aprendizajes).
    #[serde(default = "una")]
    pub veces: u32,
}

/// Los pares provienen de dictados del usuario: por la regla de privacidad
/// (no loguear contenido sensible) el Debug de AppSettings no debe volcarlos.
impl std::fmt::Debug for ParMemoria {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ParMemoria(«redactado», veces: {})", self.veces)
    }
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
    // insertados) = candidato a sustitución, con sus anclas vecinas (la
    // palabra igual anterior y la siguiente) para el rescate contextual.
    let mut pares = Vec::new();
    let (mut del, mut ins): (Vec<&str>, Vec<&str>) = (Vec::new(), Vec::new());
    let mut ancla_previa: Option<&str> = None;
    let (mut i, mut j) = (0usize, 0usize);
    loop {
        let igual = i < n && j < m && a[i] == b[j];
        if igual || (i == n && j == m) {
            let ancla_siguiente = if igual { Some(a[i]) } else { None };
            registrar_candidato(&mut pares, &del, &ins, ancla_previa, ancla_siguiente);
            del.clear();
            ins.clear();
            if i == n && j == m {
                break;
            }
            ancla_previa = Some(a[i]);
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

fn registrar_candidato(
    pares: &mut Vec<(String, String)>,
    del: &[&str],
    ins: &[&str],
    ancla_previa: Option<&str>,
    ancla_siguiente: Option<&str>,
) {
    if del.is_empty() || ins.is_empty() {
        return; // inserción o borrado puro: contenido, no corrección
    }
    // Dos correcciones adyacentes sin ancla entre medio («taury vite» →
    // «Tauri Vite») llegan como UNA región: si los lados quedan alineados
    // 1:1 y CADA sustitución pasa las puertas por sí sola, se aprenden como
    // pares independientes (aplican también por separado) — hallado en
    // revisión. Si alguna no pasa, se cae a la evaluación por frase.
    if del.len() == ins.len() && del.len() > 1 {
        let individuales: Vec<(String, String)> = del
            .iter()
            .zip(ins.iter())
            .filter(|(d, i)| d != i)
            .filter_map(|(d, i)| pasa_puertas(&[d], &[i]))
            .collect();
        let distintos = del.iter().zip(ins.iter()).filter(|(d, i)| d != i).count();
        if !individuales.is_empty() && individuales.len() == distintos {
            pares.extend(individuales);
            return;
        }
    }
    if let Some(par) = pasa_puertas(del, ins) {
        pares.push(par);
        return;
    }
    // Rescate contextual («me borré» → «no borré»): una sustitución 1:1 de
    // PALABRITAS (ambos lados con clave ≤ 3) no puede aprenderse sola
    // (reemplazar cada «me» sería veneno) pero sí como frase con su palabra
    // ancla vecina — dispara únicamente en ese contexto exacto. Es la clase
    // real de confusiones del oído del motor: me/no, te/té, si/sí, la/las.
    // Un lado largo («que» → «cuando») es edición de contenido y NO se
    // rescata: el ancla no debe diluir esa puerta.
    if del.len() == 1 && ins.len() == 1 {
        let corta = |w: &str| {
            let n = build_match_key(w).chars().count();
            (1..=3).contains(&n)
        };
        if !corta(del[0]) || !corta(ins[0]) {
            return;
        }
        let ancla_util = |w: &str| build_match_key(w).chars().count() >= CLAVE_MIN;
        if let Some(sig) = ancla_siguiente.filter(|w| ancla_util(w)) {
            if let Some(par) = pasa_puertas(&[del[0], sig], &[ins[0], sig]) {
                pares.push(par);
                return;
            }
        }
        if let Some(prev) = ancla_previa.filter(|w| ancla_util(w)) {
            if let Some(par) = pasa_puertas(&[prev, del[0]], &[prev, ins[0]]) {
                pares.push(par);
            }
        }
    }
}

/// Evalúa un candidato (borrados, insertados) contra todas las puertas de
/// seguridad; devuelve el par si es una corrección aprendible.
fn pasa_puertas(del: &[&str], ins: &[&str]) -> Option<(String, String)> {
    if del.len() > MAX_TOKENS_LADO || ins.len() > MAX_TOKENS_LADO {
        return None; // edición larga: contenido, no corrección
    }
    let de = del.join(" ");
    let a = ins.join(" ");
    if de == a {
        return None;
    }
    let clave_de = build_match_key(&de);
    let clave_a = build_match_key(&a);
    if clave_de.chars().count() < CLAVE_MIN {
        return None; // origen demasiado corto/ambiguo para reemplazo global
    }
    if del.iter().all(|w| is_stopword(&build_match_key(w))) {
        // Origen hecho SOLO de palabras comunes («que», «por qué», «si no»):
        // reemplazarlo globalmente es veneno, y además sería irreversible por
        // edición (la corrección inversa también caería en esta puerta). Si el
        // usuario de verdad lo quiere, tiene los Reemplazos manuales.
        return None;
    }
    if clave_de.chars().all(|c| c.is_ascii_digit()) || clave_a.chars().all(|c| c.is_ascii_digit()) {
        // Cifras («100»→«1000») son contenido, nunca ortografía del motor:
        // aprenderlas reescribiría todo número futuro — hallado en revisión.
        return None;
    }
    if clave_de != clave_a {
        let distancia = levenshtein(&clave_de, &clave_a) as f64;
        let largo = clave_de.chars().count().max(clave_a.chars().count()) as f64;
        if largo == 0.0 || distancia / largo > SIMILITUD_MAX {
            return None; // sin parecido: el usuario reescribió la idea, no corrigió
        }
    }
    Some((de, a))
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
        // Gesto de DES-aprendizaje: si el candidato es exactamente el inverso
        // de un par existente, el usuario está deshaciendo nuestra corrección
        // (la memoria puso «hola», él volvió a escribir «helo»). Se retira el
        // par y NO se aprende el reverso: aprender «hola → helo» tras el
        // deshacer envenenaría una palabra común — hallado por el usuario.
        // Si de verdad quiere el reverso, otra corrección posterior (ya sin
        // el par presente) lo enseña por el camino normal.
        if let Some(pos) = lista.iter().position(|p| p.de == a && p.a == de) {
            lista.remove(pos);
            continue;
        }
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
    fn no_aprende_frases_de_solo_palabras_comunes() {
        // «por qué» → «porque»: claves normalizadas idénticas y ambos tokens
        // en la stoplist — sin esta puerta se aprendería un par global
        // venenoso e irreversible por edición (hallado en revisión).
        assert!(aprender_de_edicion("no sé por qué vino", "no sé porque vino").is_empty());
        assert!(aprender_de_edicion("dime si no llega", "dime sino llega").is_empty());
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
    fn no_aprende_sustituciones_de_contenido() {
        // Reagendar no es corregir al motor: lev(lunes, martes) = 0.67 > 0.5.
        assert!(
            aprender_de_edicion("la reunión es el lunes", "la reunión es el martes").is_empty()
        );
    }

    #[test]
    fn no_aprende_cifras() {
        assert!(aprender_de_edicion("cuesta 100 pesos", "cuesta 1000 pesos").is_empty());
    }

    #[test]
    fn correcciones_adyacentes_se_aprenden_por_separado() {
        // Sin ancla entre medio, el diff las agrupa: deben salir como pares
        // independientes para aplicar también por separado.
        let pares = aprender_de_edicion("instala taury vite ahora", "instala Tauri Vite ahora");
        assert_eq!(
            pares,
            vec![
                ("taury".to_string(), "Tauri".to_string()),
                ("vite".to_string(), "Vite".to_string())
            ]
        );
    }

    #[test]
    fn rescata_palabra_comun_con_su_ancla_siguiente() {
        // Caso real reportado: dijo «no borré», el motor puso «me borré».
        // «me»→«no» solo sería veneno; con el ancla queda contextual y seguro.
        let pares = aprender_de_edicion(
            "dije que me borré el archivo ayer",
            "dije que no borré el archivo ayer",
        );
        assert_eq!(
            pares,
            vec![("me borré".to_string(), "no borré".to_string())]
        );
    }

    #[test]
    fn rescata_palabra_corta_con_su_ancla_previa() {
        // Corrección al final del texto: no hay ancla siguiente, se usa la
        // previa («tomar te» → «tomar té»).
        let pares = aprender_de_edicion("quiero tomar te", "quiero tomar té");
        assert_eq!(
            pares,
            vec![("tomar te".to_string(), "tomar té".to_string())]
        );
    }

    #[test]
    fn el_rescate_no_reabre_las_ediciones_de_contenido() {
        // «lunes»→«martes» está vetado por similitud (contenido), y el ancla
        // NO debe diluir esa puerta.
        assert!(aprender_de_edicion(
            "la reunión es el lunes temprano",
            "la reunión es el martes temprano"
        )
        .is_empty());
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
    fn revertir_una_correccion_la_desaprende_sin_ensenar_el_reverso() {
        // Escenario del usuario: la memoria tiene «helo → hola»; él deshace
        // la corrección en su texto (borra «hola», escribe «helo»). El diff
        // produce el candidato inverso (hola → helo): el par debe RETIRARSE y
        // el reverso NO debe aprenderse (envenenaría «hola» para siempre).
        let mut lista = Vec::new();
        incorporar(vec![("helo".into(), "hola".into())], &mut lista);
        let tocados = incorporar(vec![("hola".into(), "helo".into())], &mut lista);
        assert!(lista.is_empty(), "el par debió des-aprenderse: {lista:?}");
        assert!(tocados.is_empty(), "deshacer no es aprender");
    }

    #[test]
    fn tras_desaprender_se_puede_ensenar_el_reverso_de_verdad() {
        // Segunda intención explícita: sin el par presente, la corrección
        // inversa sí se aprende por el camino normal.
        let mut lista = Vec::new();
        incorporar(vec![("helo".into(), "hola".into())], &mut lista);
        incorporar(vec![("hola".into(), "helo".into())], &mut lista); // deshace
        incorporar(vec![("hola".into(), "helo".into())], &mut lista); // enseña
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].de, "hola");
        assert_eq!(lista[0].a, "helo");
    }

    #[test]
    fn incorporar_retira_el_par_con_mismo_destino_sin_ser_inverso_exacto() {
        // La regla anti-cadenas sigue viva para el caso NO inverso: al enseñar
        // «Roth → Ruth», el viejo «Ruth → Root» se retira (su origen coincide
        // con el destino nuevo y encadenaría Roth→Ruth→Root); no es el gesto
        // de deshacer porque los pares no son inversos exactos.
        let mut lista = Vec::new();
        incorporar(vec![("Ruth".into(), "Root".into())], &mut lista);
        incorporar(vec![("Roth".into(), "Ruth".into())], &mut lista);
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].de, "Roth");
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
