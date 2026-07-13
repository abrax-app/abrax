//! [TTS] Comandos del motor de voz adaptativo: detección de hardware, listado
//! de motores, recomendación, selección y descarga de assets (runtime GPL
//! separado + voces con checksum). `speak`/`list_voices`/`stop`/`status` NO se
//! duplican aquí: se enrutan al motor activo desde `commands/escucha.rs`.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::managers::tts::engine::{EngineId, EngineStatus};
use crate::managers::tts::hardware::HardwareInfo;
use crate::managers::tts::manager::TtsManager;
use crate::managers::tts::{kokoro, piper};

#[tauri::command]
#[specta::specta]
pub fn detect_hardware(manager: State<'_, Arc<TtsManager>>) -> Result<HardwareInfo, String> {
    Ok(manager.hardware())
}

/// Fuerza una nueva detección de hardware y recomputa el motor activo.
#[tauri::command]
#[specta::specta]
pub fn redetect_hardware(manager: State<'_, Arc<TtsManager>>) -> Result<HardwareInfo, String> {
    Ok(manager.redetect())
}

#[tauri::command]
#[specta::specta]
pub fn list_engines(manager: State<'_, Arc<TtsManager>>) -> Result<Vec<EngineStatus>, String> {
    Ok(manager.list_engines())
}

#[tauri::command]
#[specta::specta]
pub fn get_recommended_engine(manager: State<'_, Arc<TtsManager>>) -> Result<EngineId, String> {
    Ok(manager.recommended())
}

#[tauri::command]
#[specta::specta]
pub fn get_active_engine(manager: State<'_, Arc<TtsManager>>) -> Result<EngineId, String> {
    Ok(manager.active())
}

#[tauri::command]
#[specta::specta]
pub fn set_engine(manager: State<'_, Arc<TtsManager>>, engine: EngineId) -> Result<(), String> {
    manager.set_engine(engine)
}

/// Catálogo de voces Piper con su estado de instalación (para el selector de voz).
#[tauri::command]
#[specta::specta]
pub fn list_piper_voices(app: AppHandle) -> Result<Vec<piper::PiperVoiceInfo>, String> {
    Ok(piper::catalog(&app))
}

// --- Descarga de assets Piper (runtime GPL separado + voces con checksum) ----

/// Descarga+extrae el runtime Piper (GPL, proceso separado) en el primer uso.
#[tauri::command]
#[specta::specta]
pub async fn install_piper_runtime(app: AppHandle) -> Result<(), String> {
    piper::install_runtime(&app).await
}

/// Descarga una voz Piper (`.onnx` + `.onnx.json`) con verificación sha256.
#[tauri::command]
#[specta::specta]
pub async fn install_piper_voice(app: AppHandle, voice_id: String) -> Result<(), String> {
    piper::install_voice(&app, &voice_id).await
}

/// Aprovisiona el runtime Kokoro (venv + kokoro-onnx + pesos con checksum).
#[tauri::command]
#[specta::specta]
pub async fn install_kokoro_runtime(app: AppHandle) -> Result<(), String> {
    kokoro::install_runtime(&app).await
}
