//! Motor Piper: síntesis neuronal VITS en CPU vía el ejecutable `piper` (subproceso).
//!
//! Piper embebe espeak-ng (GPL-3.0), así que el **runtime se descarga en el
//! primer uso y corre como proceso separado** ("mere aggregation"): cero GPL en
//! el repo MIT ni en el instalador. Las voces `.onnx` son limpias
//! (Unlicense/CC0) y se descargan con sha256. Ver `LICENSES-THIRD-PARTY.md`.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use tauri::AppHandle;

use super::download;
use super::engine::{EngineId, TtsEngine, TtsError, TtsOptions};
use super::hardware::HardwareInfo;
use super::playback::{self, PlaybackService, TARGET_DBFS};
use super::registry::PiperVoiceInfo;
use crate::managers::escucha::VozEscucha;

/// Una voz Piper = 2 archivos (`.onnx` + `.onnx.json`), cada uno con sha256.
pub struct PiperVoice {
    pub id: &'static str,
    pub display: &'static str,
    pub lang: &'static str,
    pub onnx_url: &'static str,
    pub onnx_sha256: &'static str,
    pub json_url: &'static str,
    pub json_sha256: &'static str,
    pub size_mb: u64,
    /// ¿Voz de prosa (natural) o pensada para código (cadencia neutra)?
    pub for_code: bool,
}

/// Catálogo de voces es-419 (rhasspy/piper-voices, commit anclado; sha256
/// verificados contra la descarga real). Todas 22050 Hz.
pub const VOICES: &[PiperVoice] = &[
    PiperVoice {
        id: "es_MX-ald-medium",
        display: "Español (México) — Ald",
        lang: "es-MX",
        onnx_url: "https://huggingface.co/rhasspy/piper-voices/resolve/e21c7de8d4eab79b902f0d61e662b3f21664b8d2/es/es_MX/ald/medium/es_MX-ald-medium.onnx",
        onnx_sha256: "019b3803293c93e34a206dd2e53a3889209a514e786fd7144f7b70196c579b63",
        json_url: "https://huggingface.co/rhasspy/piper-voices/resolve/e21c7de8d4eab79b902f0d61e662b3f21664b8d2/es/es_MX/ald/medium/es_MX-ald-medium.onnx.json",
        json_sha256: "5a71498158e04afc8099bfd019c7e87c68eb9d042505a2b1a87e5c1ac2b1a61d",
        size_mb: 61,
        for_code: false,
    },
    PiperVoice {
        id: "es_ES-davefx-medium",
        display: "Español (España) — Davefx",
        lang: "es-ES",
        onnx_url: "https://huggingface.co/rhasspy/piper-voices/resolve/e21c7de8d4eab79b902f0d61e662b3f21664b8d2/es/es_ES/davefx/medium/es_ES-davefx-medium.onnx",
        onnx_sha256: "6658b03b1a6c316ee4c265a9896abc1393353c2d9e1bca7d66c2c442e222a917",
        json_url: "https://huggingface.co/rhasspy/piper-voices/resolve/e21c7de8d4eab79b902f0d61e662b3f21664b8d2/es/es_ES/davefx/medium/es_ES-davefx-medium.onnx.json",
        json_sha256: "0e0dda87c732f6f38771ff274a6380d9252f327dca77aa2963d5fbdf9ec54842",
        size_mb: 61,
        for_code: true,
    },
];

pub fn voice_by_id(id: &str) -> Option<&'static PiperVoice> {
    VOICES.iter().find(|v| v.id == id)
}

/// Catálogo de voces Piper con su estado de instalación (para el selector de voz).
pub fn catalog(app: &AppHandle) -> Vec<PiperVoiceInfo> {
    let voices_dir = download::voices_dir(app).ok();
    VOICES
        .iter()
        .map(|v| {
            let installed = voices_dir
                .as_ref()
                .map(|dir| {
                    dir.join(format!("{}.onnx", v.id)).is_file()
                        && dir.join(format!("{}.onnx.json", v.id)).is_file()
                })
                .unwrap_or(false);
            PiperVoiceInfo {
                id: v.id.to_string(),
                display: v.display.to_string(),
                lang: v.lang.to_string(),
                size_mb: v.size_mb,
                for_code: v.for_code,
                installed,
            }
        })
        .collect()
}

/// Runtime GPL-separado por plataforma (release oficial rhasspy/piper 2023.11.14-2).
pub struct PiperRuntimeAsset {
    pub url: &'static str,
    pub sha256: &'static str,
}

/// Devuelve el runtime para ESTE objetivo, o `None` si no hay build oficial.
pub fn runtime_asset() -> Option<PiperRuntimeAsset> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        Some(PiperRuntimeAsset {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_windows_amd64.zip",
            sha256: "f3c58906402b24f3a96d92145f58acba6d86c9b5db896d207f78dc80811efcea",
        })
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Some(PiperRuntimeAsset {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_linux_x86_64.tar.gz",
            sha256: "a50cb45f355b7af1f6d758c1b360717877ba0a398cc8cbe6d2a7a3a26e225992",
        })
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        Some(PiperRuntimeAsset {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_x64.tar.gz",
            sha256: "ced85c0a3df13945b1e623b878a48fdc2854d5c485b4b67f62857cf551deaf8b",
        })
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        Some(PiperRuntimeAsset {
            url: "https://github.com/rhasspy/piper/releases/download/2023.11.14-2/piper_macos_aarch64.tar.gz",
            sha256: "6b1eb03b3735946cb35216e063e7eebcc33a6bbf5dd96ec0217959bf1cdcb0cc",
        })
    }
    #[cfg(not(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
    )))]
    {
        None
    }
}

const RUNTIME_NAME: &str = "piper";

/// ¿Está el runtime Piper instalado (binario presente)?
pub fn is_runtime_installed(app: &AppHandle) -> bool {
    download::runtime_dir(app, RUNTIME_NAME)
        .ok()
        .and_then(|dir| find_piper_binary(&dir))
        .is_some()
}

/// Descarga+extrae el runtime GPL en `<datadir>/tts/runtime/piper` (1er uso).
pub async fn install_runtime(app: &AppHandle) -> Result<(), String> {
    let asset =
        runtime_asset().ok_or_else(|| "no hay runtime Piper para esta plataforma".to_string())?;
    if asset.sha256.starts_with("__PENDING") {
        return Err("sha256 del runtime Piper sin fijar para esta plataforma".to_string());
    }
    let dir = download::runtime_dir(app, RUNTIME_NAME)?;
    download::download_and_extract_archive(app, "piper-runtime", asset.url, asset.sha256, &dir)
        .await
}

/// Descarga una voz Piper (`.onnx` + `.onnx.json`) a `<datadir>/tts/voices`.
pub async fn install_voice(app: &AppHandle, voice_id: &str) -> Result<(), String> {
    let voice =
        voice_by_id(voice_id).ok_or_else(|| format!("voz Piper desconocida: {voice_id}"))?;
    let dir = download::voices_dir(app)?;
    let onnx = dir.join(format!("{}.onnx", voice.id));
    let json = dir.join(format!("{}.onnx.json", voice.id));
    download::download_file(app, voice.id, voice.onnx_url, voice.onnx_sha256, &onnx).await?;
    download::download_file(
        app,
        &format!("{}-config", voice.id),
        voice.json_url,
        voice.json_sha256,
        &json,
    )
    .await?;
    Ok(())
}

/// Busca el binario `piper`(`.exe`) dentro de un dir (el zip trae `piper/piper.exe`).
fn find_piper_binary(runtime_dir: &Path) -> Option<PathBuf> {
    let name = if cfg!(windows) { "piper.exe" } else { "piper" };
    let direct = [runtime_dir.join(name), runtime_dir.join("piper").join(name)];
    for cand in direct {
        if cand.is_file() {
            return Some(cand);
        }
    }
    find_file_bounded(runtime_dir, name, 3)
}

/// Búsqueda recursiva acotada (evita recorrer árboles enormes).
fn find_file_bounded(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() && path.file_name().map(|n| n == name).unwrap_or(false) {
            return Some(path);
        }
        if path.is_dir() {
            subdirs.push(path);
        }
    }
    for sub in subdirs {
        if let Some(found) = find_file_bounded(&sub, name, depth - 1) {
            return Some(found);
        }
    }
    None
}

/// Motor Piper. Sostiene rutas resueltas del datadir + el servicio de reproducción.
pub struct PiperEngine {
    voices_dir: PathBuf,
    runtime_dir: PathBuf,
    temp_dir: PathBuf,
    playback: Arc<PlaybackService>,
    volume: f32,
}

impl PiperEngine {
    pub fn new(
        voices_dir: PathBuf,
        runtime_dir: PathBuf,
        temp_dir: PathBuf,
        playback: Arc<PlaybackService>,
        volume: f32,
    ) -> Self {
        Self {
            voices_dir,
            runtime_dir,
            temp_dir,
            playback,
            volume,
        }
    }

    fn voice_onnx(&self, voice_id: &str) -> PathBuf {
        self.voices_dir.join(format!("{voice_id}.onnx"))
    }

    fn voice_json(&self, voice_id: &str) -> PathBuf {
        self.voices_dir.join(format!("{voice_id}.onnx.json"))
    }

    fn is_voice_present(&self, voice_id: &str) -> bool {
        self.voice_onnx(voice_id).is_file() && self.voice_json(voice_id).is_file()
    }

    fn available_voices(&self) -> Vec<&'static PiperVoice> {
        VOICES
            .iter()
            .filter(|v| self.is_voice_present(v.id))
            .collect()
    }
}

impl TtsEngine for PiperEngine {
    fn id(&self) -> EngineId {
        EngineId::Piper
    }

    fn is_available(&self, _hw: &HardwareInfo) -> bool {
        find_piper_binary(&self.runtime_dir).is_some() && !self.available_voices().is_empty()
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        opts: &TtsOptions,
    ) -> Result<(), TtsError> {
        let bin = find_piper_binary(&self.runtime_dir)
            .ok_or_else(|| TtsError::NotAvailable("runtime Piper no instalado".into()))?;
        let available = self.available_voices();
        if available.is_empty() {
            return Err(TtsError::NotAvailable(
                "no hay voces Piper descargadas".into(),
            ));
        }
        // Voz pedida si está presente; si no, la primera disponible.
        let voice_id = voice
            .filter(|v| self.is_voice_present(v))
            .map(|v| v.to_string())
            .unwrap_or_else(|| available[0].id.to_string());
        let model_path = self.voice_onnx(&voice_id);

        std::fs::create_dir_all(&self.temp_dir).map_err(|e| TtsError::Io(e.to_string()))?;
        let out_wav = self.temp_dir.join(format!("piper_{voice_id}.wav"));

        // length_scale = inverso de la velocidad (rate>1 = más rápido).
        let length_scale = if opts.rate > 0.05 {
            1.0 / opts.rate
        } else {
            1.0
        };
        // El runtime resuelve espeak-ng-data y sus DLLs relativo a su carpeta.
        let work_dir = bin.parent().unwrap_or(self.runtime_dir.as_path());

        let mut child = crate::utils::comando_silencioso(&bin)
            .arg("--model")
            .arg(&model_path)
            .arg("--output_file")
            .arg(&out_wav)
            .arg("--length_scale")
            .arg(format!("{length_scale:.3}"))
            .current_dir(work_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TtsError::Io(format!("no se pudo lanzar piper: {e}")))?;
        crate::utils::adoptar_hijo(&child);

        if let Some(mut stdin) = child.stdin.take() {
            // UTF-8 crudo directo (sin líos de encoding de consola).
            stdin
                .write_all(text.as_bytes())
                .map_err(|e| TtsError::Io(e.to_string()))?;
            let _ = stdin.write_all(b"\n");
        }
        let output = child
            .wait_with_output()
            .map_err(|e| TtsError::Io(e.to_string()))?;
        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            return Err(TtsError::Synthesis(format!("piper falló: {}", err.trim())));
        }

        let (mut samples, sample_rate) =
            playback::read_wav_mono_f32(&out_wav).map_err(TtsError::Synthesis)?;
        playback::normalize_peak_dbfs(&mut samples, TARGET_DBFS);
        let _ = std::fs::remove_file(&out_wav);
        self.playback
            .play(samples, sample_rate, self.volume)
            .map_err(TtsError::Playback)?;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), TtsError> {
        self.playback.stop();
        Ok(())
    }

    fn is_speaking(&self) -> bool {
        self.playback.is_playing()
    }

    fn list_voices(&self) -> Result<Vec<VozEscucha>, TtsError> {
        Ok(self
            .available_voices()
            .iter()
            .map(|v| VozEscucha {
                id: v.id.to_string(),
                nombre: v.display.to_string(),
                idioma: v.lang.to_string(),
                es_espanol: v.lang.starts_with("es"),
            })
            .collect())
    }
}
