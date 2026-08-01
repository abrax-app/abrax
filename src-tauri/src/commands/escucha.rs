//! [ESCUCHA] Comandos Tauri de la sección Escucha (lectura en voz alta).
//!
//! El frontend maneja la cola de oraciones: `escucha_speak` habla UN trozo
//! (interrumpiendo el anterior), y el panel espera con `escucha_status` a que
//! termine para avanzar el resaltado y pedir el siguiente.

use crate::managers::escucha::preproceso::{
    preprocesar, ModoLectura, OracionHablable, VerbosidadSimbolos,
};
use crate::managers::escucha::{EstadoEscucha, VozEscucha};
use crate::managers::tts::manager::TtsManager;
use std::sync::Arc;
use tauri::{AppHandle, State};

// speak/list_voices/stop/status se ENRUTAN al motor activo vía `TtsManager`
// (sistema, Piper, …) sin duplicar comandos: el manager decide el motor y, para
// el del sistema, delega en el `EscuchaManager` que envuelve.

#[tauri::command]
#[specta::specta]
pub fn escucha_list_voices(manager: State<'_, Arc<TtsManager>>) -> Result<Vec<VozEscucha>, String> {
    manager.list_voices()
}

/// `rate` es un multiplicador de velocidad (1.0 = normal). Cada motor lo mapea a
/// su rango (el del sistema vía `escucha::map_rate`; Piper vía `length_scale`;
/// online/Kokoro vía el servidor). `pitch` es el tono en Hz — SOLO lo aplica el
/// motor online (edge-tts); los demás lo ignoran.
///
/// **Async + `spawn_blocking`**: la síntesis neuronal puede tardar segundos la
/// 1.ª vez (arranca el servidor y carga el modelo). Si corriera en el hilo
/// principal, congelaría la UI y bloquearía toda otra IPC (incluido `Detener`).
/// Al ejecutarla en el pool bloqueante, el hilo principal queda libre y los
/// comandos de parada/estado responden al instante.
#[tauri::command]
#[specta::specta]
pub async fn escucha_speak(
    manager: State<'_, Arc<TtsManager>>,
    texto: String,
    voz_id: Option<String>,
    rate: Option<f32>,
    pitch: Option<i32>,
) -> Result<(), String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || manager.speak(texto, voz_id, rate, pitch))
        .await
        .map_err(|e| format!("tarea de síntesis abortó: {e}"))?
}

#[tauri::command]
#[specta::specta]
pub fn escucha_stop(manager: State<'_, Arc<TtsManager>>) -> Result<(), String> {
    manager.stop()
}

#[tauri::command]
#[specta::specta]
pub fn escucha_status(manager: State<'_, Arc<TtsManager>>) -> Result<EstadoEscucha, String> {
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
/// un change_* por campo en shortcut/mod.rs) para mantener acotada la
/// superficie de ese archivo compartido.
#[tauri::command]
#[specta::specta]
pub fn escucha_update_settings(
    app: AppHandle,
    voz_prosa: Option<String>,
    voz_codigo: Option<String>,
    verbosidad: VerbosidadSimbolos,
) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.escucha_voz_prosa = voz_prosa;
    settings.escucha_voz_codigo = voz_codigo;
    settings.escucha_verbosidad_simbolos = verbosidad;
    crate::settings::write_settings(&app, settings);
    Ok(())
}

/// Persiste los "Ajustes de voz" del motor: velocidad de lectura (multiplicador,
/// todos los motores) y tono en Hz (solo online). Se aplican a "Probar voz" y a
/// la lectura del panel Escucha.
#[tauri::command]
#[specta::specta]
pub fn update_tts_ajustes(app: AppHandle, velocidad: f32, tono: i32) -> Result<(), String> {
    let mut settings = crate::settings::get_settings(&app);
    settings.tts_velocidad = velocidad.clamp(0.5, 3.0);
    settings.tts_tono = tono.clamp(-100, 100);
    crate::settings::write_settings(&app, settings);
    Ok(())
}
