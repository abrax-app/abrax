//! Motor Chatterbox (Resemble AI, MIT): tier PREMIUM con GPU (CUDA NVIDIA o MPS
//! Apple). Corre como **servidor Python local** (`resources/tts/chatterbox_server.py`,
//! embebido) sobre un runtime torch aprovisionado en el primer uso. Ver
//! `pyserver.rs`. El audio nunca sale del equipo (127.0.0.1).

use tauri::AppHandle;

use super::download;
use super::engine::EngineId;
use super::pyserver::{self, PyServerConfig, VoiceSpec};

pub const RUNTIME_NAME: &str = "chatterbox";

pub static CONFIG: PyServerConfig = PyServerConfig {
    id: EngineId::Chatterbox,
    server_source: include_str!("../../../resources/tts/chatterbox_server.py"),
    needs_gpu: true,
    device_arg: "auto",
    // Chatterbox multilingüe: la "voz" es el idioma; el modelo usa su voz por
    // defecto por idioma (clonación con audio de referencia = futuro).
    voices: &[
        VoiceSpec {
            id: "es",
            display: "Español",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "en",
            display: "English",
            lang: "en",
            es_espanol: false,
        },
    ],
};

/// ¿Está el runtime Chatterbox aprovisionado (venv con torch+chatterbox)?
pub fn is_installed(app: &AppHandle) -> bool {
    download::runtime_dir(app, RUNTIME_NAME)
        .map(|dir| pyserver::PyServerEngine::is_provisioned(&dir))
        .unwrap_or(false)
}

/// Aprovisiona el runtime en el primer uso: venv (uv) + chatterbox-tts (torch).
/// Operación pesada (torch ~GB); el modelo se baja solo en la 1.ª síntesis.
pub async fn install_runtime(app: &AppHandle) -> Result<(), String> {
    let dir = download::runtime_dir(app, RUNTIME_NAME)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let venv = dir.join(".venv");
    let venv_str = venv.to_string_lossy().to_string();
    pyserver::run_uv(&["venv", &venv_str, "--python", "3.12"]).await?;
    let py = pyserver::venv_python(&dir);
    let py_str = py.to_string_lossy().to_string();
    // --torch-backend=auto: uv elige CUDA/CPU según el equipo.
    pyserver::run_uv(&[
        "pip",
        "install",
        "--python",
        &py_str,
        "chatterbox-tts",
        "--torch-backend=auto",
    ])
    .await?;
    Ok(())
}
