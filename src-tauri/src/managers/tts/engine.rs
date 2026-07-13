//! Abstracción común de los motores TTS y su identidad.
//!
//! **Nota de diseño (desviación consciente del trait idealizado del prompt):**
//! el prompt proponía `synthesize(...) -> AudioOutput`. Pero el motor del
//! sistema (crate `tts`: SAPI/AVSpeech/speech-dispatcher) **sintetiza y
//! reproduce dentro del SO** — nunca entrega muestras PCM (ver
//! `managers/escucha/mod.rs`). Por eso el trait gira en torno a `speak`, donde
//! **cada motor posee su reproducción**: el del sistema habla por el SO; los
//! neuronales sintetizan a WAV, normalizan a −3 dBFS y reproducen por su propio
//! sink `rodio`. Así el fallback universal encaja sin fingir que produce PCM.

use serde::{Deserialize, Serialize};
use specta::Type;

use super::hardware::HardwareInfo;
use crate::managers::escucha::VozEscucha;

/// Identidad de un motor TTS. Se persiste en settings (`tts_selected_engine`).
/// **Ninguna variante es de nube** — invariante del producto.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Type)]
#[serde(rename_all = "snake_case")]
pub enum EngineId {
    /// Voces del SO (SAPI / AVSpeech / speech-dispatcher). Fallback universal.
    System,
    /// Piper (VITS ONNX). Estándar neuronal liviano en CPU.
    Piper,
    /// Kokoro-82M (ONNX). Premium en CPU.
    Kokoro,
}

impl EngineId {
    /// Todos los motores, en orden de "tier" descendente para mostrar.
    pub const ALL: [EngineId; 3] = [EngineId::Kokoro, EngineId::Piper, EngineId::System];

    pub fn display_name(&self) -> &'static str {
        match self {
            EngineId::System => "Voces del sistema",
            EngineId::Piper => "Piper",
            EngineId::Kokoro => "Kokoro",
        }
    }

    /// Invariante duro: **todos los motores son locales, ninguno usa la nube.**
    pub fn is_local(&self) -> bool {
        matches!(self, EngineId::System | EngineId::Piper | EngineId::Kokoro)
    }
}

/// Requisitos de un motor para orientar la UI y la recomendación.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, Type)]
pub struct EngineRequirements {
    /// Necesita GPU compatible (CUDA NVIDIA o Metal Apple).
    pub needs_gpu: bool,
    /// Necesita descargar modelo/runtime en el primer uso.
    pub needs_download: bool,
}

/// Estado de un motor para el selector "Elegir otro motor".
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct EngineStatus {
    pub id: EngineId,
    pub display_name: String,
    /// ¿Utilizable AHORA en este equipo? (hardware compatible + runtime/modelo presente).
    pub available: bool,
    /// Motivo de NO disponibilidad, ya localizado para la UI
    /// (p.ej. "requiere GPU NVIDIA" o "requiere descargar la voz (60 MB)").
    pub reason: Option<String>,
    /// ¿Es el recomendado para este hardware?
    pub recommended: bool,
    pub requirements: EngineRequirements,
}

/// Opciones de síntesis comunes a todos los motores.
#[derive(Debug, Clone, Copy)]
pub struct TtsOptions {
    /// Multiplicador de velocidad (1.0 = normal). Cada motor lo mapea a su rango.
    pub rate: f32,
}

impl Default for TtsOptions {
    fn default() -> Self {
        Self { rate: 1.0 }
    }
}

/// Errores accionables de un motor. Nunca filtran el texto dictado.
#[derive(Debug, Clone)]
pub enum TtsError {
    /// El motor no está disponible (hardware/dependencia); el mensaje explica por qué.
    NotAvailable(String),
    /// Fallo al sintetizar.
    Synthesis(String),
    /// Fallo al reproducir.
    Playback(String),
    /// Fallo de E/S (subproceso, archivo, red).
    Io(String),
}

impl std::fmt::Display for TtsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TtsError::NotAvailable(m) => write!(f, "motor no disponible: {m}"),
            TtsError::Synthesis(m) => write!(f, "fallo de síntesis: {m}"),
            TtsError::Playback(m) => write!(f, "fallo de reproducción: {m}"),
            TtsError::Io(m) => write!(f, "fallo de E/S: {m}"),
        }
    }
}

impl std::error::Error for TtsError {}

/// Contrato común de un motor TTS local. `Send` porque el registro vive en un
/// hilo de trabajo dedicado (patrón actor, como Escucha).
pub trait TtsEngine: Send {
    /// Identidad del motor (para diagnóstico/enrutado). El nombre visible y los
    /// requisitos se derivan de `EngineId`/`registry`, no del trait.
    fn id(&self) -> EngineId;

    /// ¿Puede usarse en este equipo AHORA? (hardware compatible **y** runtime/
    /// modelo ya presente). El fallback en runtime depende de esto.
    fn is_available(&self, hw: &HardwareInfo) -> bool;

    /// Sintetiza `text` con la voz `voice` (id específico del motor; `None` = la
    /// voz por defecto del motor) y lo **reproduce**. Cada motor posee su salida
    /// (el del sistema por el SO; los neuronales normalizan a −3 dBFS y suenan
    /// por su propio sink). Retorna al encolar/arrancar, no al terminar el audio.
    fn speak(&mut self, text: &str, voice: Option<&str>, opts: &TtsOptions) -> Result<(), TtsError>;

    /// Detiene cualquier reproducción/síntesis en curso.
    fn stop(&mut self) -> Result<(), TtsError>;

    /// ¿Hay audio sonando ahora? (base del resaltado por sondeo del frontend).
    fn is_speaking(&self) -> bool;

    /// Voces disponibles de ESTE motor (para el selector de voz).
    fn list_voices(&self) -> Result<Vec<VozEscucha>, TtsError>;
}
