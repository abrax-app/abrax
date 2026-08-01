//! Keyboard shortcut management module
//!
//! This module provides a unified interface for keyboard shortcuts with
//! multiple backend implementations:
//!
//! - `tauri`: Uses Tauri's built-in global-shortcut plugin
//! - `handy_keys`: Uses the handy-keys library for more control
//!
//! The active implementation is determined by the `keyboard_implementation`
//! setting and can be changed at runtime.

mod handler;
pub mod handy_keys;
mod tauri_impl;

use log::{error, info, warn};
use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::settings::{self, get_settings, KeyboardImplementation, ShortcutBinding};

// Note: Commands are accessed via shortcut::handy_keys:: in lib.rs

/// Initialize shortcuts using the configured implementation
pub fn init_shortcuts(app: &AppHandle) {
    let user_settings = settings::load_or_create_app_settings(app);

    // Check which implementation to use
    match user_settings.keyboard_implementation {
        KeyboardImplementation::Tauri => {
            tauri_impl::init_shortcuts(app);
        }
        KeyboardImplementation::HandyKeys => {
            if let Err(e) = handy_keys::init_shortcuts(app) {
                error!("Failed to initialize handy-keys shortcuts: {}", e);
                // Fall back to Tauri implementation and persist this fallback
                warn!("Falling back to Tauri global shortcut implementation and saving fallback to settings");

                // Update settings to persist the fallback so we don't retry HandyKeys on next launch
                let mut settings = settings::get_settings(app);
                settings.keyboard_implementation = KeyboardImplementation::Tauri;
                settings::write_settings(app, settings);

                tauri_impl::init_shortcuts(app);
            }
        }
    }
}

/// Register the cancel shortcut (called when recording starts)
pub fn register_cancel_shortcut(app: &AppHandle) {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::register_cancel_shortcut(app),
        KeyboardImplementation::HandyKeys => handy_keys::register_cancel_shortcut(app),
    }
}

/// Unregister the cancel shortcut (called when recording stops)
pub fn unregister_cancel_shortcut(app: &AppHandle) {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::unregister_cancel_shortcut(app),
        KeyboardImplementation::HandyKeys => handy_keys::unregister_cancel_shortcut(app),
    }
}

/// Register a shortcut using the appropriate implementation
pub fn register_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::register_shortcut(app, binding),
        KeyboardImplementation::HandyKeys => handy_keys::register_shortcut(app, binding),
    }
}

/// Unregister a shortcut using the appropriate implementation
pub fn unregister_shortcut(app: &AppHandle, binding: ShortcutBinding) -> Result<(), String> {
    let settings = get_settings(app);
    match settings.keyboard_implementation {
        KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, binding),
        KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, binding),
    }
}

// ============================================================================
// Binding Management Commands
// ============================================================================

#[derive(Serialize, Type)]
pub struct BindingResponse {
    success: bool,
    binding: Option<ShortcutBinding>,
    error: Option<String>,
}

#[tauri::command]
#[specta::specta]
pub fn change_binding(
    app: AppHandle,
    id: String,
    binding: String,
) -> Result<BindingResponse, String> {
    // Reject empty bindings — every shortcut should have a value
    if binding.trim().is_empty() {
        return Err("Binding cannot be empty".to_string());
    }

    let mut settings = settings::get_settings(&app);

    // Get the binding to modify, or create it from defaults if it doesn't exist
    let binding_to_modify = match settings.bindings.get(&id) {
        Some(binding) => binding.clone(),
        None => {
            // Try to get the default binding for this id
            let default_settings = settings::get_default_settings();
            match default_settings.bindings.get(&id) {
                Some(default_binding) => {
                    warn!(
                        "Binding '{}' not found in settings, creating from defaults",
                        id
                    );
                    default_binding.clone()
                }
                None => {
                    let error_msg = format!("Binding with id '{}' not found in defaults", id);
                    warn!("change_binding error: {}", error_msg);
                    return Ok(BindingResponse {
                        success: false,
                        binding: None,
                        error: Some(error_msg),
                    });
                }
            }
        }
    };

    // If this is the cancel binding, just update the settings and return
    // It's managed dynamically, so we don't register/unregister here
    if id == "cancel" {
        if let Some(mut b) = settings.bindings.get(&id).cloned() {
            b.current_binding = binding;
            settings.bindings.insert(id.clone(), b.clone());
            settings::write_settings(&app, settings);
            return Ok(BindingResponse {
                success: true,
                binding: Some(b.clone()),
                error: None,
            });
        }
    }

    // Unregister the existing binding
    if let Err(e) = unregister_shortcut(&app, binding_to_modify.clone()) {
        let error_msg = format!("Failed to unregister shortcut: {}", e);
        error!("change_binding error: {}", error_msg);
    }

    // Validate the new shortcut for the current keyboard implementation
    if let Err(e) = validate_shortcut_for_implementation(&binding, settings.keyboard_implementation)
    {
        warn!("change_binding validation error: {}", e);
        return Err(e);
    }

    // Create an updated binding
    let mut updated_binding = binding_to_modify;
    updated_binding.current_binding = binding;

    // Register the new binding
    if let Err(e) = register_shortcut(&app, updated_binding.clone()) {
        let error_msg = format!("Failed to register shortcut: {}", e);
        error!("change_binding error: {}", error_msg);
        return Ok(BindingResponse {
            success: false,
            binding: None,
            error: Some(error_msg),
        });
    }

    // Update the binding in the settings
    settings.bindings.insert(id, updated_binding.clone());

    // Save the settings
    settings::write_settings(&app, settings);

    // Return the updated binding
    Ok(BindingResponse {
        success: true,
        binding: Some(updated_binding),
        error: None,
    })
}

#[tauri::command]
#[specta::specta]
pub fn reset_binding(app: AppHandle, id: String) -> Result<BindingResponse, String> {
    let binding = settings::get_stored_binding(&app, &id);
    change_binding(app, id, binding.default_binding)
}

/// Temporarily unregister a binding while the user is editing it in the UI.
/// This avoids firing the action while keys are being recorded.
#[tauri::command]
#[specta::specta]
pub fn suspend_binding(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(b) = settings::get_bindings(&app).get(&id).cloned() {
        if let Err(e) = unregister_shortcut(&app, b) {
            error!("suspend_binding error for id '{}': {}", id, e);
            return Err(e);
        }
    }
    Ok(())
}

/// Re-register the binding after the user has finished editing.
#[tauri::command]
#[specta::specta]
pub fn resume_binding(app: AppHandle, id: String) -> Result<(), String> {
    if let Some(b) = settings::get_bindings(&app).get(&id).cloned() {
        if let Err(e) = register_shortcut(&app, b) {
            error!("resume_binding error for id '{}': {}", id, e);
            return Err(e);
        }
    }
    Ok(())
}

// ============================================================================
// Keyboard Implementation Switching
// ============================================================================

/// Result of changing keyboard implementation
#[derive(Serialize, Type)]
pub struct ImplementationChangeResult {
    pub success: bool,
    /// List of binding IDs that were reset to defaults due to incompatibility
    pub reset_bindings: Vec<String>,
}

/// Change the keyboard implementation with runtime switching.
/// This will unregister all shortcuts from the old implementation,
/// validate shortcuts for the new implementation (resetting invalid ones to defaults),
/// and register them with the new implementation.
#[tauri::command]
#[specta::specta]
pub fn change_keyboard_implementation_setting(
    app: AppHandle,
    implementation: String,
) -> Result<ImplementationChangeResult, String> {
    let current_settings = settings::get_settings(&app);
    let current_impl = current_settings.keyboard_implementation;
    let new_impl = parse_keyboard_implementation(&implementation);

    // If same implementation, nothing to do
    if current_impl == new_impl {
        return Ok(ImplementationChangeResult {
            success: true,
            reset_bindings: vec![],
        });
    }

    info!(
        "Switching keyboard implementation from {:?} to {:?}",
        current_impl, new_impl
    );

    // Unregister all shortcuts from the current implementation
    unregister_all_shortcuts(&app, current_impl);

    // Update the setting
    let mut settings = settings::get_settings(&app);
    settings.keyboard_implementation = new_impl;
    settings::write_settings(&app, settings);

    // Initialize new implementation if needed (HandyKeys needs state)
    if new_impl == KeyboardImplementation::HandyKeys && initialize_handy_keys_with_rollback(&app)? {
        // Shortcuts already registered during init
        return Ok(ImplementationChangeResult {
            success: true,
            reset_bindings: vec![],
        });
    }

    // Register all shortcuts with new implementation, resetting invalid ones
    let reset_bindings = register_all_shortcuts_for_implementation(&app, new_impl);

    info!("Keyboard implementation switched to {:?}", new_impl);

    Ok(ImplementationChangeResult {
        success: true,
        reset_bindings,
    })
}

// ============================================================================
// Validation Helpers
// ============================================================================

/// Validate a shortcut for a specific implementation
/// Rechaza atajos globales que secuestrarían el uso normal del teclado: una sola
/// tecla SIN modificador que sirve para escribir o navegar (flechas, espacio,
/// enter, tab, retroceso/suprimir, inicio/fin/página, o una letra o dígito
/// suelto). Registradas como atajo global con push-to-talk, cada pulsación
/// normal de esa tecla dispara un dictado en toda la máquina — es exactamente el
/// fallo de un binding `transcribe = "up"`, donde cada flecha arriba tecleaba
/// texto en el campo enfocado.
///
/// Se permiten: cualquier combinación con modificador (`ctrl+space`), las teclas
/// de función sueltas (`F1`–`F24`) y los modificadores solos (push-to-talk con
/// un modificador). Solo se examina la tecla suelta; combinaciones raras las
/// resuelve el parser de cada implementación.
const MENSAJE_ATAJO_INSEGURO: &str =
    "Esa tecla sola secuestraría su función normal en todo el sistema. Combínala con Ctrl, \
     Alt o Shift (p. ej. Ctrl+Espacio), o usa una tecla de función (F1–F12).";

fn es_atajo_global_seguro(raw: &str) -> Result<(), String> {
    const MODIFICADORES: &[&str] = &[
        "ctrl", "control", "alt", "option", "opt", "altgr", "shift", "cmd", "command", "super",
        "win", "windows", "meta", "hyper",
    ];

    // La tecla '+' literal se perdería al partir por '+'; trátala aparte.
    if raw.trim() == "+" {
        return Err(MENSAJE_ATAJO_INSEGURO.to_string());
    }

    let tokens: Vec<String> = raw
        .split('+')
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    // Con modificador (o solo modificadores) es seguro; sin tokens lo maneja la
    // comprobación de "vacío" de cada implementación.
    if tokens.is_empty() || tokens.iter().any(|t| MODIFICADORES.contains(&t.as_str())) {
        return Ok(());
    }

    // Sin modificador: solo se examina la tecla suelta (combos raros sin
    // modificador los resuelve el parser de cada implementación).
    if tokens.len() != 1 {
        return Ok(());
    }
    let k = tokens[0].as_str();

    // Teclas de función sueltas (F1–F24): seguras como atajo global.
    let es_funcion = k
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .map(|n| (1..=24).contains(&n))
        .unwrap_or(false);
    if es_funcion {
        return Ok(());
    }

    // Peligrosa = cualquier tecla de UN carácter (letra, dígito o puntuación:
    // - . , / ; = ` [ ] ' \ …, y no-ASCII), o del teclado numérico, o de
    // navegación/edición. Todas se usan a diario para escribir o moverse, así que
    // como atajo global con push-to-talk secuestrarían el teclado.
    let un_caracter = k.chars().count() == 1;
    let numpad = k.starts_with("numpad ")
        || matches!(
            k,
            "keypad0"
                | "keypad1"
                | "keypad2"
                | "keypad3"
                | "keypad4"
                | "keypad5"
                | "keypad6"
                | "keypad7"
                | "keypad8"
                | "keypad9"
                | "keypaddecimal"
                | "keypad."
        );
    let nav_o_edicion = matches!(
        k,
        "up" | "down"
            | "left"
            | "right"
            | "arrowup"
            | "arrowdown"
            | "arrowleft"
            | "arrowright"
            | "space"
            | "spacebar"
            | "enter"
            | "return"
            | "tab"
            | "backspace"
            | "delete"
            | "del"
            | "home"
            | "end"
            | "pageup"
            | "pagedown"
            | "pgup"
            | "pgdn"
    );
    if un_caracter || numpad || nav_o_edicion {
        return Err(MENSAJE_ATAJO_INSEGURO.to_string());
    }
    Ok(())
}

/// Restablece a su valor por defecto cualquier atajo GUARDADO que secuestraría el
/// teclado (ver [`es_atajo_global_seguro`]) y persiste el cambio una sola vez. Se
/// llama al arrancar por CUALQUIER implementación (handy_keys y tauri) y en el
/// fallback, para curar configs peligrosas heredadas de versiones sin la
/// validación de `change_binding`. `cancel` se salta (se registra aparte).
/// Avisa al usuario de los atajos que NO se pudieron registrar, con su id y su
/// combinación, para que sepa cuál reasignar.
///
/// Existe porque el aviso que había solo saltaba cuando NINGUNO se registraba: si
/// el dictado entraba y otro chocaba, nadie decía nada y el usuario pulsaba una
/// tecla muerta sin explicación. Encontrado el 30/07 con `ctrl+shift+l`, que en
/// ese equipo lo ocupaba Loom con un hook global.
///
/// Ningún default puede garantizarse —depende del software instalado—, así que la
/// defensa real es avisar y dejar reasignar, no acertar la tecla.
pub(crate) fn avisar_atajos_ocupados(app: &AppHandle, ocupados: &[String]) {
    if ocupados.is_empty() {
        return;
    }
    let lista = ocupados.join(", ");
    warn!("atajos que otra aplicación ya tenía tomados: {lista}");
    crate::user_alerts::alert(
        app,
        crate::user_alerts::AlertKind::AtajoOcupado,
        Some(lista),
    );
}

fn sanear_bindings_peligrosos(app: &AppHandle) {
    let defaults = settings::get_default_settings().bindings;
    let mut s = settings::load_or_create_app_settings(app);
    let mut sanados: Vec<String> = Vec::new();

    for (id, binding) in s.bindings.clone() {
        if id == "cancel" {
            continue;
        }
        if es_atajo_global_seguro(&binding.current_binding).is_ok() {
            continue;
        }
        let Some(def) = defaults.get(&id) else {
            continue;
        };
        info!(
            "Atajo '{}' ('{}') es inseguro como atajo global; se restablece a '{}'.",
            id, binding.current_binding, def.current_binding
        );
        let mut sano = binding.clone();
        sano.current_binding = def.current_binding.clone();
        s.bindings.insert(id.clone(), sano);
        sanados.push(id);
    }

    if !sanados.is_empty() {
        settings::write_settings(app, s);
        info!(
            "Atajos restablecidos por seguridad al arrancar: {:?}",
            sanados
        );
    }
}

fn validate_shortcut_for_implementation(
    raw: &str,
    implementation: KeyboardImplementation,
) -> Result<(), String> {
    // Guarda transversal (ambas implementaciones): fuera teclas sueltas que
    // secuestran el teclado. Va antes del parser específico.
    es_atajo_global_seguro(raw)?;
    match implementation {
        KeyboardImplementation::Tauri => tauri_impl::validate_shortcut(raw),
        KeyboardImplementation::HandyKeys => handy_keys::validate_shortcut(raw),
    }
}

/// Parse a keyboard implementation string into the enum
fn parse_keyboard_implementation(s: &str) -> KeyboardImplementation {
    match s {
        "tauri" => KeyboardImplementation::Tauri,
        "handy_keys" => KeyboardImplementation::HandyKeys,
        other => {
            warn!(
                "Invalid keyboard implementation '{}', defaulting to tauri",
                other
            );
            KeyboardImplementation::Tauri
        }
    }
}

/// Unregister all shortcuts for the current implementation
fn unregister_all_shortcuts(app: &AppHandle, implementation: KeyboardImplementation) {
    let bindings = settings::get_bindings(app);

    for (id, binding) in bindings {
        // Skip cancel shortcut as it's dynamically registered
        if id == "cancel" {
            continue;
        }

        let result = match implementation {
            KeyboardImplementation::Tauri => tauri_impl::unregister_shortcut(app, binding),
            KeyboardImplementation::HandyKeys => handy_keys::unregister_shortcut(app, binding),
        };

        if let Err(e) = result {
            warn!(
                "Failed to unregister shortcut '{}' during switch: {}",
                id, e
            );
        }
    }
}

/// Register all shortcuts for a specific implementation, validating and resetting invalid ones
fn register_all_shortcuts_for_implementation(
    app: &AppHandle,
    implementation: KeyboardImplementation,
) -> Vec<String> {
    let mut reset_bindings = Vec::new();
    let default_bindings = settings::get_default_settings().bindings;
    let mut current_settings = settings::get_settings(app);

    for (id, default_binding) in &default_bindings {
        // Skip cancel shortcut as it's dynamically registered
        if id == "cancel" {
            continue;
        }

        // Skip post-processing shortcut when the feature is disabled

        let mut binding = current_settings
            .bindings
            .get(id)
            .cloned()
            .unwrap_or_else(|| default_binding.clone());

        // Validate the shortcut for the target implementation
        if let Err(e) =
            validate_shortcut_for_implementation(&binding.current_binding, implementation)
        {
            info!(
                "Shortcut '{}' ({}) is invalid for {:?}: {}. Resetting to default.",
                id, binding.current_binding, implementation, e
            );

            // Reset to default
            binding.current_binding = default_binding.current_binding.clone();
            current_settings
                .bindings
                .insert(id.clone(), binding.clone());
            reset_bindings.push(id.clone());
        }

        // Register with the appropriate implementation
        let result = match implementation {
            KeyboardImplementation::Tauri => tauri_impl::register_shortcut(app, binding),
            KeyboardImplementation::HandyKeys => handy_keys::register_shortcut(app, binding),
        };

        if let Err(e) = result {
            error!(
                "Failed to register shortcut '{}' for {:?}: {}",
                id, implementation, e
            );
        }
    }

    // Save settings if any bindings were reset
    if !reset_bindings.is_empty() {
        settings::write_settings(app, current_settings);
    }

    reset_bindings
}

/// Initialize HandyKeys if not already initialized, with rollback on failure
fn initialize_handy_keys_with_rollback(app: &AppHandle) -> Result<bool, String> {
    if app.try_state::<handy_keys::HandyKeysState>().is_some() {
        return Ok(false); // Already initialized, caller should continue
    }

    if let Err(e) = handy_keys::init_shortcuts(app) {
        error!("Failed to initialize HandyKeys: {}", e);
        // Rollback to Tauri
        let mut settings = settings::get_settings(app);
        settings.keyboard_implementation = KeyboardImplementation::Tauri;
        settings::write_settings(app, settings);
        tauri_impl::init_shortcuts(app);
        return Err(format!(
            "Failed to initialize HandyKeys: {}. Reverted to Tauri.",
            e
        ));
    }

    // init_shortcuts already registered shortcuts
    Ok(true)
}

#[cfg(test)]
mod tests_atajo_seguro {
    use super::es_atajo_global_seguro;

    #[test]
    fn rechaza_teclas_sueltas_de_uso_corriente() {
        for k in [
            "up",
            "down",
            "left",
            "right",
            "space",
            "enter",
            "tab",
            "backspace",
            "delete",
            "home",
            "end",
            "pageup",
            "a",
            "z",
            "0",
            "9",
            "UP",
            "Space",
            // Puntuación/OEM suelta (el backtick es un PTT clásico):
            "-",
            ".",
            ",",
            "/",
            ";",
            "=",
            "`",
            "[",
            "]",
            "'",
            "\\",
            "+", // se perdería en split('+'); tratada aparte
            "§",
            // Teclado numérico:
            "keypad5",
            "keypaddecimal",
            "numpad 5",
        ] {
            assert!(
                es_atajo_global_seguro(k).is_err(),
                "'{k}' debería rechazarse como atajo global"
            );
        }
    }

    #[test]
    fn acepta_combinaciones_funciones_y_modificadores_solos() {
        for k in [
            "ctrl+space",
            "ctrl+shift+space",
            "alt+up",  // con modificador, la flecha ya es segura
            "ctrl+-",  // con modificador, la puntuación ya es segura
            "alt+.",   // idem
            "shift+`", // idem
            "option+space",
            "f1",
            "f8",
            "f12",
            "f24",
            "ctrl", // modificador solo (push-to-talk)
            "shift",
        ] {
            assert!(es_atajo_global_seguro(k).is_ok(), "'{k}' debería aceptarse");
        }
    }
}
