//! Descargador aislado de assets TTS (voces `.onnx` y runtimes).
//!
//! Reutiliza el MISMO patrón probado del gestor de modelos ASR
//! (`managers/model.rs`): `reqwest` con `connect_timeout`/`read_timeout` (sin
//! timeout total), descarga a `.partial`, verificación **sha256**, y extracción
//! de archivos. Pero vive en su propio árbol `<datadir>/tts/` — **sin tocar el
//! catálogo ASR ni el path del dictado** (regla dura: no romper el dictado).
//!
//! El **runtime GPL** (piper.exe embebe espeak-ng, GPL-3.0) se descarga por aquí
//! en el primer uso: NUNCA se embebe ni se redistribuye en el repo MIT
//! ("mere aggregation"). Ver `LICENSES-THIRD-PARTY.md`.

use std::path::{Path, PathBuf};

use futures_util::StreamExt;
use serde::Serialize;
use sha2::{Digest, Sha256};
use specta::Type;
use tauri::{AppHandle, Emitter};

/// Progreso de descarga de un asset TTS (evento `tts-download-progress`).
#[derive(Serialize, Clone, Type)]
pub struct TtsDownloadProgress {
    pub asset_id: String,
    pub downloaded: u64,
    pub total: u64,
    pub percentage: f64,
}

/// Raíz de los datos TTS: `<datadir>/tts` (portable-aware). Se crea si no existe.
pub fn tts_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = crate::portable::app_data_dir(app)
        .map_err(|e| format!("no se pudo resolver el datadir: {e}"))?
        .join("tts");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Carpeta de voces neuronales: `<datadir>/tts/voices`.
pub fn voices_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = tts_dir(app)?.join("voices");
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("no se pudo crear {}: {e}", dir.display()))?;
    Ok(dir)
}

/// Carpeta de runtimes (piper, kokoro): `<datadir>/tts/runtime/<name>`.
pub fn runtime_dir(app: &AppHandle, name: &str) -> Result<PathBuf, String> {
    let dir = tts_dir(app)?.join("runtime").join(name);
    Ok(dir)
}

/// sha256 hex de un archivo, en trozos de 64 KiB (como el gestor de modelos).
pub fn compute_sha256(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Verifica el sha256 esperado; en desajuste BORRA el archivo (para que el
/// próximo intento empiece limpio) y devuelve error accionable.
fn verify_sha256(path: &Path, expected: &str, asset_id: &str) -> Result<(), String> {
    match compute_sha256(path) {
        Ok(actual) if actual.eq_ignore_ascii_case(expected) => Ok(()),
        Ok(actual) => {
            let _ = std::fs::remove_file(path);
            Err(format!(
                "verificación fallida de '{asset_id}': sha256 esperado {expected}, obtenido {actual}. Reintenta."
            ))
        }
        Err(e) => {
            let _ = std::fs::remove_file(path);
            Err(format!(
                "no se pudo verificar '{asset_id}': {e}. Reintenta."
            ))
        }
    }
}

/// Descarga una URL a `dest` (archivo simple) verificando su sha256. Cliente con
/// `connect_timeout`/`read_timeout` (sin timeout total: archivos grandes tardan
/// minutos legítimamente). Progreso por `tts-download-progress` (throttle 100ms).
pub async fn download_file(
    app: &AppHandle,
    asset_id: &str,
    url: &str,
    expected_sha256: &str,
    dest: &Path,
) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let partial = dest.with_extension("partial");

    let client = reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(30))
        .read_timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let response = client.get(url).send().await.map_err(|e| e.to_string())?;
    if !response.status().is_success() {
        return Err(format!(
            "descarga de '{asset_id}' falló: HTTP {}",
            response.status()
        ));
    }
    let total = response.content_length().unwrap_or(0);

    let mut file = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();
    let mut last_emit = std::time::Instant::now();
    let throttle = std::time::Duration::from_millis(100);
    emit_progress(app, asset_id, 0, total);

    use std::io::Write;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        if last_emit.elapsed() >= throttle {
            emit_progress(app, asset_id, downloaded, total);
            last_emit = std::time::Instant::now();
        }
    }
    file.flush().map_err(|e| e.to_string())?;
    drop(file);
    emit_progress(app, asset_id, downloaded, total);

    if total > 0 {
        let actual = partial.metadata().map_err(|e| e.to_string())?.len();
        if actual != total {
            let _ = std::fs::remove_file(&partial);
            return Err(format!(
                "descarga incompleta de '{asset_id}': esperado {total} bytes, obtenido {actual}"
            ));
        }
    }

    // Verificación sha256 en un hilo bloqueante (no estanca el executor async).
    let vpath = partial.clone();
    let vexp = expected_sha256.to_string();
    let vid = asset_id.to_string();
    tokio::task::spawn_blocking(move || verify_sha256(&vpath, &vexp, &vid))
        .await
        .map_err(|e| format!("tarea de verificación abortó: {e}"))??;

    std::fs::rename(&partial, dest).map_err(|e| e.to_string())?;
    Ok(())
}

/// Descarga un archivo comprimido (`.zip` o `.tar.gz`) verificando su sha256 y lo
/// extrae dentro de `dest_dir` (extracción atómica: a un dir temporal y luego
/// `rename`). Usado para runtimes (piper, etc.) que traen un ejecutable + datos.
pub async fn download_and_extract_archive(
    app: &AppHandle,
    asset_id: &str,
    url: &str,
    expected_sha256: &str,
    dest_dir: &Path,
) -> Result<(), String> {
    let staging = tts_dir(app)?.join(format!("{asset_id}.archive"));
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    download_file(app, asset_id, url, expected_sha256, &staging).await?;

    let temp_extract = dest_dir.with_extension("extracting");
    if temp_extract.exists() {
        let _ = std::fs::remove_dir_all(&temp_extract);
    }
    std::fs::create_dir_all(&temp_extract).map_err(|e| e.to_string())?;

    let extract_result = if url.ends_with(".zip") {
        extract_zip(&staging, &temp_extract)
    } else if url.ends_with(".tar.gz") || url.ends_with(".tgz") {
        extract_tar_gz(&staging, &temp_extract)
    } else {
        Err(format!(
            "formato de archivo desconocido para '{asset_id}': {url}"
        ))
    };

    if let Err(e) = extract_result {
        let _ = std::fs::remove_dir_all(&temp_extract);
        let _ = std::fs::remove_file(&staging);
        return Err(e);
    }

    if dest_dir.exists() {
        let _ = std::fs::remove_dir_all(dest_dir);
    }
    if let Some(parent) = dest_dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::rename(&temp_extract, dest_dir).map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&staging);
    Ok(())
}

fn extract_zip(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("zip inválido: {e}"))?;
    zip.extract(dest)
        .map_err(|e| format!("no se pudo extraer el zip: {e}"))?;
    Ok(())
}

fn extract_tar_gz(archive: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    let tar = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(tar);
    archive
        .unpack(dest)
        .map_err(|e| format!("no se pudo extraer el tar.gz: {e}"))?;
    Ok(())
}

fn emit_progress(app: &AppHandle, asset_id: &str, downloaded: u64, total: u64) {
    let percentage = if total > 0 {
        (downloaded as f64 / total as f64) * 100.0
    } else {
        0.0
    };
    let _ = app.emit(
        "tts-download-progress",
        TtsDownloadProgress {
            asset_id: asset_id.to_string(),
            downloaded,
            total,
            percentage,
        },
    );
}
