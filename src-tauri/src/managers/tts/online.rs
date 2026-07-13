//! Motor ONLINE (edge-tts / voces neuronales de Microsoft). ⚠️ **NO ES LOCAL**:
//! envía el texto a los servidores de Microsoft (requiere internet). Excepción
//! deliberada al principio "100% local" de ABRAX (decisión de producto), ofrecida
//! etiquetada y desactivada por defecto — **nunca se recomienda ni es fallback**.
//! Reutiliza `PyServerEngine` (servidor Python local que a su vez llama a la nube).

use tauri::AppHandle;

use super::download;
use super::engine::EngineId;
use super::pyserver::{self, PyServerConfig, VoiceSpec};

pub const RUNTIME_NAME: &str = "online";

pub static CONFIG: PyServerConfig = PyServerConfig {
    id: EngineId::Online,
    server_source: include_str!("../../../resources/tts/online_server.py"),
    needs_gpu: false,
    device_arg: "online",
    // Voces neuronales de Microsoft (Azure) por edge-tts: es-MX/es-ES + en-US, M/F.
    voices: &[
        VoiceSpec {
            id: "es-MX-DaliaNeural",
            display: "Español (México) · femenino (Dalia)",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-MX-JorgeNeural",
            display: "Español (México) · masculino (Jorge)",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-ES-ElviraNeural",
            display: "Español (España) · femenino (Elvira)",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "en-US-AriaNeural",
            display: "English (US) · female (Aria)",
            lang: "en",
            es_espanol: false,
        },
        VoiceSpec {
            id: "en-US-GuyNeural",
            display: "English (US) · male (Guy)",
            lang: "en",
            es_espanol: false,
        },
    ],
};

/// ¿Está el runtime online aprovisionado (venv con edge-tts)?
pub fn is_installed(app: &AppHandle) -> bool {
    download::runtime_dir(app, RUNTIME_NAME)
        .map(|dir| pyserver::PyServerEngine::is_provisioned(&dir))
        .unwrap_or(false)
}

/// Aprovisiona el runtime online (venv + edge-tts + miniaudio). Liviano, sin torch.
pub async fn install_runtime(app: &AppHandle) -> Result<(), String> {
    let dir = download::runtime_dir(app, RUNTIME_NAME)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let venv = dir.join(".venv");
    let venv_str = venv.to_string_lossy().to_string();
    pyserver::run_uv(&["venv", &venv_str, "--python", "3.12"]).await?;
    let py = pyserver::venv_python(&dir);
    let py_str = py.to_string_lossy().to_string();
    pyserver::run_uv(&["pip", "install", "--python", &py_str, "edge-tts", "miniaudio"]).await?;
    Ok(())
}
