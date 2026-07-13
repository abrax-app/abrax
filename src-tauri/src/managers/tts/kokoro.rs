//! Motor Kokoro-82M (Apache-2.0): premium en CPU. Corre como **servidor Python
//! local** (`resources/tts/kokoro_server.py`, embebido) sobre un runtime
//! kokoro-onnx aprovisionado en el primer uso. Los pesos `.onnx`/`.bin`
//! (Apache-2.0) se descargan con sha256 al runtime_dir y el servidor los carga
//! desde su CWD. El fonemizador español de kokoro-onnx usa espeak-ng (GPL): por
//! eso vive en el venv separado, nunca en el repo MIT. Ver `pyserver.rs`.

use tauri::AppHandle;

use super::download;
use super::engine::EngineId;
use super::pyserver::{self, PyServerConfig};

pub const RUNTIME_NAME: &str = "kokoro";

pub static CONFIG: PyServerConfig = PyServerConfig {
    id: EngineId::Kokoro,
    server_source: include_str!("../../../resources/tts/kokoro_server.py"),
    needs_gpu: false,
    device_arg: "cpu",
    default_voice: "em_alex",
};

// Pesos Kokoro (Apache-2.0), release de kokoro-onnx; sha256 verificado.
const MODEL_URL: &str = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/kokoro-v1.0.onnx";
const MODEL_SHA: &str = "7d5df8ecf7d4b1878015a32686053fd0eebe2bc377234608764cc0ef3636a6c5";
const VOICES_URL: &str = "https://github.com/thewh1teagle/kokoro-onnx/releases/download/model-files-v1.0/voices-v1.0.bin";
const VOICES_SHA: &str = "bca610b8308e8d99f32e6fe4197e7ec01679264efed0cac9140fe9c29f1fbf7d";

/// ¿Modelos Kokoro presentes en el runtime_dir?
pub fn models_present(app: &AppHandle) -> bool {
    download::runtime_dir(app, RUNTIME_NAME)
        .map(|d| d.join("kokoro-v1.0.onnx").is_file() && d.join("voices-v1.0.bin").is_file())
        .unwrap_or(false)
}

/// ¿Runtime (venv) + modelos listos?
pub fn is_installed(app: &AppHandle) -> bool {
    download::runtime_dir(app, RUNTIME_NAME)
        .map(|dir| pyserver::PyServerEngine::is_provisioned(&dir))
        .unwrap_or(false)
        && models_present(app)
}

/// Aprovisiona el runtime en el primer uso: venv (uv) + kokoro-onnx + pesos.
pub async fn install_runtime(app: &AppHandle) -> Result<(), String> {
    let dir = download::runtime_dir(app, RUNTIME_NAME)?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let venv = dir.join(".venv");
    let venv_str = venv.to_string_lossy().to_string();
    pyserver::run_uv(&["venv", &venv_str, "--python", "3.12"]).await?;
    let py = pyserver::venv_python(&dir);
    let py_str = py.to_string_lossy().to_string();
    pyserver::run_uv(&["pip", "install", "--python", &py_str, "kokoro-onnx"]).await?;
    // Pesos (Apache-2.0) con checksum, al lado del servidor (CWD del server).
    download::download_file(app, "kokoro-model", MODEL_URL, MODEL_SHA, &dir.join("kokoro-v1.0.onnx"))
        .await?;
    download::download_file(app, "kokoro-voices", VOICES_URL, VOICES_SHA, &dir.join("voices-v1.0.bin"))
        .await?;
    Ok(())
}
