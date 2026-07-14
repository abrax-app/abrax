//! Requisitos estáticos por motor, para que la UI componga el "por qué no está
//! disponible" (i18n) sin strings de copy en el backend.

use serde::Serialize;
use specta::Type;

use super::engine::{EngineId, EngineRequirements};

/// Info de una voz Piper para la UI (catálogo + estado de descarga). Vive aquí
/// (módulo siempre compilado) y no en `piper` para que la firma del comando
/// `list_piper_voices` sea estable con y sin la feature `advanced-tts` (bindings
/// idénticos). Con la feature OFF, el comando devuelve una lista vacía.
#[derive(Serialize, Clone, Type)]
pub struct PiperVoiceInfo {
    pub id: String,
    pub display: String,
    pub lang: String,
    pub size_mb: u64,
    /// Voz pensada para leer código (cadencia neutra) vs. prosa.
    pub for_code: bool,
    pub installed: bool,
}

pub fn requirements_for(id: EngineId) -> EngineRequirements {
    match id {
        EngineId::System => EngineRequirements {
            needs_gpu: false,
            needs_download: false,
            needs_internet: false,
        },
        EngineId::Piper => EngineRequirements {
            needs_gpu: false,
            needs_download: true,
            needs_internet: false,
        },
        EngineId::Kokoro => EngineRequirements {
            needs_gpu: false,
            needs_download: true,
            needs_internet: false,
        },
        EngineId::Online => EngineRequirements {
            needs_gpu: false,
            needs_download: true,
            needs_internet: true,
        },
    }
}
