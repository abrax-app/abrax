//! Canal único de errores de usuario (F1): la app nunca falla en silencio.
//!
//! Todo error que el usuario debe ver pasa por [`alert`], que hace tres cosas:
//! 1. Lo guarda en un registro reciente (el «centro de errores» lo lee al
//!    abrir la ventana, aunque el fallo haya ocurrido con la app en bandeja).
//! 2. Emite el evento tipado [`UserAlertEvent`] — un solo listener en el
//!    frontend lo convierte en toast localizado + entrada del centro.
//! 3. Si la ventana principal no está visible, envía una notificación nativa
//!    del sistema, localizada con el mismo mecanismo compilado del tray
//!    (`tray_i18n`, sección `tray` de los 22 locales).
//!
//! Privacidad: los `detail` son mensajes de error técnicos (dispositivo,
//! motor, red) — jamás contenido dictado (S3).

use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Manager};
use tauri_plugin_notification::NotificationExt;
use tauri_specta::Event;

/// Cuántos errores recientes conserva el registro para el centro de errores.
const MAX_RECENT: usize = 20;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "snake_case")]
pub enum AlertKind {
    RecordingPermissionDenied,
    RecordingNoDevice,
    /// La grabación no capturó audio (0 muestras): mic mudo/desconectado, o
    /// «Audio del sistema» activo pero sin nada sonando.
    RecordingNoAudio,
    /// El usuario soltó la tecla antes de alcanzar a hablar. NO es un problema
    /// del micrófono: culpar al micrófono aquí manda a revisar el hardware
    /// equivocado, que es exactamente lo que pasó en la medición del 26/07.
    RecordingTooShort,
    /// Se capturó audio y el motor terminó bien, pero no reconoció ni una
    /// palabra. Antes esta rama ocultaba el overlay sin decir nada: el usuario
    /// veía «no pasó nada» y no tenía forma de saber por qué.
    TranscriptionEmpty,
    /// No se pudo registrar ningún atajo global. Sin esto la app queda abierta y
    /// aparentemente sana, pero el atajo no existe y nada lo dice.
    ShortcutRegistration,
    Recording,
    Transcription,
    Paste,
    ModelLoad,
    ModelDownload,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
pub struct UserAlertEvent {
    pub kind: AlertKind,
    /// Mensaje técnico (error de motor/red/dispositivo); nunca texto dictado.
    pub detail: Option<String>,
    /// Momento del fallo, epoch en milisegundos.
    pub ts_ms: f64,
}

static RECENT: Mutex<VecDeque<UserAlertEvent>> = Mutex::new(VecDeque::new());

fn main_window_visible(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false) && !w.is_minimized().unwrap_or(false))
        .unwrap_or(false)
}

/// Reporta un error de usuario por el canal único (registro + evento + posible
/// notificación nativa). Nunca falla: cada pata es best-effort.
pub fn alert(app: &AppHandle, kind: AlertKind, detail: Option<String>) {
    let ts_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64;
    let event = UserAlertEvent {
        kind,
        detail,
        ts_ms,
    };

    {
        let mut recent = RECENT.lock().unwrap();
        recent.push_front(event.clone());
        recent.truncate(MAX_RECENT);
    }

    if let Err(e) = event.clone().emit(app) {
        log::warn!("user_alerts: failed to emit alert event: {e}");
    }

    // Con la ventana oculta (tray, --start-hidden) el toast del frontend es
    // invisible: la notificación del sistema es la única superficie que el
    // usuario puede ver en ese momento.
    if !main_window_visible(app) {
        let lang = crate::settings::get_settings(app).app_language;
        let strings = crate::tray_i18n::get_tray_translations(Some(lang));
        let body = match kind {
            AlertKind::RecordingPermissionDenied
            | AlertKind::RecordingNoDevice
            | AlertKind::RecordingNoAudio
            | AlertKind::Recording
            // Soltar la tecla antes de tiempo también es «no se pudo grabar»
            // para la notificación del sistema, que no admite matices: el
            // mensaje fino (mantener el atajo) vive en el toast del frontend,
            // que es quien conoce el atajo real. Reusar la cadena existente
            // evita 22 traducciones más para una superficie de una línea.
            | AlertKind::RecordingTooShort => strings.error_recording,
            // No reconocer palabras es un resultado del motor: cae del mismo
            // lado que un fallo de transcripción.
            AlertKind::TranscriptionEmpty => strings.error_transcription,
            // Sin atajo no se puede dictar: cae del lado de «no se pudo grabar».
            AlertKind::ShortcutRegistration => strings.error_recording,
            AlertKind::Transcription => strings.error_transcription,
            AlertKind::Paste => strings.error_paste,
            AlertKind::ModelLoad => strings.error_model_load,
            AlertKind::ModelDownload => strings.error_model_download,
        };
        if let Err(e) = app
            .notification()
            .builder()
            .title(&strings.error_title)
            .body(&body)
            .show()
        {
            log::warn!("user_alerts: failed to show system notification: {e}");
        }
    }
}

/// Errores recientes (más nuevo primero) para poblar el centro de errores al
/// abrir la ventana — cubre los fallos ocurridos antes de montar el listener.
#[tauri::command]
#[specta::specta]
pub fn get_recent_alerts() -> Vec<UserAlertEvent> {
    RECENT.lock().unwrap().iter().cloned().collect()
}

/// Descartar el centro de errores.
#[tauri::command]
#[specta::specta]
pub fn clear_recent_alerts() {
    RECENT.lock().unwrap().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_registry_caps_and_orders_newest_first() {
        clear_recent_alerts();
        for i in 0..(MAX_RECENT + 5) {
            let mut recent = RECENT.lock().unwrap();
            recent.push_front(UserAlertEvent {
                kind: AlertKind::Recording,
                detail: Some(format!("e{i}")),
                ts_ms: i as f64,
            });
            recent.truncate(MAX_RECENT);
        }
        let alerts = get_recent_alerts();
        assert_eq!(alerts.len(), MAX_RECENT);
        assert_eq!(alerts[0].detail.as_deref(), Some("e24"));
        clear_recent_alerts();
        assert!(get_recent_alerts().is_empty());
    }
}
