//! Superficie de comandos de AJUSTES de toda la app (temas, overlay, audio,
//! corrección, post-proceso, aceleradores…). Vivía dentro de `shortcut/` por
//! herencia del upstream — aquí solo hay lectura/escritura de settings y sus
//! efectos; la lógica de atajos sigue en `crate::shortcut`.
//!
//! Los NOMBRES de los comandos no cambian (specta genera los mismos bindings):
//! mover el módulo es invisible para el frontend.

use log::warn;
use tauri::{AppHandle, Manager};
use tauri_plugin_autostart::ManagerExt;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
use crate::settings::APPLE_INTELLIGENCE_DEFAULT_MODEL_ID;
use crate::settings::{
    self, AppSettings, AutoSubmitKey, ClipboardHandling, CorreccionModo, CorreccionMotor,
    EsferaModo, OverlayPosition, OverlayStyle, PasteMethod, SoundTheme, Theme, TypingTool, UiShell,
    UiTheme,
};
use crate::tray;

#[tauri::command]
#[specta::specta]
pub fn change_ptt_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.push_to_talk = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_audio_feedback_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.audio_feedback = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_audio_feedback_volume_setting(app: AppHandle, volume: f32) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.audio_feedback_volume = volume;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_sound_theme_setting(app: AppHandle, theme: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match theme.as_str() {
        "abrax" => SoundTheme::Abrax,
        "marimba" => SoundTheme::Marimba,
        "pop" => SoundTheme::Pop,
        "custom" => SoundTheme::Custom,
        other => {
            warn!("Invalid sound theme '{}', defaulting to abrax", other);
            SoundTheme::Abrax
        }
    };
    settings.sound_theme = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_theme_setting(app: AppHandle, theme: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match theme.as_str() {
        "system" => Theme::System,
        "light" => Theme::Light,
        "dark" => Theme::Dark,
        other => {
            warn!("Invalid theme '{}', defaulting to system", other);
            Theme::System
        }
    };
    settings.theme = parsed;
    let effective = effective_window_theme(&settings);
    settings::write_settings(&app, settings);
    #[cfg(target_os = "windows")]
    apply_window_theme(&app, effective);
    #[cfg(not(target_os = "windows"))]
    let _ = effective;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_ui_theme_setting(app: AppHandle, ui_theme: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match ui_theme.as_str() {
        "abrax" => UiTheme::Abrax,
        "imperial" => UiTheme::Imperial,
        "escuderia" => UiTheme::Escuderia,
        other => {
            warn!("Invalid ui theme '{}', defaulting to abrax", other);
            UiTheme::Abrax
        }
    };
    settings.ui_theme = parsed;
    let effective = effective_window_theme(&settings);
    settings::write_settings(&app, settings);
    #[cfg(target_os = "windows")]
    apply_window_theme(&app, effective);
    #[cfg(not(target_os = "windows"))]
    let _ = effective;
    Ok(())
}

/// Persists the window shell (`classic`/`retro`/`quiet`). Unlike the palette,
/// the shell only takes full effect on the next launch, because the
/// frameless/transparent chrome is decided when the window is built; this
/// command just records the choice so the next boot builds the right window.
#[tauri::command]
#[specta::specta]
pub fn change_ui_shell_setting(app: AppHandle, ui_shell: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match ui_shell.as_str() {
        "classic" => UiShell::Classic,
        "retro" => UiShell::Retro,
        "quiet" => UiShell::Quiet,
        "bancada" => UiShell::Bancada,
        other => {
            warn!("Invalid ui shell '{}', defaulting to classic", other);
            UiShell::Classic
        }
    };
    settings.ui_shell = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_correccion_modo_setting(app: AppHandle, modo: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match modo.as_str() {
        "literal" => CorreccionModo::Literal,
        // «pulido» era el tercer modo, retirado con el «Pulido con IA» el 29/07.
        // Se acepta y cae en `Limpio`, igual que el alias de serde: un valor
        // legado no es un valor inválido y no debe degradar a `Literal`.
        "limpio" | "pulido" => CorreccionModo::Limpio,
        other => {
            warn!("Modo de corrección inválido '{}', se usa literal", other);
            CorreccionModo::Literal
        }
    };
    settings.correccion_modo = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_correccion_motor_setting(app: AppHandle, motor: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match motor.as_str() {
        // «auto» y «modelo» pedían el LLM local, retirado el 29/07: entran en
        // solo-reglas, que es lo que hacían de hecho sin un modelo disponible.
        "solo_reglas" | "auto" | "modelo" => CorreccionMotor::SoloReglas,
        "desactivado" => CorreccionMotor::Desactivado,
        other => {
            // Ante un valor desconocido, el estado seguro es apagado.
            warn!("Motor de corrección inválido '{}', se desactiva", other);
            CorreccionMotor::Desactivado
        }
    };
    settings.correccion_motor = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Persists the Esfera overlay behaviour (`audio`/`palabras`). Takes effect on
/// the next dictation: the overlay reads it when it becomes visible and the
/// backend reads it when a transcription starts, so no live re-wiring is needed.
#[tauri::command]
#[specta::specta]
pub fn change_esfera_modo_setting(app: AppHandle, esfera_modo: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match esfera_modo.as_str() {
        "audio" => EsferaModo::Audio,
        "palabras" => EsferaModo::Palabras,
        other => {
            warn!("Invalid esfera modo '{}', defaulting to audio", other);
            EsferaModo::Audio
        }
    };
    settings.esfera_modo = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Light/dark mode the window chrome should actually show: Imperial and
/// Escuderia are dark by design and force dark regardless of the stored
/// [`Theme`], which stays untouched and governs again when the palette
/// returns to Abrax.
pub fn effective_window_theme(settings: &AppSettings) -> Theme {
    match settings.ui_theme {
        UiTheme::Imperial | UiTheme::Escuderia => Theme::Dark,
        UiTheme::Abrax => settings.theme,
    }
}

/// Applies the appearance setting to the Windows title bar, which CSS
/// `data-theme` cannot reach. `System` clears the override so the window follows
/// Windows. Call this on startup and whenever the setting changes to keep the
/// title bar in sync with the in-app palette.
#[cfg(target_os = "windows")]
pub fn apply_window_theme(app: &AppHandle, theme: Theme) {
    let window_theme = match theme {
        Theme::System => None,
        Theme::Light => Some(tauri::Theme::Light),
        Theme::Dark => Some(tauri::Theme::Dark),
    };
    if let Some(window) = app.get_webview_window("main") {
        if let Err(e) = window.set_theme(window_theme) {
            warn!("Failed to apply window theme: {}", e);
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_translate_to_english_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.translate_to_english = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_diarization_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.diarization_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_diarization_num_speakers_setting(app: AppHandle, num: u32) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.diarization_num_speakers = num;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Conmuta «Audio del sistema» y, si hace falta, CAMBIA EL MODELO.
///
/// Canary es el recomendado porque arranca en un minuto y responde en 2 s, y
/// para dictado corto eso ES el producto. Pero con audio de sistema se queda
/// corto: medido el 29/07 sobre grabaciones reales de loopback, devolvió cadena
/// vacía en 3 de 5 capturas donde Nemotron y Turbo sí transcribieron. El usuario
/// no tiene por qué saber eso, así que la app elige por él.
///
/// Se cambia AL CONMUTAR, no en cada captura: usa el mismo camino que cuando el
/// usuario elige un modelo a mano (`selected_model` + `reload_model_on_next_use`),
/// que está exercitado a diario. Cambiar el motor dentro del flujo de grabación
/// metería la orquestación de carga/descarga en la ruta crítica del dictado.
///
/// Si no hay ningún modelo apto descargado, NO se adivina: se avisa.
#[tauri::command]
#[specta::specta]
pub fn change_capture_system_audio_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.capture_system_audio = enabled;

    let mm = app.state::<std::sync::Arc<crate::managers::model::ModelManager>>();
    let mut recargar_modelo = false;
    let mut sin_modelo_apto = false;

    if enabled {
        // Solo actuamos si el modelo activo se queda corto. Si el usuario ya
        // tiene uno bueno puesto, no se le toca nada.
        if crate::managers::model::ModelManager::se_queda_corto_para_sistema(
            &settings.selected_model,
        ) {
            match mm.modelo_para_sistema_descargado() {
                Some(apto) => {
                    log::info!(
                        "audio del sistema: {} se queda corto, se cambia a {}",
                        settings.selected_model,
                        apto
                    );
                    settings.modelo_antes_de_sistema = Some(settings.selected_model.clone());
                    settings.selected_model = apto;
                    recargar_modelo = true;
                }
                None => sin_modelo_apto = true,
            }
        }
    } else if let Some(previo) = settings.modelo_antes_de_sistema.take() {
        // Solo restauramos lo que cambiamos nosotros, y solo si el modelo actual
        // sigue siendo el que pusimos: si el usuario eligió otro a mano en el
        // medio, su elección manda y no se pisa.
        if !crate::managers::model::ModelManager::se_queda_corto_para_sistema(
            &settings.selected_model,
        ) {
            log::info!(
                "audio del sistema apagado: se restaura {} (estaba {})",
                previo,
                settings.selected_model
            );
            settings.selected_model = previo;
            recargar_modelo = true;
        }
    }

    settings::write_settings(&app, settings);

    // El dispositivo cambia (mic <-> salida loopback): invalida la caché para que
    // la próxima grabación re-resuelva.
    if let Some(rm) =
        app.try_state::<std::sync::Arc<crate::managers::audio::AudioRecordingManager>>()
    {
        rm.invalidate_device_cache();
    }

    if recargar_modelo {
        if let Some(tm) =
            app.try_state::<std::sync::Arc<crate::managers::transcription::TranscriptionManager>>()
        {
            tm.reload_model_on_next_use();
        }
    }

    // El aviso va DESPUÉS de persistir: el modo queda activo (el usuario lo
    // pidió) y se le dice qué le falta para que funcione, con el peso real.
    if sin_modelo_apto {
        log::info!("audio del sistema: no hay modelo apto descargado, se avisa");
        crate::user_alerts::alert(
            &app,
            crate::user_alerts::AlertKind::SistemaSinModeloApto,
            None,
        );
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_selected_language_setting(app: AppHandle, language: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.selected_language = language;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_overlay_position_setting(app: AppHandle, position: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match position.as_str() {
        // "none" is retired (visibility is overlay_style now); fold legacy callers
        // onto Bottom rather than warn.
        "none" | "bottom" => OverlayPosition::Bottom,
        "top" => OverlayPosition::Top,
        other => {
            warn!("Invalid overlay position '{}', defaulting to bottom", other);
            OverlayPosition::Bottom
        }
    };
    settings.overlay_position = parsed;
    settings::write_settings(&app, settings);

    // Whether the overlay shows at all is owned by overlay_style now; position
    // only ever toggles Top/Bottom, so the enabled cache is untouched here.
    // Update overlay position without recreating window
    crate::overlay::update_overlay_position(&app);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_overlay_style_setting(app: AppHandle, style: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match style.as_str() {
        "none" => OverlayStyle::None,
        "minimal" => OverlayStyle::Minimal,
        "live" => OverlayStyle::Live,
        "esfera" => OverlayStyle::Esfera,
        other => {
            warn!("Invalid overlay style '{}', defaulting to minimal", other);
            OverlayStyle::Minimal
        }
    };
    settings.overlay_style = parsed;
    settings::write_settings(&app, settings);

    // Spectrum emission follows the overlay's subscription (start/stop_spectrum),
    // so no cached style flag needs syncing here anymore (R8).

    // Reposition in case the window needs to re-center for the new style.
    crate::overlay::update_overlay_position(&app);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_debug_mode_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.debug_mode = enabled;
    settings::write_settings(&app, settings);

    // Keep webview log streaming in sync: the live log viewer only exists in
    // debug mode, so logs are forwarded to the frontend only while it is on.
    crate::WEBVIEW_LOG_STREAMING.store(enabled, std::sync::atomic::Ordering::Relaxed);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_start_hidden_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.start_hidden = enabled;
    settings::write_settings(&app, settings);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_autostart_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.autostart_enabled = enabled;
    settings::write_settings(&app, settings);

    // Apply the autostart setting immediately. El fallo se propaga: antes se
    // tragaba con `let _ =` y el toggle mentía (hallado en revisión).
    let autostart_manager = app.autolaunch();
    let resultado = if enabled {
        autostart_manager.enable()
    } else {
        autostart_manager.disable()
    };
    resultado.map_err(|e| format!("autostart: {e}"))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_update_checks_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.update_checks_enabled = enabled;
    settings::write_settings(&app, settings);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_show_whats_new_on_update_setting(
    app: AppHandle,
    enabled: bool,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.show_whats_new_on_update = enabled;
    settings::write_settings(&app, settings);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_whats_new_last_seen_version_setting(
    app: AppHandle,
    version: String,
) -> Result<(), String> {
    let version = version.trim().to_string();
    let mut settings = settings::get_settings(&app);
    settings.whats_new_last_seen_version = version.clone();
    settings::write_settings(&app, settings);

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_custom_words(app: AppHandle, words: Vec<String>) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.custom_words = words;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn update_custom_filler_words(
    app: AppHandle,
    words: Option<Vec<String>>,
) -> Result<(), String> {
    // None = lista por defecto del idioma · Some(vec) = lista propia · Some([]) = filtro apagado
    let mut settings = settings::get_settings(&app);
    settings.custom_filler_words = words;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_word_correction_threshold_setting(
    app: AppHandle,
    threshold: f64,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.word_correction_threshold = threshold;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_extra_recording_buffer_setting(app: AppHandle, ms: u64) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.extra_recording_buffer_ms = ms;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_delay_ms_setting(app: AppHandle, ms: u64) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.paste_delay_ms = ms;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_delay_after_ms_setting(app: AppHandle, ms: u64) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.paste_delay_after_ms = ms;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_paste_method_setting(app: AppHandle, method: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match method.as_str() {
        "ctrl_v" => PasteMethod::CtrlV,
        "direct" => PasteMethod::Direct,
        "none" => PasteMethod::None,
        "shift_insert" => PasteMethod::ShiftInsert,
        "ctrl_shift_v" => PasteMethod::CtrlShiftV,
        "external_script" => PasteMethod::ExternalScript,
        other => {
            warn!("Invalid paste method '{}', defaulting to ctrl_v", other);
            PasteMethod::CtrlV
        }
    };
    settings.paste_method = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_available_typing_tools() -> Vec<String> {
    #[cfg(target_os = "linux")]
    {
        crate::clipboard::get_available_typing_tools()
    }
    #[cfg(not(target_os = "linux"))]
    {
        vec!["auto".to_string()]
    }
}

#[tauri::command]
#[specta::specta]
pub fn change_typing_tool_setting(app: AppHandle, tool: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match tool.as_str() {
        "auto" => TypingTool::Auto,
        "wtype" => TypingTool::Wtype,
        "kwtype" => TypingTool::Kwtype,
        "dotool" => TypingTool::Dotool,
        "ydotool" => TypingTool::Ydotool,
        "xdotool" => TypingTool::Xdotool,
        other => {
            warn!("Invalid typing tool '{}', defaulting to auto", other);
            TypingTool::Auto
        }
    };
    settings.typing_tool = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_external_script_path_setting(
    app: AppHandle,
    path: Option<String>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.external_script_path = path;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_clipboard_handling_setting(app: AppHandle, handling: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match handling.as_str() {
        "dont_modify" => ClipboardHandling::DontModify,
        "copy_to_clipboard" => ClipboardHandling::CopyToClipboard,
        other => {
            warn!(
                "Invalid clipboard handling '{}', defaulting to dont_modify",
                other
            );
            ClipboardHandling::DontModify
        }
    };
    settings.clipboard_handling = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_auto_submit_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.auto_submit = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_auto_submit_key_setting(app: AppHandle, key: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    let parsed = match key.as_str() {
        "enter" => AutoSubmitKey::Enter,
        "ctrl_enter" => AutoSubmitKey::CtrlEnter,
        "cmd_enter" => AutoSubmitKey::CmdEnter,
        other => {
            warn!("Invalid auto submit key '{}', defaulting to enter", other);
            AutoSubmitKey::Enter
        }
    };
    settings.auto_submit_key = parsed;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_experimental_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.experimental_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_mute_while_recording_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.mute_while_recording = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_append_trailing_space_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.append_trailing_space = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_lazy_stream_close_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.lazy_stream_close = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

/// Los cuatro ajustes de abajo NO TENIAN COMANDO, y por eso sus interruptores
/// eran un placebo: el valor cambiaba en pantalla, `settingsStore` no encontraba
/// manejador, escribia un `console.warn` en una consola que nadie mira, y a Rust
/// no llegaba nunca. Al reabrir Ajustes el interruptor volvia a su sitio.
///
/// Nada mas engañoso que un control que se deja pulsar y no hace nada. Cazado
/// el 30/07 auditando el trabajo heredado.

#[tauri::command]
#[specta::specta]
pub fn change_autocorreccion_activa_setting(app: AppHandle, activa: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.autocorreccion_activa = activa;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_emoji_dictado_setting(app: AppHandle, activo: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.emoji_dictado = activo;
    settings::write_settings(&app, settings);
    Ok(())
}

/// `None` = las senales de fabrica, `Some(vec![])` = nivel apagado, lista propia
/// = reemplaza a las de fabrica. Mismo contrato que las muletillas.
#[tauri::command]
#[specta::specta]
pub fn change_autocorreccion_senales_borrado_setting(
    app: AppHandle,
    senales: Option<Vec<String>>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.autocorreccion_senales_borrado = senales;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_autocorreccion_senales_sustitucion_setting(
    app: AppHandle,
    senales: Option<Vec<String>>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.autocorreccion_senales_sustitucion = senales;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_vad_enabled_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.vad_enabled = enabled;
    settings::write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_app_language_setting(app: AppHandle, language: String) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.app_language = language.clone();
    settings::write_settings(&app, settings);

    // Refresh the tray menu with the new language
    tray::update_tray_menu(&app, Some(&language));

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_show_tray_icon_setting(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.show_tray_icon = enabled;
    settings::write_settings(&app, settings);

    // Apply change immediately
    tray::set_tray_visibility(&app, enabled);

    Ok(())
}

/// Save accelerator settings and make the next model use reload with them.
/// The currently running transcription, if any, keeps its existing engine.
fn save_accelerator_and_reload_next_use(app: &AppHandle, s: settings::AppSettings) {
    settings::write_settings(app, s);

    let tm = app.state::<std::sync::Arc<crate::managers::transcription::TranscriptionManager>>();
    tm.reload_model_on_next_use();
}

#[tauri::command]
#[specta::specta]
pub fn change_transcribe_accelerator_setting(
    app: AppHandle,
    accelerator: settings::TranscribeAcceleratorSetting,
) -> Result<(), String> {
    let mut s = settings::get_settings(&app);
    s.transcribe_accelerator = accelerator;
    save_accelerator_and_reload_next_use(&app, s);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_ort_accelerator_setting(
    app: AppHandle,
    accelerator: settings::OrtAcceleratorSetting,
) -> Result<(), String> {
    let mut s = settings::get_settings(&app);
    s.ort_accelerator = accelerator;
    save_accelerator_and_reload_next_use(&app, s);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn change_transcribe_gpu_device(app: AppHandle, device: i32) -> Result<(), String> {
    let mut s = settings::get_settings(&app);
    s.transcribe_gpu_device = device;
    save_accelerator_and_reload_next_use(&app, s);
    Ok(())
}

/// Return which accelerators and GPU devices are available for this build.
///
/// First-call cost is dominated by enumerating GPU devices through the
/// transcribe.cpp Metal/Vulkan backend, which loads dynamic libraries and
/// probes hardware. Run it on the blocking pool so the webview thread
/// stays responsive — see also the startup pre-warm in `lib.rs`.
#[tauri::command]
#[specta::specta]
pub async fn get_available_accelerators() -> crate::managers::transcription::AvailableAccelerators {
    tauri::async_runtime::spawn_blocking(crate::managers::transcription::get_available_accelerators)
        .await
        .expect("get_available_accelerators panicked")
}

/// Los 1 531 nombres que reconoce el emoji dictado, para consultarlos en la app.
///
/// La tabla esta compilada dentro del binario, asi que esto no toca disco ni
/// red: es leer un `&'static str` y partirlo. Se manda entera de una vez (unos
/// 50 KB) porque filtrar en el frontend es instantaneo y evita un viaje de IPC
/// por cada letra tecleada en el buscador.
#[tauri::command]
#[specta::specta]
pub fn listar_emojis() -> Vec<crate::correccion::emoji::EntradaEmoji> {
    crate::correccion::emoji::listar()
}
