use crate::managers::audio::AudioRecordingManager;
use crate::managers::transcription::TranscriptionManager;
use crate::shortcut;
use crate::TranscriptionCoordinator;
use log::info;
use std::sync::Arc;
use tauri::{AppHandle, Manager};

/// Construye un `Command` que **no abre consola en Windows**.
///
/// En Windows, un proceso hijo lanzado desde una app GUI abre su propia ventana
/// de consola —el recuadro negro que parpadea— salvo que se pida lo contrario con
/// `CREATE_NO_WINDOW`. Nada de lo que ABRAX lanza por dentro debe ser visible: el
/// usuario descargó una voz, no pidió abrir una terminal.
///
/// Encontrado el 30/07 al descargar un modelo de voz: `uv` creando el entorno de
/// Python abría una consola que aparecía y desaparecía. Ningún `Command` del
/// proyecto usaba el flag, así que pasaba en los cinco sitios que lanzan procesos
/// en Windows.
///
/// **Úsalo en vez de `Command::new` para cualquier proceso que pueda correr en
/// Windows.** En el resto de plataformas es exactamente `Command::new`.
pub fn comando_silencioso(programa: impl AsRef<std::ffi::OsStr>) -> std::process::Command {
    let mut cmd = std::process::Command::new(programa);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW. Se escribe el literal en vez de depender de la crate
        // `windows-sys` solo para esta constante.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Centralized cancellation function that can be called from anywhere in the app.
/// Handles cancelling both recording and transcription operations and updates UI state.
pub fn cancel_current_operation(app: &AppHandle) {
    info!("Initiating operation cancellation...");

    // Unregister the cancel shortcut asynchronously
    shortcut::unregister_cancel_shortcut(app);

    // Cancel any ongoing recording
    let audio_manager = app.state::<Arc<AudioRecordingManager>>();
    let recording_was_active = audio_manager.is_recording();
    audio_manager.cancel_recording();
    // Deshacer el mute-al-grabar si estaba puesto: la ruta de cancelación no
    // pasa por el stop normal (que sí lo quita), y sin esto la salida — incluida
    // la lectura del TTS — quedaría silenciada. `remove_mute` es idempotente.
    audio_manager.remove_mute();

    // Abandon any live streaming transcription
    let tm = app.state::<Arc<TranscriptionManager>>();
    tm.cancel_stream();

    // Update tray icon and hide overlay
    crate::tray::change_tray_icon(app, crate::tray::TrayIconState::Idle);
    crate::overlay::hide_recording_overlay(app);

    // Unload model if immediate unload is enabled
    tm.maybe_unload_immediately("cancellation");

    // Notify coordinator so it can keep lifecycle state coherent.
    if let Some(coordinator) = app.try_state::<TranscriptionCoordinator>() {
        coordinator.notify_cancel(recording_was_active);
    }

    info!("Operation cancellation completed - returned to idle state");
}

/// Whether the platform can render a transparent, frameless window. Windows
/// (WebView2) and macOS (private API, enabled in `tauri.conf.json`) support it;
/// on Linux it needs a compositor, so we require an active display server and
/// otherwise fall back to the decorated classic-style window (the retro shell
/// still renders, just on a solid backdrop instead of a floating one).
pub fn supports_transparency() -> bool {
    #[cfg(target_os = "linux")]
    {
        std::env::var("WAYLAND_DISPLAY").is_ok() || std::env::var("DISPLAY").is_ok()
    }
    #[cfg(not(target_os = "linux"))]
    {
        true
    }
}

/// Check if using the Wayland display server protocol
#[cfg(target_os = "linux")]
pub fn is_wayland() -> bool {
    std::env::var("WAYLAND_DISPLAY").is_ok()
        || std::env::var("XDG_SESSION_TYPE")
            .map(|v| v.to_lowercase() == "wayland")
            .unwrap_or(false)
}

/// Check if running on KDE Plasma desktop environment
#[cfg(target_os = "linux")]
pub fn is_kde_plasma() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|v| v.to_uppercase().contains("KDE"))
        .unwrap_or(false)
        || std::env::var("KDE_SESSION_VERSION").is_ok()
}

/// Check if running on KDE Plasma with Wayland
#[cfg(target_os = "linux")]
pub fn is_kde_wayland() -> bool {
    is_wayland() && is_kde_plasma()
}

/// Asocia un proceso hijo al Job de ABRAX, para que **el sistema operativo** lo
/// mate cuando muera la app.
///
/// # Por qué existe
///
/// Los servidores de voz son procesos aparte (`python.exe` dentro del venv, el
/// binario de Piper). Sus destructores los matan al cerrar bien… pero **`Drop` no
/// corre si la app muere a la fuerza**: un cierre desde el Administrador de
/// tareas, un cuelgue, un `Stop-Process`. Ahí el hijo sobrevive.
///
/// Y un hijo huérfano no es solo RAM desperdiciada: retiene su propio ejecutable
/// dentro del venv, así que `remove_dir_all` falla con «Acceso denegado», `uv
/// venv` se niega, y **reinstalar el motor deja de ser posible desde la app**.
/// Medido el 30/07 en un equipo real: tres pythons huérfanos bloqueando los dos
/// motores a la vez.
///
/// Un Job Object con `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` lo resuelve en la raíz:
/// cuando el proceso de ABRAX termina —de la forma que sea— Windows cierra el
/// handle del Job y mata a todo lo que tenga dentro. No depende de que corra
/// ningún código nuestro, que es justo la garantía que faltaba.
///
/// Es best-effort: si algo falla se registra y se sigue. Un motor de voz sin
/// niñera es peor que no tenerlo, pero mucho mejor que no arrancar.
#[cfg(target_os = "windows")]
pub fn adoptar_hijo(child: &std::process::Child) {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::Foundation::HANDLE;
    use windows::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    // Un solo Job para toda la app, creado la primera vez que se necesita. Se
    // deja vivo a propósito durante toda la ejecución: si se cerrara su handle,
    // KILL_ON_JOB_CLOSE mataría a los hijos EN ESE MOMENTO, que es exactamente lo
    // contrario de lo que se busca.
    static JOB: std::sync::OnceLock<isize> = std::sync::OnceLock::new();

    let job = *JOB.get_or_init(|| unsafe {
        let handle = match CreateJobObjectW(None, None) {
            Ok(h) => h,
            Err(e) => {
                log::warn!("[job] no se pudo crear el Job Object: {e}");
                return 0;
            }
        };
        let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        if let Err(e) = SetInformationJobObject(
            handle,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        ) {
            log::warn!("[job] no se pudo configurar KILL_ON_JOB_CLOSE: {e}");
            return 0;
        }
        log::info!("[job] Job Object listo: los hijos morirán con la app");
        handle.0 as isize
    });

    if job == 0 {
        return; // ya se avisó al crearlo
    }
    unsafe {
        if let Err(e) = AssignProcessToJobObject(
            HANDLE(job as *mut std::ffi::c_void),
            HANDLE(child.as_raw_handle() as *mut std::ffi::c_void),
        ) {
            log::warn!("[job] no se pudo adoptar el proceso hijo: {e}");
        }
    }
}

/// En Unix no hace falta: el caso que motiva esto —un ejecutable en uso que no se
/// puede borrar— no existe ahí, y los huérfanos los recoge el sistema.
#[cfg(not(target_os = "windows"))]
pub fn adoptar_hijo(_child: &std::process::Child) {}
