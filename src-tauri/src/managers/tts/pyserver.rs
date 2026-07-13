//! Motor genérico "servidor Python local": ABRAX arranca un servidor de síntesis
//! (nuestro script MIT, embebido en el binario) usando un runtime Python
//! **provisionado aparte en el primer uso** (torch/kokoro-onnx, que arrastran
//! espeak-ng GPL). El runtime NUNCA se embebe ni redistribuye en el repo MIT
//! ("mere aggregation"): se aprovisiona con `uv` en `<datadir>/tts/runtime/<x>`.
//! Chatterbox y Kokoro son configuraciones de este motor. El audio va y viene
//! por 127.0.0.1 — nunca sale del equipo.

use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use super::engine::{EngineId, TtsEngine, TtsError, TtsOptions};
use super::hardware::{GpuVendor, HardwareInfo};
use super::playback::{self, PlaybackService, TARGET_DBFS};
use crate::managers::escucha::VozEscucha;

/// Configuración estática de un motor basado en servidor Python.
pub struct PyServerConfig {
    pub id: EngineId,
    /// Fuente del script del servidor (embebida por `include_str!`).
    pub server_source: &'static str,
    /// Requiere GPU compatible (CUDA/MPS) para estar disponible.
    pub needs_gpu: bool,
    /// Argumento de dispositivo que se pasa al servidor.
    pub device_arg: &'static str,
    /// Voz por defecto (id que entiende el servidor), o vacío.
    pub default_voice: &'static str,
}

pub struct PyServerEngine {
    cfg: &'static PyServerConfig,
    runtime_dir: PathBuf,
    playback: std::sync::Arc<PlaybackService>,
    volume: f32,
    child: Option<Child>,
    base_url: String,
}

impl PyServerEngine {
    pub fn new(
        cfg: &'static PyServerConfig,
        runtime_dir: PathBuf,
        playback: std::sync::Arc<PlaybackService>,
        volume: f32,
    ) -> Self {
        Self {
            cfg,
            runtime_dir,
            playback,
            volume,
            child: None,
            base_url: String::new(),
        }
    }

    /// ¿Está el runtime aprovisionado? (intérprete del venv presente).
    pub fn is_provisioned(runtime_dir: &std::path::Path) -> bool {
        venv_python(runtime_dir).is_file()
    }

    /// ¿La GPU de este equipo es compatible con motores que la exigen?
    pub fn gpu_compatible(hw: &HardwareInfo) -> bool {
        matches!(hw.gpu_vendor, GpuVendor::Nvidia | GpuVendor::Apple)
    }

    fn server_script_path(&self) -> PathBuf {
        self.runtime_dir.join("server.py")
    }

    /// Escribe el script embebido al disco (si cambió) para lanzarlo.
    fn write_server_script(&self) -> Result<PathBuf, TtsError> {
        std::fs::create_dir_all(&self.runtime_dir).map_err(|e| TtsError::Io(e.to_string()))?;
        let path = self.server_script_path();
        let refresh = std::fs::read_to_string(&path)
            .map(|existing| existing != self.cfg.server_source)
            .unwrap_or(true);
        if refresh {
            let mut f = std::fs::File::create(&path).map_err(|e| TtsError::Io(e.to_string()))?;
            f.write_all(self.cfg.server_source.as_bytes())
                .map_err(|e| TtsError::Io(e.to_string()))?;
        }
        Ok(path)
    }

    fn health_ok(&self) -> bool {
        if self.base_url.is_empty() {
            return false;
        }
        reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(2))
            .build()
            .ok()
            .and_then(|c| c.get(format!("{}/health", self.base_url)).send().ok())
            .map(|r| r.status().is_success())
            .unwrap_or(false)
    }

    /// Garantiza un servidor vivo y sano, arrancándolo si hace falta.
    fn ensure_server(&mut self) -> Result<(), TtsError> {
        // ¿el hijo sigue vivo y sano? (borrows separados para no solapar).
        let alive = self
            .child
            .as_mut()
            .map(|c| matches!(c.try_wait(), Ok(None)))
            .unwrap_or(false);
        if alive && self.health_ok() {
            return Ok(());
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
        }

        let python = venv_python(&self.runtime_dir);
        if !python.is_file() {
            return Err(TtsError::NotAvailable(
                "runtime del motor no aprovisionado".into(),
            ));
        }
        let script = self.write_server_script()?;
        let port = free_port().ok_or_else(|| TtsError::Io("sin puerto local libre".into()))?;

        let mut child = Command::new(&python)
            .arg(&script)
            .arg("--port")
            .arg(port.to_string())
            .arg("--device")
            .arg(self.cfg.device_arg)
            .current_dir(&self.runtime_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| TtsError::Io(format!("no se pudo lanzar el servidor: {e}")))?;

        // Espera la línea "READY" (la carga del modelo puede tardar).
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| TtsError::Io("servidor sin stdout".into()))?;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if line.starts_with("READY") {
                    let _ = tx.send(());
                    break;
                }
            }
        });

        match rx.recv_timeout(Duration::from_secs(180)) {
            Ok(()) => {
                self.child = Some(child);
                self.base_url = format!("http://127.0.0.1:{port}");
                Ok(())
            }
            Err(_) => {
                let _ = child.kill();
                Err(TtsError::NotAvailable(
                    "el servidor de voz no respondió a tiempo".into(),
                ))
            }
        }
    }
}

impl Drop for PyServerEngine {
    fn drop(&mut self) {
        if let Some(child) = self.child.as_mut() {
            let _ = child.kill();
        }
    }
}

impl TtsEngine for PyServerEngine {
    fn id(&self) -> EngineId {
        self.cfg.id
    }

    fn is_available(&self, hw: &HardwareInfo) -> bool {
        (!self.cfg.needs_gpu || Self::gpu_compatible(hw))
            && Self::is_provisioned(&self.runtime_dir)
    }

    fn speak(&mut self, text: &str, voice: Option<&str>, _opts: &TtsOptions) -> Result<(), TtsError> {
        self.ensure_server()?;
        let voz = voice.unwrap_or(self.cfg.default_voice);
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| TtsError::Io(e.to_string()))?;
        let resp = client
            .post(format!("{}/synthesize", self.base_url))
            .json(&serde_json::json!({ "text": text, "language_id": "es", "voice": voz }))
            .send()
            .map_err(|e| TtsError::Synthesis(e.to_string()))?;
        if !resp.status().is_success() {
            return Err(TtsError::Synthesis(format!(
                "el servidor devolvió {}",
                resp.status()
            )));
        }
        let bytes = resp.bytes().map_err(|e| TtsError::Io(e.to_string()))?;
        let (mut samples, sr) =
            playback::read_wav_mono_f32_from_bytes(&bytes).map_err(TtsError::Synthesis)?;
        playback::normalize_peak_dbfs(&mut samples, TARGET_DBFS);
        self.playback
            .play(samples, sr, self.volume)
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
        // Voz única por defecto (los servidores exponen su voz es-419).
        Ok(vec![VozEscucha {
            id: self.cfg.default_voice.to_string(),
            nombre: format!("{} (es-419)", self.cfg.id.display_name()),
            idioma: "es".to_string(),
            es_espanol: true,
        }])
    }
}

/// Pide un puerto efímero libre al SO y lo devuelve (cerrando el listener; hay
/// una pequeña ventana de carrera, aceptable para localhost).
fn free_port() -> Option<u16> {
    TcpListener::bind("127.0.0.1:0")
        .ok()
        .and_then(|l| l.local_addr().ok())
        .map(|a| a.port())
}

/// Ruta del intérprete del venv de un runtime.
pub fn venv_python(runtime_dir: &std::path::Path) -> PathBuf {
    if cfg!(windows) {
        runtime_dir.join(".venv").join("Scripts").join("python.exe")
    } else {
        runtime_dir.join(".venv").join("bin").join("python")
    }
}

/// Ejecuta `uv` con argumentos (aprovisionamiento de runtimes en el 1er uso).
/// Requiere `uv` en el PATH; devuelve un error accionable si no está. Corre en
/// un hilo bloqueante para no estancar el executor async.
pub async fn run_uv(args: &[&str]) -> Result<(), String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let out = std::process::Command::new("uv")
            .args(&owned)
            .output()
            .map_err(|e| format!("no se pudo ejecutar 'uv' (¿instalado?): {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "uv falló: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        Ok(())
    })
    .await
    .map_err(|e| format!("tarea uv abortó: {e}"))?
}
