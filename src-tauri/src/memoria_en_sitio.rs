//! Memoria en el sitio: aprender de las correcciones hechas DONDE se dicta.
//!
//! El flujo cómodo que pide el producto: el usuario dicta, ABRAX tipea, el
//! usuario corrige una palabra ahí mismo (en su editor) y sigue. Al EMPEZAR el
//! siguiente dictado, ABRAX relee el campo de texto enfocado vía la API de
//! accesibilidad del sistema (UI Automation en Windows; en otras plataformas
//! esta vía se degrada a no-op y queda el camino del Historial), localiza lo
//! que él mismo tipeó la última vez y, si el usuario lo corrigió, aprende la
//! diferencia con las MISMAS puertas de seguridad de la Memoria.
//!
//! Privacidad (regla del producto: procesamiento local, nada de loguear
//! contenido): solo se lee el campo enfocado en el instante en que el usuario
//! ya decidió dictar AHÍ; solo se compara contra el último texto que ABRAX
//! tipeó; nada del campo se persiste ni se loguea — únicamente los pares
//! aprendidos (que ya viven redactados en logs). Lectura acotada y con
//! vigencia: si pasó mucho tiempo o el foco está en otra aplicación, no se
//! hace nada.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use log::{debug, info};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tauri_specta::Event;

use crate::audio_toolkit::build_match_key;
use crate::memoria::{self, ParMemoria};
use crate::settings::{get_settings, write_settings};

/// Vigencia del último dictado para el aprendizaje en el sitio: pasado esto,
/// el campo puede haber cambiado demasiado como para atribuir la edición.
const VIGENCIA: Duration = Duration::from_secs(10 * 60);
/// Coincidencia mínima (razón de LCS por claves) para aceptar que una ventana
/// del campo ES el dictado anterior editado, y no otro texto.
const MIN_COINCIDENCIA: f64 = 0.5;
/// Cota de palabras del campo leído (campos enormes no se rastrillan).
const MAX_PALABRAS_CAMPO: usize = 4000;
/// Máximo de caracteres pedidos al control enfocado.
#[cfg(windows)]
const MAX_CHARS_CAMPO: i32 = 40_000;

/// Último texto tipeado por ABRAX y dónde (pid de la ventana en foco al pegar).
struct UltimoDictado {
    texto: String,
    proceso: u32,
    cuando: Instant,
}

static ULTIMO: Mutex<Option<UltimoDictado>> = Mutex::new(None);

/// Evento hacia la UI cuando el aprendizaje en el sitio suma pares (para el
/// toast «ABRAX aprendió …»).
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MemoriaAprendida {
    pub pares: Vec<ParMemoria>,
}

/// Registra el texto recién tipeado y la app en foco. Llamar tras un pegado
/// exitoso.
pub fn registrar_dictado(texto: &str) {
    let texto = texto.trim();
    if texto.is_empty() {
        return;
    }
    let proceso = pid_ventana_en_foco();
    *ULTIMO.lock().unwrap() = Some(UltimoDictado {
        texto: texto.to_string(),
        proceso,
        cuando: Instant::now(),
    });
}

/// Al empezar un dictado: relee el campo enfocado y aprende de la diferencia
/// con el último texto tipeado. Diseñado para correr en un hilo aparte (la
/// lectura por accesibilidad usa COM y puede tardar unos milisegundos).
pub fn aprender_del_campo(app: &AppHandle) {
    let settings = get_settings(app);
    if !settings.memoria_activa || !settings.memoria_en_sitio {
        return;
    }
    let (texto_previo, proceso_previo) = {
        let guardia = ULTIMO.lock().unwrap();
        match guardia.as_ref() {
            Some(u) if u.cuando.elapsed() <= VIGENCIA => (u.texto.clone(), u.proceso),
            _ => return,
        }
    };
    // Mismo destino: si el foco está en otra aplicación, la edición no es
    // atribuible al dictado anterior.
    if proceso_previo == 0 || pid_ventana_en_foco() != proceso_previo {
        return;
    }
    let Some(campo) = leer_texto_enfocado() else {
        return;
    };
    let pares = comparar_con_campo(&texto_previo, &campo);
    if pares.is_empty() {
        return;
    }
    let mut s = get_settings(app);
    let tocados = memoria::incorporar(pares, &mut s.memoria_correcciones);
    write_settings(app, s);
    info!(
        "Memoria en el sitio: {} corrección(es) aprendida(s) del campo enfocado",
        tocados.len()
    );
    let _ = MemoriaAprendida { pares: tocados }.emit(app);
}

/// Compara el texto tipeado con el contenido actual del campo: busca la
/// ventana de palabras que mejor coincide (LCS sobre claves normalizadas, con
/// largo ±2) y, si la coincidencia es suficiente, aprende del diff. El texto
/// intacto (fast-path por substring) no aprende nada.
pub fn comparar_con_campo(tipeado: &str, campo: &str) -> Vec<(String, String)> {
    let tipeado = tipeado.trim();
    if tipeado.is_empty() || campo.contains(tipeado) {
        return Vec::new();
    }
    let obj: Vec<&str> = tipeado.split_whitespace().collect();
    let n = obj.len();
    let palabras: Vec<&str> = campo.split_whitespace().collect();
    if n == 0 || palabras.is_empty() || palabras.len() > MAX_PALABRAS_CAMPO {
        return Vec::new();
    }
    let claves_obj: Vec<String> = obj.iter().map(|w| build_match_key(w)).collect();
    let claves_campo: Vec<String> = palabras.iter().map(|w| build_match_key(w)).collect();

    let mut mejor: Option<(f64, usize, usize)> = None; // (razón, inicio, largo)
    for delta in -2isize..=2 {
        let m = n as isize + delta;
        if m < 1 || m as usize > palabras.len() {
            continue;
        }
        let m = m as usize;
        for i in 0..=palabras.len() - m {
            let razon = lcs_len(&claves_obj, &claves_campo[i..i + m]) as f64 / n.max(m) as f64;
            if mejor.is_none_or(|(r, _, _)| razon > r) {
                mejor = Some((razon, i, m));
            }
        }
    }
    let Some((razon, i, m)) = mejor else {
        return Vec::new();
    };
    if razon < MIN_COINCIDENCIA {
        return Vec::new(); // el dictado anterior ya no está reconocible en el campo
    }
    memoria::aprender_de_edicion(tipeado, &palabras[i..i + m].join(" "))
}

fn lcs_len(a: &[String], b: &[String]) -> usize {
    let (n, m) = (a.len(), b.len());
    let mut fila = vec![0usize; m + 1];
    for i in (0..n).rev() {
        let mut diagonal = 0usize; // dp[i+1][j+1]
        for j in (0..m).rev() {
            let abajo = fila[j]; // dp[i+1][j]
            fila[j] = if a[i] == b[j] {
                diagonal + 1
            } else {
                abajo.max(fila[j + 1])
            };
            diagonal = abajo;
        }
    }
    fila[0]
}

#[cfg(windows)]
fn pid_ventana_en_foco() -> u32 {
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return 0;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        pid
    }
}

#[cfg(not(windows))]
fn pid_ventana_en_foco() -> u32 {
    0
}

/// Texto del control enfocado vía UI Automation (TextPattern con fallback a
/// ValuePattern). Best-effort: cualquier fallo devuelve None y no se aprende.
#[cfg(windows)]
fn leer_texto_enfocado() -> Option<String> {
    use windows::core::Interface;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Accessibility::{
        CUIAutomation, IUIAutomation, IUIAutomationTextPattern, IUIAutomationValuePattern,
        UIA_TextPatternId, UIA_ValuePatternId,
    };

    unsafe {
        // Puede venir ya inicializado en este hilo; ambos casos sirven.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let auto: IUIAutomation = CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)
            .map_err(|e| debug!("UIA no disponible: {e}"))
            .ok()?;
        let elemento = auto.GetFocusedElement().ok()?;
        if let Ok(patron) = elemento.GetCurrentPattern(UIA_TextPatternId) {
            if let Ok(texto) = patron.cast::<IUIAutomationTextPattern>() {
                if let Ok(rango) = texto.DocumentRange() {
                    if let Ok(bstr) = rango.GetText(MAX_CHARS_CAMPO) {
                        return Some(bstr.to_string());
                    }
                }
            }
        }
        if let Ok(patron) = elemento.GetCurrentPattern(UIA_ValuePatternId) {
            if let Ok(valor) = patron.cast::<IUIAutomationValuePattern>() {
                if let Ok(bstr) = valor.CurrentValue() {
                    return Some(bstr.to_string());
                }
            }
        }
        None
    }
}

#[cfg(not(windows))]
fn leer_texto_enfocado() -> Option<String> {
    // Fase Windows primero; macOS (AXUIElement) y AT-SPI quedan documentados
    // como siguiente paso. El camino del Historial funciona en todas partes.
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn campo_intacto_no_aprende() {
        let t = "el portapapeles crash";
        assert!(comparar_con_campo(t, "hola\nel portapapeles crash\nchao").is_empty());
    }

    #[test]
    fn correccion_en_el_campo_se_aprende() {
        // ABRAX tipeó «portavapeles»; el usuario lo corrigió ahí mismo, entre
        // más texto suyo.
        let pares = comparar_con_campo(
            "el portavapeles crash",
            "notas del día\nrevisar: el portapapeles crash cuando pego\nfin",
        );
        assert_eq!(
            pares,
            vec![("portavapeles".to_string(), "portapapeles".to_string())]
        );
    }

    #[test]
    fn correccion_de_mayusculas_se_aprende() {
        let pares = comparar_con_campo("hablamos de github hoy", "ayer hablamos de GitHub hoy si");
        assert_eq!(pares, vec![("github".to_string(), "GitHub".to_string())]);
    }

    #[test]
    fn campo_sin_el_dictado_no_aprende() {
        assert!(comparar_con_campo(
            "el portavapeles crash",
            "un documento totalmente distinto sin relación alguna"
        )
        .is_empty());
    }

    #[test]
    fn campo_enorme_no_se_rastrilla() {
        let campo = "palabra ".repeat(MAX_PALABRAS_CAMPO + 10);
        assert!(comparar_con_campo("el portavapeles crash", &campo).is_empty());
    }

    #[test]
    fn borrar_una_palabra_del_dictado_no_aprende() {
        // El usuario quitó una palabra (contenido): inserciones/borrados puros
        // no son correcciones — las puertas de la memoria aplican igual aquí.
        assert!(
            comparar_con_campo("el portapapeles crash feo", "el portapapeles crash").is_empty()
        );
    }
}
