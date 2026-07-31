use log::{debug, warn};
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;
use std::collections::HashMap;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

// Custom deserializer to handle both old numeric format (1-5) and new string format ("trace", "debug", etc.)
impl<'de> Deserialize<'de> for LogLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct LogLevelVisitor;

        impl<'de> Visitor<'de> for LogLevelVisitor {
            type Value = LogLevel;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("a string or integer representing log level")
            }

            fn visit_str<E: de::Error>(self, value: &str) -> Result<LogLevel, E> {
                match value.to_lowercase().as_str() {
                    "trace" => Ok(LogLevel::Trace),
                    "debug" => Ok(LogLevel::Debug),
                    "info" => Ok(LogLevel::Info),
                    "warn" => Ok(LogLevel::Warn),
                    "error" => Ok(LogLevel::Error),
                    _ => Err(E::unknown_variant(
                        value,
                        &["trace", "debug", "info", "warn", "error"],
                    )),
                }
            }

            fn visit_u64<E: de::Error>(self, value: u64) -> Result<LogLevel, E> {
                match value {
                    1 => Ok(LogLevel::Trace),
                    2 => Ok(LogLevel::Debug),
                    3 => Ok(LogLevel::Info),
                    4 => Ok(LogLevel::Warn),
                    5 => Ok(LogLevel::Error),
                    _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &"1-5")),
                }
            }
        }

        deserializer.deserialize_any(LogLevelVisitor)
    }
}

impl From<LogLevel> for tauri_plugin_log::LogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Trace => tauri_plugin_log::LogLevel::Trace,
            LogLevel::Debug => tauri_plugin_log::LogLevel::Debug,
            LogLevel::Info => tauri_plugin_log::LogLevel::Info,
            LogLevel::Warn => tauri_plugin_log::LogLevel::Warn,
            LogLevel::Error => tauri_plugin_log::LogLevel::Error,
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct ShortcutBinding {
    pub id: String,
    pub name: String,
    pub description: String,
    pub default_binding: String,
    pub current_binding: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayPosition {
    Top,
    // `none` is retired: overlay visibility is owned by `OverlayStyle` now. The
    // alias keeps legacy stores (`"overlay_position": "none"`) deserializing
    // instead of failing the whole load; the one-time overlay migration reads the
    // raw stored string to recover the old "hidden" intent as `OverlayStyle::None`.
    #[serde(alias = "none")]
    Bottom,
}

/// Un reemplazo exacto del Diccionario Vivo: token transcrito → texto final.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Type)]
pub struct CustomReplacement {
    pub from: String,
    pub to: String,
}

/// Proyecto activo del Diccionario Vivo (un solo proyecto en el MVP).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct DictionaryProject {
    pub path: String,
    pub enabled: bool,
    /// Última indexación, epoch en milisegundos.
    pub last_indexed_ms: Option<f64>,
}

/// Which recording overlay to display. `Minimal` and `Live` share one base
/// (the pill); `Live` grows into the panel that shows live transcription text.
/// `Esfera` renders the audio-reactive sphere on a square stage. `None` hides
/// the overlay entirely. Decoupled from whether the model runs in streaming
/// mode (that is driven purely by model capability).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum OverlayStyle {
    None,
    Minimal,
    Live,
    Esfera,
}

/// Behaviour of the `Esfera` overlay. `Audio` is the original audio-reactive
/// sphere (unchanged). `Palabras` keeps that pulse but also receives the words
/// as they are transcribed: each dictated word flies to the membrane, is read
/// for an instant and dissolves into points that push outward. Only meaningful
/// while `overlay_style` is `Esfera`; the backend gates word emission on it.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "lowercase")]
pub enum EsferaModo {
    Audio,
    Palabras,
}

/// Cuánto transforma el módulo de corrección local (`correccion`) el dictado
/// antes de insertarlo. `Literal` solo ortotipografía (espacios, mayúsculas);
/// `Limpio` añade autocorrecciones habladas («el martes, perdón, el miércoles»),
/// tildes seguras y verbalización (numerales, identificadores, símbolos).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum CorreccionModo {
    Literal,
    /// El alias `pulido` NO es decorativo: existió un tercer modo que reservaba
    /// la reestructuración a un LLM local, retirado el 29/07. Quien lo tuviera
    /// guardado trae `"pulido"` en su `settings_store.json`, y sin este alias el
    /// fichero ENTERO dejaría de parsear — perderían todos sus ajustes, no solo
    /// este campo. `#[serde(default)]` no salva de esto: cubre claves ausentes,
    /// no valores inválidos. Al siguiente guardado se reescribe como `limpio`.
    #[serde(alias = "pulido")]
    Limpio,
}

/// Qué motor ejecuta la corrección. `Desactivado` (default) = passthrough
/// exacto, el pipeline queda como si el módulo no existiera. `SoloReglas` aplica
/// la capa determinista.
///
/// Tuvo `Auto` y `Modelo`, que pedían el LLM local del «Pulido con IA»
/// (retirado el 29/07). Ambos entran ahora por alias en `SoloReglas`, que es
/// exactamente lo que hacían en la práctica siempre que no hubiera un modelo
/// disponible. Ver el comentario de [`CorreccionModo::Limpio`] para por qué los
/// alias son obligatorios y no un detalle.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum CorreccionMotor {
    #[serde(alias = "auto", alias = "modelo")]
    SoloReglas,
    Desactivado,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ModelUnloadTimeout {
    Never,
    Immediately,
    Min2,
    #[default]
    Min5,
    Min10,
    Min15,
    Hour1,
    Sec15, // Debug mode only
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum PasteMethod {
    CtrlV,
    Direct,
    None,
    ShiftInsert,
    CtrlShiftV,
    ExternalScript,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardHandling {
    #[default]
    DontModify,
    CopyToClipboard,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutoSubmitKey {
    #[default]
    Enter,
    CtrlEnter,
    CmdEnter,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum RecordingRetentionPeriod {
    Never,
    PreserveLimit,
    Days3,
    Weeks2,
    Months3,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum KeyboardImplementation {
    Tauri,
    HandyKeys,
}

impl Default for KeyboardImplementation {
    fn default() -> Self {
        #[cfg(target_os = "linux")]
        return KeyboardImplementation::Tauri;
        #[cfg(not(target_os = "linux"))]
        return KeyboardImplementation::HandyKeys;
    }
}

impl Default for PasteMethod {
    fn default() -> Self {
        // Default to CtrlV for macOS and Windows, Direct for Linux
        #[cfg(target_os = "linux")]
        return PasteMethod::Direct;
        #[cfg(not(target_os = "linux"))]
        return PasteMethod::CtrlV;
    }
}

impl ModelUnloadTimeout {
    pub fn to_minutes(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Min2 => Some(2),
            ModelUnloadTimeout::Min5 => Some(5),
            ModelUnloadTimeout::Min10 => Some(10),
            ModelUnloadTimeout::Min15 => Some(15),
            ModelUnloadTimeout::Hour1 => Some(60),
            ModelUnloadTimeout::Sec15 => Some(0), // Special case for debug - handled separately
        }
    }

    pub fn to_seconds(self) -> Option<u64> {
        match self {
            ModelUnloadTimeout::Never => None,
            ModelUnloadTimeout::Immediately => Some(0), // Special case for immediate unloading
            ModelUnloadTimeout::Sec15 => Some(15),
            _ => self.to_minutes().map(|m| m * 60),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum SoundTheme {
    Abrax,
    Marimba,
    Pop,
    Custom,
}

impl SoundTheme {
    fn as_str(&self) -> &'static str {
        match self {
            SoundTheme::Abrax => "abrax",
            SoundTheme::Marimba => "marimba",
            SoundTheme::Pop => "pop",
            SoundTheme::Custom => "custom",
        }
    }

    pub fn to_start_path(self) -> String {
        format!("resources/{}_start.wav", self.as_str())
    }

    pub fn to_stop_path(self) -> String {
        format!("resources/{}_stop.wav", self.as_str())
    }
}

/// UI appearance mode. `System` follows the OS `prefers-color-scheme`; `Light`
/// and `Dark` force one of the two palettes Abrax already ships.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    System,
    Light,
    Dark,
}

/// Color palette for the whole UI, orthogonal to [`Theme`] (light/dark).
/// `Abrax` is the brand palette (cyan/violet/magenta); `Imperial` is a
/// gold/amber/red palette and `Escuderia` a racing red/black/white palette,
/// both dark by design, so they force dark mode while active (the stored
/// [`Theme`] is preserved and applies again on switching back).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum UiTheme {
    Abrax,
    Imperial,
    Escuderia,
}

/// Shape of the main window, orthogonal to [`UiTheme`] (palette) and [`Theme`]
/// (light/dark). `Classic` is the default decorated settings window and the
/// permanent fallback. `Retro` is a frameless/transparent shell: the app
/// becomes a stack of retro-player windows. It consumes the palette tokens, so a
/// shell never hardcodes color. The frameless/transparent chrome is decided at
/// window build time, so switching shells takes full effect on the next launch.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum UiShell {
    Classic,
    Retro,
    Quiet,
    Bancada,
}

impl UiShell {
    /// Whether this shell wants a frameless, transparent window. `Classic`
    /// keeps the native decorated chrome; `Retro` and `Quiet` paint their own.
    pub fn wants_transparency(self) -> bool {
        matches!(self, UiShell::Retro | UiShell::Quiet | UiShell::Bancada)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TypingTool {
    #[default]
    Auto,
    Wtype,
    Kwtype,
    Dotool,
    Ydotool,
    Xdotool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranscribeAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Gpu,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type, Default)]
#[serde(rename_all = "snake_case")]
pub enum OrtAcceleratorSetting {
    #[default]
    Auto,
    Cpu,
    Cuda,
    #[serde(rename = "directml")]
    DirectMl,
    Rocm,
}

/* still handy for composing the initial JSON in the store ------------- */
/// The container-level `serde(default)` (backed by the `Default` impl below)
/// guarantees every field — including ones added in the future — falls back to
/// its `get_default_settings()` value when missing from a stored settings
/// object, so a partial store can never fail the whole load (#1619).
/// Field-level defaults below take precedence where present.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
#[serde(default)]
pub struct AppSettings {
    /// Internal settings schema marker for one-time migrations. Fresh installs
    /// start at the current version; existing stores missing this key are
    /// treated as version 0 and migrated forward.
    #[serde(default = "default_settings_schema_version")]
    pub settings_schema_version: u32,
    /// Defaults to empty on partial stores; the load path merges in the
    /// default bindings for any missing keys before the settings are used.
    #[serde(default)]
    pub bindings: HashMap<String, ShortcutBinding>,
    #[serde(default = "default_push_to_talk")]
    pub push_to_talk: bool,
    #[serde(default)]
    pub audio_feedback: bool,
    #[serde(default = "default_audio_feedback_volume")]
    pub audio_feedback_volume: f32,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: SoundTheme,
    /// Carpeta donde se descargan los modelos. `None` = la carpeta de datos de
    /// la app (comportamiento histórico). Se expone para que el usuario elija
    /// **en qué disco** viven los modelos, que pesan varios GB.
    ///
    /// Se guarda como ruta absoluta. Si al arrancar apunta a algo que ya no
    /// existe (disco externo desconectado, carpeta borrada), quien la resuelve
    /// degrada a la carpeta por defecto en vez de fallar.
    #[serde(default)]
    pub models_dir: Option<String>,
    #[serde(default = "default_start_hidden")]
    pub start_hidden: bool,
    #[serde(default = "default_autostart_enabled")]
    pub autostart_enabled: bool,
    #[serde(default = "default_update_checks_enabled")]
    pub update_checks_enabled: bool,
    #[serde(default = "default_show_whats_new_on_update")]
    pub show_whats_new_on_update: bool,
    /// The app version whose What's New the user has already seen. Fresh installs
    /// default to the current version (nothing is "new" to them). Existing users
    /// upgrading from before this key existed are blanked by the migration so they
    /// see the current release's notes — see `apply_settings_migrations`.
    #[serde(default = "default_whats_new_last_seen_version")]
    pub whats_new_last_seen_version: String,
    #[serde(default = "default_model")]
    pub selected_model: String,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default = "default_always_on_microphone")]
    pub always_on_microphone: bool,
    #[serde(default)]
    pub selected_microphone: Option<String>,
    #[serde(default)]
    pub clamshell_microphone: Option<String>,
    #[serde(default)]
    pub selected_output_device: Option<String>,
    #[serde(default = "default_translate_to_english")]
    pub translate_to_english: bool,
    #[serde(default = "default_selected_language")]
    pub selected_language: String,
    #[serde(default = "default_overlay_position")]
    pub overlay_position: OverlayPosition,
    #[serde(default = "default_debug_mode")]
    pub debug_mode: bool,
    #[serde(default = "default_log_level")]
    pub log_level: LogLevel,
    #[serde(default = "default_custom_words")]
    pub custom_words: Vec<String>,
    /// Reemplazos exactos por token (F5.1): "Ruth"→"rut" solo dispara con el
    /// token exacto "Ruth" (case-sensitive) — colisión con "ruta" imposible
    /// por diseño. Es la vía inmune del Diccionario Vivo.
    #[serde(default = "default_custom_replacements")]
    pub custom_replacements: Vec<CustomReplacement>,
    /// Memoria de correcciones: pares `de → a` aprendidos de las ediciones
    /// del usuario en el Historial. Se aplican como reemplazo exacto por
    /// frase en el post-proceso. Todo local.
    #[serde(default = "default_memoria_activa")]
    pub memoria_activa: bool,
    /// Aprender también EN EL SITIO: al empezar un dictado se relee el campo
    /// enfocado (accesibilidad, local) y se aprende de las correcciones que el
    /// usuario hizo ahí sobre el dictado anterior.
    #[serde(default = "default_memoria_activa")]
    pub memoria_en_sitio: bool,
    #[serde(default)]
    pub memoria_correcciones: Vec<crate::memoria::ParMemoria>,
    /// Proyecto activo del Diccionario Vivo (F5.3): ABRAX aprende la jerga
    /// del código indexándolo localmente. El índice vive en el datadir;
    /// nada sale del equipo.
    #[serde(default)]
    pub dictionary_project: Option<DictionaryProject>,
    #[serde(default)]
    pub model_unload_timeout: ModelUnloadTimeout,
    #[serde(default = "default_word_correction_threshold")]
    pub word_correction_threshold: f64,
    #[serde(default = "default_history_limit")]
    pub history_limit: usize,
    #[serde(default = "default_recording_retention_period")]
    pub recording_retention_period: RecordingRetentionPeriod,
    #[serde(default)]
    pub paste_method: PasteMethod,
    #[serde(default)]
    pub clipboard_handling: ClipboardHandling,
    #[serde(default = "default_auto_submit")]
    pub auto_submit: bool,
    #[serde(default)]
    pub auto_submit_key: AutoSubmitKey,
    #[serde(default)]
    pub mute_while_recording: bool,
    #[serde(default)]
    pub append_trailing_space: bool,
    #[serde(default = "default_app_language")]
    pub app_language: String,
    #[serde(default = "default_theme")]
    pub theme: Theme,
    #[serde(default = "default_ui_theme")]
    pub ui_theme: UiTheme,
    #[serde(default = "default_ui_shell")]
    pub ui_shell: UiShell,
    #[serde(default = "default_correccion_modo")]
    pub correccion_modo: CorreccionModo,
    #[serde(default = "default_correccion_motor")]
    pub correccion_motor: CorreccionMotor,
    /// ¿Convertir los numerales hablados a cifras? Vive APARTE del modo
    /// «Limpio» aunque corra dentro de el.
    ///
    /// Es la unica capa de todo el paquete que cambia el ESTILO del texto y no
    /// solo su forma: convierte TODO numeral, no solo los tecnicos, asi que
    /// «el video no puede superar los dos minutos» sale «los 2 minutos». Eso no
    /// es un fallo —hace exactamente lo que promete— pero es una decision de
    /// redaccion que no todo el mundo quiere, y meterla en el mismo interruptor
    /// que las tildes y los simbolos obligaba a tragarsela entera o renunciar a
    /// todo. Con su propia llave, se puede tener lo demas sin esto.
    #[serde(default = "default_correccion_numeros")]
    pub correccion_numeros: bool,
    #[serde(default)]
    pub experimental_enabled: bool,
    #[serde(default)]
    pub lazy_stream_close: bool,
    #[serde(default)]
    pub keyboard_implementation: KeyboardImplementation,
    #[serde(default = "default_show_tray_icon")]
    pub show_tray_icon: bool,
    #[serde(default = "default_paste_delay_ms")]
    pub paste_delay_ms: u64,
    #[serde(default = "default_paste_delay_after_ms")]
    pub paste_delay_after_ms: u64,
    #[serde(default = "default_typing_tool")]
    pub typing_tool: TypingTool,
    #[serde(default)]
    pub external_script_path: Option<String>,
    #[serde(default)]
    pub custom_filler_words: Option<Vec<String>>,
    /// Autocorrección hablada: si quien dicta se corrige a sí mismo en voz alta
    /// («…el martes, no, perdón, el miércoles»), el texto sale ya corregido.
    /// Por REGLAS y 100% local — no usa Post Proceso/BYOK ni ningún modelo.
    /// **Apagada por defecto**: borra texto, y eso se activa a conciencia.
    /// Emoji dictado: «emoji cara feliz» → 🙂. Por tabla, sin ningún modelo.
    ///
    /// **Encendido de fábrica**, al contrario que la autocorrección hablada, y a
    /// propósito: esto no puede dañar texto. Solo actúa detrás de la palabra
    /// «emoji» —que no aparece por casualidad dictando prosa— y si no reconoce
    /// el nombre no toca nada. La autocorrección va apagada porque BORRA; esto
    /// solo añade, y solo cuando se lo piden.
    #[serde(default = "default_emoji_dictado")]
    pub emoji_dictado: bool,
    /// Modelo que estaba seleccionado ANTES de que la app lo cambiara sola al
    /// activar «Audio del sistema», para poder devolverlo al apagarlo.
    ///
    /// `None` = la app no lo tocó. Si el usuario elige otro modelo a mano
    /// mientras el modo está activo, esto se limpia y ya no se restaura nada:
    /// una elección explícita del usuario nunca se pisa.
    #[serde(default)]
    pub modelo_antes_de_sistema: Option<String>,
    #[serde(default = "default_autocorreccion_activa")]
    pub autocorreccion_activa: bool,
    /// Señales de borrado explícito (nivel 1). Mismo contrato que las
    /// muletillas: `null` = las de fábrica, `[]` = nivel apagado, lista propia
    /// = reemplaza a las de fábrica.
    #[serde(default)]
    pub autocorreccion_senales_borrado: Option<Vec<String>>,
    /// Señales de sustitución con paralelo (nivel 2). Mismo contrato que
    /// `autocorreccion_senales_borrado`.
    #[serde(default)]
    pub autocorreccion_senales_sustitucion: Option<Vec<String>>,
    #[serde(default)]
    pub transcribe_accelerator: TranscribeAcceleratorSetting,
    #[serde(default)]
    pub ort_accelerator: OrtAcceleratorSetting,
    #[serde(default = "default_transcribe_gpu_device")]
    pub transcribe_gpu_device: i32,
    #[serde(default)]
    pub extra_recording_buffer_ms: u64,
    #[serde(default = "default_vad_enabled")]
    pub vad_enabled: bool,
    /// Diarización de hablantes (offline): etiqueta la transcripción con
    /// `[Hablante N]` cuando hay varias voces. Opt-in (cuesta CPU y requiere los
    /// modelos ONNX de diarización). Por defecto apagada.
    #[serde(default)]
    pub diarization_enabled: bool,
    /// Pista de número de hablantes para la diarización. `0` = auto (detecta solo,
    /// menos fiable same-mic); `N>=1` = fuerza EXACTAMENTE N hablantes (la vía más
    /// fiable cuando el usuario sabe cuántas voces hay).
    #[serde(default)]
    pub diarization_num_speakers: u32,
    /// Captura el AUDIO DEL SISTEMA (loopback del dispositivo de salida) en vez del
    /// micrófono — para transcribir reuniones online (Teams/Zoom/Meet) donde las
    /// voces salen por los parlantes, no entran por el mic. Off = micrófono normal.
    #[serde(default)]
    pub capture_system_audio: bool,
    /// Which recording overlay to show: None / Minimal / Live. Streaming mode is
    /// not gated on this — that follows model capability. Migrated from the old
    /// `overlay_position` (position `none` → style `None`).
    #[serde(default = "default_overlay_style")]
    pub overlay_style: OverlayStyle,
    /// Behaviour of the `Esfera` overlay: audio-reactive only, or also receiving
    /// the dictated words as they are transcribed (see [`EsferaModo`]).
    #[serde(default = "default_esfera_modo")]
    pub esfera_modo: EsferaModo,
    // [ESCUCHA] --- Lectura en voz alta ---------------------------------------
    /// Id de la voz del sistema para la prosa (None = la app elige la primera es-*).
    #[serde(default)]
    pub escucha_voz_prosa: Option<String>,
    /// Id de la voz del sistema para el código.
    #[serde(default)]
    pub escucha_voz_codigo: Option<String>,
    /// Cuánto símbolo se pronuncia al leer código (Natural calla los cierres).
    #[serde(default)]
    pub escucha_verbosidad_simbolos: crate::managers::escucha::preproceso::VerbosidadSimbolos,
    // [ESCUCHA] -----------------------------------------------------------------
    // [TTS] --- Motor de voz adaptativo -----------------------------------------
    /// Motor TTS elegido; None = decidir por `tts_auto_detect` (recomendación por hardware).
    #[serde(default)]
    pub tts_selected_engine: Option<crate::managers::tts::engine::EngineId>,
    /// Autodetectar el mejor motor LOCAL disponible cuando no hay elección explícita.
    #[serde(default = "default_tts_auto_detect")]
    pub tts_auto_detect: bool,
    /// Id de la voz del motor activo (None = la app elige la primera disponible).
    #[serde(default)]
    pub tts_voice: Option<String>,
    /// Velocidad de lectura como multiplicador (1.0 = normal). 3 niveles en la UI
    /// (Normal 1.0 / Rápida 1.3 / Muy rápida 1.6); aplica a TODOS los motores.
    #[serde(default = "default_tts_velocidad")]
    pub tts_velocidad: f32,
    /// Tono (pitch) en Hz para el motor ONLINE (edge-tts `pitch`). 0 = normal.
    /// Los demás motores lo ignoran (no exponen control de tono). 3 niveles en la
    /// UI (Grave −40 / Normal 0 / Agudo +40).
    #[serde(default)]
    pub tts_tono: i32,
    // [TTS] ---------------------------------------------------------------------
}

fn default_model() -> String {
    "".to_string()
}

const CURRENT_SETTINGS_SCHEMA_VERSION: u32 = 3;

fn default_settings_schema_version() -> u32 {
    CURRENT_SETTINGS_SCHEMA_VERSION
}

fn default_push_to_talk() -> bool {
    true
}

fn default_always_on_microphone() -> bool {
    false
}

fn default_translate_to_english() -> bool {
    false
}

fn default_start_hidden() -> bool {
    false
}

fn default_autostart_enabled() -> bool {
    false
}

fn default_update_checks_enabled() -> bool {
    // Abrax: updater deshabilitado por defecto. La desconexión del upstream
    // (endpoint + par de claves minisign) está pendiente hasta Fase 11; ver
    // docs/RELEASING.md. Ningún build debe buscar releases de cjpais/Handy.
    false
}

fn default_show_whats_new_on_update() -> bool {
    true
}

fn default_whats_new_last_seen_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn default_selected_language() -> String {
    // Producto es-419: el idioma de fábrica es ESPAÑOL, no auto-detección.
    //
    // Con "auto" el fallo era este: los modelos que no saben detectar idioma
    // (Canary declara `lang_detect: false`) no pueden honrar "auto", así que
    // `effective_language` los mandaba al fallback cableado «prefer English»
    // (managers/model.rs) y el dictado salía en inglés. No era un reset del
    // ajuste: era una coerción. Con "es" de fábrica, Canary —que sí habla
    // español— resuelve a español y el fallback ni se toca.
    //
    // Instalaciones existentes NO cambian: el merge de settings solo rellena
    // claves ausentes.
    "es".to_string()
}

fn default_overlay_position() -> OverlayPosition {
    // Position only matters when the overlay is shown; whether it shows at all is
    // `overlay_style` (Linux defaults that to None). So a single default suffices.
    OverlayPosition::Bottom
}

fn default_overlay_style() -> OverlayStyle {
    // `Minimal` de fábrica: la píldora discreta con la onda de voz.
    //
    // HISTORIA DE ESTE DEFAULT, porque ha cambiado dos veces y conviene no
    // deshacerlo por descuido:
    //   · `Live` (heredado del upstream) — se cambió porque quien instalaba,
    //     dictaba una frase y cerraba nunca veía la esfera.
    //   · `Esfera` — la imagen de marca, para que apareciera sola al dictar.
    //   · `Minimal` (ahora) — decisión de producto del 29/07: la píldora con la
    //     onda de voz es lo primero que se ve al dictar.
    //
    // COSTE ASUMIDO A SABIENDAS: la esfera ya NO sale de fábrica, así que hay que
    // entrar a Ajustes → Avanzado para verla. Sigue a un clic, y la landing y el
    // video la muestran igual.
    //
    // Linux se queda sin overlay por defecto (el overlay depende de una ventana
    // transparente que no todos los compositores dan). Position es independiente y
    // solo elige arriba vs. abajo.
    #[cfg(target_os = "linux")]
    return OverlayStyle::None;
    #[cfg(not(target_os = "linux"))]
    return OverlayStyle::Minimal;
}

fn default_esfera_modo() -> EsferaModo {
    // The words mode is opt-in: the sphere keeps its original audio-reactive
    // behaviour until the user chooses it.
    EsferaModo::Audio
}

fn default_vad_enabled() -> bool {
    true
}

fn default_debug_mode() -> bool {
    false
}

fn default_memoria_activa() -> bool {
    true
}

/// Encendida de fábrica desde el 30/07.
///
/// Estuvo apagada por una razón buena: el nivel 1 (borrado explícito) disparaba
/// con sus señales EN MITAD de una frase, y como borra la oración anterior
/// completa, destrozaba prosa corriente —medido, seis de ocho frases—. Ahora el
/// nivel 1 exige que la señal CIERRE el dictado (`borrado_cierra_el_dictado`),
/// que es como se dicta una orden de verdad, y con eso las mismas frases salen
/// intactas mientras las correcciones siguen funcionando.
///
/// El nivel 2 nunca fue el problema: su regla de oro es no tocar nada sin un
/// paralelo claro.
fn default_autocorreccion_activa() -> bool {
    true
}

fn default_emoji_dictado() -> bool {
    true
}

fn default_log_level() -> LogLevel {
    LogLevel::Debug
}

/// La marca se escribe sola de fábrica: el ASR transcribe «Abrax»/«abrax» y
/// el producto se llama ABRAX. Solo variantes de caja del nombre propio —
/// jamás palabras reales del idioma («abraza» es un verbo y queda fuera; si
/// alguien la quiere, la añade a su lista). Reemplazo exacto por token: no
/// puede colisionar con nada más.
fn default_custom_replacements() -> Vec<CustomReplacement> {
    ["Abrax", "abrax", "ábrax", "Ábrax"]
        .into_iter()
        .map(|from| CustomReplacement {
            from: from.to_string(),
            to: "ABRAX".to_string(),
        })
        .collect()
}

fn default_word_correction_threshold() -> f64 {
    0.18
}

fn default_paste_delay_ms() -> u64 {
    60
}

fn default_paste_delay_after_ms() -> u64 {
    60
}

fn default_auto_submit() -> bool {
    false
}

fn default_history_limit() -> usize {
    5
}

fn default_recording_retention_period() -> RecordingRetentionPeriod {
    RecordingRetentionPeriod::PreserveLimit
}

fn default_audio_feedback_volume() -> f32 {
    1.0
}

fn default_sound_theme() -> SoundTheme {
    SoundTheme::Abrax
}

fn default_theme() -> Theme {
    Theme::System
}

fn default_ui_theme() -> UiTheme {
    UiTheme::Abrax
}

fn default_ui_shell() -> UiShell {
    // Quiet de fábrica. La medición en un Windows limpio dio 3/10 y una de las
    // quejas fue «abruma mucho»: tras elegir el modelo, lo primero que veía el
    // usuario era el formulario de ajustes del shell clásico, sin una sola línea
    // que dijera qué hacer. Quiet abre en un inicio con la esfera, el atajo a la
    // vista y las últimas transcripciones.
    //
    // Clásico sigue siendo el respaldo permanente y está a un clic en
    // Acerca de → Forma de la ventana. Las instalaciones existentes no cambian:
    // el merge de settings solo rellena claves ausentes.
    UiShell::Quiet
}

/// El nombre del producto viene sembrado: en español **b y v son el mismo
/// fonema**, así que ningún modelo puede distinguir "Abrax" de "Avrax" por el
/// sonido — es una moneda al aire. Con la palabra en esta lista viaja como
/// contexto al modelo (whisper la recibe como initial prompt) y el corrector
/// difuso remata después. Una app de dictado no debería escribir mal su propio
/// nombre.
fn default_custom_words() -> Vec<String> {
    vec!["Abrax".to_string()]
}

/// `Limpio` de fabrica: es el modo donde viven los simbolos dictados, las
/// tildes, los correos y el tartamudeo. `Literal` solo hace espacios y
/// mayusculas, asi que dejarlo por defecto habria sido destapar la pantalla y
/// que el paquete siguiera sin llegar — el interruptor visible pero la funcion
/// no. Se puede volver a `Literal` desde la misma pantalla.
fn default_correccion_modo() -> CorreccionModo {
    CorreccionModo::Limpio
}

/// Encendido de fabrica desde el 30/07, por decision de producto de Winston.
///
/// Estuvo en `Desactivado` con su pantalla oculta desde el 25/07 («se rediseña
/// por separado»), y ese rediseño no volvio: el resultado fue un paquete entero
/// —simbolos, tildes, correos, tartamudeo, ortotipografia— que nadie podia
/// encender ni sabia que existia.
///
/// `SoloReglas` es determinista: tablas y reglas, sin ningun modelo, sin red y
/// sin latencia. La pantalla sigue estando para volver a `Desactivado`.
fn default_correccion_motor() -> CorreccionMotor {
    CorreccionMotor::SoloReglas
}

/// El conversor de numerales viene encendido con el resto, pero se puede apagar
/// solo. Ver [`AppSettings::correccion_numeros`].
fn default_correccion_numeros() -> bool {
    true
}

fn default_app_language() -> String {
    tauri_plugin_os::locale()
        .map(|l| l.replace('_', "-"))
        .unwrap_or_else(|| "en".to_string())
}

fn default_show_tray_icon() -> bool {
    true
}

fn default_transcribe_gpu_device() -> i32 {
    -1 // auto
}

fn default_typing_tool() -> TypingTool {
    TypingTool::Auto
}

// [TTS]
fn default_tts_auto_detect() -> bool {
    true
}

// [TTS]
fn default_tts_velocidad() -> f32 {
    1.0
}

pub const SETTINGS_STORE_PATH: &str = "settings_store.json";

pub fn get_default_settings() -> AppSettings {
    #[cfg(target_os = "windows")]
    let default_shortcut = "ctrl+space";
    // macOS: SOLO MODIFICADORES, a propósito. Cuando cualquier proceso activa
    // Secure Event Input (un campo de contraseña enfocado, el "Secure Keyboard
    // Entry" de Terminal, un `loginwindow` colgado), los CGEventTaps dejan de
    // recibir KeyDown/KeyUp pero los FlagsChanged siguen llegando. Un atajo CON
    // tecla (el viejo `option+space`) muere ahí en silencio: ni dispara ni avisa.
    // Uno de solo modificadores es inmune por diseño.
    //
    // Se eligió Control+Option, y no otro par, porque: (a) macOS no reserva ⌃⌥
    // para ninguna acción de texto o navegación del día a día — a diferencia de
    // ⌘⌥ (forzar salida, pestañas, inspector) o ⇧⌘ (deshacer, buscar); (b) son
    // dos modificadores, no uno suelto, así que no se pulsa por accidente; y
    // (c) están juntos en el borde izquierdo, cómodos de sostener con una mano
    // mientras se dicta. Ojo al elegir alternativas: handy-keys empareja los
    // modificadores por SUBCONJUNTO, así que un atajo de solo modificadores se
    // dispara con CUALQUIER acorde que los contenga — por eso no sirven pares
    // que se usan como prefijo (⌥⇧+flechas selecciona por palabra, y activaría
    // el dictado en cada selección).
    //
    // Salvedades conocidas, SIN VERIFICAR todavía en un Mac real:
    //  - ⌃⌥ es la "tecla VO" de VoiceOver. Quien lo use tendrá que reasignar el
    //    atajo (VoiceOver viene desactivado de fábrica).
    //  - Por el emparejamiento por subconjunto, ⌃⌥⌘ —frecuente en herramientas
    //    de desarrollo, Xcode entre ellas— TAMBIÉN dispara este atajo. Si en
    //    uso real resultan falsos disparos, la salida es volver a un atajo con
    //    tecla y fusionar el fallback de `secure_input` (rama
    //    `feat/macos-secure-input`), que es lo que lo hace viable.
    // El paso 6 del checklist de QA en Mac decide entre las dos vías.
    //
    // Solo afecta a instalaciones nuevas: las configuraciones existentes
    // conservan su `current_binding` y no se tocan.
    #[cfg(target_os = "macos")]
    let default_shortcut = "ctrl+option";
    #[cfg(target_os = "linux")]
    let default_shortcut = "ctrl+space";
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    let default_shortcut = "alt+space";

    let mut bindings = HashMap::new();
    bindings.insert(
        "transcribe".to_string(),
        ShortcutBinding {
            id: "transcribe".to_string(),
            name: "Transcribe".to_string(),
            description: "Converts your speech into text.".to_string(),
            default_binding: default_shortcut.to_string(),
            current_binding: default_shortcut.to_string(),
        },
    );
    bindings.insert(
        "cancel".to_string(),
        ShortcutBinding {
            id: "cancel".to_string(),
            name: "Cancel".to_string(),
            description: "Cancels the current recording.".to_string(),
            default_binding: "escape".to_string(),
            current_binding: "escape".to_string(),
        },
    );

    // Leer en voz alta la selección de cualquier aplicación.
    //
    // POR QUÉ NO ES `ctrl+l`, que es lo que se pidió: Ctrl+L está de las más
    // ocupadas que hay. En Chrome, Edge y Firefox enfoca la barra de direcciones;
    // en el Explorador de Windows, también; en una terminal limpia la pantalla; en
    // VS Code selecciona la línea. Un atajo GLOBAL lo captura antes que la app en
    // foco, así que registrarlo dejaría a cualquiera sin barra de direcciones en
    // el navegador. `es_atajo_global_seguro` no lo frenaría: solo rechaza teclas
    // sueltas sin modificador, no colisiones con aplicaciones.
    //
    // ELECCIÓN DEL USUARIO (30/07), tras descartar dos candidatos por colisión:
    //   · `ctrl+l`       — barra de direcciones en Chrome/Edge/Firefox y en el
    //                      Explorador; limpiar pantalla en una terminal.
    //   · `ctrl+shift+l` — lo ocupa Loom con un hook global.
    //
    // `ctrl+shift+r` es recarga forzada en los navegadores. Se acepta a sabiendas:
    // es un gesto de desarrollador, no navegación básica como la barra de
    // direcciones. Y es lo que pidió el dueño del producto.
    //
    // NINGÚN default puede garantizarse: depende del software instalado en cada
    // equipo. Por eso lo que de verdad protege no es la elección de la tecla sino
    // que el fallo al registrarla AVISE (ver `AlertKind::AtajoOcupado`), y que se
    // pueda cambiar desde Ajustes → General.
    //
    // macOS usa Control+Option, la convención que ya eligió el dictado.
    #[cfg(target_os = "macos")]
    let leer_shortcut = "ctrl+option+r";
    #[cfg(not(target_os = "macos"))]
    let leer_shortcut = "ctrl+shift+r";
    bindings.insert(
        "leer_seleccion".to_string(),
        ShortcutBinding {
            id: "leer_seleccion".to_string(),
            name: "Leer la selección en voz alta".to_string(),
            description:
                "Lee en voz alta el texto que tengas seleccionado en cualquier aplicación. \
                 Púlsalo otra vez para callar."
                    .to_string(),
            default_binding: leer_shortcut.to_string(),
            current_binding: leer_shortcut.to_string(),
        },
    );

    AppSettings {
        settings_schema_version: default_settings_schema_version(),
        bindings,
        push_to_talk: default_push_to_talk(),
        audio_feedback: false,
        audio_feedback_volume: default_audio_feedback_volume(),
        sound_theme: default_sound_theme(),
        models_dir: None,
        start_hidden: default_start_hidden(),
        autostart_enabled: default_autostart_enabled(),
        update_checks_enabled: default_update_checks_enabled(),
        show_whats_new_on_update: default_show_whats_new_on_update(),
        whats_new_last_seen_version: default_whats_new_last_seen_version(),
        selected_model: "".to_string(),
        onboarding_completed: false,
        always_on_microphone: false,
        selected_microphone: None,
        clamshell_microphone: None,
        selected_output_device: None,
        translate_to_english: false,
        selected_language: default_selected_language(),
        overlay_position: default_overlay_position(),
        debug_mode: false,
        log_level: default_log_level(),
        custom_words: default_custom_words(),
        custom_replacements: default_custom_replacements(),
        memoria_activa: default_memoria_activa(),
        memoria_en_sitio: default_memoria_activa(),
        memoria_correcciones: Vec::new(),
        dictionary_project: None,
        model_unload_timeout: ModelUnloadTimeout::default(),
        word_correction_threshold: default_word_correction_threshold(),
        history_limit: default_history_limit(),
        recording_retention_period: default_recording_retention_period(),
        paste_method: PasteMethod::default(),
        clipboard_handling: ClipboardHandling::default(),
        auto_submit: default_auto_submit(),
        auto_submit_key: AutoSubmitKey::default(),
        mute_while_recording: false,
        append_trailing_space: false,
        app_language: default_app_language(),
        theme: default_theme(),
        ui_theme: default_ui_theme(),
        ui_shell: default_ui_shell(),
        correccion_modo: default_correccion_modo(),
        correccion_motor: default_correccion_motor(),
        correccion_numeros: default_correccion_numeros(),
        experimental_enabled: false,
        lazy_stream_close: false,
        keyboard_implementation: KeyboardImplementation::default(),
        show_tray_icon: default_show_tray_icon(),
        paste_delay_ms: default_paste_delay_ms(),
        paste_delay_after_ms: default_paste_delay_after_ms(),
        typing_tool: default_typing_tool(),
        external_script_path: None,
        custom_filler_words: None,
        emoji_dictado: default_emoji_dictado(),
        modelo_antes_de_sistema: None,
        autocorreccion_activa: default_autocorreccion_activa(),
        autocorreccion_senales_borrado: None,
        autocorreccion_senales_sustitucion: None,
        transcribe_accelerator: TranscribeAcceleratorSetting::default(),
        ort_accelerator: OrtAcceleratorSetting::default(),
        transcribe_gpu_device: default_transcribe_gpu_device(),
        extra_recording_buffer_ms: 0,
        vad_enabled: default_vad_enabled(),
        diarization_enabled: false,
        diarization_num_speakers: 0,
        capture_system_audio: false,
        overlay_style: default_overlay_style(),
        esfera_modo: default_esfera_modo(),
        // [ESCUCHA]
        escucha_voz_prosa: None,
        escucha_voz_codigo: None,
        escucha_verbosidad_simbolos: Default::default(),
        // [TTS]
        tts_selected_engine: None,
        tts_auto_detect: default_tts_auto_detect(),
        tts_voice: None,
        tts_velocidad: default_tts_velocidad(),
        tts_tono: 0,
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        get_default_settings()
    }
}

impl AppSettings {}

/// Startup entry point. Same load-or-create/salvage/migrate behavior as
/// `get_settings`; kept as a named alias for call-site clarity, plus a
/// one-time debug dump of the loaded settings.
pub fn load_or_create_app_settings(app: &AppHandle) -> AppSettings {
    let settings = get_settings(app);
    debug!("Loaded settings: {:?}", settings);
    settings
}

pub fn get_settings(app: &AppHandle) -> AppSettings {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    // Settings reads also persist one-time migrations. Migration helpers are
    // idempotent, so this converges after the first read of an older store.
    let settings = if let Some(settings_value) = store.get("settings") {
        let (mut settings, mut updated) =
            match serde_json::from_value::<AppSettings>(settings_value.clone()) {
                Ok(settings) => (settings, false),
                Err(e) => {
                    warn!("Failed to parse stored settings ({e}); salvaging valid fields");
                    (salvage_settings(&settings_value), true)
                }
            };

        if apply_settings_migrations(&mut settings, &settings_value) {
            updated = true;
        }

        // Merge in any bindings added since this store was written.
        for (key, value) in get_default_settings().bindings {
            if let std::collections::hash_map::Entry::Vacant(entry) = settings.bindings.entry(key) {
                debug!("Adding missing binding: {}", entry.key());
                entry.insert(value);
                updated = true;
            }
        }

        if updated {
            store.set("settings", serde_json::to_value(&settings).unwrap());
        }

        settings
    } else {
        let default_settings = get_default_settings();
        store.set("settings", serde_json::to_value(&default_settings).unwrap());
        default_settings
    };

    settings
}

/// Rebuilds settings from a store value that failed to deserialize as a whole.
/// Every stored field that is individually valid is kept; only broken values
/// (e.g. an enum variant written by a newer or older version) fall back to
/// their default. This means one bad field can never reset the rest of the
/// user's configuration (#1619).
fn salvage_settings(stored: &serde_json::Value) -> AppSettings {
    let Some(stored_map) = stored.as_object() else {
        warn!("Stored settings are not a JSON object; falling back to defaults");
        return get_default_settings();
    };

    let mut merged = serde_json::to_value(get_default_settings())
        .expect("default settings serialize to a JSON object");

    for (key, value) in stored_map {
        let previous = merged
            .as_object_mut()
            .expect("merged settings stay an object")
            .insert(key.clone(), value.clone());
        if serde_json::from_value::<AppSettings>(merged.clone()).is_err() {
            // Log only the key: values may hold secrets (e.g. API keys).
            warn!("Dropping invalid settings field '{key}', keeping its default");
            let map = merged
                .as_object_mut()
                .expect("merged settings stay an object");
            match previous {
                Some(previous) => map.insert(key.clone(), previous),
                None => map.remove(key),
            };
        }
    }

    serde_json::from_value(merged).unwrap_or_else(|e| {
        warn!("Failed to reassemble salvaged settings ({e}); falling back to defaults");
        get_default_settings()
    })
}

fn apply_settings_migrations(
    settings: &mut AppSettings,
    settings_value: &serde_json::Value,
) -> bool {
    let mut updated = false;

    // One-time onboarding migration: users with an explicit selected model have
    // already made it through model selection. Users who merely have compatible
    // files on disk should still see onboarding.
    if settings_value.get("onboarding_completed").is_none() {
        settings.onboarding_completed = !settings.selected_model.is_empty();
        updated = true;
    }

    // One-time What's New migration: migrations only run on an existing store
    // (fresh installs stamp the current version via get_default_settings). A
    // missing key here means a user upgrading from before it existed — blank it
    // so they see the current release's What's New, mirroring the onboarding
    // migration's explicit first-run-vs-upgrade decision.
    if settings_value.get("whats_new_last_seen_version").is_none() {
        settings.whats_new_last_seen_version = String::new();
        updated = true;
    }

    let stored_schema_version = settings_value
        .get("settings_schema_version")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    if stored_schema_version < 1 {
        // `transcribe_gpu_device` used to be a UI ordinal; it is now a
        // transcribe.cpp registry index. A positive legacy value can point at a
        // different GPU after CPU/accelerator/backend devices are included in
        // the registry, so reset ambiguous explicit selections to Auto once.
        if settings.transcribe_gpu_device > 0 {
            settings.transcribe_accelerator = TranscribeAcceleratorSetting::Auto;
            settings.transcribe_gpu_device = default_transcribe_gpu_device();
        }
        updated = true;
    }

    if stored_schema_version < 2 {
        // «Correccion local» pasa a venir encendida (30/07). Cambiar el default
        // NO alcanza a quien ya tiene la clave guardada —serde solo lo aplica
        // cuando FALTA—, y esa es la mayoria: la clave existe desde el 25/07 con
        // valor `Desactivado`. Sin esta migracion, la funcion llegaria solo a
        // instalaciones nuevas y todo el mundo que viniera actualizando seguiria
        // sin verla, que es exactamente el problema que se esta arreglando.
        //
        // Se toca SOLO si sigue en el valor que nadie eligio (`Desactivado` era
        // el default de fabrica, no una preferencia): si alguien lo puso a mano
        // en algo distinto, su eleccion manda y no se pisa.
        if matches!(settings.correccion_motor, CorreccionMotor::Desactivado) {
            settings.correccion_motor = default_correccion_motor();
            settings.correccion_modo = default_correccion_modo();
            log::info!("migracion: correccion local encendida (venia en el default viejo)");
        }
    }

    if stored_schema_version < 3 {
        // La autocorreccion hablada tambien viene encendida (decision del
        // 30/07). Va en su propio paso y no en el anterior porque los stores ya
        // migrados quedaron en 2: meterla alli no habria alcanzado a nadie que
        // ya hubiera abierto la app hoy.
        //
        // Aqui SI se pisa un `false` guardado, y hay razon para hacerlo sin
        // remordimiento: hasta hoy ese interruptor era un PLACEBO —no existia
        // comando Tauri para el, ver `dbd9aa1`—, asi que su valor nunca pudo
        // salir de una eleccion del usuario. Todo `false` almacenado es el
        // default viejo, no una preferencia. Cuando alguien lo apague a partir
        // de ahora quedara guardado de verdad, y ninguna migracion futura debe
        // volver a tocarlo.
        if !settings.autocorreccion_activa {
            settings.autocorreccion_activa = default_autocorreccion_activa();
            log::info!("migracion: autocorreccion hablada encendida (venia del default viejo)");
        }
    }

    if stored_schema_version < CURRENT_SETTINGS_SCHEMA_VERSION as u64 {
        settings.settings_schema_version = CURRENT_SETTINGS_SCHEMA_VERSION;
        updated = true;
    }

    // One-time overlay migration (only while the new key is absent): the retired
    // overlay_position `none` meant "hide the overlay" → OverlayStyle::None; any
    // other position had it visible → Live. The position enum no longer has a
    // `none` variant (legacy "none" deserializes to Bottom via a serde alias), so
    // read the raw stored string to recover the old intent.
    if settings_value.get("overlay_style").is_none() {
        let was_hidden = settings_value
            .get("overlay_position")
            .and_then(|v| v.as_str())
            == Some("none");
        settings.overlay_style = if was_hidden {
            OverlayStyle::None
        } else {
            OverlayStyle::Live
        };
        updated = true;
    }

    // PURGA DE LAS CLAVES API DEL POST-PROCESO (retirado el 29/07).
    //
    // `write_settings` serializa el STRUCT, asi que un campo que ya no existe
    // desaparece del fichero al escribirlo. Pero eso solo pasa cuando algo
    // escribe, y aqui hay CLAVES API del usuario en disco: no se deja al azar de
    // que toque otro ajuste algun dia. Detectar cualquier clave legada y devolver
    // `true` fuerza la reescritura inmediata al arrancar, que las borra.
    //
    // No hay nada que copiar a ningun campo nuevo: la funcion no existe. Esto es
    // solo el disparador del borrado.
    const LEGADAS_POST_PROCESO: &[&str] = &[
        "post_process_enabled",
        "post_process_provider_id",
        "post_process_providers",
        "post_process_api_keys",
        "post_process_models",
        "post_process_prompts",
        "post_process_selected_prompt_id",
    ];
    if LEGADAS_POST_PROCESO
        .iter()
        .any(|k| settings_value.get(k).is_some())
    {
        // Sin log del contenido, evidentemente (regla S3).
        log::info!("settings: purgando ajustes legados del post-proceso retirado");
        updated = true;
    }

    updated
}

pub fn write_settings(app: &AppHandle, settings: AppSettings) {
    let store = app
        .store(crate::portable::store_path(SETTINGS_STORE_PATH))
        .expect("Failed to initialize store");

    store.set("settings", serde_json::to_value(&settings).unwrap());
}

pub fn get_bindings(app: &AppHandle) -> HashMap<String, ShortcutBinding> {
    let settings = get_settings(app);

    settings.bindings
}

pub fn get_stored_binding(app: &AppHandle, id: &str) -> ShortcutBinding {
    let bindings = get_bindings(app);

    let binding = bindings.get(id).unwrap().clone();

    binding
}

pub fn get_history_limit(app: &AppHandle) -> usize {
    let settings = get_settings(app);
    settings.history_limit
}

pub fn get_recording_retention_period(app: &AppHandle) -> RecordingRetentionPeriod {
    let settings = get_settings(app);
    settings.recording_retention_period
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_settings_json() -> serde_json::Value {
        serde_json::to_value(get_default_settings()).unwrap()
    }

    /// En macOS el atajo de dictado por defecto tiene que ser de SOLO
    /// modificadores. Con Secure Event Input activo los CGEventTaps dejan de
    /// entregar KeyDown/KeyUp y un atajo con tecla muere en silencio; los
    /// FlagsChanged siguen llegando, así que los de solo modificadores sobreviven.
    /// Este test existe para que nadie vuelva a poner una tecla en el default
    /// sin darse cuenta de lo que cuesta.
    #[test]
    #[cfg(target_os = "macos")]
    fn macos_default_transcribe_shortcut_is_modifier_only() {
        const MODIFICADORES: &[&str] = &[
            "ctrl", "control", "alt", "option", "opt", "shift", "cmd", "command", "super", "fn",
        ];
        let settings = get_default_settings();
        let atajo = &settings.bindings.get("transcribe").unwrap().default_binding;

        let tokens: Vec<String> = atajo
            .split('+')
            .map(|t| t.trim().to_ascii_lowercase())
            .filter(|t| !t.is_empty())
            .collect();

        assert!(
            tokens.len() >= 2,
            "el default de macOS debe llevar al menos dos modificadores para no \
             dispararse por accidente; es {atajo:?}"
        );
        for t in &tokens {
            assert!(
                MODIFICADORES.contains(&t.as_str()),
                "{atajo:?} lleva la tecla {t:?}: con Secure Input activo el atajo \
                 dejaría de funcionar en silencio"
            );
        }
    }

    /// Every field must survive a partial store: a missing key must never fail
    /// the whole-settings parse (#1619). `json!({})` is the extreme case.
    #[test]
    fn empty_store_parses_with_defaults() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("all AppSettings fields need serde defaults");
        assert!(settings.push_to_talk);
        assert!(!settings.audio_feedback);
        // Bindings default to empty; the load path merges the real defaults in.
        assert!(settings.bindings.is_empty());
    }

    /// Frozen snapshot of a real v0.9.0-era settings store, as written to
    /// disk. This pins backwards compatibility: it must always parse strictly
    /// (no salvage) and require no migration rewrite.
    ///
    /// If a schema change breaks this test, do NOT just update the fixture —
    /// it stands in for the stores on users' machines. Add a
    /// `#[serde(alias)]`/`#[serde(other)]` or a one-time migration in
    /// `apply_settings_migrations` so old values keep loading, and only extend
    /// the fixture alongside that.
    #[test]
    fn frozen_v0_9_store_parses_strictly_without_migration() {
        // Note "log_level": 2 — the legacy numeric format, kept deliberately.
        let stored: serde_json::Value = serde_json::from_str(
            r##"{
            "settings_schema_version": 1,
            "bindings": {
                "transcribe": {
                    "id": "transcribe",
                    "name": "Transcribe",
                    "description": "Converts your speech into text.",
                    "default_binding": "option+space",
                    "current_binding": "f13"
                },
                "transcribe_with_post_process": {
                    "id": "transcribe_with_post_process",
                    "name": "Transcribe with Post-Processing",
                    "description": "Converts your speech into text and applies AI post-processing.",
                    "default_binding": "option+shift+space",
                    "current_binding": "option+shift+space"
                },
                "cancel": {
                    "id": "cancel",
                    "name": "Cancel",
                    "description": "Cancels the current recording.",
                    "default_binding": "escape",
                    "current_binding": "escape"
                }
            },
            "push_to_talk": false,
            "audio_feedback": true,
            "audio_feedback_volume": 0.8,
            "sound_theme": "pop",
            "start_hidden": false,
            "autostart_enabled": true,
            "update_checks_enabled": true,
            "show_whats_new_on_update": true,
            "whats_new_last_seen_version": "0.9.0",
            "selected_model": "whisper-large-v3-turbo",
            "onboarding_completed": true,
            "always_on_microphone": false,
            "selected_microphone": "MacBook Pro Microphone",
            "clamshell_microphone": null,
            "selected_output_device": null,
            "translate_to_english": false,
            "selected_language": "en",
            "overlay_position": "bottom",
            "debug_mode": false,
            "log_level": 2,
            "custom_words": ["Handy", "cjpais"],
            "model_unload_timeout": "min5",
            "word_correction_threshold": 0.18,
            "history_limit": 5,
            "recording_retention_period": "preserve_limit",
            "paste_method": "ctrl_v",
            "clipboard_handling": "dont_modify",
            "auto_submit": false,
            "auto_submit_key": "enter",
            "post_process_enabled": false,
            "post_process_provider_id": "openai",
            "post_process_providers": [
                {
                    "id": "openai",
                    "label": "OpenAI",
                    "base_url": "https://api.openai.com/v1",
                    "allow_base_url_edit": false,
                    "models_endpoint": null,
                    "supports_structured_output": true
                }
            ],
            "post_process_api_keys": { "openai": "" },
            "post_process_models": { "openai": "gpt-4o-mini" },
            "post_process_prompts": [
                { "id": "default", "name": "Default", "prompt": "Clean up the transcript." }
            ],
            "post_process_selected_prompt_id": null,
            "mute_while_recording": false,
            "append_trailing_space": false,
            "app_language": "en",
            "experimental_enabled": false,
            "lazy_stream_close": false,
            "keyboard_implementation": "handy_keys",
            "show_tray_icon": true,
            "paste_delay_ms": 60,
            "typing_tool": "auto",
            "external_script_path": null,
            "custom_filler_words": null,
            "transcribe_accelerator": "gpu",
            "ort_accelerator": "auto",
            "transcribe_gpu_device": 0,
            "extra_recording_buffer_ms": 0,
            "vad_enabled": true,
            "overlay_style": "live"
        }"##,
        )
        .expect("fixture is valid JSON");

        let mut settings: AppSettings = serde_json::from_value(stored.clone())
            .expect("a stored v0.9.0 settings object must keep parsing strictly");

        assert_eq!(settings.selected_model, "whisper-large-v3-turbo");
        assert_eq!(settings.bindings["transcribe"].current_binding, "f13");
        assert_eq!(settings.log_level, LogLevel::Debug);
        assert_eq!(settings.sound_theme, SoundTheme::Pop);

        // El fixture SIGUE INTACTO, como manda el comentario de arriba. Lo que
        // cambia es la expectativa: este store trae ajustes del «Post Proceso»,
        // retirado el 29/07, y entre ellos CLAVES API. La migración de purga los
        // detecta y pide UNA reescritura, que es exactamente el objetivo. Es
        // una vez: tras reescribir, esas claves ya no están en el fichero y la
        // siguiente lectura no vuelve a pedirla (lo fija
        // `la_purga_borra_las_claves_api_del_fichero`).
        assert!(
            apply_settings_migrations(&mut settings, &stored),
            "un store con ajustes legados del post-proceso debe pedir la purga"
        );
    }

    /// La purga tiene que BORRAR las claves API del fichero, no solo pedir una
    /// reescritura. Se comprueba sobre el resultado serializado, que es lo que
    /// acaba en disco — no sobre el struct en memoria, que obviamente ya no
    /// tiene los campos. Y se comprueba que es idempotente: a la segunda lectura
    /// ya no hay nada que purgar.
    #[test]
    fn la_purga_borra_las_claves_api_del_fichero() {
        let guardado = serde_json::json!({
            "selected_model": "whisper-large-v3-turbo",
            "post_process_enabled": true,
            "post_process_provider_id": "openai",
            "post_process_api_keys": { "openai": "sk-proj-secreto-12345" },
            "post_process_models": { "openai": "gpt-4o-mini" },
            "post_process_selected_prompt_id": "algo"
        });

        let mut settings: AppSettings =
            serde_json::from_value(guardado.clone()).expect("debe seguir parseando");
        assert!(
            apply_settings_migrations(&mut settings, &guardado),
            "debe pedir la purga"
        );

        // Lo que se escribiría a disco (`write_settings` serializa el struct).
        let escrito = serde_json::to_value(&settings).expect("serializa");
        let texto = escrito.to_string();
        assert!(
            !texto.contains("sk-proj-secreto-12345"),
            "la clave API sobrevivió a la purga"
        );
        for clave in [
            "post_process_enabled",
            "post_process_api_keys",
            "post_process_models",
            "post_process_provider_id",
            "post_process_selected_prompt_id",
        ] {
            assert!(
                escrito.get(clave).is_none(),
                "«{clave}» sigue en el fichero tras la purga"
            );
        }

        // Idempotente: sobre lo ya purgado no hay nada que hacer.
        let mut otra: AppSettings = serde_json::from_value(escrito.clone()).expect("re-parsea");
        assert!(
            !apply_settings_migrations(&mut otra, &escrito),
            "la purga no debe repetirse en cada lectura"
        );
    }

    #[test]
    fn salvage_preserves_valid_fields_when_one_value_is_invalid() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "selected_model".into(),
            serde_json::json!("parakeet-tdt-0.6b-v3"),
        );
        map.insert("onboarding_completed".into(), serde_json::json!(true));
        // An enum variant this build doesn't know, e.g. written by a newer
        // version before a downgrade.
        map.insert("sound_theme".into(), serde_json::json!("theremin"));
        stored["bindings"]["transcribe"]["current_binding"] = serde_json::json!("f13");

        // Precondition: this is exactly the whole-store parse failure from
        // #1619 that used to reset everything to defaults.
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "parakeet-tdt-0.6b-v3");
        assert!(salvaged.onboarding_completed);
        assert_eq!(salvaged.bindings["transcribe"].current_binding, "f13");
        assert_eq!(salvaged.sound_theme, default_sound_theme());
    }

    /// A store written before the palette existed has no `ui_theme` key and
    /// must load with the Abrax palette; an unknown palette (e.g. written by
    /// a newer build) must salvage to the default instead of resetting.
    #[test]
    fn ui_theme_defaults_and_salvages() {
        let settings: AppSettings =
            serde_json::from_value(serde_json::json!({})).expect("ui_theme needs a serde default");
        assert_eq!(settings.ui_theme, UiTheme::Abrax);

        let mut stored = default_settings_json();
        stored
            .as_object_mut()
            .unwrap()
            .insert("ui_theme".into(), serde_json::json!("cosmic"));
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());
        assert_eq!(salvage_settings(&stored).ui_theme, default_ui_theme());
    }

    /// A store written before shells existed has no `ui_shell` key and must
    /// load as the factory default; an unknown shell (e.g. written by a newer
    /// build) must salvage to that default instead of resetting the whole store.
    #[test]
    fn ui_shell_defaults_and_salvages() {
        let settings: AppSettings =
            serde_json::from_value(serde_json::json!({})).expect("ui_shell needs a serde default");
        assert_eq!(settings.ui_shell, default_ui_shell());

        let mut stored = default_settings_json();
        stored
            .as_object_mut()
            .unwrap()
            .insert("ui_shell".into(), serde_json::json!("holographic"));
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());
        assert_eq!(salvage_settings(&stored).ui_shell, default_ui_shell());
    }

    /// Una instalación nueva trae el nombre del producto sembrado (b/v son el
    /// mismo fonema en español), pero un store existente manda: si el usuario
    /// vació la lista o la editó, su decisión se respeta.
    #[test]
    fn custom_words_siembra_el_nombre_solo_en_instalaciones_nuevas() {
        let nuevas: AppSettings =
            serde_json::from_value(serde_json::json!({})).expect("custom_words needs a default");
        assert_eq!(nuevas.custom_words, vec!["Abrax".to_string()]);
        assert_eq!(
            get_default_settings().custom_words,
            vec!["Abrax".to_string()]
        );

        let vaciada: AppSettings = serde_json::from_value(serde_json::json!({"custom_words": []}))
            .expect("una lista vacía explícita es válida");
        assert!(vaciada.custom_words.is_empty());

        let propia: AppSettings =
            serde_json::from_value(serde_json::json!({"custom_words": ["useAuthStore"]}))
                .expect("una lista propia es válida");
        assert_eq!(propia.custom_words, vec!["useAuthStore".to_string()]);
    }

    /// Un store anterior al módulo de corrección no tiene `correccion_modo` y
    /// debe cargar como Limpio (el default desde el 30/07); un valor desconocido
    /// salva al default en vez de resetear el store completo.
    #[test]
    fn correccion_modo_defaults_and_salvages() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("correccion_modo needs a serde default");
        assert_eq!(settings.correccion_modo, CorreccionModo::Limpio);

        let mut stored = default_settings_json();
        stored
            .as_object_mut()
            .unwrap()
            .insert("correccion_modo".into(), serde_json::json!("telepatico"));
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());
        assert_eq!(
            salvage_settings(&stored).correccion_modo,
            default_correccion_modo()
        );
    }

    /// El motor de corrección nace en `SoloReglas` (decisión de producto del
    /// 30/07: estuvo apagado y oculto cinco días y nadie podía encenderlo). Un
    /// valor desconocido salva al default en vez de resetear el store.
    #[test]
    fn correccion_motor_defaults_and_salvages() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("correccion_motor needs a serde default");
        assert_eq!(settings.correccion_motor, CorreccionMotor::SoloReglas);

        let mut stored = default_settings_json();
        stored
            .as_object_mut()
            .unwrap()
            .insert("correccion_motor".into(), serde_json::json!("cuantico"));
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());
        assert_eq!(
            salvage_settings(&stored).correccion_motor,
            default_correccion_motor()
        );
    }

    /// A store written before the words mode existed has no `esfera_modo` key and
    /// must load as Audio (opt-in); an unknown value must salvage to the default
    /// instead of resetting the whole store.
    #[test]
    fn esfera_modo_defaults_and_salvages() {
        let settings: AppSettings = serde_json::from_value(serde_json::json!({}))
            .expect("esfera_modo needs a serde default");
        assert_eq!(settings.esfera_modo, EsferaModo::Audio);

        let mut stored = default_settings_json();
        stored
            .as_object_mut()
            .unwrap()
            .insert("esfera_modo".into(), serde_json::json!("holograma"));
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());
        assert_eq!(salvage_settings(&stored).esfera_modo, default_esfera_modo());
    }

    #[test]
    fn tts_engine_defaults_and_salvages() {
        // Store vacío → defaults TTS (auto-detección, sin motor/voz fijados).
        let settings: AppSettings =
            serde_json::from_value(serde_json::json!({})).expect("tts fields need serde defaults");
        assert_eq!(settings.tts_selected_engine, None);
        assert!(settings.tts_auto_detect);
        assert_eq!(settings.tts_voice, None);

        // Variante de motor desconocida (build futuro) → salvage la descarta a
        // None sin resetear el resto de settings.
        let mut stored = default_settings_json();
        stored.as_object_mut().unwrap().insert(
            "tts_selected_engine".into(),
            serde_json::json!("hologram_voice"),
        );
        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());
        assert_eq!(salvage_settings(&stored).tts_selected_engine, None);
    }

    #[test]
    fn salvage_drops_only_wrong_typed_fields() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert("paste_delay_ms".into(), serde_json::json!("sixty"));
        map.insert("sound_theme".into(), serde_json::json!(42));
        map.insert("custom_words".into(), serde_json::json!(["handy"]));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.paste_delay_ms, default_paste_delay_ms());
        assert_eq!(salvaged.sound_theme, default_sound_theme());
        assert_eq!(salvaged.custom_words, vec!["handy".to_string()]);
    }

    #[test]
    fn salvage_of_poisoned_bindings_keeps_other_fields() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        // One malformed entry poisons the whole bindings map, but must not
        // take the rest of the settings down with it.
        map.insert(
            "bindings".into(),
            serde_json::json!({ "transcribe": { "id": 42 } }),
        );
        map.insert("selected_model".into(), serde_json::json!("whisper-small"));

        assert!(serde_json::from_value::<AppSettings>(stored.clone()).is_err());

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "whisper-small");
        let defaults = get_default_settings();
        assert_eq!(
            salvaged.bindings["transcribe"].current_binding,
            defaults.bindings["transcribe"].current_binding
        );
    }

    #[test]
    fn salvage_tolerates_unknown_keys() {
        let mut stored = default_settings_json();
        let map = stored.as_object_mut().unwrap();
        map.insert(
            "field_from_the_future".into(),
            serde_json::json!({ "nested": true }),
        );
        map.insert("selected_model".into(), serde_json::json!("kept"));
        map.insert("sound_theme".into(), serde_json::json!("theremin"));

        let salvaged = salvage_settings(&stored);
        assert_eq!(salvaged.selected_model, "kept");
        assert_eq!(salvaged.sound_theme, default_sound_theme());
    }

    #[test]
    fn salvage_of_non_object_store_falls_back_to_defaults() {
        for stored in [
            serde_json::json!("corrupt"),
            serde_json::json!(null),
            serde_json::json!([1, 2, 3]),
        ] {
            let salvaged = salvage_settings(&stored);
            assert_eq!(
                serde_json::to_value(&salvaged).unwrap(),
                default_settings_json()
            );
        }
    }

    #[test]
    fn default_settings_disable_auto_submit() {
        let settings = get_default_settings();
        assert!(!settings.auto_submit);
        assert_eq!(settings.auto_submit_key, AutoSubmitKey::Enter);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[cfg(not(target_os = "linux"))]
    #[test]
    fn la_marca_se_escribe_sola_de_fabrica() {
        // ABRAX debe salir en mayúsculas desde la primera instalación: las
        // variantes de caja del nombre están en los reemplazos por defecto.
        // «abraza» (el verbo) NO puede estar: es una palabra real del idioma.
        let s = get_default_settings();
        let de: Vec<&str> = s
            .custom_replacements
            .iter()
            .map(|r| r.from.as_str())
            .collect();
        assert!(de.contains(&"Abrax") && de.contains(&"abrax"));
        assert!(!de.contains(&"abraza"));
        assert!(s.custom_replacements.iter().all(|r| r.to == "ABRAX"));
    }

    // El default del overlay depende de la plataforma, así que su guardián
    // también. El test anterior afirmaba `Esfera` SIN guarda de `cfg`, con lo que
    // en Linux —donde el default es `None` a propósito, porque no todos los
    // compositores dan ventana transparente— fallaba. Fallo latente heredado que
    // se arregla aquí de paso, ya que se estaba tocando este mismo test.
    #[test]
    #[cfg(not(target_os = "linux"))]
    fn default_overlay_style_es_minimal() {
        // Decisión de producto del 29/07: de fábrica sale la píldora con la onda
        // de voz. Este test es el guardián del default: si alguien lo cambia sin
        // querer, aquí se ve. La razón y el coste están en
        // `default_overlay_style`.
        let settings = get_default_settings();
        assert_eq!(settings.overlay_style, OverlayStyle::Minimal);
    }

    #[test]
    #[cfg(not(target_os = "linux"))]
    fn el_overlay_sale_encendido_de_fabrica() {
        // Lo que NO puede pasar, elijamos el estilo que sea: que el overlay venga
        // apagado y nadie vea nada al dictar. Ese fue el fallo original con `Live`
        // heredado, y por eso se guarda aparte del estilo concreto.
        let settings = get_default_settings();
        assert_ne!(settings.overlay_style, OverlayStyle::None);
    }

    /// El atajo de leer la selección viene de fábrica y bien formado.
    ///
    /// LÍMITE DECLARADO: aquí NO se comprueba que exista su entrada en
    /// `actions::ACTION_MAP`, que es el emparejamiento que de verdad importa —un
    /// binding sin acción es un atajo global fantasma, registrado en el sistema y
    /// sin efecto. No se puede: referenciar `ACTION_MAP` desde cualquier test de
    /// esta crate hace que el binario de test no arranque en Windows
    /// (`STATUS_ENTRYPOINT_NOT_FOUND`, 0xc0000139) porque arrastra dependencias
    /// nativas que el ejecutable de test no tiene al lado. Verificado: con la
    /// aserción, 0 tests corren; sin ella, 389 pasan.
    ///
    /// Así que el emparejamiento se sostiene a mano y está escrito junto a
    /// `ACTION_MAP`: **todo id de `bindings` necesita su entrada ahí**.
    #[test]
    fn el_atajo_de_leer_seleccion_existe_y_esta_bien_formado() {
        let settings = get_default_settings();
        let b = settings
            .bindings
            .get("leer_seleccion")
            .expect("el binding debe venir de fábrica");
        assert_eq!(b.id, "leer_seleccion");
        assert!(!b.default_binding.is_empty());
        assert_eq!(b.default_binding, b.current_binding);
    }

    /// NO puede ser `ctrl+l` ni `ctrl+shift+l`, los dos candidatos descartados:
    ///
    ///   · `ctrl+l` es la barra de direcciones en Chrome, Edge, Firefox y el
    ///     Explorador de Windows, y limpiar pantalla en una terminal. Un atajo
    ///     global lo captura ANTES que la app en foco, así que dejaría a
    ///     cualquiera sin barra de direcciones.
    ///   · `ctrl+shift+l` lo ocupa Loom con un hook global (medido el 30/07 en el
    ///     equipo de desarrollo).
    ///
    /// Si alguien pone uno de los dos a mano, este test explica por qué no.
    #[test]
    fn el_atajo_de_leer_seleccion_no_secuestra_ctrl_l() {
        let settings = get_default_settings();
        let atajo = settings.bindings["leer_seleccion"]
            .default_binding
            .to_ascii_lowercase();
        let tokens: Vec<&str> = atajo.split('+').map(str::trim).collect();
        let con_ctrl = tokens.iter().any(|t| *t == "ctrl" || *t == "control");
        let solo_ctrl_l = tokens.len() == 2 && tokens.contains(&"l") && con_ctrl;
        assert!(
            !solo_ctrl_l,
            "«{atajo}» secuestraría la barra de direcciones del navegador"
        );
        let ctrl_shift_l =
            tokens.len() == 3 && tokens.contains(&"l") && tokens.contains(&"shift") && con_ctrl;
        assert!(!ctrl_shift_l, "«{atajo}» lo ocupa Loom");
        // Y sigue siendo un atajo global válido para el validador del repo.
        assert!(tokens.len() >= 2, "sin modificador secuestraría el teclado");
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn default_overlay_style_en_linux_es_none() {
        // En Linux se apaga a propósito: el overlay necesita una ventana
        // transparente y no todos los compositores la dan.
        let settings = get_default_settings();
        assert_eq!(settings.overlay_style, OverlayStyle::None);
    }

    #[test]
    fn overlay_migration_keeps_disabled_overlay_off() {
        let mut settings = get_default_settings();

        // Legacy store: overlay was hidden via the retired position "none".
        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "none"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::None);
    }

    #[test]
    fn legacy_none_overlay_position_deserializes_to_bottom() {
        // A persisted "none" must not fail the whole settings load; the serde
        // alias folds it onto Bottom (visibility is owned by overlay_style).
        let raw = serde_json::json!({ "overlay_position": "none" });
        let position: OverlayPosition =
            serde_json::from_value(raw.get("overlay_position").unwrap().clone())
                .expect("legacy \"none\" should deserialize, not error");
        assert_eq!(position, OverlayPosition::Bottom);
    }

    #[test]
    fn overlay_migration_promotes_enabled_overlay_to_live() {
        let mut settings = get_default_settings();
        settings.overlay_position = OverlayPosition::Top;
        settings.overlay_style = OverlayStyle::Minimal;

        let raw = serde_json::json!({
            "selected_model": "",
            "overlay_position": "top"
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(settings.overlay_style, OverlayStyle::Live);
        assert_eq!(settings.overlay_position, OverlayPosition::Top);
    }

    #[test]
    fn gpu_device_migration_resets_legacy_positive_selection_to_auto() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;
        settings.transcribe_gpu_device = 2;

        let raw = serde_json::json!({
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });

        assert!(apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Auto
        );
        assert_eq!(
            settings.transcribe_gpu_device,
            default_transcribe_gpu_device()
        );
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    #[test]
    fn gpu_device_migration_keeps_current_schema_positive_selection() {
        let mut settings = get_default_settings();
        settings.transcribe_accelerator = TranscribeAcceleratorSetting::Gpu;
        settings.transcribe_gpu_device = 2;

        let raw = serde_json::json!({
            "settings_schema_version": CURRENT_SETTINGS_SCHEMA_VERSION,
            "onboarding_completed": false,
            "whats_new_last_seen_version": default_whats_new_last_seen_version(),
            "overlay_style": "live",
            "transcribe_accelerator": "gpu",
            "transcribe_gpu_device": 2
        });

        assert!(!apply_settings_migrations(&mut settings, &raw));
        assert_eq!(
            settings.transcribe_accelerator,
            TranscribeAcceleratorSetting::Gpu
        );
        assert_eq!(settings.transcribe_gpu_device, 2);
    }

    /// La migración es la pieza de la que depende que la función llegue a quien
    /// YA tenía Abrax instalado: cambiar el default no alcanza, porque serde
    /// solo lo aplica cuando la clave FALTA, y esa clave existe desde el 25/07.
    #[test]
    fn migracion_enciende_la_correccion_en_un_store_viejo() {
        let mut stored = default_settings_json();
        let obj = stored.as_object_mut().unwrap();
        obj.insert("settings_schema_version".into(), serde_json::json!(1));
        obj.insert("correccion_motor".into(), serde_json::json!("desactivado"));
        obj.insert("correccion_modo".into(), serde_json::json!("literal"));

        let mut settings: AppSettings = serde_json::from_value(stored.clone()).unwrap();
        assert!(matches!(
            settings.correccion_motor,
            CorreccionMotor::Desactivado
        ));

        let cambio = apply_settings_migrations(&mut settings, &stored);
        assert!(cambio, "la migración debe marcar el store como actualizado");
        assert!(matches!(
            settings.correccion_motor,
            CorreccionMotor::SoloReglas
        ));
        assert_eq!(settings.correccion_modo, CorreccionModo::Limpio);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    /// Control NEGATIVO: si el usuario eligió algo a mano, su elección manda.
    /// Sin este test la migración podría pisar preferencias y nadie lo notaría.
    #[test]
    fn migracion_no_pisa_una_eleccion_del_usuario() {
        let mut stored = default_settings_json();
        let obj = stored.as_object_mut().unwrap();
        obj.insert("settings_schema_version".into(), serde_json::json!(1));
        obj.insert("correccion_motor".into(), serde_json::json!("solo_reglas"));
        obj.insert("correccion_modo".into(), serde_json::json!("literal"));

        let mut settings: AppSettings = serde_json::from_value(stored.clone()).unwrap();
        apply_settings_migrations(&mut settings, &stored);
        // Eligió Literal a mano: se respeta, no se sube a Limpio.
        assert_eq!(settings.correccion_modo, CorreccionModo::Literal);
    }

    /// La autocorreccion hablada debe llegar encendida tambien a quien ya tenia
    /// Abrax. Su `false` guardado NO es una preferencia: el interruptor era un
    /// placebo (sin comando Tauri) hasta el 30/07, asi que nadie pudo elegirlo.
    #[test]
    fn migracion_enciende_la_autocorreccion_hablada() {
        let mut stored = default_settings_json();
        let obj = stored.as_object_mut().unwrap();
        obj.insert("settings_schema_version".into(), serde_json::json!(2));
        obj.insert("autocorreccion_activa".into(), serde_json::json!(false));

        let mut settings: AppSettings = serde_json::from_value(stored.clone()).unwrap();
        assert!(!settings.autocorreccion_activa);
        apply_settings_migrations(&mut settings, &stored);
        assert!(settings.autocorreccion_activa);
        assert_eq!(
            settings.settings_schema_version,
            CURRENT_SETTINGS_SCHEMA_VERSION
        );
    }

    /// Y no se vuelve a tocar: un store ya en el esquema actual se queda como
    /// esta, para que apagarla a mano sea una decision que dure.
    #[test]
    fn migracion_ya_al_dia_respeta_el_apagado() {
        let mut stored = default_settings_json();
        let obj = stored.as_object_mut().unwrap();
        obj.insert(
            "settings_schema_version".into(),
            serde_json::json!(CURRENT_SETTINGS_SCHEMA_VERSION),
        );
        obj.insert("autocorreccion_activa".into(), serde_json::json!(false));

        let mut settings: AppSettings = serde_json::from_value(stored.clone()).unwrap();
        apply_settings_migrations(&mut settings, &stored);
        assert!(!settings.autocorreccion_activa, "se pisó un apagado real");
    }
}
