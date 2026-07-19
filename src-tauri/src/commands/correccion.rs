//! Comandos del módulo de corrección local para la UI de ajustes.

/// Detecta si hay un Ollama vivo en 127.0.0.1 y qué modelos tiene descargados.
/// `None` = no corre (o no respondió en 500 ms); `Some(vec![])` = corre pero
/// sin modelos. La UI usa la distinción para explicar el estado del motor:
/// «no detectado» vs «detectado, sin modelos» vs «detectado (nombre)».
#[tauri::command]
#[specta::specta]
pub async fn detectar_correccion_ollama() -> Result<Option<Vec<String>>, String> {
    Ok(crate::correccion::fraseador::detectar_ollama().await)
}
