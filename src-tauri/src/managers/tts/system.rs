//! Motor del sistema: adapta el `EscuchaManager` (crate `tts`, voces del SO) al
//! trait `TtsEngine`. Es el **fallback universal** — siempre disponible, sin
//! descargas, 100% local. Reutiliza tal cual el motor de Escucha existente.

use std::sync::Arc;

use super::engine::{EngineId, TtsEngine, TtsError, TtsOptions};
use super::hardware::HardwareInfo;
use crate::managers::escucha::{EscuchaManager, VozEscucha};

pub struct SystemEngine {
    escucha: Arc<EscuchaManager>,
}

impl SystemEngine {
    pub fn new(escucha: Arc<EscuchaManager>) -> Self {
        Self { escucha }
    }
}

impl TtsEngine for SystemEngine {
    fn id(&self) -> EngineId {
        EngineId::System
    }

    fn is_available(&self, _hw: &HardwareInfo) -> bool {
        // Disponible si el TTS del SO inicializa y hay al menos una voz.
        self.escucha
            .status()
            .map(|s| s.motor_disponible)
            .unwrap_or(false)
    }

    fn speak(&mut self, text: &str, voice: Option<&str>, opts: &TtsOptions) -> Result<(), TtsError> {
        self.escucha
            .speak(
                text.to_string(),
                voice.map(|v| v.to_string()),
                Some(opts.rate),
            )
            .map_err(TtsError::Synthesis)
    }

    fn stop(&mut self) -> Result<(), TtsError> {
        self.escucha.stop().map_err(TtsError::Playback)
    }

    fn is_speaking(&self) -> bool {
        self.escucha.status().map(|s| s.hablando).unwrap_or(false)
    }

    fn list_voices(&self) -> Result<Vec<VozEscucha>, TtsError> {
        self.escucha.list_voices().map_err(TtsError::Synthesis)
    }
}
