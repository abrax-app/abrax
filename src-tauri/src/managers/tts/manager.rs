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
            let rec = super::recommend::recommend_engine(&self.hardware_snapshot());
            // La recomendación NO puede caer en un motor que esta máquina no
            // puede habilitar. Con `uv` ausente, el hardware seguía apuntando a
            // Kokoro: la lista ya no lo ofrecía, pero la pantalla decía
            // «Recomendamos Kokoro» y el botón «Usar recomendado» llevaba a un
            // motor invisible. Un consejo que no se puede seguir es peor que no
            // dar consejo.
            if super::registry::necesita_uv(rec) {
                // Piper es nativo en las cuatro plataformas; si tampoco valiera,
                // el sistema siempre está.
                if super::registry::necesita_uv(EngineId::Piper) {
                    EngineId::System
                } else {
                    EngineId::Piper
                }
            } else {
                rec
            }
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
    /// Si el motor activo CAMBIA, corta cualquier audio en curso **antes** de
    /// reenrutar: así la lectura no queda sonando por el motor viejo mientras el
    /// bucle avanza sobre el nuevo (evita "dos voces a la vez").
    pub fn set_engine(&self, id: EngineId) -> Result<(), String> {
        let mut s = settings::get_settings(&self.app);
        s.tts_selected_engine = Some(id);
        settings::write_settings(&self.app, s);
        let resuelto = self.resolve_active();
        let active = engine_to_u8(resuelto);
        let prev = self.active.swap(active, Ordering::SeqCst);
        if prev != active {
            let _ = self.stop();
        }
        // SOLO se sanea si el motor que el usuario pidió es el que quedó ACTIVO.
        //
        // Si pidió Online y hoy no arranca, `resolve_active` degrada al Sistema —
        // y sanear entonces le cambiaría la voz por una del SISTEMA, pisándole la
        // preferencia por un fallo temporal. Cuando el motor vuelva, se habría
        // perdido su elección. Visto en vivo el 30/07: el log mostró la voz
        // cambiada a un token del registro de Windows justo tras degradar.
        //
        // Con el motor caído no hace falta sanear igualmente: el fallback usa su
        // propia voz por defecto.
        if resuelto == id {
            self.sanear_voz_guardada();
        }
        Ok(())
    }

    /// Tras cambiar de motor, descarta la voz guardada si NO pertenece al nuevo.
    ///
    /// # El fallo que arregla
    ///
    /// Cada motor tiene su propio catálogo: los ids de Piper no existen en Kokoro,
    /// ni los del sistema en el Online. `set_engine` persistía el motor pero
    /// dejaba intacto `escucha_voz_prosa`, así que al leer se le pasaba al motor
    /// nuevo un id que no conoce, ese motor fallaba, y `speak` **degradaba al
    /// sistema en silencio**. El usuario veía su motor seleccionado en Ajustes y
    /// oía la voz básica del SO, sin ninguna pista de por qué. Reportado el 30/07.
    ///
    /// Se elige una voz del motor nuevo, en español si la hay: dejarlo en `None`
    /// también funcionaría (el motor usaría su default) pero perdería la
    /// preferencia de idioma, que en un producto es-419 no es un detalle.
    ///
    /// Nunca falla hacia arriba: si no se puede listar las voces del motor nuevo
    /// (un servidor que aún no arrancó, por ejemplo), se deja en `None` y que el
    /// motor decida. Peor sería bloquear el cambio de motor por esto.
    fn sanear_voz_guardada(&self) {
        let voces = self.list_voices().unwrap_or_default();
        let mut s = settings::get_settings(&self.app);

        let prosa = voz_para_el_motor(&s.escucha_voz_prosa, &voces);
        let codigo = voz_para_el_motor(&s.escucha_voz_codigo, &voces);
        if prosa.is_none() && codigo.is_none() {
            return; // las dos siguen valiendo
        }
        if let Some(nueva) = prosa {
            log::info!("[tts] la voz de prosa era de otro motor; se cambia a {nueva:?}");
            s.escucha_voz_prosa = nueva;
        }
        if let Some(nueva) = codigo {
            s.escucha_voz_codigo = nueva;
        }
        settings::write_settings(&self.app, s);
    }

    /// Vuelve a detectar hardware y recomputa el activo (comando "volver a detectar").
    pub fn redetect(&self) -> HardwareInfo {
        let hw = detect_hardware();
        if let Ok(mut h) = self.hardware.lock() {
            *h = hw.clone();
        }
        let active = engine_to_u8(self.resolve_active());
        let prev = self.active.swap(active, Ordering::SeqCst);
        // Si redetectar cambió el motor activo, corta el audio en curso.
        if prev != active {
            let _ = self.stop();
        }
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
            // Solo se ofrece lo que ESTA MAQUINA puede habilitar de verdad.
            //
            // Kokoro y Online levantan un servidor Python y lo aprovisionan con
            // `uv` (`pyserver::run_uv`), que NO viaja con la app. En cualquier
            // equipo recien instalado —el Mac de un juez— pulsar «Habilitar»
            // devolvia «no se pudo ejecutar 'uv' (¿instalado?)». Y en el equipo
            // de desarrollo funcionaba, porque ahi `uv` estaba puesto a mano:
            // por eso llego al dia de entrega sin que nadie lo viera.
            //
            // Piper NO pasa por ahi: es un binario nativo + un `.onnx`, los dos
            // con sha256 fijado y con runtime publicado para Windows x64, Linux
            // x64, macOS x64 y macOS aarch64. Ese sigue ofreciendose siempre.
            //
            // Un boton que falla es peor que un boton que no esta.
            // Fuera SIEMPRE los que dependen de `uv`, tenga la maquina `uv` o no.
            //
            // Primero se filtro solo cuando faltaba, para no quitarle nada a
            // quien lo tuviera. Eso hacia que el build de ENTREGA se viera
            // distinto en cada equipo: en el de desarrollo salian los cuatro y
            // en el de un juez dos. Un producto que no se puede demostrar tal
            // como lo recibe el usuario no se puede ni grabar ni comprobar.
            //
            // Y Online no es solo fragil: manda el texto a servidores de
            // Microsoft, que es justo lo contrario de lo que promete la app.
            //
            // Quedan los dos que funcionan en cualquier equipo recien
            // instalado: el del sistema y Piper (nativo, con runtime firmado
            // para Windows, Linux y las dos arquitecturas de macOS).
            EngineId::ALL
                .iter()
                .filter(|&&id| !super::registry::necesita_uv(id))
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
                let playing = self
                    .playback
                    .lock()
                    .ok()
                    .and_then(|g| g.clone())
                    .map(|p| p.is_playing())
                    .unwrap_or(false);
                // Incluye el motor del SISTEMA por si `speak` degradó a él
                // (fallback: el neuronal falló → sys.speak por el SO, que NO usa
                // el PlaybackService). Sin esto, el sondeo creería que la oración
                // terminó y avanzaría encimando la voz de fallback.
                let sys = self.escucha.status().map(|s| s.hablando).unwrap_or(false);
                Ok(EstadoEscucha {
                    hablando: playing || sys,
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

/// Decide si una voz guardada sigue sirviendo para el catálogo dado.
///
/// `None` = la actual vale, no hay que tocar nada.
/// `Some(nueva)` = hay que reemplazarla por `nueva` (que puede ser `None` si el
/// motor no ofrece ninguna voz).
///
/// Se separa de [`TtsManager::sanear_voz_guardada`] para poder PROBARLA: aquella
/// necesita un `AppHandle` de Tauri y un test no puede construirlo. Toda la
/// decisión vive aquí; el método solo lee settings, llama a esto y escribe.
fn voz_para_el_motor(
    actual: &Option<String>,
    voces: &[crate::managers::escucha::VozEscucha],
) -> Option<Option<String>> {
    // Sin voz guardada no hay nada que sanear: el motor usará su default.
    let Some(id) = actual else {
        return None;
    };
    if voces.iter().any(|v| &v.id == id) {
        return None; // pertenece a este motor
    }
    // Español primero: en un producto es-419 caer en una voz inglesa por orden
    // alfabético sería un arreglo peor que el fallo.
    Some(
        voces
            .iter()
            .find(|v| v.es_espanol)
            .or_else(|| voces.first())
            .map(|v| v.id.clone()),
    )
}

#[cfg(test)]
mod tests_voz_motor {
    use super::voz_para_el_motor;
    use crate::managers::escucha::VozEscucha;

    fn voz(id: &str, es: bool) -> VozEscucha {
        VozEscucha {
            id: id.to_string(),
            nombre: id.to_string(),
            idioma: if es { "es-MX".into() } else { "en-US".into() },
            es_espanol: es,
        }
    }

    /// EL CASO REPORTADO: se cambia de motor y la voz guardada es del anterior.
    /// Sin esto, `speak` fallaba y degradaba al sistema EN SILENCIO — el usuario
    /// veía su motor elegido y oía la voz básica del SO.
    #[test]
    fn una_voz_de_otro_motor_se_reemplaza() {
        let catalogo = [
            voz("es-MX-DaliaNeural", true),
            voz("en-US-AriaNeural", false),
        ];
        let guardada = Some("Microsoft Sabina Desktop".to_string()); // voz del SO
        assert_eq!(
            voz_para_el_motor(&guardada, &catalogo),
            Some(Some("es-MX-DaliaNeural".to_string()))
        );
    }

    #[test]
    fn una_voz_del_mismo_motor_no_se_toca() {
        let catalogo = [voz("es-MX-DaliaNeural", true)];
        let guardada = Some("es-MX-DaliaNeural".to_string());
        assert_eq!(voz_para_el_motor(&guardada, &catalogo), None);
    }

    #[test]
    fn sin_voz_guardada_no_hay_nada_que_sanear() {
        let catalogo = [voz("es-MX-DaliaNeural", true)];
        assert_eq!(voz_para_el_motor(&None, &catalogo), None);
    }

    /// Prefiere ESPAÑOL aunque el catálogo empiece por otro idioma: caer en una
    /// voz inglesa sería un arreglo peor que el fallo en un producto es-419.
    #[test]
    fn prefiere_espanol_aunque_no_sea_la_primera() {
        let catalogo = [
            voz("en-GB-RyanNeural", false),
            voz("en-US-AriaNeural", false),
            voz("es-CL-CatalinaNeural", true),
        ];
        let guardada = Some("otra-cosa".to_string());
        assert_eq!(
            voz_para_el_motor(&guardada, &catalogo),
            Some(Some("es-CL-CatalinaNeural".to_string()))
        );
    }

    /// Sin voces en español se coge la primera que haya: mejor una voz inglesa
    /// que ninguna. (Piper recién instalado con una sola voz, por ejemplo.)
    #[test]
    fn sin_espanol_cae_en_la_primera() {
        let catalogo = [voz("en-US-AriaNeural", false)];
        let guardada = Some("es-MX-DaliaNeural".to_string());
        assert_eq!(
            voz_para_el_motor(&guardada, &catalogo),
            Some(Some("en-US-AriaNeural".to_string()))
        );
    }

    /// Catálogo vacío: se limpia a `None` y decide el motor. Lo que NO puede
    /// pasar es conservar un id que no existe, que es el fallo original.
    #[test]
    fn catalogo_vacio_limpia_la_voz() {
        let guardada = Some("es-MX-DaliaNeural".to_string());
        assert_eq!(voz_para_el_motor(&guardada, &[]), Some(None));
    }
}
