//! Enumeración de discos/volúmenes con espacio libre, para que el usuario elija
//! dónde descargar los modelos (transcripción y LLM pesan varios GB). Solo
//! lectura; no toca nada.

use std::sync::Arc;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::managers::model::ModelManager;
use crate::settings::get_settings;

/// Un disco/volumen montado con su espacio.
#[derive(Serialize, Type, Clone)]
pub struct DiscoInfo {
    /// Punto de montaje: en Windows la unidad (`C:\`), en Unix la ruta de montaje.
    pub punto_montaje: String,
    /// Nombre/etiqueta del volumen (puede venir vacío).
    pub nombre: String,
    pub total_bytes: u64,
    pub libre_bytes: u64,
    /// `true` si es extraíble (USB, tarjeta…): útil para avisar antes de poner
    /// ahí modelos que la app espera encontrar luego.
    pub removible: bool,
}

/// Lista los discos con su espacio libre, deduplicados por punto de montaje y
/// ordenados. Best-effort: si el SO no expone discos, devuelve lista vacía.
#[tauri::command]
#[specta::specta]
pub fn listar_discos() -> Vec<DiscoInfo> {
    let discos = sysinfo::Disks::new_with_refreshed_list();
    let mut out: Vec<DiscoInfo> = discos
        .iter()
        .map(|d| DiscoInfo {
            punto_montaje: d.mount_point().to_string_lossy().into_owned(),
            nombre: d.name().to_string_lossy().into_owned(),
            total_bytes: d.total_space(),
            libre_bytes: d.available_space(),
            removible: d.is_removable(),
        })
        .filter(|d| d.total_bytes > 0)
        .collect();
    out.sort_by(|a, b| a.punto_montaje.cmp(&b.punto_montaje));
    out.dedup_by(|a, b| a.punto_montaje == b.punto_montaje);
    out
}

/// Carpeta de descarga de modelos: la efectiva y la de por defecto (para que la
/// UI muestre dónde están y ofrezca "volver a la original").
#[derive(Serialize, Type, Clone)]
pub struct CarpetaModelos {
    /// Carpeta efectiva actual (respeta la elegida en ajustes).
    pub actual: String,
    /// Carpeta por defecto (dentro de los datos de la app).
    pub por_defecto: String,
    /// `true` si el usuario eligió una carpeta distinta a la de por defecto.
    pub personalizada: bool,
}

/// Devuelve la carpeta de modelos efectiva y la de por defecto.
#[tauri::command]
#[specta::specta]
pub fn obtener_carpeta_modelos(app: AppHandle) -> CarpetaModelos {
    let mm = app.state::<Arc<ModelManager>>();
    let personalizada = get_settings(&app)
        .models_dir
        .map(|s| !s.trim().is_empty())
        .unwrap_or(false);
    CarpetaModelos {
        actual: mm.models_dir().to_string_lossy().into_owned(),
        por_defecto: mm.default_models_dir().to_string_lossy().into_owned(),
        personalizada,
    }
}
