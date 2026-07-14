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
    // Catálogo de voces neuronales de Microsoft (Azure) vía edge-tts. Todos los
    // ids fueron conciliados contra `edge-tts --list-voices` (existen exactos).
    // El `id` es la etiqueta BCP-47 completa (`es-CO-SalomeNeural`): el selector
    // la usa para AGRUPAR por idioma/país. El `display` es conciso (nombre ·
    // género) porque el país lo aporta el encabezado del grupo. La primera es la
    // voz por defecto (es-MX/Dalia, LATAM neutro). El país queda fuera del
    // `display` a propósito; los NOMBRES de voz/país nunca se traducen.
    //
    // Español (LATAM + España): 9 países × M/F. Inglés (para términos técnicos):
    // en-US + en-GB × M/F. Motor ONLINE, opt-in, jamás recomendado ni fallback.
    voices: &[
        // — Español · México (voz por defecto: Dalia) —
        VoiceSpec {
            id: "es-MX-DaliaNeural",
            display: "Dalia · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-MX-JorgeNeural",
            display: "Jorge · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Chile —
        VoiceSpec {
            id: "es-CL-CatalinaNeural",
            display: "Catalina · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-CL-LorenzoNeural",
            display: "Lorenzo · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Colombia —
        VoiceSpec {
            id: "es-CO-SalomeNeural",
            display: "Salomé · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-CO-GonzaloNeural",
            display: "Gonzalo · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Argentina —
        VoiceSpec {
            id: "es-AR-ElenaNeural",
            display: "Elena · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-AR-TomasNeural",
            display: "Tomás · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Perú —
        VoiceSpec {
            id: "es-PE-CamilaNeural",
            display: "Camila · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-PE-AlexNeural",
            display: "Alex · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Venezuela —
        VoiceSpec {
            id: "es-VE-PaolaNeural",
            display: "Paola · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-VE-SebastianNeural",
            display: "Sebastián · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Costa Rica —
        VoiceSpec {
            id: "es-CR-MariaNeural",
            display: "María · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-CR-JuanNeural",
            display: "Juan · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · Estados Unidos (bilingüe) —
        VoiceSpec {
            id: "es-US-PalomaNeural",
            display: "Paloma · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-US-AlonsoNeural",
            display: "Alonso · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Español · España —
        VoiceSpec {
            id: "es-ES-ElviraNeural",
            display: "Elvira · femenina",
            lang: "es",
            es_espanol: true,
        },
        VoiceSpec {
            id: "es-ES-AlvaroNeural",
            display: "Álvaro · masculino",
            lang: "es",
            es_espanol: true,
        },
        // — Inglés · Estados Unidos —
        VoiceSpec {
            id: "en-US-AriaNeural",
            display: "Aria · female",
            lang: "en",
            es_espanol: false,
        },
        VoiceSpec {
            id: "en-US-GuyNeural",
            display: "Guy · male",
            lang: "en",
            es_espanol: false,
        },
        // — Inglés · Reino Unido —
        VoiceSpec {
            id: "en-GB-SoniaNeural",
            display: "Sonia · female",
            lang: "en",
            es_espanol: false,
        },
        VoiceSpec {
            id: "en-GB-RyanNeural",
            display: "Ryan · male",
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
    pyserver::run_uv(&[
        "pip",
        "install",
        "--python",
        &py_str,
        "edge-tts",
        "miniaudio",
    ])
    .await?;
    Ok(())
}
