use crate::TranscriptionCoordinator;
#[cfg(unix)]
use log::debug;
use log::warn;
use tauri::{AppHandle, Manager};

#[cfg(unix)]
use signal_hook::consts::{SIGUSR1, SIGUSR2};
#[cfg(unix)]
use signal_hook::iterator::Signals;
#[cfg(unix)]
use std::thread;

/// Send a transcription input to the coordinator.
/// Used by signal handlers, CLI flags, and any other external trigger.
pub fn send_transcription_input(app: &AppHandle, binding_id: &str, source: &str) {
    if let Some(c) = app.try_state::<TranscriptionCoordinator>() {
        c.send_input(binding_id, source, true, false);
    } else {
        warn!("TranscriptionCoordinator not initialized");
    }
}

/// Toggle dictation from the UI (e.g. a retro transport button). Mirrors
/// the global-shortcut / CLI `--toggle-transcription` path by reusing the shared
/// coordinator entry point, so it adds no new recording pipeline.
#[tauri::command]
#[specta::specta]
pub fn trigger_transcription(app: AppHandle) {
    send_transcription_input(&app, "transcribe", "ui");
}

#[cfg(unix)]
pub fn setup_signal_handler(app_handle: AppHandle, mut signals: Signals) {
    debug!("Signal handlers registered (SIGUSR1, SIGUSR2)");
    thread::spawn(move || {
        for sig in signals.forever() {
            let (binding_id, signal_name) = match sig {
                // SIGUSR1 disparaba el atajo de post-proceso, retirado el
                // 29/07. Se mantiene mapeado a transcribir en vez de dejarlo
                // sin efecto: un script que lo usara seguiria dictando en vez
                // de dejar de funcionar sin decir nada.
                SIGUSR1 | SIGUSR2 => ("transcribe", "SIGUSR"),
                _ => continue,
            };
            debug!("Received {signal_name}");
            send_transcription_input(&app_handle, binding_id, signal_name);
        }
    });
}
