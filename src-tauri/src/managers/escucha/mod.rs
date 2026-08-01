//! [ESCUCHA] Motor de lectura en voz alta.
//!
//! MVP sobre el TTS del sistema operativo vía el crate `tts` (WinRT/SAPI en
//! Windows, AVSpeechSynthesizer en macOS, speech-dispatcher en Linux): cero
//! descargas, voces ya instaladas, 100% local. La voz neural premium (Piper)
//! queda solo documentada en docs/ESCUCHA_FASE2.md.
//!
//! `tts::Tts` no es `Send` en todas las plataformas, así que la instancia vive
//! en un hilo de trabajo propio; los comandos le llegan por un canal mpsc y
//! responden por un canal de vuelta con timeout, de modo que un fallo del
//! motor nunca cuelga el hilo de IPC de Tauri.

pub mod preproceso;

use serde::{Deserialize, Serialize};
use specta::Type;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Mutex;
use std::time::Duration;

/// Una voz instalada en el sistema operativo.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct VozEscucha {
    pub id: String,
    pub nombre: String,
    /// Etiqueta BCP-47 reportada por el SO (p. ej. "es-MX", "en-US").
    pub idioma: String,
    /// `true` si el idioma empieza con "es" — el selector las lista primero.
    pub es_espanol: bool,
}

#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct EstadoEscucha {
    pub hablando: bool,
    /// `false` si el motor del SO no pudo inicializarse (sin TTS instalado,
    /// speech-dispatcher ausente en Linux, etc.).
    pub motor_disponible: bool,
}

enum Cmd {
    ListVoices(Sender<Result<Vec<VozEscucha>, String>>),
    Speak {
        texto: String,
        voz_id: Option<String>,
        rate: Option<f32>,
        reply: Sender<Result<(), String>>,
    },
    Stop(Sender<Result<(), String>>),
    Status(Sender<Result<EstadoEscucha, String>>),
}

/// Cuánto esperamos la respuesta del hilo del motor antes de rendirnos.
/// La síntesis en sí es asíncrona (speak encola y retorna), así que ninguna
/// operación legítima debería acercarse a este límite.
const REPLY_TIMEOUT: Duration = Duration::from_secs(5);

pub struct EscuchaManager {
    tx: Mutex<Sender<Cmd>>,
}

impl EscuchaManager {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Cmd>();
        std::thread::Builder::new()
            .name("escucha-tts".into())
            .spawn(move || worker_loop(rx))
            .expect("no se pudo crear el hilo del motor de Escucha");
        Self { tx: Mutex::new(tx) }
    }

    fn request<T>(
        &self,
        build: impl FnOnce(Sender<Result<T, String>>) -> Cmd,
    ) -> Result<T, String> {
        let (reply_tx, reply_rx) = mpsc::channel();
        {
            let tx = self.tx.lock().map_err(|_| "canal del motor envenenado")?;
            tx.send(build(reply_tx))
                .map_err(|_| "el hilo del motor de Escucha terminó".to_string())?;
        }
        reply_rx
            .recv_timeout(REPLY_TIMEOUT)
            .map_err(|_| "el motor de Escucha no respondió a tiempo".to_string())?
    }

    pub fn list_voices(&self) -> Result<Vec<VozEscucha>, String> {
        self.request(Cmd::ListVoices)
    }

    /// Habla un único trozo de texto, interrumpiendo lo que estuviera sonando.
    /// La cola de oraciones vive en el frontend: habla una, espera con
    /// `status()` a que termine y pide la siguiente (así el resaltado avanza).
    /// `rate` es un multiplicador de velocidad (1.0 = normal, clamp 0.25–3.0).
    pub fn speak(
        &self,
        texto: String,
        voz_id: Option<String>,
        rate: Option<f32>,
    ) -> Result<(), String> {
        self.request(|reply| Cmd::Speak {
            texto,
            voz_id,
            rate,
            reply,
        })
    }

    pub fn stop(&self) -> Result<(), String> {
        self.request(Cmd::Stop)
    }

    pub fn status(&self) -> Result<EstadoEscucha, String> {
        self.request(Cmd::Status)
    }
}

impl Default for EscuchaManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Estado que vive en el hilo del motor: la instancia `Tts` (inicializada
/// perezosamente en el primer comando, para no pagar el costo ni arriesgar un
/// fallo de plataforma durante el arranque de la app) y la última voz aplicada
/// (para no re-buscarla en cada oración).
struct Worker {
    tts: Option<Result<tts::Tts, String>>,
    voz_actual: Option<String>,
}

impl Worker {
    fn engine(&mut self) -> Result<&mut tts::Tts, String> {
        if self.tts.is_none() {
            self.tts = Some(
                tts::Tts::default()
                    .map_err(|e| format!("no se pudo inicializar el TTS del sistema: {e}")),
            );
        }
        match self.tts.as_mut().unwrap() {
            Ok(tts) => Ok(tts),
            Err(e) => Err(e.clone()),
        }
    }
}

fn worker_loop(rx: Receiver<Cmd>) {
    let mut worker = Worker {
        tts: None,
        voz_actual: None,
    };

    while let Ok(cmd) = rx.recv() {
        match cmd {
            Cmd::ListVoices(reply) => {
                let _ = reply.send(list_voices(&mut worker));
            }
            Cmd::Speak {
                texto,
                voz_id,
                rate,
                reply,
            } => {
                let _ = reply.send(speak(&mut worker, &texto, voz_id, rate));
            }
            Cmd::Stop(reply) => {
                let result = worker
                    .engine()
                    .and_then(|tts| tts.stop().map(|_| ()).map_err(|e| e.to_string()));
                let _ = reply.send(result);
            }
            Cmd::Status(reply) => {
                let result = match worker.engine() {
                    Ok(tts) => tts
                        .is_speaking()
                        .map(|hablando| EstadoEscucha {
                            hablando,
                            motor_disponible: true,
                        })
                        .map_err(|e| e.to_string()),
                    Err(_) => Ok(EstadoEscucha {
                        hablando: false,
                        motor_disponible: false,
                    }),
                };
                let _ = reply.send(result);
            }
        }
    }
}

fn list_voices(worker: &mut Worker) -> Result<Vec<VozEscucha>, String> {
    let tts = worker.engine()?;
    let voices = tts.voices().map_err(|e| e.to_string())?;
    let mut result: Vec<VozEscucha> = voices
        .iter()
        .map(|v| {
            let idioma = v.language().to_string();
            let es_espanol = idioma.to_lowercase().starts_with("es");
            VozEscucha {
                id: v.id(),
                nombre: v.name(),
                idioma,
                es_espanol,
            }
        })
        .collect();
    // Voces en español primero, luego el resto, cada grupo ordenado por nombre.
    result.sort_by(|a, b| {
        b.es_espanol
            .cmp(&a.es_espanol)
            .then_with(|| a.nombre.cmp(&b.nombre))
    });
    Ok(result)
}

fn speak(
    worker: &mut Worker,
    texto: &str,
    voz_id: Option<String>,
    rate: Option<f32>,
) -> Result<(), String> {
    // Buscar la voz antes de tomar prestado el engine mutablemente.
    if let Some(id) = voz_id {
        if worker.voz_actual.as_deref() != Some(id.as_str()) {
            let tts = worker.engine()?;
            let voices = tts.voices().map_err(|e| e.to_string())?;
            let voz = voices
                .into_iter()
                .find(|v| v.id() == id)
                .ok_or_else(|| format!("voz desconocida: {id}"))?;
            tts.set_voice(&voz).map_err(|e| e.to_string())?;
            worker.voz_actual = Some(id);
        }
    }

    let tts = worker.engine()?;
    if let Some(rate) = rate {
        let valor = map_rate(rate, tts.min_rate(), tts.normal_rate(), tts.max_rate());
        tts.set_rate(valor).map_err(|e| e.to_string())?;
    }
    tts.speak(texto, true)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Convierte un multiplicador de velocidad (1.0 = normal) al rango nativo del
/// backend. En Windows (WinRT) el rate nativo YA es un multiplicador
/// (normal = 1.0), así que `rate * normal` es exacto; en AVSpeech (normal 0.5)
/// escala razonablemente. speech-dispatcher usa un rango centrado en 0, donde
/// multiplicar no sirve — ahí interpolamos linealmente contra los extremos.
fn map_rate(rate: f32, min: f32, normal: f32, max: f32) -> f32 {
    let rate = rate.clamp(0.25, 3.0);
    let valor = if normal != 0.0 {
        rate * normal
    } else if rate >= 1.0 {
        // 1.0→normal, 3.0→max
        normal + (rate - 1.0) / 2.0 * (max - normal)
    } else {
        // 0.25→min, 1.0→normal
        normal - (1.0 - rate) / 0.75 * (normal - min)
    };
    valor.clamp(min, max)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_rate_windows_es_identidad() {
        // WinRT: min 0.5, normal 1.0, max 6.0 — el multiplicador pasa directo.
        assert_eq!(map_rate(1.0, 0.5, 1.0, 6.0), 1.0);
        assert_eq!(map_rate(2.0, 0.5, 1.0, 6.0), 2.0);
        assert_eq!(map_rate(0.5, 0.5, 1.0, 6.0), 0.5);
        // Fuera de rango se recorta al rango nativo.
        assert_eq!(map_rate(0.25, 0.5, 1.0, 6.0), 0.5);
    }

    #[test]
    fn map_rate_rango_centrado_en_cero_interpola() {
        // speech-dispatcher: -100..100, normal 0.
        assert_eq!(map_rate(1.0, -100.0, 0.0, 100.0), 0.0);
        assert_eq!(map_rate(3.0, -100.0, 0.0, 100.0), 100.0);
        assert_eq!(map_rate(0.25, -100.0, 0.0, 100.0), -100.0);
    }

    /// Humo E1: requiere voces del sistema y salida de audio; se corre a mano:
    /// `cargo test escucha_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn escucha_smoke_habla_es() {
        let manager = EscuchaManager::new();
        let voces = manager.list_voices().expect("enumerar voces");
        assert!(!voces.is_empty(), "el sistema no reporta ninguna voz TTS");
        for v in &voces {
            println!("{} | {} | {}", v.id, v.nombre, v.idioma);
        }
        let voz_es = voces.iter().find(|v| v.es_espanol);
        manager
            .speak(
                "Hola, soy ABRAX. Escucha tu código.".to_string(),
                voz_es.map(|v| v.id.clone()),
                Some(1.0),
            )
            .expect("hablar");
        std::thread::sleep(Duration::from_millis(300));
        let mut vueltas = 0;
        while manager.status().expect("status").hablando && vueltas < 150 {
            std::thread::sleep(Duration::from_millis(100));
            vueltas += 1;
        }
        assert!(vueltas > 0, "is_speaking nunca reportó true");
        assert!(vueltas < 150, "la locución no terminó en 15 s");
    }
}
