use crate::audio_feedback;
use crate::audio_toolkit::audio::{list_input_devices, list_output_devices};
use crate::managers::audio::{AudioRecordingManager, MicrophoneMode};
use crate::settings::{get_settings, write_settings};
use log::warn;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

#[cfg(target_os = "windows")]
use winreg::{
    enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    RegKey, HKEY,
};

#[derive(Serialize, Type)]
pub struct CustomSounds {
    start: bool,
    stop: bool,
}

fn custom_sound_exists(app: &AppHandle, sound_type: &str) -> bool {
    crate::portable::resolve_app_data(app, &format!("custom_{}.wav", sound_type))
        .is_ok_and(|path| path.exists())
}

#[tauri::command]
#[specta::specta]
pub fn check_custom_sounds(app: AppHandle) -> CustomSounds {
    CustomSounds {
        start: custom_sound_exists(&app, "start"),
        stop: custom_sound_exists(&app, "stop"),
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct AudioDevice {
    pub index: String,
    pub name: String,
    pub is_default: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PermissionAccess {
    Allowed,
    Denied,
    Unknown,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct WindowsMicrophonePermissionStatus {
    pub supported: bool,
    pub overall_access: PermissionAccess,
    pub device_access: PermissionAccess,
    pub app_access: PermissionAccess,
    pub desktop_app_access: PermissionAccess,
}

#[cfg(target_os = "windows")]
fn read_registry_permission_access(root_hkey: HKEY, path: &str) -> PermissionAccess {
    let root = RegKey::predef(root_hkey);
    let Ok(key) = root.open_subkey(path) else {
        return PermissionAccess::Unknown;
    };

    let Ok(value) = key.get_value::<String, _>("Value") else {
        return PermissionAccess::Unknown;
    };

    match value.to_ascii_lowercase().as_str() {
        "allow" => PermissionAccess::Allowed,
        "deny" => PermissionAccess::Denied,
        _ => PermissionAccess::Unknown,
    }
}

#[cfg(target_os = "windows")]
fn get_windows_microphone_permission_status_impl() -> WindowsMicrophonePermissionStatus {
    const MICROPHONE_PATH: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone";
    const DESKTOP_APPS_PATH: &str =
        "Software\\Microsoft\\Windows\\CurrentVersion\\CapabilityAccessManager\\ConsentStore\\microphone\\NonPackaged";

    let device_access = read_registry_permission_access(HKEY_LOCAL_MACHINE, MICROPHONE_PATH);
    let app_access = read_registry_permission_access(HKEY_CURRENT_USER, MICROPHONE_PATH);
    let desktop_app_access = read_registry_permission_access(HKEY_CURRENT_USER, DESKTOP_APPS_PATH);

    let overall_access = if [device_access, app_access, desktop_app_access]
        .into_iter()
        .any(|access| access == PermissionAccess::Denied)
    {
        PermissionAccess::Denied
    } else if [device_access, app_access, desktop_app_access]
        .into_iter()
        .all(|access| access == PermissionAccess::Allowed)
    {
        PermissionAccess::Allowed
    } else {
        PermissionAccess::Unknown
    };

    WindowsMicrophonePermissionStatus {
        supported: true,
        overall_access,
        device_access,
        app_access,
        desktop_app_access,
    }
}

#[tauri::command]
#[specta::specta]
pub fn get_windows_microphone_permission_status() -> WindowsMicrophonePermissionStatus {
    #[cfg(target_os = "windows")]
    {
        get_windows_microphone_permission_status_impl()
    }

    #[cfg(not(target_os = "windows"))]
    {
        WindowsMicrophonePermissionStatus {
            supported: false,
            overall_access: PermissionAccess::Unknown,
            device_access: PermissionAccess::Unknown,
            app_access: PermissionAccess::Unknown,
            desktop_app_access: PermissionAccess::Unknown,
        }
    }
}

#[tauri::command]
#[specta::specta]
pub fn open_microphone_privacy_settings() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::process::Command;
        Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:privacy-microphone"])
            .spawn()
            .map_err(|e| format!("Failed to open Windows microphone privacy settings: {}", e))?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err("Opening microphone privacy settings is only supported on Windows".to_string())
    }
}

#[tauri::command]
#[specta::specta]
pub fn update_microphone_mode(app: AppHandle, always_on: bool) -> Result<(), String> {
    // Update settings
    let mut settings = get_settings(&app);
    settings.always_on_microphone = always_on;
    write_settings(&app, settings);

    // Update the audio manager mode
    let rm = app.state::<Arc<AudioRecordingManager>>();
    let new_mode = if always_on {
        MicrophoneMode::AlwaysOn
    } else {
        MicrophoneMode::OnDemand
    };

    rm.update_mode(new_mode)
        .map_err(|e| format!("Failed to update microphone mode: {}", e))
}

#[tauri::command]
#[specta::specta]
pub fn get_available_microphones() -> Result<Vec<AudioDevice>, String> {
    let devices =
        list_input_devices().map_err(|e| format!("Failed to list audio devices: {}", e))?;

    let mut result = vec![AudioDevice {
        index: "default".to_string(),
        name: "Default".to_string(),
        is_default: true,
    }];

    result.extend(devices.into_iter().map(|d| AudioDevice {
        index: d.index,
        name: d.name,
        is_default: false, // The explicit default is handled separately
    }));

    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn set_selected_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);

    // Update the audio manager to use the new device
    let rm = app.state::<Arc<AudioRecordingManager>>();
    rm.update_selected_device()
        .map_err(|e| format!("Failed to update selected device: {}", e))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn get_available_output_devices() -> Result<Vec<AudioDevice>, String> {
    let devices =
        list_output_devices().map_err(|e| format!("Failed to list output devices: {}", e))?;

    let mut result = vec![AudioDevice {
        index: "default".to_string(),
        name: "Default".to_string(),
        is_default: true,
    }];

    result.extend(devices.into_iter().map(|d| AudioDevice {
        index: d.index,
        name: d.name,
        is_default: false, // The explicit default is handled separately
    }));

    Ok(result)
}

#[tauri::command]
#[specta::specta]
pub fn set_selected_output_device(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.selected_output_device = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn play_test_sound(app: AppHandle, sound_type: String) {
    let sound = match sound_type.as_str() {
        "start" => audio_feedback::SoundType::Start,
        "stop" => audio_feedback::SoundType::Stop,
        _ => {
            warn!("Unknown sound type: {}", sound_type);
            return;
        }
    };
    audio_feedback::play_test_sound(&app, sound);
}

#[tauri::command]
#[specta::specta]
pub fn set_clamshell_microphone(app: AppHandle, device_name: String) -> Result<(), String> {
    let mut settings = get_settings(&app);
    settings.clamshell_microphone = if device_name == "default" {
        None
    } else {
        Some(device_name)
    };
    write_settings(&app, settings);
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn is_recording(app: AppHandle) -> bool {
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    audio_manager.is_recording()
}

/// Veredicto de la prueba de micrófono, calculado sobre la señal REAL que
/// recibiría el modelo (16 kHz mono, post-captura, sin VAD).
#[derive(Serialize, Type, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VeredictoMicrofono {
    /// Pico < -40 dBFS: el micrófono no está entregando señal.
    SinSenal,
    /// RMS < -34 dBFS: demasiado bajo — el dictado va a fallar.
    MuyBajo,
    /// RMS entre -34 y -28 dBFS: funciona, pero con errores de precisión.
    Bajo,
    /// RMS ≥ -28 dBFS sin saturación: zona sana.
    Sano,
    /// Más del 1% de muestras al tope: el micrófono está saturando.
    Saturado,
}

#[derive(Serialize, Type, Debug, Clone)]
pub struct PruebaMicrofono {
    /// Nombre real del dispositivo usado — revela qué hay detrás de "Default".
    pub dispositivo: String,
    /// `true` si se usó el default del sistema (no había micrófono elegido).
    pub es_default: bool,
    pub duracion_s: f32,
    pub rms_db: f32,
    pub pico_db: f32,
    pub clip_pct: f32,
    pub veredicto: VeredictoMicrofono,
    /// Ruta absoluta del WAV grabado, para reproducirlo en la UI.
    pub wav: String,
}

/// Piso de dBFS para silencio digital. Evitamos `-inf`: `serde_json` lo
/// serializa como `null` y el frontend hace `.toFixed()` sobre él → crash.
/// -120 dBFS está muy por debajo del ruido de cualquier micrófono real.
const DB_PISO: f32 = -120.0;

/// RMS y pico en dBFS + % de muestras saturadas, sobre f32 en [-1, 1].
fn estadisticas_senal(samples: &[f32]) -> (f32, f32, f32) {
    if samples.is_empty() {
        return (DB_PISO, DB_PISO, 0.0);
    }
    let mut suma_sq = 0.0f64;
    let mut pico = 0.0f32;
    let mut clip = 0usize;
    for &s in samples {
        let a = s.abs();
        suma_sq += (s as f64) * (s as f64);
        if a > pico {
            pico = a;
        }
        if a >= 0.985 {
            clip += 1;
        }
    }
    let rms = (suma_sq / samples.len() as f64).sqrt() as f32;
    let db = |x: f32| {
        if x > 0.0 {
            (20.0 * x.log10()).max(DB_PISO)
        } else {
            DB_PISO
        }
    };
    (
        db(rms),
        db(pico),
        clip as f32 * 100.0 / samples.len() as f32,
    )
}

/// Umbrales elegidos con grabaciones reales: los dictados que transcribían mal
/// medían RMS -34..-36 dBFS; los que transcribían bien, -24..-26 dBFS.
fn veredicto_senal(rms_db: f32, pico_db: f32, clip_pct: f32) -> VeredictoMicrofono {
    if pico_db < -40.0 {
        VeredictoMicrofono::SinSenal
    } else if clip_pct > 1.0 {
        VeredictoMicrofono::Saturado
    } else if rms_db < -34.0 {
        VeredictoMicrofono::MuyBajo
    } else if rms_db < -28.0 {
        VeredictoMicrofono::Bajo
    } else {
        VeredictoMicrofono::Sano
    }
}

/// Graba unos segundos con el micrófono configurado (o el default del sistema),
/// SIN VAD y sin tocar el pipeline de dictado, y devuelve nivel + veredicto +
/// el WAV para reproducir. Es la respuesta a "¿qué está escuchando ABRAX de
/// verdad?": la misma señal 16 kHz mono que recibiría el modelo.
#[tauri::command]
#[specta::specta]
pub async fn probar_microfono(app: AppHandle, duracion_ms: u32) -> Result<PruebaMicrofono, String> {
    use crate::audio_toolkit::{save_wav_file, AudioRecorder, VadPolicy};

    let duracion = duracion_ms.clamp(1000, 5000);
    let wav_path = crate::portable::resolve_app_data(&app, "prueba_microfono.wav")
        .map_err(|e| format!("No se pudo resolver la ruta del WAV: {e}"))?;
    let settings = get_settings(&app);

    tauri::async_runtime::spawn_blocking(move || {
        // Resolución del dispositivo: igual que el pipeline (por nombre), pero
        // revelando SIEMPRE el nombre real — "Default" deja de ser caja negra.
        let (device, dispositivo, es_default) = match &settings.selected_microphone {
            Some(nombre) if !nombre.is_empty() && nombre != "default" => {
                let dev = list_input_devices()
                    .map_err(|e| format!("No se pudieron listar micrófonos: {e}"))?
                    .into_iter()
                    .find(|d| &d.name == nombre)
                    .map(|d| d.device);
                match dev {
                    Some(d) => (Some(d), nombre.clone(), false),
                    None => {
                        warn!("Micrófono configurado no encontrado; se prueba el default");
                        (None, nombre_default_sistema(), true)
                    }
                }
            }
            _ => (None, nombre_default_sistema(), true),
        };

        let mut rec = AudioRecorder::new().map_err(|e| format!("Recorder: {e}"))?;
        rec.open(device, None)
            .map_err(|e| format!("Micrófono: {e}"))?;
        rec.start(VadPolicy::Disabled)
            .map_err(|e| format!("Inicio de captura: {e}"))?;
        std::thread::sleep(std::time::Duration::from_millis(u64::from(duracion)));
        let samples = rec.stop().map_err(|e| format!("Fin de captura: {e}"))?;
        let _ = rec.close();

        let (rms_db, pico_db, clip_pct) = estadisticas_senal(&samples);
        save_wav_file(&wav_path, &samples)
            .map_err(|e| format!("No se pudo guardar el WAV: {e}"))?;

        // Solo métricas al log — jamás contenido (el WAV queda en el datadir).
        log::info!(
            "prueba de micrófono: '{}' default={} {:.2}s rms={:.1}dBFS pico={:.1}dBFS clip={:.2}%",
            dispositivo,
            es_default,
            samples.len() as f32 / 16000.0,
            rms_db,
            pico_db,
            clip_pct
        );

        Ok(PruebaMicrofono {
            dispositivo,
            es_default,
            duracion_s: samples.len() as f32 / 16000.0,
            rms_db,
            pico_db,
            clip_pct,
            veredicto: veredicto_senal(rms_db, pico_db, clip_pct),
            wav: wav_path.to_string_lossy().to_string(),
        })
    })
    .await
    .map_err(|e| format!("La prueba de micrófono se interrumpió: {e}"))?
}

/// Nombre del dispositivo que resuelve el default del sistema (rol Console de
/// WASAPI vía cpal — puede diferir del default de "Comunicaciones" que usan
/// apps de llamadas; por eso lo mostramos siempre).
fn nombre_default_sistema() -> String {
    use cpal::traits::{DeviceTrait, HostTrait};
    crate::audio_toolkit::get_cpal_host()
        .default_input_device()
        .and_then(|d| d.name().ok())
        .unwrap_or_else(|| "desconocido".to_string())
}

/// Subscribe the calling window to spectrum frames (R8 subscription model).
/// Idempotent per window label; frames flow only while ≥1 subscriber exists.
#[tauri::command]
#[specta::specta]
pub fn start_spectrum(window: tauri::WebviewWindow) {
    crate::overlay::spectrum_subscribe(window.label());
}

/// Unsubscribe the calling window from spectrum frames. When the last
/// subscriber leaves, spectrum analysis and emission stop entirely.
#[tauri::command]
#[specta::specta]
pub fn stop_spectrum(window: tauri::WebviewWindow) {
    crate::overlay::spectrum_unsubscribe(window.label());
}

#[cfg(test)]
mod tests_prueba_microfono {
    use super::*;

    #[test]
    fn silencio_absoluto_es_sin_senal() {
        let (rms, pico, clip) = estadisticas_senal(&vec![0.0f32; 16000]);
        assert_eq!(
            veredicto_senal(rms, pico, clip),
            VeredictoMicrofono::SinSenal
        );
    }

    #[test]
    fn nivel_de_dictado_malo_real_es_muy_bajo() {
        // Réplica del caso de campo: RMS ≈ -36 dBFS (señal senoidal a 0.0158).
        let señal: Vec<f32> = (0..16000)
            .map(|i| 0.0224 * (i as f32 * 0.5).sin())
            .collect();
        let (rms, pico, clip) = estadisticas_senal(&señal);
        assert!(rms < -34.0, "rms {rms}");
        assert_eq!(
            veredicto_senal(rms, pico, clip),
            VeredictoMicrofono::MuyBajo
        );
    }

    #[test]
    fn nivel_sano_es_sano() {
        // RMS ≈ -23 dBFS: la zona de los dictados que sí transcribían bien.
        let señal: Vec<f32> = (0..16000).map(|i| 0.1 * (i as f32 * 0.5).sin()).collect();
        let (rms, pico, clip) = estadisticas_senal(&señal);
        assert_eq!(veredicto_senal(rms, pico, clip), VeredictoMicrofono::Sano);
    }

    #[test]
    fn saturacion_gana_aunque_el_rms_sea_sano() {
        let mut señal: Vec<f32> = (0..16000).map(|i| 0.2 * (i as f32 * 0.5).sin()).collect();
        for s in señal.iter_mut().take(400) {
            *s = 1.0; // 2.5% de muestras al tope
        }
        let (rms, pico, clip) = estadisticas_senal(&señal);
        assert!(clip > 1.0);
        assert_eq!(
            veredicto_senal(rms, pico, clip),
            VeredictoMicrofono::Saturado
        );
    }

    #[test]
    fn senal_vacia_no_paniquea() {
        let (rms, pico, clip) = estadisticas_senal(&[]);
        assert_eq!(
            veredicto_senal(rms, pico, clip),
            VeredictoMicrofono::SinSenal
        );
    }

    #[test]
    fn silencio_devuelve_db_finitos_serializables() {
        // Regresión: -inf se serializaba como `null` en JSON y el frontend
        // hacía `.toFixed()` sobre él → TypeError justo con el mic silenciado,
        // el caso que la prueba debe diagnosticar. Los dB deben ser finitos.
        for señal in [vec![0.0f32; 16000], vec![]] {
            let (rms, pico, _) = estadisticas_senal(&señal);
            assert!(rms.is_finite(), "rms no finito: {rms}");
            assert!(pico.is_finite(), "pico no finito: {pico}");
            assert_eq!(rms, DB_PISO);
            assert_eq!(pico, DB_PISO);
        }
    }
}
