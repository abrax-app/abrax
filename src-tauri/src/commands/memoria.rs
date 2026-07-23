//! Comando de la Memoria de correcciones: guardar la edición de una
//! transcripción del Historial y aprender de ella.

use std::sync::Arc;

use tauri::{AppHandle, State};

use crate::managers::history::HistoryManager;
use crate::memoria::{self, ParMemoria};
use crate::settings::{get_settings, write_settings};

/// Guarda el texto editado de una entrada del Historial y, si la Memoria está
/// activa, aprende de la diferencia (pares `de → a` que pasan las puertas de
/// seguridad). Devuelve los pares aprendidos/actualizados en esta edición para
/// que la UI los muestre. La entrada se actualiza aunque no se aprenda nada.
#[tauri::command]
#[specta::specta]
pub async fn editar_transcripcion(
    app: AppHandle,
    history_manager: State<'_, Arc<HistoryManager>>,
    id: i64,
    texto: String,
) -> Result<Vec<ParMemoria>, String> {
    let texto = texto.trim().to_string();
    if texto.is_empty() {
        return Err("el texto editado no puede quedar vacío".into());
    }

    let entrada = history_manager
        .get_entry_by_id(id)
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| format!("la entrada {id} no existe"))?;

    let mut aprendidos = Vec::new();
    let settings = get_settings(&app);
    if settings.memoria_activa && entrada.transcription_text != texto {
        let pares = memoria::aprender_de_edicion(&entrada.transcription_text, &texto);
        if !pares.is_empty() {
            // Releer antes de escribir: minimiza la ventana de pisar otros
            // cambios de ajustes hechos entre la lectura de arriba y aquí.
            let mut s = get_settings(&app);
            aprendidos = memoria::incorporar(pares, &mut s.memoria_correcciones);
            write_settings(&app, s);
        }
    }

    // Actualiza el texto conservando el post-proceso existente; el manager
    // emite `history-update-payload` (Updated) y la UI se refresca sola.
    history_manager
        .update_transcription(
            id,
            texto,
            entrada.post_processed_text,
            entrada.post_process_prompt,
        )
        .map_err(|e| e.to_string())?;

    Ok(aprendidos)
}
