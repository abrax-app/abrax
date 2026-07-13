//! `TtsManager`: registro multi-motor + enrutado. Envuelve el `EscuchaManager`
//! (motor del sistema) y añade los motores neuronales (Piper/Kokoro/Chatterbox).
//! Resuelve el motor **activo** desde settings + hardware (recomendación) y
//! enruta `speak`/`list_voices`/`stop`/`status` a ese motor — **sin duplicar los
//! comandos de Escucha**. Los motores neuronales se construyen de forma perezosa
//! (no se toma el dispositivo de audio en el arranque).

use std::sync::{Arc, Mutex};

use tauri::AppHandle;

use super::engine::{EngineId, EngineStatus, TtsEngine, TtsOptions};
use super::hardware::{detect_hardware, HardwareInfo};
use super::playback::PlaybackService;
use super::recommend::{recommend_engine, resolve_engine};
use super::{chatterbox, download, kokoro, piper, pyserver};
use crate::managers::escucha::{EscuchaManager, EstadoEscucha, VozEscucha};
use crate::settings;

/// Estado perezoso de los motores neuronales + el motor activo vigente.
struct Inner {
    playback: Option<Arc<PlaybackService>>,
    piper: Option<piper::PiperEngine>,
    chatterbox: Option<pyserver::PyServerEngine>,
    kokoro: Option<pyserver::PyServerEngine>,
    active: EngineId,
}

pub struct TtsManager {
    app: AppHandle,
    escucha: Arc<EscuchaManager>,
    hardware: Mutex<HardwareInfo>,
    inner: Mutex<Inner>,
}

impl TtsManager {
    pub fn new(app: AppHandle, escucha: Arc<EscuchaManager>) -> Self {
        let hardware = detect_hardware();
        let manager = Self {
            app,
            escucha,
            hardware: Mutex::new(hardware),
            inner: Mutex::new(Inner {
                playback: None,
                piper: None,
                chatterbox: None,
                kokoro: None,
                active: EngineId::System,
            }),
        };
        // Resuelve el motor activo desde settings + hardware al arrancar.
        let active = manager.resolve_active();
        if let Ok(mut inner) = manager.inner.lock() {
            inner.active = active;
        }
        manager
    }

    fn hardware_snapshot(&self) -> HardwareInfo {
        self.hardware
            .lock()
            .map(|h| h.clone())
            .unwrap_or_else(|_| detect_hardware())
    }

    /// ¿Está disponible este motor AHORA? (hardware compatible + runtime/modelo).
    pub fn is_engine_available(&self, id: EngineId) -> bool {
        let hw = self.hardware_snapshot();
        match id {
            EngineId::System => self
                .escucha
                .status()
                .map(|s| s.motor_disponible)
                .unwrap_or(false),
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
            EngineId::Chatterbox => {
                pyserver::PyServerEngine::gpu_compatible(&hw) && chatterbox::is_installed(&self.app)
            }
            EngineId::Kokoro => kokoro::is_installed(&self.app),
        }
    }

    /// Motor recomendado para este hardware (ideal, sin considerar descarga).
    pub fn recommended(&self) -> EngineId {
        recommend_engine(&self.hardware_snapshot())
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

    /// Motor activo cacheado.
    pub fn active(&self) -> EngineId {
        self.inner
            .lock()
            .map(|i| i.active)
            .unwrap_or(EngineId::System)
    }

    /// Fija el motor seleccionado (persiste en settings) y recomputa el activo.
    pub fn set_engine(&self, id: EngineId) -> Result<(), String> {
        let mut s = settings::get_settings(&self.app);
        s.tts_selected_engine = Some(id);
        settings::write_settings(&self.app, s);
        let active = self.resolve_active();
        if let Ok(mut inner) = self.inner.lock() {
            inner.active = active;
        }
        Ok(())
    }

    /// Vuelve a detectar hardware y recomputa el activo (comando "volver a detectar").
    pub fn redetect(&self) -> HardwareInfo {
        let hw = detect_hardware();
        if let Ok(mut h) = self.hardware.lock() {
            *h = hw.clone();
        }
        let active = self.resolve_active();
        if let Ok(mut inner) = self.inner.lock() {
            inner.active = active;
        }
        hw
    }

    pub fn hardware(&self) -> HardwareInfo {
        self.hardware_snapshot()
    }

    /// Estado por motor para el selector "elegir otro motor".
    pub fn list_engines(&self) -> Vec<EngineStatus> {
        let recommended = self.recommended();
        EngineId::ALL
            .iter()
            // Invariante: sólo se listan motores locales (nunca nube).
            .filter(|id| id.is_local())
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

    fn ensure_playback(&self, inner: &mut Inner) -> Result<Arc<PlaybackService>, String> {
        if let Some(p) = &inner.playback {
            return Ok(p.clone());
        }
        let device = settings::get_settings(&self.app).selected_output_device.clone();
        let p = PlaybackService::new(device)?;
        inner.playback = Some(p.clone());
        Ok(p)
    }

    fn ensure_piper(&self, inner: &mut Inner) -> Result<(), String> {
        if inner.piper.is_some() {
            return Ok(());
        }
        let playback = self.ensure_playback(inner)?;
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

    /// Construye perezosamente un motor basado en servidor Python (Chatterbox/Kokoro).
    fn ensure_pyserver(&self, inner: &mut Inner, id: EngineId) -> Result<(), String> {
        let playback = self.ensure_playback(inner)?;
        match id {
            EngineId::Chatterbox if inner.chatterbox.is_none() => {
                let dir = download::runtime_dir(&self.app, chatterbox::RUNTIME_NAME)?;
                inner.chatterbox = Some(pyserver::PyServerEngine::new(
                    &chatterbox::CONFIG,
                    self.app.clone(),
                    dir,
                    playback,
                    1.0,
                ));
            }
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
            _ => {}
        }
        Ok(())
    }

    /// Ejecuta `f` sobre el motor ACTIVO, construyéndolo perezosamente si es neuronal.
    fn with_active<R>(&self, f: impl FnOnce(&mut dyn TtsEngine) -> R) -> Result<R, String> {
        let mut inner = self.inner.lock().map_err(|_| "tts inner envenenado".to_string())?;
        match inner.active {
            EngineId::System => {
                // Motor del sistema: adaptador barato sobre el EscuchaManager.
                let mut eng = super::system::SystemEngine::new(self.escucha.clone());
                Ok(f(&mut eng))
            }
            EngineId::Piper => {
                self.ensure_piper(&mut inner)?;
                let eng = inner
                    .piper
                    .as_mut()
                    .ok_or_else(|| "piper no inicializado".to_string())?;
                Ok(f(eng))
            }
            EngineId::Chatterbox => {
                self.ensure_pyserver(&mut inner, EngineId::Chatterbox)?;
                let eng = inner
                    .chatterbox
                    .as_mut()
                    .ok_or_else(|| "chatterbox no inicializado".to_string())?;
                Ok(f(eng))
            }
            EngineId::Kokoro => {
                self.ensure_pyserver(&mut inner, EngineId::Kokoro)?;
                let eng = inner
                    .kokoro
                    .as_mut()
                    .ok_or_else(|| "kokoro no inicializado".to_string())?;
                Ok(f(eng))
            }
        }
    }

    // --- API enrutada (reemplaza las llamadas directas a EscuchaManager) ---

    pub fn speak(
        &self,
        texto: String,
        voz_id: Option<String>,
        rate: Option<f32>,
    ) -> Result<(), String> {
        let opts = TtsOptions {
            rate: rate.unwrap_or(1.0),
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

    pub fn stop(&self) -> Result<(), String> {
        self.with_active(|eng| eng.stop())?.map_err(|e| e.to_string())
    }

    pub fn status(&self) -> Result<EstadoEscucha, String> {
        let hw = self.hardware_snapshot();
        self.with_active(|eng| EstadoEscucha {
            hablando: eng.is_speaking(),
            motor_disponible: eng.is_available(&hw),
        })
    }

    pub fn list_voices(&self) -> Result<Vec<VozEscucha>, String> {
        self.with_active(|eng| eng.list_voices())?
            .map_err(|e| e.to_string())
    }
}
