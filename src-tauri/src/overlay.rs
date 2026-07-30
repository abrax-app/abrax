use crate::audio_toolkit::SpectrumFrame;
use crate::input;
use crate::settings;
use crate::settings::{OverlayPosition, OverlayStyle};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

#[cfg(not(target_os = "macos"))]
use log::debug;

#[cfg(not(target_os = "macos"))]
use tauri::WebviewWindowBuilder;

#[cfg(target_os = "macos")]
use tauri::WebviewUrl;

#[cfg(target_os = "macos")]
use tauri_nspanel::{tauri_panel, CollectionBehavior, PanelBuilder, PanelLevel, StyleMask};

#[cfg(target_os = "linux")]
use gtk_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

#[cfg(target_os = "linux")]
use std::env;

#[cfg(target_os = "macos")]
tauri_panel! {
    panel!(RecordingOverlayPanel {
        config: {
            can_become_key_window: false,
            is_floating_panel: true
        }
    })
}

// Native overlay window sizes (logical points). One window is reused for every
// state and resized in `show_overlay_state`; each size need only be at least as
// large as the card it hosts (the `--ov-*` vars in RecordingOverlay.css). The
// card is CSS-anchored flush to the screen edge, so window height doesn't move
// where the card sits — only OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET do. Keep
// these in sync with the CSS card geometry.
//
// Compact overlay (Minimal / transcribing / processing): the 40h pill animates
// width from 172 (--ov-rest-w) to 216 (--ov-work-w) and expands from center, so
// the window must fit the widest state plus a little slack.
const OVERLAY_WIDTH: f64 = 256.0;
const OVERLAY_HEIGHT: f64 = 46.0;

// Actual is 394x118, just a little extra
const OVERLAY_STREAM_WIDTH: f64 = 400.0;
const OVERLAY_STREAM_HEIGHT: f64 = 120.0;

// Esfera style: a square stage for the audio-reactive sphere plus a slim
// status row underneath. Constant across states so the window never resizes
// mid-dictation (the sphere itself morphs between recording/working).
/// Overlay de LECTURA (Escucha): un parlante y nada más. Deliberadamente
/// diminuto — aparece mientras se lee en voz alta y no debe competir con lo que
/// el usuario esté mirando, que es justo el texto que le están leyendo.
const OVERLAY_LEYENDO_WIDTH: f64 = 76.0;
const OVERLAY_LEYENDO_HEIGHT: f64 = 44.0;

const OVERLAY_ESFERA_WIDTH: f64 = 240.0;
const OVERLAY_ESFERA_HEIGHT: f64 = 252.0;

/// Overlay window size (logical) for a given style + UI state.
fn overlay_dimensions(style: OverlayStyle, state: &str) -> (f64, f64) {
    // `leyendo` se comprueba ANTES que el estilo: leer en voz alta no es dictar, y
    // no debe heredar el tamaño de la esfera ni del panel Live. Es el mismo
    // overlay reutilizado, con su propia forma.
    if state == "leyendo" {
        (OVERLAY_LEYENDO_WIDTH, OVERLAY_LEYENDO_HEIGHT)
    } else if style == OverlayStyle::Esfera {
        (OVERLAY_ESFERA_WIDTH, OVERLAY_ESFERA_HEIGHT)
    } else if state == "streaming" {
        (OVERLAY_STREAM_WIDTH, OVERLAY_STREAM_HEIGHT)
    } else {
        (OVERLAY_WIDTH, OVERLAY_HEIGHT)
    }
}

static LAST_SPECTRUM_EMIT: AtomicU64 = AtomicU64::new(0);
const EMIT_THROTTLE_MS: u64 = 33; // ~30 FPS

#[cfg(target_os = "macos")]
const OVERLAY_TOP_OFFSET: f64 = 46.0;
#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_TOP_OFFSET: f64 = 4.0;

#[cfg(target_os = "macos")]
const OVERLAY_BOTTOM_OFFSET: f64 = 15.0;

#[cfg(any(target_os = "windows", target_os = "linux"))]
const OVERLAY_BOTTOM_OFFSET: f64 = 40.0;

#[cfg(target_os = "linux")]
fn update_gtk_layer_shell_anchors(overlay_window: &tauri::webview::WebviewWindow) {
    let window_clone = overlay_window.clone();
    let _ = overlay_window.run_on_main_thread(move || {
        // Try to get the GTK window from the Tauri webview
        if let Ok(gtk_window) = window_clone.gtk_window() {
            let settings = settings::get_settings(window_clone.app_handle());
            match settings.overlay_position {
                OverlayPosition::Top => {
                    gtk_window.set_anchor(Edge::Top, true);
                    gtk_window.set_anchor(Edge::Bottom, false);
                }
                OverlayPosition::Bottom => {
                    gtk_window.set_anchor(Edge::Bottom, true);
                    gtk_window.set_anchor(Edge::Top, false);
                }
            }
        }
    });
}

/// Returns true when the environment variable is set to a truthy value
/// (e.g. "1", "true", "yes", "on").
/// "0", "false", "no", "off" and empty string are treated as falsy (case-insensitive).
/// Returns false when the variable is not set.
#[cfg(target_os = "linux")]
fn env_flag_enabled(name: &str) -> bool {
    match env::var(name) {
        Ok(v) => !matches!(
            v.trim().to_ascii_lowercase().as_str(),
            "" | "0" | "false" | "no" | "off"
        ),
        Err(_) => false,
    }
}

/// Initializes GTK layer shell for Linux overlay window
/// Returns true if layer shell was successfully initialized, false otherwise
#[cfg(target_os = "linux")]
fn init_gtk_layer_shell(overlay_window: &tauri::webview::WebviewWindow) -> bool {
    if env_flag_enabled("HANDY_NO_GTK_LAYER_SHELL") {
        debug!("Skipping GTK layer shell init (HANDY_NO_GTK_LAYER_SHELL is enabled)");
        return false;
    }

    if !gtk_layer_shell::is_supported() {
        return false;
    }

    // Try to get the GTK window from the Tauri webview
    if let Ok(gtk_window) = overlay_window.gtk_window() {
        // Initialize layer shell
        gtk_window.init_layer_shell();
        gtk_window.set_layer(Layer::Overlay);
        gtk_window.set_keyboard_mode(KeyboardMode::None);
        gtk_window.set_exclusive_zone(0);

        update_gtk_layer_shell_anchors(overlay_window);

        return true;
    }
    false
}

/// Forces a window to be topmost using Win32 API (Windows only)
/// This is more reliable than Tauri's set_always_on_top which can be overridden
#[cfg(target_os = "windows")]
fn force_overlay_topmost(overlay_window: &tauri::webview::WebviewWindow) {
    use windows::Win32::UI::WindowsAndMessaging::{
        SetWindowPos, HWND_TOPMOST, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW,
    };

    // Clone because run_on_main_thread takes 'static
    let overlay_clone = overlay_window.clone();

    // Make sure the Win32 call happens on the UI thread
    let _ = overlay_clone.clone().run_on_main_thread(move || {
        if let Ok(hwnd) = overlay_clone.hwnd() {
            unsafe {
                // Force Z-order: make this window topmost without changing size/pos or stealing focus
                let _ = SetWindowPos(
                    hwnd,
                    Some(HWND_TOPMOST),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                );
            }
        }
    });
}

fn get_monitor_with_cursor(app_handle: &AppHandle) -> Option<tauri::Monitor> {
    if let Some(mouse_location) = input::get_cursor_position(app_handle) {
        if let Ok(monitors) = app_handle.available_monitors() {
            for monitor in monitors {
                // Tauri's monitor position/size are physical pixels, but enigo
                // may return logical coordinates (confirmed on macOS via
                // NSEvent::mouseLocation; on Windows, GetCursorPos behavior
                // depends on the process DPI-awareness context). Dividing by
                // scale_factor normalizes to logical, which is safe regardless:
                // if enigo returns logical it matches directly, and if it returns
                // physical on a scale=1 monitor the division is a no-op.
                let scale = monitor.scale_factor();
                let pos = PhysicalPosition::new(
                    (monitor.position().x as f64 / scale) as i32,
                    (monitor.position().y as f64 / scale) as i32,
                );
                let size = PhysicalSize::new(
                    (monitor.size().width as f64 / scale) as u32,
                    (monitor.size().height as f64 / scale) as u32,
                );
                if is_mouse_within_monitor(mouse_location, &pos, &size) {
                    return Some(monitor);
                }
            }
        }
    }

    app_handle.primary_monitor().ok().flatten()
}

fn is_mouse_within_monitor(
    mouse_pos: (i32, i32),
    monitor_pos: &PhysicalPosition<i32>,
    monitor_size: &PhysicalSize<u32>,
) -> bool {
    let (mouse_x, mouse_y) = mouse_pos;
    let PhysicalPosition {
        x: monitor_x,
        y: monitor_y,
    } = *monitor_pos;
    let PhysicalSize {
        width: monitor_width,
        height: monitor_height,
    } = *monitor_size;

    mouse_x >= monitor_x
        && mouse_x < (monitor_x + monitor_width as i32)
        && mouse_y >= monitor_y
        && mouse_y < (monitor_y + monitor_height as i32)
}

/// Returns overlay position in logical coordinates (points on macOS).
///
/// Uses monitor position/size directly rather than work_area(), which can
/// return incorrect coordinates on macOS for monitors with negative positions.
/// The per-platform OVERLAY_TOP_OFFSET / OVERLAY_BOTTOM_OFFSET constants
/// already account for system chrome (menu bar, taskbar).
///
/// We must use LogicalPosition (not PhysicalPosition) because Tauri/tao
/// converts PhysicalPosition using the scale factor of the monitor the window
/// is *currently* on, which is wrong when moving cross-monitor.
fn calculate_overlay_position(
    app_handle: &AppHandle,
    width: f64,
    height: f64,
) -> Option<(f64, f64)> {
    let monitor = get_monitor_with_cursor(app_handle)?;
    let scale = monitor.scale_factor();
    let monitor_x = monitor.position().x as f64 / scale;
    let monitor_y = monitor.position().y as f64 / scale;
    let monitor_width = monitor.size().width as f64 / scale;
    let monitor_height = monitor.size().height as f64 / scale;

    let settings = settings::get_settings(app_handle);

    let x = monitor_x + (monitor_width - width) / 2.0;
    let y = match settings.overlay_position {
        OverlayPosition::Top => monitor_y + OVERLAY_TOP_OFFSET,
        OverlayPosition::Bottom => monitor_y + monitor_height - height - OVERLAY_BOTTOM_OFFSET,
    };

    Some((x, y))
}

/// Current overlay window size in logical units (points), for repositioning
/// without assuming a fixed size (compact vs. streaming).
fn current_overlay_logical_size(window: &tauri::webview::WebviewWindow) -> Option<(f64, f64)> {
    let size = window.inner_size().ok()?;
    let scale = window.scale_factor().ok()?;
    Some((size.width as f64 / scale, size.height as f64 / scale))
}

/// Creates the recording overlay window and keeps it hidden by default
#[cfg(not(target_os = "macos"))]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    // On Linux (Wayland), monitor detection often fails, but we don't need exact coordinates
    // for Layer Shell as we use anchors. On other platforms, we require a monitor.
    #[cfg(not(target_os = "linux"))]
    {
        let position = calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT);
        if position.is_none() {
            debug!("Failed to determine overlay position, not creating overlay window");
            return;
        }
    }

    // Position starts unset — update_overlay_position() sets the correct
    // LogicalPosition before the overlay is shown.
    let mut builder = WebviewWindowBuilder::new(
        app_handle,
        "recording_overlay",
        tauri::WebviewUrl::App("src/overlay/index.html".into()),
    )
    .title("Recording")
    .resizable(false)
    .inner_size(OVERLAY_WIDTH, OVERLAY_HEIGHT)
    .shadow(false)
    .maximizable(false)
    .minimizable(false)
    .closable(false)
    .accept_first_mouse(true)
    .decorations(false)
    .always_on_top(true)
    .skip_taskbar(true)
    .transparent(true)
    .focusable(false)
    .focused(false)
    .visible(false);

    if let Some(data_dir) = crate::portable::data_dir() {
        builder = builder.data_directory(data_dir.join("webview"));
    }

    #[allow(unused_variables)]
    match builder.build() {
        Ok(window) => {
            #[cfg(target_os = "linux")]
            {
                // Try to initialize GTK layer shell, ignore errors if compositor doesn't support it
                if init_gtk_layer_shell(&window) {
                    debug!("GTK layer shell initialized for overlay window");
                } else {
                    debug!("GTK layer shell not available, falling back to regular window");
                }
            }

            debug!("Recording overlay window created successfully (hidden)");
        }
        Err(e) => {
            debug!("Failed to create recording overlay window: {}", e);
        }
    }
}

/// Creates the recording overlay panel and keeps it hidden by default (macOS)
#[cfg(target_os = "macos")]
pub fn create_recording_overlay(app_handle: &AppHandle) {
    if let Some((x, y)) = calculate_overlay_position(app_handle, OVERLAY_WIDTH, OVERLAY_HEIGHT) {
        // PanelBuilder creates a Tauri window then converts it to NSPanel.
        // The window remains registered, so get_webview_window() still works.
        match PanelBuilder::<_, RecordingOverlayPanel>::new(app_handle, "recording_overlay")
            .url(WebviewUrl::App("src/overlay/index.html".into()))
            .title("Recording")
            .position(tauri::Position::Logical(tauri::LogicalPosition { x, y }))
            .level(PanelLevel::Status)
            .size(tauri::Size::Logical(tauri::LogicalSize {
                width: OVERLAY_WIDTH,
                height: OVERLAY_HEIGHT,
            }))
            .has_shadow(false)
            .transparent(true)
            .no_activate(true)
            .corner_radius(0.0)
            .style_mask(StyleMask::empty().borderless().nonactivating_panel())
            .with_window(|w| w.decorations(false).transparent(true).focusable(false))
            .collection_behavior(
                CollectionBehavior::new()
                    .can_join_all_spaces()
                    .full_screen_auxiliary(),
            )
            .build()
        {
            Ok(panel) => {
                panel.hide();
            }
            Err(e) => {
                log::error!("Failed to create recording overlay panel: {}", e);
            }
        }
    }
}

fn show_overlay_state(app_handle: &AppHandle, state: &str) {
    // Whether the overlay shows at all is governed by overlay_style; position
    // only chooses Top vs Bottom placement.
    let settings = settings::get_settings(app_handle);
    if settings.overlay_style == OverlayStyle::None {
        return;
    }

    // Size the overlay for this style + state (compact vs. streaming vs.
    // esfera), then position it.
    let (width, height) = overlay_dimensions(settings.overlay_style, state);
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        update_gtk_layer_shell_anchors(&overlay_window);

        let size_started = std::time::Instant::now();
        let _ = overlay_window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        let size_elapsed = size_started.elapsed();

        let pos_started = std::time::Instant::now();
        let mut set_pos_elapsed = std::time::Duration::ZERO;
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let set_pos_started = std::time::Instant::now();
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
            set_pos_elapsed = set_pos_started.elapsed();
        }
        let pos_calc_elapsed = pos_started.elapsed() - set_pos_elapsed;

        let show_started = std::time::Instant::now();
        let _ = overlay_window.show();
        let show_elapsed = show_started.elapsed();

        // On Windows, aggressively re-assert "topmost" in the native Z-order after showing
        #[cfg(target_os = "windows")]
        force_overlay_topmost(&overlay_window);

        // Cada show reclama el overlay: invalida cualquier hide diferido en
        // vuelo (linger del modo palabras) para que jamás oculte una sesión
        // nueva. Ver `hide_recording_overlay_after`.
        OVERLAY_GENERATION.fetch_add(1, Ordering::Relaxed);
        let _ = overlay_window.emit("show-overlay", state);
        log::debug!(
            "overlay '{}': set_size={:?} pos_calc={:?} set_pos={:?} show={:?}",
            state,
            size_elapsed,
            pos_calc_elapsed,
            set_pos_elapsed,
            show_elapsed
        );
    }
}

/// Shows the recording overlay window with fade-in animation
pub fn show_recording_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "recording");
}

/// Muestra el overlay de LECTURA: un parlante que retumba mientras Escucha lee.
///
/// Se oculta con `hide_recording_overlay`, igual que los demás estados — es la
/// misma ventana, no una nueva. Respeta `overlay_style == None`: si el usuario
/// apagó los overlays, este tampoco aparece.
pub fn show_leyendo_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "leyendo");
}

/// Shows the larger streaming overlay that displays live transcription text
pub fn show_streaming_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "streaming");
}

/// Shows the transcribing overlay window
pub fn show_transcribing_overlay(app_handle: &AppHandle) {
    show_overlay_state(app_handle, "transcribing");
}

// `show_processing_overlay` estaba aquí y se retiró el 29/07: el estado
// «processing» del overlay solo lo disparaba el paso del LLM del «Post Proceso».
// El frontend aún lo tiene en su unión de tipos, inofensivo, pero ya no puede
// llegar: si algún día vuelve un paso de trabajo largo, esta es su forma.

/// Updates the overlay window position based on current settings
pub fn update_overlay_position(app_handle: &AppHandle) {
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        #[cfg(target_os = "linux")]
        {
            update_gtk_layer_shell_anchors(&overlay_window);
        }

        // Use the window's current size so centering stays correct whether the
        // overlay is in compact or streaming layout.
        let (width, height) = current_overlay_logical_size(&overlay_window)
            .unwrap_or((OVERLAY_WIDTH, OVERLAY_HEIGHT));
        if let Some((x, y)) = calculate_overlay_position(app_handle, width, height) {
            let _ = overlay_window
                .set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
        }
    }
}

/// Generación del overlay: se incrementa en cada show. Un hide diferido captura
/// la generación al programarse y solo oculta si nadie re-mostró el overlay en
/// el intermedio.
static OVERLAY_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Oculta el overlay tras `delay`, salvo que un nuevo show lo haya reclamado en
/// el intermedio. Con delay cero es idéntico a `hide_recording_overlay`.
///
/// Para el modo Esfera «palabras»: el batch del finalize llega ~250 ms antes
/// del hide y moriría sin verse; este linger deja que las últimas palabras
/// completen vuelo+lectura+disolución. El texto ya se pegó (el paste no se
/// retrasa), la ventana jamás toma foco, y R8 se mantiene: la suscripción de
/// espectro vive mientras el overlay es visible y, sin frames nuevos, la esfera
/// decae a su respiración en calma.
pub fn hide_recording_overlay_after(app_handle: &AppHandle, delay: std::time::Duration) {
    if delay.is_zero() {
        hide_recording_overlay(app_handle);
        return;
    }
    let generation = OVERLAY_GENERATION.load(Ordering::Relaxed);
    let app = app_handle.clone();
    std::thread::spawn(move || {
        std::thread::sleep(delay);
        if OVERLAY_GENERATION.load(Ordering::Relaxed) == generation {
            hide_recording_overlay(&app);
        } else {
            log::debug!("linger de palabras cancelado: el overlay fue re-mostrado");
        }
    });
}

/// Hides the recording overlay window with fade-out animation
pub fn hide_recording_overlay(app_handle: &AppHandle) {
    // Always hide the overlay regardless of settings - if setting was changed while recording,
    // we still want to hide it properly
    if let Some(overlay_window) = app_handle.get_webview_window("recording_overlay") {
        // Emit event to trigger fade-out animation
        let _ = overlay_window.emit("hide-overlay", ());
        // Hide the window after a short delay to allow animation to complete
        let window_clone = overlay_window.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            let _ = window_clone.hide();
            // Failsafe for R8: even if the overlay webview never processed
            // `hide-overlay` (frozen/crashed webview), a hidden overlay must
            // not keep the spectrum pipeline alive. Idempotent with the
            // overlay's own stop_spectrum call.
            spectrum_unsubscribe("recording_overlay");
        });
    }
}

// ───────────────────────── Spectrum subscription (R8) ─────────────────────────
//
// Emission model: windows subscribe via the `start_spectrum`/`stop_spectrum`
// commands (keyed by window label, so a double-subscribe from React StrictMode
// stays idempotent). The shared gate feeds the recorder's consumer loop, which
// skips FFT analysis entirely while nobody is subscribed — an always-on
// microphone therefore never produces spectrum work or events 24/7. This
// replaces the old overlay_style cache (`OVERLAY_ENABLED`): "overlay hidden →
// unsubscribed" is a strictly stronger guarantee, and also covers the hidden-
// webview WebKit accumulation from issue #1279.

static SPECTRUM_SUBSCRIBERS: LazyLock<Mutex<HashSet<String>>> =
    LazyLock::new(|| Mutex::new(HashSet::new()));
static SPECTRUM_WANTED: LazyLock<Arc<AtomicBool>> =
    LazyLock::new(|| Arc::new(AtomicBool::new(false)));
static SPECTRUM_FRAMES_EMITTED: AtomicU64 = AtomicU64::new(0);

/// The shared gate handed to the audio recorder: `true` while at least one
/// window is subscribed to spectrum frames.
pub fn spectrum_gate() -> Arc<AtomicBool> {
    Arc::clone(&SPECTRUM_WANTED)
}

pub fn spectrum_subscribe(label: &str) {
    let mut subs = SPECTRUM_SUBSCRIBERS.lock().unwrap();
    subs.insert(label.to_string());
    SPECTRUM_WANTED.store(true, Ordering::Relaxed);
    log::debug!("spectrum: '{}' subscribed ({} total)", label, subs.len());
}

pub fn spectrum_unsubscribe(label: &str) {
    let mut subs = SPECTRUM_SUBSCRIBERS.lock().unwrap();
    let removed = subs.remove(label);
    if subs.is_empty() {
        SPECTRUM_WANTED.store(false, Ordering::Relaxed);
        // Session counter makes "cero emisión al detener" verifiable in the log.
        let frames = SPECTRUM_FRAMES_EMITTED.swap(0, Ordering::Relaxed);
        if removed {
            log::debug!(
                "spectrum: '{}' unsubscribed — emission stopped ({} frames this session)",
                label,
                frames
            );
        }
    } else if removed {
        log::debug!(
            "spectrum: '{}' unsubscribed ({} remaining)",
            label,
            subs.len()
        );
    }
}

pub fn emit_spectrum(app_handle: &AppHandle, frame: &SpectrumFrame) {
    // Belt over the recorder-side gate: no subscribers, no emission.
    if !SPECTRUM_WANTED.load(Ordering::Relaxed) {
        return;
    }

    // Throttle to ~30 FPS. The raw audio callback fires far faster than the
    // UI needs; capping emission rate cuts the per-frame `eval_script`/IPC
    // volume that drives the wry memory growth in issue #1279
    // (upstream tauri-apps/wry#1489).
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    let last = LAST_SPECTRUM_EMIT.load(Ordering::Relaxed);
    if now.saturating_sub(last) < EMIT_THROTTLE_MS {
        return;
    }
    LAST_SPECTRUM_EMIT.store(now, Ordering::Relaxed);

    // `emit_to` each subscriber label (instead of a broadcast `emit`) keeps
    // this a single eval_script per subscribed webview — hidden webviews with
    // no subscription never see the event (issue #1279).
    let subs = SPECTRUM_SUBSCRIBERS.lock().unwrap();
    for label in subs.iter() {
        let _ = app_handle.emit_to(label.as_str(), "spectrum", frame);
    }
    if !subs.is_empty() {
        SPECTRUM_FRAMES_EMITTED.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod spectrum_tests {
    use super::*;

    /// Subscribe/unsubscribe must be idempotent per label (React StrictMode
    /// double-mounts) and the gate must only close when the last one leaves.
    #[test]
    fn subscription_gate_follows_subscribers() {
        // Serialize against other tests touching the same statics.
        spectrum_unsubscribe("test_a");
        spectrum_unsubscribe("test_b");

        spectrum_subscribe("test_a");
        spectrum_subscribe("test_a"); // idempotent
        spectrum_subscribe("test_b");
        assert!(SPECTRUM_WANTED.load(Ordering::Relaxed));

        spectrum_unsubscribe("test_a");
        assert!(
            SPECTRUM_WANTED.load(Ordering::Relaxed),
            "gate must stay open while a subscriber remains"
        );

        spectrum_unsubscribe("test_b");
        assert!(
            !SPECTRUM_WANTED.load(Ordering::Relaxed),
            "gate must close when the last subscriber leaves"
        );
    }
}
