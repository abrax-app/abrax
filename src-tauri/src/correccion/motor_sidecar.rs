//! Sidecar del "Pulido con IA" local: gestiona el runtime `llama-server`
//! (llama.cpp) como **proceso aislado** y su ciclo de vida.
//!
//! Por qué proceso aparte y no embebido: llama.cpp y whisper.cpp exportan los
//! mismos símbolos `ggml_*` sin versionar; en el mismo proceso se pisarían y
//! podrían romper el dictado. En procesos separados cada uno trae su propio
//! ggml y no hay conflicto. El dictado nunca depende de esto.
//!
//! Runtime-off por defecto: nada se descarga ni se lanza hasta que el usuario
//! elige un modelo. El runtime (`llama-server`) se baja on-first-use con
//! verificación sha256 (patrón de los modelos de transcripción); la variante
//! (CPU o Vulkan) se elige según el hardware detectado. Apagado por inactividad
//! para no dejar un modelo grande ocupando RAM/VRAM.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::managers::tts::hardware::{detect_hardware, GpuVendor, HardwareInfo};

/// Variante del runtime a usar según el hardware.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variante {
    /// CPU puro. Portable, sin drivers; lento con modelos grandes.
    Cpu,
    /// Aceleración GPU vía Vulkan (comparte el stack que ABRAX ya usa para
    /// whisper, pero en su propio proceso).
    Vulkan,
}

impl Variante {
    fn etiqueta(self) -> &'static str {
        match self {
            Variante::Cpu => "cpu",
            Variante::Vulkan => "vulkan",
        }
    }
}

/// Elige la variante según la GPU detectada: una GPU dedicada usable → Vulkan;
/// si no, CPU. En macOS el runtime es único (Metal), así que la etiqueta es
/// informativa.
pub fn elegir_variante(gpu: GpuVendor) -> Variante {
    match gpu {
        // GPUs con Vulkan sólido y VRAM propia.
        GpuVendor::Nvidia | GpuVendor::Amd => Variante::Vulkan,
        // Intel/Apple/desconocida/ninguna → CPU por seguridad (Vulkan iGPU rinde
        // parecido a CPU y añade superficie de fallo de drivers).
        _ => Variante::Cpu,
    }
}

/// Archivo del runtime a descargar para la plataforma y variante actuales.
pub struct RuntimeAsset {
    pub url: String,
    pub sha256: &'static str,
    pub es_zip: bool,
}

const BASE: &str = "https://github.com/ggml-org/llama.cpp/releases/download/b10068";

/// Devuelve el asset del runtime para `variante`, o `None` si la plataforma no
/// está soportada (se degradaría a "sin sidecar", nunca rompe el dictado).
pub fn runtime_asset(variante: Variante) -> Option<RuntimeAsset> {
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    {
        return Some(match variante {
            Variante::Cpu => RuntimeAsset {
                url: format!("{BASE}/llama-b10068-bin-win-cpu-x64.zip"),
                sha256: "01d5f30876acfb4a0be59396710f450213495c7181d8fbcce2fad045835ceb89",
                es_zip: true,
            },
            Variante::Vulkan => RuntimeAsset {
                url: format!("{BASE}/llama-b10068-bin-win-vulkan-x64.zip"),
                sha256: "4f3e6fd215fdf22d2fd6232a5501f9e791a93d9193db4faf59e391eff90f6169",
                es_zip: true,
            },
        });
    }
    // Otras plataformas: sha256 pendientes de fijar al empaquetar en esos
    // objetivos. Hasta entonces, sin sidecar (degradación segura).
    #[cfg(not(all(target_os = "windows", target_arch = "x86_64")))]
    {
        let _ = variante;
        None
    }
}

/// Nombre del ejecutable de `llama-server` en esta plataforma.
fn nombre_binario() -> &'static str {
    if cfg!(windows) {
        "llama-server.exe"
    } else {
        "llama-server"
    }
}

/// Carpeta donde vive el runtime extraído, aislada del árbol de modelos ASR.
pub fn runtime_dir(datadir: &Path, variante: Variante) -> PathBuf {
    datadir
        .join("correccion")
        .join("runtime")
        .join(format!("llama-{}", variante.etiqueta()))
}

/// Busca `llama-server(.exe)` bajo `dir`, hasta 2 niveles (el zip puede extraer
/// a una subcarpeta). `None` si no está.
pub fn find_llama_server(dir: &Path) -> Option<PathBuf> {
    let bin = nombre_binario();
    let directo = dir.join(bin);
    if directo.is_file() {
        return Some(directo);
    }
    let entradas = std::fs::read_dir(dir).ok()?;
    for e in entradas.flatten() {
        let p = e.path();
        if p.is_dir() {
            let anidado = p.join(bin);
            if anidado.is_file() {
                return Some(anidado);
            }
        }
    }
    None
}

/// SHA-256 en hex de un archivo, por trozos (no carga todo a memoria). Mismo
/// formato que `managers::model` (`{:x}` sobre el digest de sha2).
fn sha256_archivo(path: &Path) -> std::io::Result<String> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Descarga (si hace falta) y extrae el runtime, devolviendo la ruta al binario
/// `llama-server`. Idempotente: si ya está extraído y el binario existe, no baja
/// nada. Verifica sha256 del archivo descargado antes de extraer.
pub async fn ensure_runtime(datadir: &Path, variante: Variante) -> Result<PathBuf, String> {
    let dir = runtime_dir(datadir, variante);
    if let Some(bin) = find_llama_server(&dir) {
        return Ok(bin);
    }

    let asset = runtime_asset(variante)
        .ok_or_else(|| "plataforma sin runtime de Pulido disponible".to_string())?;

    std::fs::create_dir_all(&dir).map_err(|e| format!("no se pudo crear {dir:?}: {e}"))?;
    let archivo = dir.join(if asset.es_zip { "rt.zip" } else { "rt.tar.gz" });

    // Descarga a un temporal y verifica sha256.
    descargar(&asset.url, &archivo).await?;
    let real = sha256_archivo(&archivo).map_err(|e| format!("sha256: {e}"))?;
    if !real.eq_ignore_ascii_case(asset.sha256) {
        let _ = std::fs::remove_file(&archivo);
        return Err(format!(
            "sha256 del runtime no coincide (esperado {}, obtenido {real})",
            asset.sha256
        ));
    }

    // Extrae en un hilo bloqueante (I/O de disco).
    let dir2 = dir.clone();
    let es_zip = asset.es_zip;
    let archivo2 = archivo.clone();
    tokio::task::spawn_blocking(move || extraer(&archivo2, &dir2, es_zip))
        .await
        .map_err(|e| format!("tarea de extracción: {e}"))??;
    let _ = std::fs::remove_file(&archivo);

    find_llama_server(&dir)
        .ok_or_else(|| "no se encontró llama-server tras extraer el runtime".to_string())
}

async fn descargar(url: &str, destino: &Path) -> Result<(), String> {
    use tokio::io::AsyncWriteExt;
    // Timeout TOTAL además del de conexión: una descarga que se establece pero
    // se estanca a mitad no debe colgar la tarea de arranque para siempre
    // (regla dura: timeouts en todo cliente HTTP). El runtime pesa ~20-35 MB;
    // 5 min cubren redes lentas de sobra.
    let cliente = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .timeout(Duration::from_secs(300))
        .build()
        .map_err(|e| format!("cliente HTTP: {e}"))?;
    let resp = cliente
        .get(url)
        .send()
        .await
        .map_err(|e| format!("descarga: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("descarga: estado {}", resp.status()));
    }
    let mut f = tokio::fs::File::create(destino)
        .await
        .map_err(|e| format!("crear {destino:?}: {e}"))?;
    let mut stream = resp.bytes_stream();
    use futures_util::StreamExt;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("stream: {e}"))?;
        f.write_all(&chunk)
            .await
            .map_err(|e| format!("escribir: {e}"))?;
    }
    f.flush().await.map_err(|e| format!("flush: {e}"))?;
    Ok(())
}

fn extraer(archivo: &Path, destino: &Path, es_zip: bool) -> Result<(), String> {
    let f = std::fs::File::open(archivo).map_err(|e| format!("abrir {archivo:?}: {e}"))?;
    if es_zip {
        let mut zip = zip::ZipArchive::new(f).map_err(|e| format!("zip: {e}"))?;
        zip.extract(destino)
            .map_err(|e| format!("extraer zip: {e}"))?;
    } else {
        let gz = flate2::read::GzDecoder::new(f);
        let mut tar = tar::Archive::new(gz);
        tar.unpack(destino)
            .map_err(|e| format!("extraer tar.gz: {e}"))?;
    }
    Ok(())
}

// ── ciclo de vida del proceso ────────────────────────────────────────────────

/// Un `llama-server` corriendo: el hijo, el puerto y qué modelo tiene cargado.
struct Instancia {
    child: Child,
    port: u16,
    modelo_id: String,
}

impl Drop for Instancia {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Gestiona una única instancia de `llama-server` bajo demanda, apagándola por
/// inactividad. Vive como estado de Tauri; no lanza nada hasta `solicitar_arranque`.
pub struct SidecarManager {
    inner: Mutex<Option<Instancia>>,
    last_activity: AtomicU64,
    inicio: Instant,
    /// Minutos de inactividad tras los que se descarga el modelo (0 = nunca).
    idle_min: u64,
    /// Single-flight: hay un arranque en vuelo. Evita que dos dictados seguidos
    /// lancen dos `llama-server`.
    arrancando: AtomicBool,
}

impl Default for SidecarManager {
    fn default() -> Self {
        Self {
            inner: Mutex::new(None),
            last_activity: AtomicU64::new(0),
            inicio: Instant::now(),
            idle_min: 5,
            arrancando: AtomicBool::new(false),
        }
    }
}

impl SidecarManager {
    pub fn new() -> Self {
        Self::default()
    }

    fn touch(&self) {
        let ms = self.inicio.elapsed().as_millis() as u64;
        self.last_activity.store(ms, Ordering::Relaxed);
    }

    /// Comprobación RÁPIDA (solo lock, sin lanzar nada) de si el sidecar YA
    /// corre con `modelo_id`. Devuelve el puerto o `None`. Es lo único que toca
    /// la ruta de dictado: nunca bloquea esperando un arranque.
    pub fn motor_listo(&self, modelo_id: &str) -> Option<u16> {
        let mut guard = self.inner.lock().unwrap();
        let inst = guard.as_mut()?;
        let vivo = matches!(inst.child.try_wait(), Ok(None));
        if vivo && inst.modelo_id == modelo_id {
            let port = inst.port;
            drop(guard);
            self.touch();
            Some(port)
        } else {
            None
        }
    }

    /// Pide que el sidecar arranque (o cambie de modelo) EN SEGUNDO PLANO. No
    /// bloquea: el dictado en curso usa reglas/Ollama y el modelo quedará listo
    /// para el siguiente. Single-flight: si ya hay un arranque en vuelo, no
    /// hace nada. El arranque real (descarga del runtime + carga del modelo)
    /// vive en [`Self::arrancar`].
    pub fn solicitar_arranque(
        self: &Arc<Self>,
        datadir: PathBuf,
        modelo_id: String,
        gguf: PathBuf,
    ) {
        if self.arrancando.swap(true, Ordering::AcqRel) {
            return; // ya hay uno arrancando
        }
        let this = Arc::clone(self);
        tauri::async_runtime::spawn(async move {
            if let Err(e) = this.arrancar(&datadir, &modelo_id, &gguf).await {
                log::debug!("correccion: arranque del sidecar falló ({e})");
            }
            this.arrancando.store(false, Ordering::Release);
        });
    }

    /// Arranque real (bloqueante): reutiliza si ya corre con ese modelo, si no
    /// baja el runtime (primer uso) y lanza `llama-server`. Corre en segundo
    /// plano vía [`Self::solicitar_arranque`]; nunca en la ruta de dictado.
    async fn arrancar(
        &self,
        datadir: &Path,
        modelo_id: &str,
        modelo_gguf: &Path,
    ) -> Result<u16, String> {
        let hw = detect_hardware();
        let variante = elegir_variante(hw.gpu_vendor);

        // ¿Ya está bien? (mismo modelo, proceso vivo)
        {
            let mut guard = self.inner.lock().unwrap();
            if let Some(inst) = guard.as_mut() {
                let vivo = matches!(inst.child.try_wait(), Ok(None));
                if vivo && inst.modelo_id == modelo_id {
                    self.touch();
                    return Ok(inst.port);
                }
                // Modelo distinto o proceso muerto: reemplazar (Drop mata el viejo).
                *guard = None;
            }
        }

        let bin = ensure_runtime(datadir, variante).await?;
        let inst = lanzar(&bin, modelo_id, modelo_gguf, variante, &hw).await?;
        let port = inst.port;
        // `touch()` ANTES del store: si el watcher cae entre ambos, no debe ver
        // la instancia recién nacida con un `last_activity` caduco y matarla.
        self.touch();
        *self.inner.lock().unwrap() = Some(inst);
        Ok(port)
    }

    /// Apaga el sidecar si lleva más de `idle_min` sin actividad. Pensado para
    /// llamarse desde un watcher periódico (o al cerrar la app).
    pub fn stop_si_inactivo(&self) {
        if self.idle_min == 0 {
            return;
        }
        let ahora = self.inicio.elapsed().as_millis() as u64;
        let ultimo = self.last_activity.load(Ordering::Relaxed);
        if ahora.saturating_sub(ultimo) > self.idle_min * 60_000 {
            self.stop();
        }
    }

    /// Apaga el sidecar ahora (Drop mata el proceso).
    pub fn stop(&self) {
        *self.inner.lock().unwrap() = None;
    }
}

/// Puerto libre efímero en loopback (se cierra y se pasa a llama-server; hay una
/// ventana de carrera mínima, igual que el patrón de los servidores TTS).
fn puerto_libre() -> Result<u16, String> {
    let l = std::net::TcpListener::bind("127.0.0.1:0").map_err(|e| format!("puerto: {e}"))?;
    let p = l.local_addr().map_err(|e| format!("puerto: {e}"))?.port();
    Ok(p)
}

/// Lanza `llama-server` con el modelo y espera a que responda `/health`.
async fn lanzar(
    bin: &Path,
    modelo_id: &str,
    modelo_gguf: &Path,
    variante: Variante,
    hw: &HardwareInfo,
) -> Result<Instancia, String> {
    let port = puerto_libre()?;
    let hilos = hw.cpu_threads.unwrap_or(4).clamp(1, 16).to_string();
    let work_dir = bin.parent().unwrap_or(Path::new("."));

    let mut cmd = Command::new(bin);
    cmd.current_dir(work_dir) // resuelve sus DLLs (ggml, etc.) relativas
        .arg("-m")
        .arg(modelo_gguf)
        .args(["--host", "127.0.0.1"])
        .args(["--port", &port.to_string()])
        .args(["-c", "4096"])
        .args(["--threads", &hilos])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if variante == Variante::Vulkan {
        // Descarga todas las capas posibles a la GPU.
        cmd.args(["-ngl", "999"]);
    }
    // No abrir una consola ni robar el foco (regla dura del overlay).
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }

    let child = cmd
        .spawn()
        .map_err(|e| format!("no se pudo lanzar llama-server: {e}"))?;
    let mut inst = Instancia {
        child,
        port,
        modelo_id: modelo_id.to_string(),
    };

    // Espera a /health. El primer arranque carga el modelo a RAM/VRAM: los
    // grandes tardan, por eso el presupuesto es generoso. Si el proceso muere
    // mientras tanto, abortamos.
    let cliente = reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .map_err(|e| format!("cliente: {e}"))?;
    let url = format!("http://127.0.0.1:{port}/health");
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if let Ok(Some(status)) = inst.child.try_wait() {
            return Err(format!(
                "llama-server terminó al arrancar (código {status})"
            ));
        }
        if let Ok(r) = cliente.get(&url).send().await {
            if r.status().is_success() {
                return Ok(inst);
            }
        }
        if Instant::now() > deadline {
            return Err("llama-server no respondió /health a tiempo".to_string());
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variante_segun_gpu() {
        assert_eq!(elegir_variante(GpuVendor::Nvidia), Variante::Vulkan);
        assert_eq!(elegir_variante(GpuVendor::Amd), Variante::Vulkan);
        assert_eq!(elegir_variante(GpuVendor::Intel), Variante::Cpu);
        assert_eq!(elegir_variante(GpuVendor::Apple), Variante::Cpu);
        assert_eq!(elegir_variante(GpuVendor::None), Variante::Cpu);
        assert_eq!(elegir_variante(GpuVendor::Unknown), Variante::Cpu);
    }

    #[test]
    fn runtime_dir_esta_aislado_del_arbol_asr() {
        let d = runtime_dir(Path::new("/data"), Variante::Vulkan);
        assert!(
            d.ends_with("correccion/runtime/llama-vulkan")
                || d.ends_with("correccion\\runtime\\llama-vulkan")
        );
    }

    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    #[test]
    fn asset_windows_tiene_sha_de_64_hex() {
        for v in [Variante::Cpu, Variante::Vulkan] {
            let a = runtime_asset(v).unwrap();
            assert_eq!(a.sha256.len(), 64);
            assert!(a.sha256.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(a.url.starts_with("https://github.com/ggml-org/llama.cpp"));
            assert!(a.es_zip);
        }
    }

    #[test]
    fn nombre_binario_por_plataforma() {
        let n = nombre_binario();
        if cfg!(windows) {
            assert_eq!(n, "llama-server.exe");
        } else {
            assert_eq!(n, "llama-server");
        }
    }
}
