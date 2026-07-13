//! Reproducción de audio para los motores neuronales (Piper/Kokoro/Chatterbox).
//!
//! El motor del sistema (SAPI/AVSpeech) reproduce por el SO; los neuronales
//! producen PCM y lo suenan por aquí: un `rodio::OutputStream` vive confinado en
//! un hilo dedicado (`OutputStream` no es `Send`), y se le mandan buffers de
//! muestras ya **normalizadas a −3 dBFS** (Piper saturaba al 100% en el spike).

use std::path::Path;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};

/// Objetivo de normalización de pico. −3 dBFS deja margen anti-clipping.
pub const TARGET_DBFS: f32 = -3.0;

enum PlayCmd {
    Play {
        samples: Vec<f32>,
        sample_rate: u32,
        volume: f32,
    },
    Shutdown,
}

/// Servicio de reproducción con un `OutputStream` confinado a su hilo.
pub struct PlaybackService {
    tx: Mutex<Sender<PlayCmd>>,
    /// Sink actual (interrumpible). `Sink` sí es `Send+Sync`, así que stop/estado
    /// se consultan desde el handle sin pasar por el canal.
    current: Arc<Mutex<Option<rodio::Sink>>>,
}

impl PlaybackService {
    /// Crea el servicio abriendo el dispositivo de salida `device_name`
    /// (`None`/"Default" = el predeterminado). Propaga el error de apertura.
    pub fn new(device_name: Option<String>) -> Result<Arc<Self>, String> {
        let (tx, rx) = mpsc::channel::<PlayCmd>();
        let current: Arc<Mutex<Option<rodio::Sink>>> = Arc::new(Mutex::new(None));
        let current_thread = current.clone();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), String>>();

        std::thread::Builder::new()
            .name("tts-playback".into())
            .spawn(move || {
                // OutputStream no es Send: se crea DENTRO del hilo y se mantiene
                // vivo hasta Shutdown (si se dropea, el audio se corta).
                let stream = match build_stream(device_name) {
                    Ok(s) => {
                        let _ = ready_tx.send(Ok(()));
                        s
                    }
                    Err(e) => {
                        let _ = ready_tx.send(Err(e));
                        return;
                    }
                };
                let mixer = stream.mixer();
                while let Ok(cmd) = rx.recv() {
                    match cmd {
                        PlayCmd::Play {
                            samples,
                            sample_rate,
                            volume,
                        } => {
                            // Interrumpe lo que estuviera sonando.
                            if let Some(sink) =
                                current_thread.lock().ok().and_then(|mut g| g.take())
                            {
                                sink.stop();
                            }
                            let src = rodio::buffer::SamplesBuffer::new(1, sample_rate, samples);
                            let sink = rodio::Sink::connect_new(mixer);
                            sink.set_volume(volume);
                            sink.append(src);
                            if let Ok(mut g) = current_thread.lock() {
                                *g = Some(sink);
                            }
                        }
                        PlayCmd::Shutdown => break,
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        ready_rx
            .recv()
            .map_err(|e| format!("hilo de reproducción no arrancó: {e}"))??;
        Ok(Arc::new(Self {
            tx: Mutex::new(tx),
            current,
        }))
    }

    /// Encola muestras mono f32 para reproducir (interrumpe lo anterior).
    pub fn play(&self, samples: Vec<f32>, sample_rate: u32, volume: f32) -> Result<(), String> {
        self.tx
            .lock()
            .map_err(|_| "mutex de reproducción envenenado".to_string())?
            .send(PlayCmd::Play {
                samples,
                sample_rate,
                volume,
            })
            .map_err(|e| e.to_string())
    }

    /// Detiene la reproducción actual.
    pub fn stop(&self) {
        if let Some(sink) = self.current.lock().ok().and_then(|mut g| g.take()) {
            sink.stop();
        }
    }

    /// ¿Hay audio sonando ahora?
    pub fn is_playing(&self) -> bool {
        self.current
            .lock()
            .map(|g| g.as_ref().map(|s| !s.empty()).unwrap_or(false))
            .unwrap_or(false)
    }
}

impl Drop for PlaybackService {
    fn drop(&mut self) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(PlayCmd::Shutdown);
        }
    }
}

fn build_stream(device_name: Option<String>) -> Result<rodio::OutputStream, String> {
    use rodio::OutputStreamBuilder;
    let builder = match device_name {
        Some(name) if name != "Default" => {
            use cpal::traits::{DeviceTrait, HostTrait};
            let host = crate::audio_toolkit::get_cpal_host();
            let mut found = None;
            if let Ok(devices) = host.output_devices() {
                for d in devices {
                    if d.name().map(|n| n == name).unwrap_or(false) {
                        found = Some(d);
                        break;
                    }
                }
            }
            match found {
                Some(d) => OutputStreamBuilder::from_device(d).map_err(|e| e.to_string())?,
                None => {
                    log::warn!("[tts] dispositivo '{name}' no encontrado, usando el predeterminado");
                    OutputStreamBuilder::from_default_device().map_err(|e| e.to_string())?
                }
            }
        }
        _ => OutputStreamBuilder::from_default_device().map_err(|e| e.to_string())?,
    };
    builder.open_stream().map_err(|e| e.to_string())
}

/// Lee un WAV (PCM entero o float; estéreo → mono) a muestras f32 + sample_rate.
pub fn read_wav_mono_f32(path: &Path) -> Result<(Vec<f32>, u32), String> {
    let reader = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    read_wav_mono_f32_inner(reader)
}

/// Igual, pero desde bytes en memoria (los servidores neuronales devuelven WAV
/// por HTTP).
pub fn read_wav_mono_f32_from_bytes(bytes: &[u8]) -> Result<(Vec<f32>, u32), String> {
    let reader =
        hound::WavReader::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
    read_wav_mono_f32_inner(reader)
}

fn read_wav_mono_f32_inner<R: std::io::Read>(
    mut reader: hound::WavReader<R>,
) -> Result<(Vec<f32>, u32), String> {
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;
    let interleaved: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max)
                .collect()
        }
        hound::SampleFormat::Float => {
            reader.samples::<f32>().filter_map(|s| s.ok()).collect()
        }
    };
    let mono = if channels > 1 {
        interleaved
            .chunks(channels)
            .map(|c| c.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        interleaved
    };
    Ok((mono, spec.sample_rate))
}

/// Normaliza el pico de amplitud a `target_dbfs` (p.ej. −3.0). Escala hacia
/// arriba o abajo; evita el clipping observado en Piper (picos al 100%).
pub fn normalize_peak_dbfs(samples: &mut [f32], target_dbfs: f32) {
    let peak = samples.iter().fold(0.0f32, |m, &s| m.max(s.abs()));
    if peak > 0.0 {
        let target = 10f32.powf(target_dbfs / 20.0);
        let gain = target / peak;
        for s in samples.iter_mut() {
            *s *= gain;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_scales_peak_to_target() {
        // Pico 1.0 → debe bajar a ~0.708 (−3 dBFS).
        let mut s = vec![0.5, -1.0, 0.25];
        normalize_peak_dbfs(&mut s, -3.0);
        let peak = s.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!((peak - 0.707945).abs() < 1e-3, "pico={peak}");
    }

    #[test]
    fn normalize_amplifies_quiet_audio() {
        // Pico 0.1 → debe subir a ~0.708.
        let mut s = vec![0.1, -0.05];
        normalize_peak_dbfs(&mut s, -3.0);
        let peak = s.iter().fold(0.0f32, |m, &x| m.max(x.abs()));
        assert!((peak - 0.707945).abs() < 1e-3, "pico={peak}");
    }

    #[test]
    fn normalize_silence_is_noop() {
        let mut s = vec![0.0, 0.0];
        normalize_peak_dbfs(&mut s, -3.0);
        assert_eq!(s, vec![0.0, 0.0]);
    }
}
