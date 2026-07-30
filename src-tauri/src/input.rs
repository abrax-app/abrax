use enigo::{Enigo, Key, Keyboard, Mouse, Settings};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

/// Get the current mouse cursor position using the managed Enigo instance.
/// Returns None if the state is not available or if getting the location fails.
pub fn get_cursor_position(app_handle: &AppHandle) -> Option<(i32, i32)> {
    let enigo_state = app_handle.try_state::<EnigoState>()?;
    let enigo = enigo_state.0.lock().ok()?;
    enigo.location().ok()
}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9));
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press modifier + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Envía Copiar (Ctrl+C / ⌘C) a la ventana en foco, para capturar lo que el
/// usuario tiene seleccionado en CUALQUIER aplicación.
///
/// Es la única vía razonable: leer la selección de otra app sin copiar exige las
/// APIs de accesibilidad (UI Automation en Windows, AX en macOS), que piden
/// permisos aparte y no funcionan en todas las aplicaciones. Copiar funciona
/// donde funciona Ctrl+C, que es prácticamente todo.
///
/// **Quien llama es responsable de restaurar el portapapeles** — ver
/// [`crate::clipboard::leer_seleccion`], que lo hace. Misma regla que el pegado
/// del dictado (fix R5): jamás destruir lo que el usuario tenía copiado.
pub fn send_copy_ctrl_c(enigo: &mut Enigo) -> Result<(), String> {
    // PASO 1, Y ES EL QUE FALTABA. Esperar a que el usuario SUELTE los
    // modificadores del atajo antes de sintetizar nada.
    //
    // Sin esto, la secuencia es un desastre y se comprobó en vivo el 30/07: el
    // atajo es Ctrl+Shift+R, así que al correr esta función el usuario TODAVÍA
    // tiene Ctrl, Shift y R pulsados físicamente. Lo que se envía entonces no es
    // Ctrl+C sino Ctrl+Shift+C —que no copia— y al soltar nosotros un Ctrl que él
    // mantiene apretado, el estado del teclado del sistema queda desincronizado:
    // la tecla se queda «pegada» repitiendo caracteres y no hay forma de
    // recuperarse sin cerrar la app. Le pasó con la «c»: «cccccccc».
    //
    // El pegado del dictado nunca sufrió esto porque corre DESPUÉS de transcribir,
    // cuando el usuario ya soltó hace rato. Esta función corre al instante.
    if !esperar_modificadores_libres(std::time::Duration::from_millis(700)) {
        // Se ABORTA en vez de mandar el combo contaminado. Mejor no leer que
        // dejar el teclado inservible.
        return Err(
            "seguías con un modificador pulsado; suelta el atajo y vuelve a intentarlo".into(),
        );
    }

    #[cfg(target_os = "macos")]
    let (modifier_key, c_key_code) = (Key::Meta, Key::Other(8));
    #[cfg(target_os = "windows")]
    let (modifier_key, c_key_code) = (Key::Control, Key::Other(0x43)); // VK_C
    #[cfg(target_os = "linux")]
    let (modifier_key, c_key_code) = (Key::Control, Key::Unicode('c'));

    // PASO 2: la secuencia, con liberación GARANTIZADA.
    //
    // La versión anterior usaba `?` tras pulsar el modificador: si el clic de la C
    // fallaba, se salía de la función con Ctrl PULSADO. Aquí no hay ningún `?`
    // entre el Press y el Release — los errores se guardan y se sueltan las dos
    // teclas pase lo que pase, incluida la C por si el `Click` quedó a medias.
    let press = enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e));

    let click = if press.is_ok() {
        enigo
            .key(c_key_code, enigo::Direction::Click)
            .map_err(|e| format!("Failed to click C key: {}", e))
    } else {
        Ok(())
    };

    // Mismo margen que el pegado: la app en foco necesita un instante para
    // atender la combinación y dejar la selección en el portapapeles.
    std::thread::sleep(std::time::Duration::from_millis(100));

    // Liberar SIEMPRE, y en orden inverso. Un `Release` de una tecla que ya está
    // suelta es inofensivo; dejarla pulsada, no.
    let _ = enigo.key(c_key_code, enigo::Direction::Release);
    let release = enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e));

    press.and(click).and(release)
}

/// Espera hasta `tope` a que NO quede ningún modificador pulsado físicamente.
/// Devuelve `false` si se agotó el plazo (el usuario lo sigue apretando).
///
/// En Windows se consulta el estado real con `GetAsyncKeyState`. En el resto de
/// plataformas no hay una vía equivalente sin más dependencias, así que se espera
/// un margen fijo y se sigue: el objetivo es dar tiempo a que la pulsación
/// termine, y la liberación garantizada de arriba cubre el resto.
fn esperar_modificadores_libres(tope: std::time::Duration) -> bool {
    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetAsyncKeyState, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
        };

        // El bit alto de `GetAsyncKeyState` indica «pulsada AHORA». El bajo dice
        // «se pulsó desde la última consulta», que aquí no interesa.
        let pulsada = |vk: windows::Win32::UI::Input::KeyboardAndMouse::VIRTUAL_KEY| -> bool {
            (unsafe { GetAsyncKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
        };
        let alguno = || {
            pulsada(VK_CONTROL)
                || pulsada(VK_SHIFT)
                || pulsada(VK_MENU)
                || pulsada(VK_LWIN)
                || pulsada(VK_RWIN)
        };

        let inicio = std::time::Instant::now();
        while alguno() {
            if inicio.elapsed() >= tope {
                return false;
            }
            std::thread::sleep(std::time::Duration::from_millis(15));
        }
        // Un respiro extra: el `keyup` físico acaba de ocurrir y la app en foco
        // puede seguir procesándolo.
        std::thread::sleep(std::time::Duration::from_millis(40));
        true
    }

    #[cfg(not(target_os = "windows"))]
    {
        std::thread::sleep(tope.min(std::time::Duration::from_millis(250)));
        true
    }
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9)); // Cmd+Shift+V on macOS
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press Ctrl/Cmd + Shift + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;
    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_shift_insert(enigo: &mut Enigo) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let insert_key_code = Key::Other(0x2D); // VK_INSERT
    #[cfg(not(target_os = "windows"))]
    let insert_key_code = Key::Other(0x76); // XK_Insert (keycode 118 / 0x76, also used as fallback)

    // Press Shift + Insert
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(insert_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click Insert key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;

    Ok(())
}

/// Pastes text directly using the enigo text method.
/// This tries to use system input methods if possible, otherwise simulates keystrokes one by one.
pub fn paste_text_direct(enigo: &mut Enigo, text: &str) -> Result<(), String> {
    enigo
        .text(text)
        .map_err(|e| format!("Failed to send text directly: {}", e))?;

    Ok(())
}
