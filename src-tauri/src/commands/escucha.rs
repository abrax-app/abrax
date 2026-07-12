//! [ESCUCHA] Comandos Tauri de la sección Escucha (lectura en voz alta).
//!
//! El frontend maneja la cola de oraciones: `escucha_speak` habla UN trozo
//! (interrumpiendo el anterior), y el panel espera con `escucha_status` a que
//! termine para avanzar el resaltado y pedir el siguiente.

use crate::managers::escucha::preproceso::{
    preprocesar, ModoLectura, OracionHablable, VerbosidadSimbolos,
};
use crate::managers::escucha::{EscuchaManager, EstadoEscucha, VozEscucha};
use std::sync::Arc;
use tauri::{AppHandle, State};

#[tauri::command]
#[specta::specta]
pub fn escucha_list_voices(
    manager: State<'_, Arc<EscuchaManager>>,
) -> Result<Vec<VozEscucha>, String> {
    manager.list_voices()
}

/// `rate` es un multiplicador de velocidad (1.0 = normal); ver
/// `managers::escucha::map_rate` para el mapeo al rango nativo del backend.
#[tauri::command]
#[specta::specta]
pub fn escucha_speak(
    manager: State<'_, Arc<EscuchaManager>>,
    texto: String,
    voz_id: Option<String>,
    rate: Option<f32>,
) -> Result<(), String> {
    manager.speak(texto, voz_id, rate)
}

#[tauri::command]
#[specta::specta]
pub fn escucha_stop(manager: State<'_, Arc<EscuchaManager>>) -> Result<(), String> {
    manager.stop()
}

#[tauri::command]
#[specta::specta]
pub fn escucha_status(manager: State<'_, Arc<EscuchaManager>>) -> Result<EstadoEscucha, String> {
    manager.status()
}

/// Preprocesa contenido (markdown/código/auto) en la cola de oraciones que el
/// panel Escucha lee y resalta. Puro: no toca el motor TTS.
#[tauri::command]
#[specta::specta]
pub fn escucha_preprocess(
    contenido: String,
    modo: ModoLectura,
    verbosidad: VerbosidadSimbolos,
) -> Result<Vec<OracionHablable>, String> {
    Ok(preprocesar(&contenido, modo, verbosidad))
}

/// Lee el archivo de texto que el usuario eligió en el diálogo del panel.
/// Vive en Rust en vez de plugin-fs para no ampliar el scope compartido de
/// capabilities; el límite de tamaño evita tragar binarios gigantes.
#[tauri::command]
#[specta::specta]
pub fn escucha_read_file(ruta: String) -> Result<String, String> {
    const MAX_BYTES: u64 = 2 * 1024 * 1024;
    let meta = std::fs::metadata(&ruta).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    if !meta.is_file() {
        return Err("la ruta no es un archivo".to_string());
    }
    if meta.len() > MAX_BYTES {
        return Err("el archivo supera el límite de 2 MB".to_string());
    }
    let bytes = std::fs::read(&ruta).map_err(|e| format!("no se pudo leer el archivo: {e}"))?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

/// "Leer portapapeles": el texto copiado, leído desde Rust para no añadir el
/// permiso clipboard read al capabilities compartido. No se registra en logs.
#[tauri::command]
#[specta::specta]
pub fn escucha_read_clipboard(app: AppHandle) -> Result<String, String> {
    use tauri_plugin_clipboard_manager::ClipboardExt;
    app.clipboard()
        .read_text()
        .map_err(|e| format!("no se pudo leer el portapapeles: {e}"))
}

/// Persiste la configuración de Escucha de una vez. Comando propio (en vez de
/// un change_* por campo en shortcut/mod.rs) para no tocar ese archivo
/// caliente compartido con la sesión paralela.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub fn escucha_update_settings(
    app: AppHandle,
    voz_prosa: Option<String>,
    voz_codigo: Option<String>,
    rate_prosa: f32,
    rate_codigo: f32,
    verbosidad: VerbosidadSimbolos,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.escucha_voz_prosa = voz_prosa;
    settings.escucha_voz_codigo = voz_codigo;
    settings.escucha_rate_prosa = rate_prosa.clamp(0.25, 3.0);
    settings.escucha_rate_codigo = rate_codigo.clamp(0.25, 3.0);
    settings.escucha_verbosidad_simbolos = verbosidad;
    crate::settings::write_settings(&app, settings);
    Ok(())
}
