//! [TTS] Comandos del motor de voz adaptativo: detección de hardware, listado
//! de motores, recomendación, selección y descarga de assets (runtime GPL
//! separado + voces con checksum). `speak`/`list_voices`/`stop`/`status` NO se
//! duplican aquí: se enrutan al motor activo desde `commands/escucha.rs`.
//!
//! Las FIRMAS de todos los comandos son estables con y sin la feature
//! `advanced-tts` (bindings idénticos). Sin la feature (build de entrega) los
//! comandos de motores neuronales devuelven vacío/error accionable — pero la UI
//! nunca los invoca porque `list_engines` reporta solo el Sistema.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::managers::tts::engine::{EngineId, EngineStatus};
use crate::managers::tts::hardware::HardwareInfo;
use crate::managers::tts::manager::TtsManager;
use crate::managers::tts::registry::PiperVoiceInfo;

#[cfg(feature = "advanced-tts")]
use crate::managers::tts::{kokoro, online, piper};

/// Mensaje cuando se invoca un motor neuronal en el build de entrega (sin feature).
#[cfg(not(feature = "advanced-tts"))]
const SIN_AVANZADO: &str = "Voz avanzada no disponible en este build: solo Voces del Sistema.";

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
/// Sin `advanced-tts` devuelve una lista vacía (la UI no muestra Piper).
#[tauri::command]
#[specta::specta]
pub fn list_piper_voices(app: AppHandle) -> Result<Vec<PiperVoiceInfo>, String> {
    #[cfg(feature = "advanced-tts")]
    {
        Ok(piper::catalog(&app))
    }
    #[cfg(not(feature = "advanced-tts"))]
    {
        let _ = app;
        Ok(Vec::new())
    }
}

// --- Descarga de assets (runtime GPL separado + voces con checksum) ----------
// Sin `advanced-tts` estos comandos existen (firma estable) pero devuelven un
// error accionable; la UI del build de entrega nunca los invoca.

/// Descarga+extrae el runtime Piper (GPL, proceso separado) en el primer uso.
#[tauri::command]
#[specta::specta]
pub async fn install_piper_runtime(app: AppHandle) -> Result<(), String> {
    #[cfg(feature = "advanced-tts")]
    {
        piper::install_runtime(&app).await
    }
    #[cfg(not(feature = "advanced-tts"))]
    {
        let _ = app;
        Err(SIN_AVANZADO.to_string())
    }
}

/// Descarga una voz Piper (`.onnx` + `.onnx.json`) con verificación sha256.
#[tauri::command]
#[specta::specta]
pub async fn install_piper_voice(app: AppHandle, voice_id: String) -> Result<(), String> {
    #[cfg(feature = "advanced-tts")]
    {
        piper::install_voice(&app, &voice_id).await
    }
    #[cfg(not(feature = "advanced-tts"))]
    {
        let _ = (app, voice_id);
        Err(SIN_AVANZADO.to_string())
    }
}

/// Aprovisiona el runtime Kokoro (venv + kokoro-onnx + pesos con checksum).
#[tauri::command]
#[specta::specta]
pub async fn install_kokoro_runtime(app: AppHandle) -> Result<(), String> {
    #[cfg(feature = "advanced-tts")]
    {
        kokoro::install_runtime(&app).await
    }
    #[cfg(not(feature = "advanced-tts"))]
    {
        let _ = app;
        Err(SIN_AVANZADO.to_string())
    }
}

/// Aprovisiona el runtime de la voz ONLINE (venv + edge-tts). ⚠️ Motor de nube.
#[tauri::command]
#[specta::specta]
pub async fn install_online_runtime(app: AppHandle) -> Result<(), String> {
    #[cfg(feature = "advanced-tts")]
    {
        online::install_runtime(&app).await
    }
    #[cfg(not(feature = "advanced-tts"))]
    {
        let _ = app;
        Err(SIN_AVANZADO.to_string())
    }
}
