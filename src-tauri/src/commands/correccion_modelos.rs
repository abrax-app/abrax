//! Comandos de la UI para el catálogo de modelos LLM opcionales de "Pulido con
//! IA": listar, descargar (con progreso y sha256), cancelar, borrar y elegir el
//! activo. Todo opcional; nada corre hasta que el usuario elige un modelo.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::AsyncWriteExt;

use crate::correccion::modelos::{self, ModeloCorreccion};
use crate::correccion::motor_sidecar::SidecarManager;
use crate::managers::model::ModelManager;
use crate::settings::{get_settings, write_settings};

/// Carpeta de modelos EFECTIVA (respeta el disco elegido en ajustes). Se
/// resuelve vía el `ModelManager`, así los modelos LLM viven en el mismo disco
/// que los de transcripción (en subcarpetas distintas).
fn carpeta_modelos(app: &AppHandle) -> PathBuf {
    app.state::<Arc<ModelManager>>().models_dir()
}

/// Descargas en curso (id → bandera de cancelación). Estado de Tauri.
#[derive(Default)]
pub struct EstadoDescargas {
    activas: Mutex<HashMap<String, Arc<AtomicBool>>>,
}

/// Un modelo del catálogo con su estado local, para la UI.
#[derive(Serialize, Type, Clone)]
pub struct ModeloEstado {
    pub modelo: ModeloCorreccion,
    pub descargado: bool,
    pub descargando: bool,
    pub seleccionado: bool,
}

/// Progreso de descarga, emitido como evento `correccion-modelo-progreso`.
#[derive(Serialize, Type, Clone)]
pub struct ProgresoDescargaModelo {
    pub modelo_id: String,
    pub bajados: u64,
    pub total: u64,
}

/// Lista el catálogo con el estado de cada modelo (descargado / descargando /
/// seleccionado).
#[tauri::command]
#[specta::specta]
pub fn listar_modelos_correccion(app: AppHandle) -> Result<Vec<ModeloEstado>, String> {
    let models_dir = carpeta_modelos(&app);
    let settings = get_settings(&app);
    let estado = app.state::<EstadoDescargas>();
    let activas = estado.activas.lock().unwrap();
    Ok(modelos::catalogo()
        .into_iter()
        .map(|m| ModeloEstado {
            descargado: modelos::esta_descargado(&models_dir, &m),
            descargando: activas.contains_key(&m.id),
            seleccionado: settings.correccion_modelo_local.as_deref() == Some(m.id.as_str()),
            modelo: m,
        })
        .collect())
}

/// Descarga el GGUF de un modelo a la carpeta de modelos de corrección, con
/// eventos de progreso, verificación sha256 y `.partial` + rename atómico.
/// Cancelable con [`cancelar_descarga_correccion`].
#[tauri::command]
#[specta::specta]
pub async fn descargar_modelo_correccion(app: AppHandle, modelo_id: String) -> Result<(), String> {
    let m = modelos::por_id(&modelo_id).ok_or_else(|| "modelo desconocido".to_string())?;
    let models_dir = carpeta_modelos(&app);
    let carpeta = modelos::carpeta(&models_dir);
    tokio::fs::create_dir_all(&carpeta)
        .await
        .map_err(|e| format!("no se pudo crear la carpeta: {e}"))?;
    let destino = modelos::ruta_gguf(&models_dir, &m);
    if destino.is_file() {
        return Ok(()); // ya descargado
    }
    let parcial = carpeta.join(format!("{}.partial", m.archivo));

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let estado = app.state::<EstadoDescargas>();
        estado
            .activas
            .lock()
            .unwrap()
            .insert(modelo_id.clone(), cancel.clone());
    }

    let res = bajar_con_progreso(&app, &m, &parcial, &cancel).await;

    {
        let estado = app.state::<EstadoDescargas>();
        estado.activas.lock().unwrap().remove(&modelo_id);
    }

    if res.is_err() {
        let _ = tokio::fs::remove_file(&parcial).await;
        return res;
    }

    // Verifica integridad en un hilo bloqueante (I/O de disco).
    let esperado = m.sha256.clone();
    let parcial2 = parcial.clone();
    let real = tokio::task::spawn_blocking(move || crate::hashing::sha256_file(&parcial2))
        .await
        .map_err(|e| format!("tarea sha256: {e}"))?
        .map_err(|e| format!("sha256: {e}"))?;
    if !real.eq_ignore_ascii_case(&esperado) {
        let _ = tokio::fs::remove_file(&parcial).await;
        return Err("el archivo descargado no coincide (sha256)".to_string());
    }
    tokio::fs::rename(&parcial, &destino)
        .await
        .map_err(|e| format!("no se pudo finalizar la descarga: {e}"))?;
    Ok(())
}

async fn bajar_con_progreso(
    app: &AppHandle,
    m: &ModeloCorreccion,
    parcial: &Path,
    cancel: &Arc<AtomicBool>,
) -> Result<(), String> {
    // connect_timeout acota el establecimiento; el modelo pesa GB, así que NO se
    // pone timeout total (bloquearía descargas grandes legítimas), pero el bucle
    // revisa la cancelación en cada chunk.
    let cliente = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(30))
        .build()
        .map_err(|e| format!("cliente HTTP: {e}"))?;
    let resp = cliente
        .get(modelos::url_descarga(m))
        .send()
        .await
        .map_err(|e| format!("descarga: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("descarga: estado {}", resp.status()));
    }
    let total = resp.content_length().unwrap_or(m.tamano_bytes);
    let mut archivo = tokio::fs::File::create(parcial)
        .await
        .map_err(|e| format!("crear parcial: {e}"))?;
    let mut stream = resp.bytes_stream();
    let mut bajados: u64 = 0;
    let mut ultimo = Instant::now();
    let emitir = |bajados: u64| {
        let _ = app.emit(
            "correccion-modelo-progreso",
            ProgresoDescargaModelo {
                modelo_id: m.id.clone(),
                bajados,
                total,
            },
        );
    };
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            return Err("descarga cancelada".to_string());
        }
        let chunk = chunk.map_err(|e| format!("stream: {e}"))?;
        archivo
            .write_all(&chunk)
            .await
            .map_err(|e| format!("escribir: {e}"))?;
        bajados += chunk.len() as u64;
        if ultimo.elapsed() >= Duration::from_millis(200) {
            ultimo = Instant::now();
            emitir(bajados);
        }
    }
    archivo.flush().await.map_err(|e| format!("flush: {e}"))?;
    emitir(bajados);
    Ok(())
}

/// Marca una descarga en curso para que se cancele (el bucle la ve y aborta).
#[tauri::command]
#[specta::specta]
pub fn cancelar_descarga_correccion(app: AppHandle, modelo_id: String) {
    if let Some(flag) = app
        .state::<EstadoDescargas>()
        .activas
        .lock()
        .unwrap()
        .get(&modelo_id)
    {
        flag.store(true, Ordering::Relaxed);
    }
}

/// Borra un modelo descargado. Si era el seleccionado, lo deselecciona y apaga
/// el sidecar.
#[tauri::command]
#[specta::specta]
pub fn eliminar_modelo_correccion(app: AppHandle, modelo_id: String) -> Result<(), String> {
    let m = modelos::por_id(&modelo_id).ok_or_else(|| "modelo desconocido".to_string())?;
    let models_dir = carpeta_modelos(&app);
    let ruta = modelos::ruta_gguf(&models_dir, &m);
    if ruta.is_file() {
        std::fs::remove_file(&ruta).map_err(|e| format!("no se pudo borrar: {e}"))?;
    }
    let mut settings = get_settings(&app);
    if settings.correccion_modelo_local.as_deref() == Some(modelo_id.as_str()) {
        settings.correccion_modelo_local = None;
        write_settings(&app, settings);
        if let Some(sc) = app.try_state::<Arc<SidecarManager>>() {
            sc.stop();
        }
    }
    Ok(())
}

/// Elige (o deselecciona con `None`) el modelo activo de Pulido. Si el elegido
/// está descargado, lo pre-calienta en segundo plano para que esté listo antes
/// del primer dictado.
#[tauri::command]
#[specta::specta]
pub fn seleccionar_modelo_correccion(
    app: AppHandle,
    modelo_id: Option<String>,
) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.correccion_modelo_local = modelo_id.clone();
    write_settings(&app, settings);

    match modelo_id {
        Some(id) => {
            if let Some(m) = modelos::por_id(&id) {
                let models_dir = carpeta_modelos(&app);
                // El GGUF vive en el disco elegido (models_dir); el runtime del
                // sidecar vive en los datos de la app (app_data_dir).
                if modelos::esta_descargado(&models_dir, &m) {
                    if let (Ok(app_data), Some(sc)) = (
                        crate::portable::app_data_dir(&app),
                        app.try_state::<Arc<SidecarManager>>(),
                    ) {
                        let mgr = sc.inner().clone();
                        let gguf = modelos::ruta_gguf(&models_dir, &m);
                        mgr.solicitar_arranque(app_data, id, gguf);
                    }
                }
            }
        }
        None => {
            if let Some(sc) = app.try_state::<Arc<SidecarManager>>() {
                sc.stop();
            }
        }
    }
    Ok(())
}
