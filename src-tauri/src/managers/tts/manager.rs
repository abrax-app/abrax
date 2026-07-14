//! `TtsManager`: registro multi-motor + enrutado. Envuelve el `EscuchaManager`
//! (motor del sistema) y añade los motores neuronales (Piper y Kokoro).
//! Resuelve el motor **activo** desde settings + hardware (recomendación) y
//! enruta `speak`/`list_voices`/`stop`/`status` a ese motor — **sin duplicar los
//! comandos de Escucha**. Los motores neuronales se construyen de forma perezosa
//! (no se toma el dispositivo de audio en el arranque).
//!
//! Los motores neuronales viven tras la feature `advanced-tts` (OFF por defecto).
//! En el build de entrega solo existe el motor del Sistema: `list_engines`
//! reporta únicamente Sistema, el activo siempre es Sistema y no se compila wgpu.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use super::engine::{EngineId, EngineStatus, TtsEngine, TtsOptions};
use super::hardware::{detect_hardware, HardwareInfo};
use super::recommend::resolve_engine;
use crate::managers::escucha::{EscuchaManager, EstadoEscucha, VozEscucha};
use crate::settings;

#[cfg(feature = "advanced-tts")]
use super::playback::PlaybackService;
#[cfg(feature = "advanced-tts")]
use super::{download, kokoro, online, piper, pyserver};

/// EngineId ↔ u8 para cachear el motor activo en un átomico (lectura sin lock).
fn engine_to_u8(id: EngineId) -> u8 {
    match id {
        EngineId::System => 0,
        EngineId::Piper => 1,
        EngineId::Kokoro => 2,
        EngineId::Online => 3,
    }
}
fn engine_from_u8(v: u8) -> EngineId {
    match v {
        1 => EngineId::Piper,
        2 => EngineId::Kokoro,
        3 => EngineId::Online,
        _ => EngineId::System,
    }
}

/// Estado perezoso de los motores neuronales (registro; se toma bajo `inner`).
#[cfg_attr(not(feature = "advanced-tts"), allow(dead_code))]
struct Inner {
    #[cfg(feature = "advanced-tts")]
    piper: Option<piper::PiperEngine>,
    #[cfg(feature = "advanced-tts")]
    kokoro: Option<pyserver::PyServerEngine>,
    #[cfg(feature = "advanced-tts")]
    online: Option<pyserver::PyServerEngine>,
}

pub struct TtsManager {
    #[cfg_attr(not(feature = "advanced-tts"), allow(dead_code))]
    app: AppHandle,
    escucha: Arc<EscuchaManager>,
    hardware: Mutex<HardwareInfo>,
    /// Servicio de reproducción neuronal compartido (perezoso), **fuera de
    /// `inner`**: así `stop`/`status` cortan el audio sin esperar el lock del
    /// registro de motores, que puede estar retenido varios segundos mientras un
    /// motor neuronal carga su modelo. Es lo que hace que "Detener" responda al
    /// instante y no quede audio viejo encimado al cambiar de voz en caliente.
    #[cfg(feature = "advanced-tts")]
    playback: Mutex<Option<Arc<PlaybackService>>>,
    /// Motor activo cacheado sin lock (para `active`/`status`/`stop`).
    active: AtomicU8,
    inner: Mutex<Inner>,
}

impl TtsManager {
    pub fn new(app: AppHandle, escucha: Arc<EscuchaManager>) -> Self {
        let hardware = detect_hardware();
        let manager = Self {
            app,
            escucha,
            hardware: Mutex::new(hardware),
            #[cfg(feature = "advanced-tts")]
            playback: Mutex::new(None),
            active: AtomicU8::new(engine_to_u8(EngineId::System)),
            inner: Mutex::new(Inner {
                #[cfg(feature = "advanced-tts")]
                piper: None,
                #[cfg(feature = "advanced-tts")]
                kokoro: None,
                #[cfg(feature = "advanced-tts")]
                online: None,
            }),
        };
        // Resuelve el motor activo desde settings + hardware al arrancar.
        let active = manager.resolve_active();
        manager.active.store(engine_to_u8(active), Ordering::SeqCst);
        manager
    }

    fn hardware_snapshot(&self) -> HardwareInfo {
        self.hardware
            .lock()
            .map(|h| h.clone())
            .unwrap_or_else(|_| detect_hardware())
    }

    /// ¿Está disponible este motor AHORA? (runtime/modelo presente).
    pub fn is_engine_available(&self, id: EngineId) -> bool {
        match id {
            EngineId::System => self
                .escucha
                .status()
                .map(|s| s.motor_disponible)
                .unwrap_or(false),
            #[cfg(feature = "advanced-tts")]
            EngineId::Piper => {
                piper::is_runtime_installed(&self.app)
                    && download::voices_dir(&self.app)
                        .map(|dir| {
                            piper::VOICES.iter().any(|v| {
                                dir.join(format!("{}.onnx", v.id)).is_file()
                                    && dir.join(format!("{}.onnx.json", v.id)).is_file()
                            })
                        })
                        .unwrap_or(false)
            }
            #[cfg(feature = "advanced-tts")]
            EngineId::Kokoro => kokoro::is_installed(&self.app),
            // Online: disponible si el runtime está aprovisionado (la conexión
            // real se comprueba al sintetizar; si falla, degrada al sistema).
            #[cfg(feature = "advanced-tts")]
            EngineId::Online => online::is_installed(&self.app),
            // Sin `advanced-tts`: ningún motor neuronal está disponible.
            #[cfg(not(feature = "advanced-tts"))]
            _ => false,
        }
    }

    /// Motor recomendado para este hardware (ideal, sin considerar descarga).
    /// Sin `advanced-tts` la recomendación es siempre el Sistema.
    pub fn recommended(&self) -> EngineId {
        #[cfg(feature = "advanced-tts")]
        {
            super::recommend::recommend_engine(&self.hardware_snapshot())
        }
        #[cfg(not(feature = "advanced-tts"))]
        {
            EngineId::System
        }
    }

    /// Resuelve el motor **activo**: la elección del usuario si existe, si no la
    /// recomendación por hardware (si auto-detect), degradando por disponibilidad
    /// a Piper y luego al sistema. Nunca la nube, nunca silencio.
    pub fn resolve_active(&self) -> EngineId {
        let s = settings::get_settings(&self.app);
        let ideal = if let Some(selected) = s.tts_selected_engine {
            selected
        } else if s.tts_auto_detect {
            self.recommended()
        } else {
            EngineId::System
        };
        resolve_engine(ideal, |id| self.is_engine_available(id))
    }

    /// Motor activo cacheado (lectura sin lock).
    pub fn active(&self) -> EngineId {
        engine_from_u8(self.active.load(Ordering::SeqCst))
    }

    /// Fija el motor seleccionado (persiste en settings) y recomputa el activo.
    pub fn set_engine(&self, id: EngineId) -> Result<(), String> {
        let mut s = settings::get_settings(&self.app);
        s.tts_selected_engine = Some(id);
        settings::write_settings(&self.app, s);
        let active = self.resolve_active();
        self.active.store(engine_to_u8(active), Ordering::SeqCst);
        Ok(())
    }

    /// Vuelve a detectar hardware y recomputa el activo (comando "volver a detectar").
    pub fn redetect(&self) -> HardwareInfo {
        let hw = detect_hardware();
        if let Ok(mut h) = self.hardware.lock() {
            *h = hw.clone();
        }
        let active = self.resolve_active();
        self.active.store(engine_to_u8(active), Ordering::SeqCst);
        hw
    }

    pub fn hardware(&self) -> HardwareInfo {
        self.hardware_snapshot()
    }

    /// Estado por motor para el selector "elegir otro motor". Sin `advanced-tts`
    /// solo se expone el motor del Sistema (la UI colapsa a "solo Sistema").
    pub fn list_engines(&self) -> Vec<EngineStatus> {
        #[cfg(feature = "advanced-tts")]
        {
            let recommended = self.recommended();
            // Se listan todos, incluido Online (marcado needs_internet); la
            // recomendación jamás apunta a un motor no-local.
            EngineId::ALL
                .iter()
                .map(|&id| {
                    let requirements = super::registry::requirements_for(id);
                    EngineStatus {
                        id,
                        display_name: id.display_name().to_string(),
                        available: self.is_engine_available(id),
                        // El "por qué" lo compone la UI (i18n) desde requirements.
                        reason: None,
                        recommended: id == recommended,
                        requirements,
                    }
                })
                .collect()
        }
        #[cfg(not(feature = "advanced-tts"))]
        {
            vec![EngineStatus {
                id: EngineId::System,
                display_name: EngineId::System.display_name().to_string(),
                available: self.is_engine_available(EngineId::System),
                reason: None,
                recommended: true,
                requirements: super::registry::requirements_for(EngineId::System),
            }]
        }
    }

    /// Servicio de reproducción compartido, perezoso. Su propio mutex (separado
    /// de `inner`) para que `stop`/`status` lo alcancen aunque el registro de
    /// motores esté ocupado cargando un modelo.
    #[cfg(feature = "advanced-tts")]
    fn ensure_playback(&self) -> Result<Arc<PlaybackService>, String> {
        let mut guard = self
            .playback
            .lock()
            .map_err(|_| "playback envenenado".to_string())?;
        if let Some(p) = &*guard {
            return Ok(p.clone());
        }
        let device = settings::get_settings(&self.app)
            .selected_output_device
            .clone();
        let p = PlaybackService::new(device)?;
        *guard = Some(p.clone());
        Ok(p)
    }

    #[cfg(feature = "advanced-tts")]
    fn ensure_piper(&self, inner: &mut Inner) -> Result<(), String> {
        if inner.piper.is_some() {
            return Ok(());
        }
        let playback = self.ensure_playback()?;
        let voices_dir = download::voices_dir(&self.app)?;
        let runtime_dir = download::runtime_dir(&self.app, "piper")?;
        let temp_dir = download::tts_dir(&self.app)?.join("tmp");
        inner.piper = Some(piper::PiperEngine::new(
            voices_dir,
            runtime_dir,
            temp_dir,
            playback,
            1.0,
        ));
        Ok(())
    }

    /// Construye perezosamente un motor basado en servidor Python (Kokoro).
    #[cfg(feature = "advanced-tts")]
    fn ensure_pyserver(&self, inner: &mut Inner, id: EngineId) -> Result<(), String> {
        let playback = self.ensure_playback()?;
        match id {
            EngineId::Kokoro if inner.kokoro.is_none() => {
                let dir = download::runtime_dir(&self.app, kokoro::RUNTIME_NAME)?;
                inner.kokoro = Some(pyserver::PyServerEngine::new(
                    &kokoro::CONFIG,
                    self.app.clone(),
                    dir,
                    playback,
                    1.0,
                ));
            }
            EngineId::Online if inner.online.is_none() => {
                let dir = download::runtime_dir(&self.app, online::RUNTIME_NAME)?;
                inner.online = Some(pyserver::PyServerEngine::new(
                    &online::CONFIG,
                    self.app.clone(),
                    dir,
                    playback,
                    1.0,
                ));
            }
            _ => {}
        }
        Ok(())
    }

    /// Ejecuta `f` sobre el motor ACTIVO, construyéndolo perezosamente si es
    /// neuronal. El lock de `inner` se toma **solo** en las ramas neuronales
    /// (registro de motores); el motor del sistema no lo necesita.
    fn with_active<R>(&self, f: impl FnOnce(&mut dyn TtsEngine) -> R) -> Result<R, String> {
        match self.active() {
            EngineId::System => {
                // Motor del sistema: adaptador barato sobre el EscuchaManager.
                let mut eng = super::system::SystemEngine::new(self.escucha.clone());
                Ok(f(&mut eng))
            }
            #[cfg(feature = "advanced-tts")]
            EngineId::Piper => {
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| "tts inner envenenado".to_string())?;
                self.ensure_piper(&mut inner)?;
                let eng = inner
                    .piper
                    .as_mut()
                    .ok_or_else(|| "piper no inicializado".to_string())?;
                Ok(f(eng))
            }
            #[cfg(feature = "advanced-tts")]
            EngineId::Kokoro => {
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| "tts inner envenenado".to_string())?;
                self.ensure_pyserver(&mut inner, EngineId::Kokoro)?;
                let eng = inner
                    .kokoro
                    .as_mut()
                    .ok_or_else(|| "kokoro no inicializado".to_string())?;
                Ok(f(eng))
            }
            #[cfg(feature = "advanced-tts")]
            EngineId::Online => {
                let mut inner = self
                    .inner
                    .lock()
                    .map_err(|_| "tts inner envenenado".to_string())?;
                self.ensure_pyserver(&mut inner, EngineId::Online)?;
                let eng = inner
                    .online
                    .as_mut()
                    .ok_or_else(|| "online no inicializado".to_string())?;
                Ok(f(eng))
            }
            // Sin `advanced-tts`: cualquier selección persistida degrada al Sistema.
            #[cfg(not(feature = "advanced-tts"))]
            _ => {
                let mut eng = super::system::SystemEngine::new(self.escucha.clone());
                Ok(f(&mut eng))
            }
        }
    }

    // --- API enrutada (reemplaza las llamadas directas a EscuchaManager) ---

    pub fn speak(
        &self,
        texto: String,
        voz_id: Option<String>,
        rate: Option<f32>,
        pitch: Option<i32>,
    ) -> Result<(), String> {
        let opts = TtsOptions {
            rate: rate.unwrap_or(1.0),
            pitch_hz: pitch.unwrap_or(0),
        };
        let voz = voz_id.clone();
        // Si el motor activo neuronal falla, degrada al sistema y reintenta
        // (fallback en runtime: sin silencio, sin nube).
        let (active_id, result) =
            self.with_active(|eng| (eng.id(), eng.speak(&texto, voz.as_deref(), &opts)))?;
        match result {
            Ok(()) => Ok(()),
            Err(e) => {
                if active_id != EngineId::System {
                    log::warn!("[tts] motor {active_id:?} falló ({e}); degradando al sistema");
                    let mut sys = super::system::SystemEngine::new(self.escucha.clone());
                    sys.speak(&texto, None, &opts).map_err(|e2| e2.to_string())
                } else {
                    Err(e.to_string())
                }
            }
        }
    }

    /// Detiene la reproducción SIN pasar por el lock del registro de motores (que
    /// puede estar cargando un modelo): corta el servicio neuronal compartido y
    /// el motor del sistema, sea cual sea el activo. Así "Detener" responde al
    /// instante y no queda audio viejo encimado al cambiar de voz/motor en
    /// caliente (la causa de "escucho dos voces a la vez").
    pub fn stop(&self) -> Result<(), String> {
        #[cfg(feature = "advanced-tts")]
        if let Some(p) = self.playback.lock().ok().and_then(|g| g.clone()) {
            p.stop();
        }
        let _ = self.escucha.stop();
        Ok(())
    }

    /// Estado de reproducción SIN tomar el lock del registro, para que el sondeo
    /// de la UI (resaltado de la lectura) no se congele mientras un motor
    /// neuronal carga su modelo.
    pub fn status(&self) -> Result<EstadoEscucha, String> {
        match self.active() {
            EngineId::System => self.escucha.status(),
            #[cfg(feature = "advanced-tts")]
            other => {
                let hablando = self
                    .playback
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .map(|p| p.is_playing())
                    .unwrap_or(false);
                Ok(EstadoEscucha {
                    hablando,
                    motor_disponible: self.is_engine_available(other),
                })
            }
            #[cfg(not(feature = "advanced-tts"))]
            _ => self.escucha.status(),
        }
    }

    pub fn list_voices(&self) -> Result<Vec<VozEscucha>, String> {
        self.with_active(|eng| eng.list_voices())?
            .map_err(|e| e.to_string())
    }
}
