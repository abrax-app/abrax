//! Motor genérico "servidor Python local": ABRAX arranca un servidor de síntesis
//! (nuestro script MIT, embebido en el binario) usando un runtime Python
//! **provisionado aparte en el primer uso** (kokoro-onnx, que arrastra espeak-ng
//! GPL). El runtime NUNCA se embebe ni redistribuye en el repo MIT ("mere
//! aggregation"): se aprovisiona con `uv` en `<datadir>/tts/runtime/<x>`. Kokoro
//! es una configuración de este motor. El audio va y viene por 127.0.0.1 — nunca
//! sale del equipo.

use std::io::{BufRead, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Stdio};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use tauri::{AppHandle, Emitter};

use super::engine::{EngineId, TtsEngine, TtsError, TtsOptions};
use super::hardware::{GpuVendor, HardwareInfo};
use super::playback::{self, PlaybackService, TARGET_DBFS};
use crate::managers::escucha::VozEscucha;

/// Una voz que expone el motor (parte del "reparto" es/en, masculino/femenino).
pub struct VoiceSpec {
    /// Id que entiende el servidor (p.ej. `em_alex` en Kokoro).
    pub id: &'static str,
    /// Nombre visible en el selector.
    pub display: &'static str,
    /// Código de idioma para la síntesis (`es` / `en`).
    pub lang: &'static str,
    pub es_espanol: bool,
}

/// Configuración estática de un motor basado en servidor Python.
pub struct PyServerConfig {
    pub id: EngineId,
    /// Fuente del script del servidor (embebida por `include_str!`).
    pub server_source: &'static str,
    /// Requiere GPU compatible (CUDA/MPS) para estar disponible.
    pub needs_gpu: bool,
    /// Argumento de dispositivo que se pasa al servidor.
    pub device_arg: &'static str,
    /// Voces disponibles (la primera es la de por defecto).
    pub voices: &'static [VoiceSpec],
}

pub struct PyServerEngine {
    cfg: &'static PyServerConfig,
    app: AppHandle,
    runtime_dir: PathBuf,
    playback: std::sync::Arc<PlaybackService>,
    volume: f32,
    child: Option<Child>,
    base_url: String,
}

impl PyServerEngine {
    pub fn new(
        cfg: &'static PyServerConfig,
        app: AppHandle,
        runtime_dir: PathBuf,
        playback: std::sync::Arc<PlaybackService>,
        volume: f32,
    ) -> Self {
        Self {
            cfg,
            app,
            runtime_dir,
            playback,
            volume,
            child: None,
            base_url: String::new(),
        }
    }

    /// ¿Está el runtime aprovisionado **y completo**?
    ///
    /// Antes bastaba con que existiera el intérprete, y eso dejaba al usuario
    /// atrapado: un venv A MEDIAS —con `Scripts/python.exe` pero sin `pyvenv.cfg`
    /// ni paquetes— se reportaba como instalado, así que la UI no ofrecía
    /// reinstalarlo y el motor fallaba para siempre con «No pyvenv.cfg file».
    ///
    /// Encontrado el 30/07 en un equipo real, en los DOS motores a la vez. La
    /// causa más probable es la desinstalación: el `RmDir /r` del datadir borró el
    /// venv a medias y dejó el `python.exe` atrás. Un borrado parcial es normal
    /// —un archivo en uso, permisos—, así que la comprobación tiene que resistirlo.
    ///
    /// Se exigen las tres señales de un venv sano. `pyvenv.cfg` es la decisiva:
    /// la escribe `uv venv` al TERMINAR, así que su ausencia significa
    /// «interrumpido» sin ambigüedad.
    pub fn is_provisioned(runtime_dir: &std::path::Path) -> bool {
        let venv = runtime_dir.join(".venv");
        venv_python(runtime_dir).is_file()
            && venv.join("pyvenv.cfg").is_file()
            && Self::tiene_paquetes(&venv)
    }

    /// ¿El venv tiene algo instalado? Un venv creado pero sin `pip install` no
    /// sirve: el servidor arrancaría y moriría por un import que falta.
    fn tiene_paquetes(venv: &std::path::Path) -> bool {
        // Windows: Lib/site-packages · Unix: lib/pythonX.Y/site-packages
        let directo = venv.join("Lib").join("site-packages");
        if directo.is_dir() {
            return std::fs::read_dir(&directo)
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
        }
        let lib = venv.join("lib");
        std::fs::read_dir(&lib)
            .map(|entradas| {
                entradas.flatten().any(|e| {
                    let sp = e.path().join("site-packages");
                    sp.is_dir()
                        && std::fs::read_dir(&sp)
                            .map(|mut d| d.next().is_some())
                            .unwrap_or(false)
                })
            })
            .unwrap_or(false)
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

        // Aviso a la UI: el modelo se está cargando (la 1.ª vez tarda unos
        // segundos). Se libera con `tts-engine-ready`.
        let _ = self.app.emit("tts-engine-loading", self.cfg.id);

        let mut child = crate::utils::comando_silencioso(&python)
            .arg(&script)
            .arg("--port")
            .arg(port.to_string())
            .arg("--device")
            .arg(self.cfg.device_arg)
            .current_dir(&self.runtime_dir)
            .stdout(Stdio::piped())
            // stderr CAPTURADO, no descartado. Estaba en `Stdio::null()` y por eso
            // cualquier fallo de Python —un paquete que falta, el venv roto,
            // edge-tts sin instalar— se perdía y lo único que llegaba al usuario
            // era «no respondió a tiempo», que describe el síntoma y esconde la
            // causa. Con 180 s de plazo, «no respondió» casi nunca es lentitud:
            // es que el proceso murió. Reportado el 30/07 con el motor Online.
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| TtsError::Io(format!("no se pudo lanzar el servidor: {e}")))?;

        // El stderr se drena en su propio hilo y se guarda: si el proceso muere,
        // sus últimas líneas son el diagnóstico. Se acota a las últimas para no
        // acumular sin límite si el servidor se pone a escupir avisos.
        let motivo = Arc::new(Mutex::new(Vec::<String>::new()));
        if let Some(stderr) = child.stderr.take() {
            let motivo = motivo.clone();
            std::thread::spawn(move || {
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines().map_while(Result::ok) {
                    if let Ok(mut v) = motivo.lock() {
                        v.push(line);
                        if v.len() > 12 {
                            v.remove(0);
                        }
                    }
                }
            });
        }

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

        let ready = rx.recv_timeout(Duration::from_secs(180));
        // Libera el indicador de la UI pase lo que pase.
        let _ = self.app.emit("tts-engine-ready", self.cfg.id);
        match ready {
            Ok(()) => {
                self.child = Some(child);
                self.base_url = format!("http://127.0.0.1:{port}");
                Ok(())
            }
            Err(_) => {
                // ¿Murió, o de verdad tardó? Son cosas distintas y el mensaje debe
                // distinguirlas: con 180 s de plazo, «tardó» casi nunca es cierto.
                let salida = child.try_wait().ok().flatten();
                let _ = child.kill();
                let detalle = motivo
                    .lock()
                    .ok()
                    .map(|v| v.join(" | "))
                    .filter(|s| !s.trim().is_empty());

                let msg = match (salida, detalle) {
                    (Some(code), Some(d)) => {
                        format!("el servidor de voz se cerró ({code}): {d}")
                    }
                    (Some(code), None) => format!(
                        "el servidor de voz se cerró ({code}) sin decir por qué. \
                         Prueba a reinstalar el motor desde Escucha."
                    ),
                    (None, Some(d)) => format!("el servidor de voz no arrancó: {d}"),
                    (None, None) => "el servidor de voz no respondió a tiempo".into(),
                };
                log::warn!("[tts] {msg}");
                Err(TtsError::NotAvailable(msg))
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
        (!self.cfg.needs_gpu || Self::gpu_compatible(hw)) && Self::is_provisioned(&self.runtime_dir)
    }

    fn speak(
        &mut self,
        text: &str,
        voice: Option<&str>,
        opts: &TtsOptions,
    ) -> Result<(), TtsError> {
        self.ensure_server()?;
        // Voz pedida si es válida; si no, la primera del reparto. El idioma sale
        // de la voz (es/en) para que el servidor fonemice correctamente.
        let voz = voice
            .filter(|v| self.cfg.voices.iter().any(|s| s.id == *v))
            .or_else(|| self.cfg.voices.first().map(|s| s.id))
            .unwrap_or("");
        let lang = self
            .cfg
            .voices
            .iter()
            .find(|s| s.id == voz)
            .map(|s| s.lang)
            .unwrap_or("es");
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .map_err(|e| TtsError::Io(e.to_string()))?;
        // `rate` (multiplicador de velocidad) lo usan todos los servidores; `pitch`
        // (Hz) solo el online (edge-tts) — Kokoro lo ignora.
        let resp = client
            .post(format!("{}/synthesize", self.base_url))
            .json(&serde_json::json!({
                "text": text,
                "language_id": lang,
                "voice": voz,
                "rate": opts.rate,
                "pitch": opts.pitch_hz,
            }))
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
        Ok(self
            .cfg
            .voices
            .iter()
            .map(|s| VozEscucha {
                id: s.id.to_string(),
                nombre: s.display.to_string(),
                idioma: s.lang.to_string(),
                es_espanol: s.es_espanol,
            })
            .collect())
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

/// Borra un venv previo antes de crearlo de nuevo.
///
/// Reinstalar tiene que ser un ARREGLO, y sobre restos no lo es: `uv venv` sobre
/// un venv corrupto puede darlo por bueno y dejarlo igual de roto. El caso que lo
/// motivó (30/07): una desinstalación borró el datadir a medias y dejó el
/// `python.exe` sin `pyvenv.cfg`; sin limpiar antes, cada reintento reproducía el
/// mismo estado.
///
/// Es best-effort a propósito: si algo impide borrar (un archivo en uso), se sigue
/// y que `uv` lo intente — fallar aquí sería peor que intentarlo.
pub fn limpiar_venv(runtime_dir: &std::path::Path) {
    let venv = runtime_dir.join(".venv");
    if !venv.exists() {
        return;
    }
    // PRIMERO matar lo que corra DESDE ahí, o el borrado falla con «Acceso
    // denegado» y todo lo demás es inútil.
    matar_procesos_del_runtime(runtime_dir);
    log::info!("[tts] limpiando venv previo en {}", venv.display());
    if let Err(e) = std::fs::remove_dir_all(&venv) {
        log::warn!("[tts] no se pudo limpiar el venv ({e}); se intenta igual");
    }
}

/// Mata los servidores de voz HUÉRFANOS que corran desde `runtime_dir`.
///
/// # Por qué hace falta
///
/// El servidor es un `python.exe` que vive DENTRO del venv. Si la app muere sin
/// pasar por `Drop` —un cierre forzado, un cuelgue— el hijo sobrevive y se queda
/// reteniendo su propio ejecutable. Entonces:
///
///   · `remove_dir_all` falla con «Acceso denegado» (os error 5);
///   · `uv venv` se niega con «A directory already exists»;
///   · y reinstalar deja de ser posible PARA SIEMPRE desde la app.
///
/// Medido el 30/07 en un equipo real: TRES pythons huérfanos de arranques
/// anteriores bloqueando los dos motores a la vez.
///
/// Se filtra por RUTA, nunca por nombre: matar todos los `python.exe` del sistema
/// se llevaría por delante el trabajo del usuario.
fn matar_procesos_del_runtime(runtime_dir: &std::path::Path) {
    let patron = format!("{}*", runtime_dir.display());
    let script = format!(
        "Get-CimInstance Win32_Process | Where-Object {{ $_.ExecutablePath -and          $_.ExecutablePath -like '{patron}' }} | ForEach-Object {{          try {{ Stop-Process -Id $_.ProcessId -Force -ErrorAction Stop }} catch {{}} }}"
    );
    #[cfg(target_os = "windows")]
    {
        let salida = crate::utils::comando_silencioso("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &script])
            .output();
        match salida {
            Ok(o) if o.status.success() => {
                log::info!("[tts] procesos huérfanos del runtime cerrados")
            }
            Ok(o) => log::warn!(
                "[tts] no se pudieron cerrar los huérfanos: {}",
                String::from_utf8_lossy(&o.stderr).trim()
            ),
            Err(e) => log::warn!("[tts] no se pudo consultar los procesos: {e}"),
        }
        // Windows tarda un instante en soltar el archivo tras terminar el proceso.
        std::thread::sleep(std::time::Duration::from_millis(300));
    }
    #[cfg(not(target_os = "windows"))]
    {
        // En Unix un ejecutable en uso SÍ se puede borrar, así que el bloqueo no
        // se da: no hace falta matar nada para reinstalar.
        let _ = script;
    }
}

/// Ejecuta `uv` con argumentos (aprovisionamiento de runtimes en el 1er uso).
/// Requiere `uv` en el PATH; devuelve un error accionable si no está. Corre en
/// un hilo bloqueante para no estancar el executor async.
pub async fn run_uv(args: &[&str]) -> Result<(), String> {
    let owned: Vec<String> = args.iter().map(|s| s.to_string()).collect();
    tokio::task::spawn_blocking(move || {
        let out = crate::utils::comando_silencioso("uv")
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
