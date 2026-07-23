//! Enumeración de discos/volúmenes con espacio libre, para que el usuario elija
//! dónde descargar los modelos (transcripción y LLM pesan varios GB), y la
//! mudanza opcional de lo ya descargado al cambiar de carpeta.

use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use log::{info, warn};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;

use crate::managers::model::ModelManager;
use crate::settings::{get_settings, write_settings};

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

/// Progreso de la mudanza de modelos (evento `mudanza-progreso`).
/// `estado` es `"progreso"`, `"listo"` o `"error"` (con `detalle`).
#[derive(Clone, Debug, Serialize, Deserialize, Type, tauri_specta::Event)]
pub struct MudanzaProgreso {
    pub estado: String,
    pub archivo: String,
    pub hechos_bytes: u64,
    pub total_bytes: u64,
    pub detalle: String,
}

/// Una sola mudanza a la vez: dos hilos moviendo la misma carpeta se pisarían.
static MUDANZA_EN_CURSO: AtomicBool = AtomicBool::new(false);

/// Cambia la carpeta de modelos y, si `mover`, muda lo ya descargado a la
/// nueva. La mudanza corre en un hilo aparte y reporta por `modelos-mudanza`;
/// el comando vuelve apenas queda lanzada. `destino: None` vuelve a la carpeta
/// por defecto.
#[tauri::command]
#[specta::specta]
pub fn cambiar_carpeta_modelos(
    app: AppHandle,
    destino: Option<String>,
    mover: bool,
) -> Result<(), String> {
    let mm = app.state::<Arc<ModelManager>>().inner().clone();
    let origen = mm.models_dir();

    let mut settings = get_settings(&app);
    settings.models_dir = destino.filter(|s| !s.trim().is_empty());
    write_settings(&app, settings);

    // Resuelve (y crea) la carpeta nueva; si es inutilizable degrada a la por
    // defecto, así que `destino_dir` siempre es real.
    let destino_dir = mm.models_dir();
    if !mover || origen == destino_dir {
        if let Err(e) = mm.rescan_local_models() {
            warn!("Rescan tras cambiar carpeta de modelos: {}", e);
        }
        return Ok(());
    }
    if destino_dir.starts_with(&origen) || origen.starts_with(&destino_dir) {
        return Err("carpetas anidadas: elige una carpeta fuera de la actual".into());
    }
    if MUDANZA_EN_CURSO.swap(true, Ordering::SeqCst) {
        return Err("ya hay una mudanza de modelos en curso".into());
    }

    std::thread::spawn(move || {
        info!(
            "Mudanza de modelos: {} → {}",
            origen.display(),
            destino_dir.display()
        );
        let resultado = mudar_contenido(&origen, &destino_dir, &mut |archivo, hechos, total| {
            emitir_mudanza(&app, "progreso", archivo, hechos, total, "");
        });
        MUDANZA_EN_CURSO.store(false, Ordering::SeqCst);
        match resultado {
            Ok(total) => {
                info!("Mudanza de modelos completa ({} bytes)", total);
                emitir_mudanza(&app, "listo", "", total, total, "");
            }
            Err(e) => {
                // Nada se pierde: cada archivo se borra del origen solo tras
                // copiarse y verificarse; lo no movido sigue donde estaba.
                warn!("Mudanza de modelos fallida: {}", e);
                emitir_mudanza(&app, "error", "", 0, 0, &e);
            }
        }
        if let Err(e) = mm.rescan_local_models() {
            warn!("Rescan tras la mudanza: {}", e);
        }
    });
    Ok(())
}

fn emitir_mudanza(
    app: &AppHandle,
    estado: &str,
    archivo: &str,
    hechos: u64,
    total: u64,
    detalle: &str,
) {
    let _ = MudanzaProgreso {
        estado: estado.to_string(),
        archivo: archivo.to_string(),
        hechos_bytes: hechos,
        total_bytes: total,
        detalle: detalle.to_string(),
    }
    .emit(app);
}

/// Muda todo el contenido de `origen` a `destino` y devuelve los bytes
/// movidos. Orden seguro por entrada: renombrar si se puede (mismo volumen);
/// si no, copiar → verificar tamaño → recién entonces borrar el origen. Un
/// fallo deja lo ya movido en destino y lo pendiente intacto en origen.
fn mudar_contenido(
    origen: &Path,
    destino: &Path,
    avance: &mut dyn FnMut(&str, u64, u64),
) -> Result<u64, String> {
    let total = tamano_recursivo(origen).map_err(|e| format!("midiendo origen: {e}"))?;
    if let Some(libre) = espacio_libre_si_otro_volumen(origen, destino) {
        if total > libre {
            return Err(format!(
                "espacio insuficiente en destino: se necesitan {} MB y hay {} MB libres",
                total / (1024 * 1024),
                libre / (1024 * 1024)
            ));
        }
    }
    fs::create_dir_all(destino).map_err(|e| format!("creando destino: {e}"))?;
    let mut hechos = 0u64;
    let entradas = fs::read_dir(origen).map_err(|e| format!("leyendo origen: {e}"))?;
    for entrada in entradas {
        let entrada = entrada.map_err(|e| format!("leyendo origen: {e}"))?;
        let nombre = entrada.file_name().to_string_lossy().into_owned();
        let dst = destino.join(entrada.file_name());
        mover_entrada(&entrada.path(), &dst, &mut |delta| {
            hechos += delta;
            avance(&nombre, hechos, total);
        })
        .map_err(|e| format!("moviendo {nombre}: {e}"))?;
    }
    Ok(hechos)
}

fn mover_entrada(src: &Path, dst: &Path, avance: &mut dyn FnMut(u64)) -> std::io::Result<()> {
    let tam = tamano_recursivo(src)?;
    // Mismo volumen: renombrar es instantáneo y no consume espacio extra.
    if !dst.exists() && fs::rename(src, dst).is_ok() {
        avance(tam);
        return Ok(());
    }
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for entrada in fs::read_dir(src)? {
            let entrada = entrada?;
            mover_entrada(&entrada.path(), &dst.join(entrada.file_name()), avance)?;
        }
        fs::remove_dir(src)?;
        return Ok(());
    }
    // Un resto idéntico de una mudanza anterior no se recopia.
    if dst.is_file() && dst.metadata()?.len() == tam {
        fs::remove_file(src)?;
        avance(tam);
        return Ok(());
    }
    copiar_con_progreso(src, dst, avance)?;
    if fs::metadata(dst)?.len() != tam {
        return Err(std::io::Error::other(format!(
            "verificación fallida: {} quedó incompleto en destino",
            dst.display()
        )));
    }
    fs::remove_file(src)?;
    Ok(())
}

fn copiar_con_progreso(src: &Path, dst: &Path, avance: &mut dyn FnMut(u64)) -> std::io::Result<()> {
    let mut lector = fs::File::open(src)?;
    let mut escritor = fs::File::create(dst)?;
    let mut buf = vec![0u8; 8 * 1024 * 1024];
    loop {
        let n = lector.read(&mut buf)?;
        if n == 0 {
            break;
        }
        escritor.write_all(&buf[..n])?;
        avance(n as u64);
    }
    escritor.sync_all()?;
    Ok(())
}

fn tamano_recursivo(p: &Path) -> std::io::Result<u64> {
    let meta = fs::metadata(p)?;
    if meta.is_file() {
        return Ok(meta.len());
    }
    let mut total = 0u64;
    for entrada in fs::read_dir(p)? {
        total += tamano_recursivo(&entrada?.path())?;
    }
    Ok(total)
}

/// Espacio libre en el volumen de `destino`, o `None` si origen y destino
/// comparten volumen (mover dentro del mismo disco no necesita espacio extra)
/// o si el SO no lo expone. Best-effort por prefijo de montaje más largo.
fn espacio_libre_si_otro_volumen(origen: &Path, destino: &Path) -> Option<u64> {
    let discos = sysinfo::Disks::new_with_refreshed_list();
    let montaje = |p: &Path| -> Option<(PathBuf, u64)> {
        discos
            .iter()
            .filter(|d| p.starts_with(d.mount_point()))
            .max_by_key(|d| d.mount_point().as_os_str().len())
            .map(|d| (d.mount_point().to_path_buf(), d.available_space()))
    };
    let (mont_dst, libre) = montaje(destino)?;
    if let Some((mont_src, _)) = montaje(origen) {
        if mont_src == mont_dst {
            return None;
        }
    }
    Some(libre)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn escribir(p: &Path, bytes: &[u8]) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, bytes).unwrap();
    }

    #[test]
    fn muda_estructura_anidada_y_vacia_el_origen() {
        let origen = tempfile::tempdir().unwrap();
        let destino = tempfile::tempdir().unwrap();
        escribir(&origen.path().join("modelo.gguf"), b"abcdefgh");
        escribir(&origen.path().join("correccion/llm.gguf"), b"1234");
        escribir(&origen.path().join("giga/enc/parte.onnx"), b"xy");

        let mut ultimo = (0u64, 0u64);
        let movidos = mudar_contenido(origen.path(), destino.path(), &mut |_, h, t| {
            ultimo = (h, t);
        })
        .unwrap();

        assert_eq!(movidos, 14);
        assert_eq!(ultimo, (14, 14));
        assert_eq!(
            fs::read(destino.path().join("modelo.gguf")).unwrap(),
            b"abcdefgh"
        );
        assert_eq!(
            fs::read(destino.path().join("correccion/llm.gguf")).unwrap(),
            b"1234"
        );
        assert_eq!(
            fs::read(destino.path().join("giga/enc/parte.onnx")).unwrap(),
            b"xy"
        );
        assert_eq!(fs::read_dir(origen.path()).unwrap().count(), 0);
    }

    #[test]
    fn resto_identico_en_destino_se_salta_pero_limpia_origen() {
        let origen = tempfile::tempdir().unwrap();
        let destino = tempfile::tempdir().unwrap();
        escribir(&origen.path().join("modelo.gguf"), b"abcdefgh");
        // Resto de una mudanza anterior interrumpida, ya completo en destino.
        escribir(&destino.path().join("modelo.gguf"), b"abcdefgh");

        let movidos = mudar_contenido(origen.path(), destino.path(), &mut |_, _, _| {}).unwrap();

        assert_eq!(movidos, 8);
        assert_eq!(fs::read_dir(origen.path()).unwrap().count(), 0);
    }

    #[test]
    fn copia_incompleta_en_destino_se_recopia() {
        let origen = tempfile::tempdir().unwrap();
        let destino = tempfile::tempdir().unwrap();
        escribir(&origen.path().join("modelo.gguf"), b"abcdefgh");
        // Copia a medias de una mudanza interrumpida: tamaño distinto.
        escribir(&destino.path().join("modelo.gguf"), b"abc");

        mudar_contenido(origen.path(), destino.path(), &mut |_, _, _| {}).unwrap();

        assert_eq!(
            fs::read(destino.path().join("modelo.gguf")).unwrap(),
            b"abcdefgh"
        );
        assert_eq!(fs::read_dir(origen.path()).unwrap().count(), 0);
    }
}
