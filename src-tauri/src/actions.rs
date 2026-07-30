use crate::audio_feedback::{play_feedback_sound, play_feedback_sound_blocking, SoundType};
use crate::audio_toolkit::{is_microphone_access_denied, is_no_input_device_error, VadPolicy};
use crate::managers::audio::AudioRecordingManager;
use crate::managers::history::HistoryManager;
use crate::managers::model::ModelManager;
use crate::managers::transcription::StreamWorkKind;
use crate::managers::transcription::TranscriptionManager;
use crate::managers::tts::manager::TtsManager;
use crate::overlay::{
    hide_recording_overlay, show_recording_overlay, show_streaming_overlay,
    show_transcribing_overlay,
};
use crate::settings::{get_settings, AppSettings, OverlayStyle};
use crate::shortcut;
use crate::tray::{change_tray_icon, TrayIconState};
use crate::utils;
use crate::TranscriptionCoordinator;
use ferrous_opencc::{config::BuiltinConfig, OpenCC};
use log::{debug, error, info, warn};
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri::Manager;

const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(25);

/// Drop guard that notifies the [`TranscriptionCoordinator`] when the
/// transcription pipeline finishes — whether it completes normally or panics.
struct FinishGuard(AppHandle);
impl Drop for FinishGuard {
    fn drop(&mut self) {
        if let Some(c) = self.0.try_state::<TranscriptionCoordinator>() {
            c.notify_processing_finished();
        }
    }
}

// Shortcut Action Trait
pub trait ShortcutAction: Send + Sync {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str);
}

// Transcribe Action. Tuvo un campo `post_process` y una segunda entrada
// en ACTION_MAP para el atajo dedicado; los dos se fueron con la funcion
// «Post Proceso» el 29/07.
struct TranscribeAction;

/// Diarización opt-in (env `ABRAX_DIARIZE`): corre el diarizador sobre el buffer
/// 16 kHz y devuelve el texto con prefijos `[Hablante N]`, alineando cada palabra
/// de whisper (canal lateral de Fase 1) con el hablante por máximo solapamiento
/// temporal. `None` si no hay palabras con tiempo o fallan los modelos.
/// Carpeta de los modelos ONNX de diarización: `ABRAX_DIARIZE_MODELS` si está
/// (dev), si no `<app_data_dir>/diarization`.
pub(crate) fn diarization_models_dir(ah: &AppHandle) -> std::path::PathBuf {
    if let Ok(d) = std::env::var("ABRAX_DIARIZE_MODELS") {
        return std::path::PathBuf::from(d);
    }
    ah.path()
        .app_data_dir()
        .map(|d| d.join("diarization"))
        .unwrap_or_else(|_| std::path::PathBuf::from("diarization"))
}

pub(crate) fn diarize_and_label(
    samples: &[f32],
    words: &[crate::managers::transcription::TimedWord],
    models_dir: &std::path::Path,
    num_speakers: Option<usize>,
    meeting: bool,
) -> Option<String> {
    if words.is_empty() {
        return None;
    }
    let mut diar = match crate::managers::diarization::Diarizer::new(
        &models_dir.join("seg.onnx"),
        &models_dir.join("emb.onnx"),
    ) {
        Ok(d) => d,
        Err(e) => {
            warn!("diarización: no se pudieron cargar los modelos: {}", e);
            return None;
        }
    };
    match diar.diarize(samples, num_speakers, meeting) {
        Ok(segs) if !segs.is_empty() => {
            let speakers = segs
                .iter()
                .map(|s| s.speaker)
                .collect::<std::collections::BTreeSet<_>>()
                .len();
            info!(
                "diarización: {} hablante(s) en {} segmento(s), {} palabra(s) (pista={:?})",
                speakers,
                segs.len(),
                words.len(),
                num_speakers
            );
            Some(label_by_speaker(words, &segs))
        }
        Ok(_) => {
            warn!("diarización: sin segmentos de hablante; se usa el texto sin etiquetar");
            None
        }
        Err(e) => {
            warn!("diarización falló: {}", e);
            None
        }
    }
}

/// Formatea milisegundos (desde el inicio de la grabación) como `MM:SS`.
fn fmt_ms(ms: i64) -> String {
    let total = (ms.max(0) / 1000) as u64;
    format!("{:02}:{:02}", total / 60, total % 60)
}

/// Antepone `[MM:SS] [Hablante N]` en cada cambio de turno (marca de minuto estilo
/// minuta de reunión): por cada palabra elige el hablante con mayor solapamiento
/// temporal.
fn label_by_speaker(
    words: &[crate::managers::transcription::TimedWord],
    segs: &[crate::managers::diarization::SpeakerSegment],
) -> String {
    let speaker_at = |t0: i64, t1: i64| -> usize {
        let (mut best, mut best_ov) = (0usize, 0i64);
        for s in segs {
            let ov = (t1.min(s.t1_ms) - t0.max(s.t0_ms)).max(0);
            if ov > best_ov {
                best_ov = ov;
                best = s.speaker;
            }
        }
        best
    };
    let mut out = String::new();
    let mut cur: Option<usize> = None;
    for w in words {
        let txt = w.text.trim();
        if txt.is_empty() {
            continue;
        }
        let spk = speaker_at(w.t0_ms, w.t1_ms).max(1);
        if Some(spk) != cur {
            if !out.is_empty() {
                out.push_str("\n\n");
            }
            out.push_str(&format!("[{}] [Hablante {}] ", fmt_ms(w.t0_ms), spk));
            cur = Some(spk);
        } else if !out.ends_with(char::is_whitespace) {
            out.push(' ');
        }
        out.push_str(txt);
    }
    out
}

async fn complete_unless_cancelled<F, C>(operation: F, is_cancelled: C) -> Option<F::Output>
where
    F: Future,
    C: Fn() -> bool,
{
    tokio::pin!(operation);

    loop {
        if is_cancelled() {
            return None;
        }

        if let Ok(result) =
            tokio::time::timeout(CANCELLATION_POLL_INTERVAL, operation.as_mut()).await
        {
            return Some(result);
        }
    }
}

fn should_use_streaming_overlay(style: OverlayStyle, is_streaming: bool) -> bool {
    style == OverlayStyle::Live && is_streaming
}

async fn maybe_convert_chinese_variant(
    effective_language: &str,
    transcription: &str,
) -> Option<String> {
    // Gate on the language the model actually transcribed in (the effective
    // language), not the persisted intent. A leftover zh-Hans/zh-Hant intent
    // from a previously selected model must not run OpenCC S2T/T2S over output a
    // non-Chinese model produced — that would silently rewrite any shared CJK
    // characters (e.g. Japanese kanji) in the result.
    let is_simplified = effective_language == "zh-Hans";
    let is_traditional = effective_language == "zh-Hant";

    if !is_simplified && !is_traditional {
        debug!("effective language is not Simplified or Traditional Chinese; skipping conversion");
        return None;
    }

    debug!(
        "Starting Chinese variant conversion using OpenCC for language: {}",
        effective_language
    );

    // Use OpenCC to convert based on selected language
    let config = if is_simplified {
        // Convert Traditional Chinese to Simplified Chinese
        BuiltinConfig::Tw2sp
    } else {
        // Convert Simplified Chinese to Traditional Chinese
        BuiltinConfig::S2tw
    };

    match OpenCC::from_config(config) {
        Ok(converter) => {
            let converted = converter.convert(transcription);
            debug!(
                "OpenCC translation completed. Input length: {}, Output length: {}",
                transcription.len(),
                converted.len()
            );
            Some(converted)
        }
        Err(e) => {
            error!("Failed to initialize OpenCC converter: {}. Falling back to original transcription.", e);
            None
        }
    }
}

pub(crate) struct ProcessedTranscription {
    pub final_text: String,
    pub post_processed_text: Option<String>,
    pub post_process_prompt: Option<String>,
}

/// Resolve the persisted language *intent* into the language the currently-loaded
/// model will actually use — the same capability-aware coercion the transcription
/// paths apply (see [`crate::managers::model::effective_language`]). Post-processing
/// resolves it independently so it agrees with the language the transcription ran
/// in, without threading a value through the pipeline.
fn resolve_effective_language(app: &AppHandle, settings: &AppSettings) -> String {
    let tm = app.state::<Arc<TranscriptionManager>>();
    let model_manager = app.state::<Arc<ModelManager>>();
    let active_model = tm
        .get_current_model()
        .unwrap_or_else(|| settings.selected_model.clone());
    match model_manager.get_model_info(&active_model) {
        Some(info) => crate::managers::model::effective_language(
            &settings.selected_language,
            &info.supported_languages,
            info.supports_language_detection,
        ),
        None => settings.selected_language.clone(),
    }
}

/// Transforma la transcripción cruda en el texto que se inserta: conversión de
/// variante china + la capa de corrección determinista.
///
/// Ya no recibe `post_process: bool`: el paso de post-proceso por LLM (la función
/// «Post Proceso», con API BYOK) se retiró el 29/07.
pub(crate) async fn process_transcription_output(
    app: &AppHandle,
    transcription: &str,
) -> ProcessedTranscription {
    let settings = get_settings(app);
    let mut final_text = transcription.to_string();

    // Resolve the language the transcription actually ran in (the persisted
    // intent coerced against the loaded model's capabilities) so OpenCC keys off
    // the effective language rather than a possibly-stale intent.
    let effective_language = resolve_effective_language(app, &settings);
    if let Some(converted_text) =
        maybe_convert_chinese_variant(&effective_language, transcription).await
    {
        final_text = converted_text;
    }

    // Corrección local (módulo `correccion`): con el motor por defecto
    // (`desactivado`) es passthrough byte a byte; solo transforma si el usuario
    // la activó en ajustes. Todo determinista y síncrono desde que se retiró el
    // «Pulido con IA» — ya no hay sidecar que resolver ni nada que esperar.
    final_text = crate::correccion::procesar(&final_text, &settings);

    // `post_processed_text` guarda el texto TRANSFORMADO cuando alguna capa lo
    // cambió, para que el historial pueda mostrar crudo vs. final. Ese uso NO era
    // del LLM —ya existía para la conversión china y la corrección determinista—
    // así que sobrevive a la retirada del «Post Proceso»: antes era la rama
    // `else`, ahora es la regla.
    let post_processed_text = if final_text != transcription {
        Some(final_text.clone())
    } else {
        None
    };

    ProcessedTranscription {
        final_text,
        post_processed_text,
        // El prompt solo tenía sentido con el LLM. Se conserva el campo porque
        // mapea a una columna de SQLite que NO se toca (las migraciones son
        // append-only e indexadas por `user_version`: quitar una corrompería las
        // bases existentes). Queda siempre `None`.
        post_process_prompt: None,
    }
}

impl ShortcutAction for TranscribeAction {
    fn start(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        let start_time = Instant::now();
        debug!("TranscribeAction::start called for binding: {}", binding_id);

        // Load model in the background
        let tm = app.state::<Arc<TranscriptionManager>>();
        let rm = app.state::<Arc<AudioRecordingManager>>();

        // Si la sección Escucha está leyendo en voz alta, córtala al empezar a
        // dictar: el TTS no debe competir con el dictado (dos audios) ni que el
        // micrófono capte la propia lectura. `stop()` es instantáneo y no-op si
        // no hay nada sonando.
        let _ = app.state::<Arc<TtsManager>>().stop();

        // Memoria en el sitio: en este instante el foco sigue en el campo
        // donde el usuario dicta — releerlo (accesibilidad, local) y aprender
        // de las correcciones que hizo sobre el dictado anterior. En hilo
        // aparte: la lectura COM no debe retrasar el inicio de la grabación.
        {
            let app_memoria = app.clone();
            std::thread::spawn(move || {
                crate::memoria_en_sitio::aprender_del_campo(&app_memoria);
            });
        }

        // Load ASR model and VAD model in parallel
        let kickoff_started = Instant::now();
        tm.initiate_model_load();
        let rm_clone = Arc::clone(&rm);
        std::thread::spawn(move || {
            if let Err(e) = rm_clone.preload_vad() {
                debug!("VAD pre-load failed: {}", e);
            }
        });
        let kickoff_elapsed = kickoff_started.elapsed();

        let binding_id = binding_id.to_string();
        let tray_started = Instant::now();
        change_tray_icon(app, TrayIconState::Recording);
        let tray_elapsed = tray_started.elapsed();

        // Get the microphone mode to determine audio feedback timing
        let plan_started = Instant::now();
        let settings = get_settings(app);
        let is_always_on = settings.always_on_microphone;

        let selected_model_info = app
            .state::<Arc<ModelManager>>()
            .get_model_info(&settings.selected_model);

        // Use the app-facing model capability as the single pre-recording source
        // for live streaming decisions. Unknown support is represented as false
        // until the model registry is updated by discovery or runtime load.
        let model_supports_streaming = selected_model_info
            .as_ref()
            .map(|m| m.supports_streaming)
            .unwrap_or(false);
        // Mantener el streaming en vivo SIEMPRE que el modelo lo soporte (texto +
        // PAL/MIN en tiempo real, aunque Hablantes esté activo). La diarización
        // necesita timestamps por palabra que el streaming no da; se resuelven
        // RE-TRANSCRIBIENDO el buffer en batch AL FINALIZAR (ver el bloque de
        // diarización en `Ok(transcription)`), sin sacrificar el preview en vivo.
        let use_streaming = model_supports_streaming;
        let vad_policy = if !settings.vad_enabled {
            VadPolicy::Disabled
        } else if use_streaming {
            VadPolicy::Streaming
        } else {
            VadPolicy::Offline
        };
        if use_streaming {
            tm.start_stream();
        }
        let plan_elapsed = plan_started.elapsed();

        // Sizing the overlay follows the same advertised capability. A model that
        // doesn't stream (or whose capability is not known yet) gets the compact
        // pill instead of an oversized transparent live window.
        let overlay_started = Instant::now();
        match settings.overlay_style {
            OverlayStyle::Live if use_streaming => show_streaming_overlay(app),
            OverlayStyle::Live | OverlayStyle::Minimal | OverlayStyle::Esfera => {
                show_recording_overlay(app)
            }
            OverlayStyle::None => {} // show_overlay_state no-ops on None anyway
        }
        // Everything above runs before capture can begin, so each span here is
        // added keypress->capture latency.
        debug!(
            "start-path pre-recording steps: model_kickoff={:?} tray={:?} settings+stream_plan={:?} overlay={:?}",
            kickoff_elapsed,
            tray_elapsed,
            plan_elapsed,
            overlay_started.elapsed()
        );
        debug!("Microphone mode - always_on: {}", is_always_on);

        let mut recording_error: Option<String> = None;
        if is_always_on {
            // Always-on mode: Play audio feedback immediately, then apply mute after sound finishes
            debug!("Always-on mode: Playing audio feedback immediately");
            let rm_clone = Arc::clone(&rm);
            let app_clone = app.clone();
            // The blocking helper exits immediately if audio feedback is disabled,
            // so we can always reuse this thread to ensure mute happens right after playback.
            std::thread::spawn(move || {
                play_feedback_sound_blocking(&app_clone, SoundType::Start);
                rm_clone.apply_mute();
            });

            if let Err(e) = rm.try_start_recording(&binding_id, vad_policy) {
                debug!("Recording failed: {}", e);
                recording_error = Some(e);
            }
        } else {
            // On-demand mode: Start recording first, then play audio feedback, then apply mute
            // This allows the microphone to be activated before playing the sound
            debug!("On-demand mode: Starting recording first, then audio feedback");
            let recording_start_time = Instant::now();
            match rm.try_start_recording(&binding_id, vad_policy) {
                Ok(()) => {
                    debug!("Recording started in {:?}", recording_start_time.elapsed());
                    // Small delay to ensure microphone stream is active
                    let app_clone = app.clone();
                    let rm_clone = Arc::clone(&rm);
                    std::thread::spawn(move || {
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        debug!("Handling delayed audio feedback/mute sequence");
                        // Helper handles disabled audio feedback by returning early, so we reuse it
                        // to keep mute sequencing consistent in every mode.
                        play_feedback_sound_blocking(&app_clone, SoundType::Start);
                        rm_clone.apply_mute();
                    });
                }
                Err(e) => {
                    debug!("Failed to start recording: {}", e);
                    recording_error = Some(e);
                }
            }
        }

        if recording_error.is_none() {
            // Dynamically register the cancel shortcut in a separate task to avoid deadlock
            shortcut::register_cancel_shortcut(app);
        } else {
            // Starting failed (for example due to blocked microphone permissions).
            // Revert UI state so we don't stay stuck in the recording overlay.
            tm.cancel_stream();
            hide_recording_overlay(app);
            change_tray_icon(app, TrayIconState::Idle);
            if let Some(err) = recording_error {
                let kind = if is_microphone_access_denied(&err) {
                    crate::user_alerts::AlertKind::RecordingPermissionDenied
                } else if is_no_input_device_error(&err) {
                    crate::user_alerts::AlertKind::RecordingNoDevice
                } else {
                    crate::user_alerts::AlertKind::Recording
                };
                crate::user_alerts::alert(app, kind, Some(err));
            }
        }

        debug!(
            "TranscribeAction::start completed in {:?}",
            start_time.elapsed()
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, _shortcut_str: &str) {
        // Unregister the cancel shortcut when transcription stops
        shortcut::unregister_cancel_shortcut(app);

        let stop_time = Instant::now();
        debug!("TranscribeAction::stop called for binding: {}", binding_id);

        let ah = app.clone();
        let rm = Arc::clone(&app.state::<Arc<AudioRecordingManager>>());
        let tm = Arc::clone(&app.state::<Arc<TranscriptionManager>>());
        let hm = Arc::clone(&app.state::<Arc<HistoryManager>>());

        change_tray_icon(app, TrayIconState::Transcribing);
        // Stop should give immediate visual feedback. Live streaming can keep
        // the larger panel, but it still switches from listening to a working
        // spinner while the stream finalizes. Non-streaming paths use the
        // compact transcribing pill (None no-ops in show_*).
        let style = get_settings(app).overlay_style;
        // Capture this before finalizing the stream so every later working state
        // targets the same overlay that was shown for this transcription.
        let use_streaming_overlay = should_use_streaming_overlay(style, tm.is_streaming());
        if use_streaming_overlay {
            tm.emit_stream_working(StreamWorkKind::Transcribing);
        } else {
            show_transcribing_overlay(app);
        }

        // Unmute before playing audio feedback so the stop sound is audible
        rm.remove_mute();

        // Play audio feedback for recording stop
        play_feedback_sound(app, SoundType::Stop);

        let binding_id = binding_id.to_string(); // Clone binding_id for the async task
        let cancel_generation = rm.cancel_generation();

        tauri::async_runtime::spawn(async move {
            let _guard = FinishGuard(ah.clone());
            debug!(
                "Starting async transcription task for binding: {}",
                binding_id
            );

            let stop_recording_time = Instant::now();
            if let Some(samples) = rm.stop_recording(&binding_id, cancel_generation) {
                debug!(
                    "Recording stopped and samples retrieved in {:?}, sample count: {}",
                    stop_recording_time.elapsed(),
                    samples.len()
                );

                if rm.was_cancelled_since(cancel_generation) {
                    debug!("Transcription operation cancelled after recording stop");
                    tm.cancel_stream();
                    hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                    return;
                }

                if samples.is_empty() {
                    debug!("Recording produced no audio samples; skipping persistence");
                    // No llegó audio. Hay DOS causas y culpar a la equivocada le
                    // cuesta al usuario diez minutos revisando su micrófono: en la
                    // medición del 26/07 pasó exactamente eso. Si la grabación duró
                    // menos que un parpadeo, el usuario soltó la tecla antes de
                    // hablar — el micrófono no tiene nada que ver.
                    const DEMASIADO_CORTA: Duration = Duration::from_millis(300);
                    let duracion = rm.last_recording_duration();
                    let solto_enseguida = duracion.is_some_and(|d| d < DEMASIADO_CORTA);

                    if solto_enseguida {
                        debug!(
                            "Recording lasted {:?} (< {:?}): user released the key too early",
                            duracion, DEMASIADO_CORTA
                        );
                        // El detalle lo compone el frontend, que es quien sabe el
                        // idioma y sabe formatear el atajo real por plataforma.
                        crate::user_alerts::alert(
                            &ah,
                            crate::user_alerts::AlertKind::RecordingTooShort,
                            None,
                        );
                    } else {
                        // Si estaba en «Audio del sistema», casi seguro no había nada
                        // sonando (ese modo escucha los parlantes, no el micrófono).
                        let detail = if get_settings(&ah).capture_system_audio {
                            "No se detectó audio del sistema. ¿Está sonando la reunión? El chip «Audio sistema» escucha lo que suena en tu PC, no tu micrófono."
                        } else {
                            "No se detectó audio del micrófono. Revisá que esté conectado, con permiso y sin silenciar."
                        };
                        crate::user_alerts::alert(
                            &ah,
                            crate::user_alerts::AlertKind::RecordingNoAudio,
                            Some(detail.to_string()),
                        );
                    }
                    // Tear down any streaming worker so its channel doesn't leak
                    // and block the next start_stream.
                    tm.cancel_stream();
                    hide_recording_overlay(&ah);
                    change_tray_icon(&ah, TrayIconState::Idle);
                } else {
                    // Save WAV concurrently with transcription
                    let sample_count = samples.len();
                    let file_name = format!("handy-{}.wav", chrono::Utc::now().timestamp());
                    let wav_path = hm.recordings_dir().join(&file_name);
                    let wav_path_for_verify = wav_path.clone();
                    let samples_for_wav = samples.clone();
                    let wav_handle = tauri::async_runtime::spawn_blocking(move || {
                        crate::audio_toolkit::save_wav_file(&wav_path, &samples_for_wav)
                    });

                    // Transcribe concurrently with WAV save. If a live stream was
                    // running, finalize it and use its text (all audio was already
                    // fed to the stream); otherwise batch-transcribe the samples.
                    // Diarización opt-in (setting `diarization_enabled`, o env
                    // ABRAX_DIARIZE para dev/CLI): clona el buffer ANTES de que
                    // transcribe() lo consuma, para poder diarizarlo después.
                    let diar_samples = if get_settings(&ah).diarization_enabled
                        || std::env::var("ABRAX_DIARIZE").is_ok()
                    {
                        Some(samples.clone())
                    } else {
                        None
                    };
                    let transcription_time = Instant::now();
                    // `streamed` marks text that came from a live stream, whose
                    // words the streaming path already fed to the Esfera overlay
                    // one by one — so the batch word emission below must skip it
                    // to avoid sending them twice.
                    let (transcription_result, streamed) = match tm.finalize_stream() {
                        // A finalized stream with usable text wins. An empty result
                        // (no active stream, produced nothing, or a finalize error
                        // after the engine was returned) falls back to a full batch
                        // transcription of the same audio. A finalize timeout is
                        // surfaced instead — the worker may still hold the engine,
                        // so a batch fallback would contend with it.
                        Ok(Some(text)) if !text.trim().is_empty() => (Ok(text), true),
                        Ok(_) => (tm.transcribe(samples), false),
                        Err(err) => (Err(err), false),
                    };

                    // Await WAV save and verify
                    let wav_saved = match wav_handle.await {
                        Ok(Ok(())) => {
                            match crate::audio_toolkit::verify_wav_file(
                                &wav_path_for_verify,
                                sample_count,
                            ) {
                                Ok(()) => true,
                                Err(e) => {
                                    error!("WAV verification failed: {}", e);
                                    false
                                }
                            }
                        }
                        Ok(Err(e)) => {
                            error!("Failed to save WAV file: {}", e);
                            false
                        }
                        Err(e) => {
                            error!("WAV save task panicked: {}", e);
                            false
                        }
                    };

                    if rm.was_cancelled_since(cancel_generation) {
                        debug!("Transcription operation cancelled before output handling");
                        hide_recording_overlay(&ah);
                        change_tray_icon(&ah, TrayIconState::Idle);
                        return;
                    }

                    match transcription_result {
                        Ok(transcription) => {
                            // Diarización (opt-in): etiqueta el texto por hablante al
                            // SOLTAR (el preview en vivo ya se mostró vía streaming).
                            // Necesita timestamps por palabra, que solo da el batch; si
                            // veníamos de streaming, `last_words` está vacío, así que
                            // RE-TRANSCRIBIMOS el buffer en batch aquí para obtenerlos.
                            let transcription = match diar_samples {
                                Some(ds) => {
                                    let mut words = tm.take_last_words().unwrap_or_default();
                                    if words.is_empty() {
                                        // Path streaming (o motor sin canal lateral de
                                        // tiempos): re-transcribe en batch para poblar
                                        // last_words. El overlay ya muestra «trabajando».
                                        let _ = tm.transcribe(ds.clone());
                                        words = tm.take_last_words().unwrap_or_default();
                                    }
                                    if words.is_empty() {
                                        transcription
                                    } else {
                                        let dir = diarization_models_dir(&ah);
                                        let s = get_settings(&ah);
                                        // Pista de nº de hablantes: 0 = auto, N = TOPE.
                                        let num_speakers = match s.diarization_num_speakers {
                                            0 => None,
                                            n => Some(n as usize),
                                        };
                                        // Modo reunión (audio del sistema) → segmentación
                                        // fina para captar voces breves de la reunión.
                                        let meeting = s.capture_system_audio;
                                        let base = transcription.clone();
                                        // Diariza en un hilo bloqueante (carga ONNX + inferencia).
                                        tauri::async_runtime::spawn_blocking(move || {
                                            diarize_and_label(
                                                &ds,
                                                &words,
                                                &dir,
                                                num_speakers,
                                                meeting,
                                            )
                                        })
                                        .await
                                        .ok()
                                        .flatten()
                                        .unwrap_or(base)
                                    }
                                }
                                None => transcription,
                            };
                            // Privacy: log timing and length, never the dictated
                            // text, so handy.log stays free of transcript bodies
                            // by default (S3).
                            debug!(
                                "Transcription completed in {:?} ({} chars)",
                                transcription_time.elapsed(),
                                transcription.chars().count()
                            );

                            // Esfera "palabras" mode with a non-streaming model:
                            // the whole transcript lands at once, so split it into
                            // words and hand them to the sphere (the overlay paces
                            // their arrival). Streamed text already flew in word by
                            // word, so skip it here to avoid duplicates. Note: with
                            // a batch model the overlay hides shortly after paste,
                            // so the visible effect is brief — the words mode is
                            // designed for streaming models, where words arrive live
                            // while recording.
                            if !streamed && tm.esfera_words_enabled() {
                                let words: Vec<String> = transcription
                                    .split_whitespace()
                                    .map(str::to_string)
                                    .collect();
                                tm.emit_transcript_words(words);
                            }

                            let Some(processed) = complete_unless_cancelled(
                                process_transcription_output(&ah, &transcription),
                                || rm.was_cancelled_since(cancel_generation),
                            )
                            .await
                            else {
                                debug!("Transcription operation cancelled during output handling");
                                hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            };

                            if rm.was_cancelled_since(cancel_generation) {
                                debug!("Transcription operation cancelled before paste");
                                hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            }

                            // Save to history if WAV was saved
                            if wav_saved {
                                if let Err(err) = hm.save_entry(
                                    file_name,
                                    transcription,
                                    // `post_process_requested`: siempre false,
                                    // la funcion se retiro. La columna se
                                    // conserva (migraciones append-only).
                                    false,
                                    processed.post_processed_text.clone(),
                                    processed.post_process_prompt.clone(),
                                ) {
                                    error!("Failed to save history entry: {}", err);
                                }
                            }

                            if processed.final_text.is_empty() {
                                // Se grabó y el motor terminó bien, pero no salió ni
                                // una palabra. Esta rama ocultaba el overlay y no
                                // decía NADA: para el usuario era «apreté el atajo y
                                // no pasó nada», sin una sola pista. Ahora avisa.
                                debug!("Transcription produced no text; alerting the user");
                                // Con «Audio del sistema» y un modelo que se
                                // queda corto, «no reconocí palabras» es
                                // verdadero pero inútil: el 29/07 el audio
                                // estaba a −14 dBFS y el usuario concluyó que
                                // la captura no funcionaba. Aquí sí sabemos la
                                // causa, así que se nombra.
                                let s_vacio = get_settings(&ah);
                                let corto = crate::managers::model::ModelManager::se_queda_corto_para_sistema(
                                    &s_vacio.selected_model,
                                );
                                let kind = if s_vacio.capture_system_audio && corto {
                                    crate::user_alerts::AlertKind::SistemaSinModeloApto
                                } else {
                                    crate::user_alerts::AlertKind::TranscriptionEmpty
                                };
                                crate::user_alerts::alert(&ah, kind, None);
                                hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                            } else {
                                let ah_clone = ah.clone();
                                let paste_time = Instant::now();
                                let final_text = processed.final_text;
                                let rm_for_paste = Arc::clone(&rm);
                                let tm_for_paste = Arc::clone(&tm);
                                ah.run_on_main_thread(move || {
                                    if rm_for_paste.was_cancelled_since(cancel_generation) {
                                        debug!("Transcription operation cancelled before paste");
                                        hide_recording_overlay(&ah_clone);
                                        change_tray_icon(&ah_clone, TrayIconState::Idle);
                                        return;
                                    }

                                    let texto_para_memoria = final_text.clone();
                                    match crate::clipboard::paste(final_text, ah_clone.clone()) {
                                        Ok(()) => {
                                            debug!(
                                                "Text pasted successfully in {:?}",
                                                paste_time.elapsed()
                                            );
                                            // Memoria en el sitio: recordar QUÉ
                                            // se tipeó y DÓNDE, para aprender de
                                            // las correcciones al próximo dictado.
                                            crate::memoria_en_sitio::registrar_dictado(
                                                &texto_para_memoria,
                                            );
                                        }
                                        Err(e) => {
                                            error!("Failed to paste transcription: {}", e);
                                            crate::user_alerts::alert(
                                                &ah_clone,
                                                crate::user_alerts::AlertKind::Paste,
                                                None,
                                            );
                                        }
                                    }
                                    // Modo palabras de la esfera: el overlay se
                                    // queda lo justo para que las palabras del
                                    // finalize completen su ciclo (el texto ya
                                    // se pegó; la ventana no roba foco). En
                                    // cualquier otro modo el linger es cero y
                                    // el hide es inmediato, como siempre.
                                    let linger = if tm_for_paste.esfera_words_enabled() {
                                        tm_for_paste.esfera_words_linger()
                                    } else {
                                        std::time::Duration::ZERO
                                    };
                                    crate::overlay::hide_recording_overlay_after(&ah_clone, linger);
                                    change_tray_icon(&ah_clone, TrayIconState::Idle);
                                })
                                .unwrap_or_else(|e| {
                                    error!("Failed to run paste on main thread: {:?}", e);
                                    hide_recording_overlay(&ah);
                                    change_tray_icon(&ah, TrayIconState::Idle);
                                });
                            }
                        }
                        Err(err) => {
                            if rm.was_cancelled_since(cancel_generation) {
                                debug!(
                                    "Transcription operation cancelled after transcription error"
                                );
                                hide_recording_overlay(&ah);
                                change_tray_icon(&ah, TrayIconState::Idle);
                                return;
                            }

                            error!("Transcription failed: {}", err);
                            // Surface the failure through the single alert
                            // channel (toast + centro + notificación nativa si
                            // la ventana está oculta). El mensaje completo
                            // también queda en handy.log por la línea de arriba.
                            crate::user_alerts::alert(
                                &ah,
                                crate::user_alerts::AlertKind::Transcription,
                                Some(err.to_string()),
                            );
                            // Save entry with empty text so user can retry
                            if wav_saved {
                                if let Err(save_err) = hm.save_entry(
                                    file_name,
                                    String::new(),
                                    // `post_process_requested`: siempre false,
                                    // la funcion se retiro. La columna se
                                    // conserva (migraciones append-only).
                                    false,
                                    None,
                                    None,
                                ) {
                                    error!("Failed to save failed history entry: {}", save_err);
                                }
                            }
                            hide_recording_overlay(&ah);
                            change_tray_icon(&ah, TrayIconState::Idle);
                        }
                    }
                }
            } else {
                debug!("No samples retrieved from recording stop");
                // Tear down any streaming worker so its channel doesn't leak.
                tm.cancel_stream();
                hide_recording_overlay(&ah);
                change_tray_icon(&ah, TrayIconState::Idle);
            }
        });

        debug!(
            "TranscribeAction::stop completed in {:?}",
            stop_time.elapsed()
        );
    }
}

// Cancel Action
struct CancelAction;

/// Lee en voz alta lo que el usuario tenga SELECCIONADO en cualquier aplicación.
///
/// Es un TOGGLE: si ya está leyendo, la pulsación calla. Si no, captura la
/// selección de la ventana en foco y la lee con la voz elegida en Escucha.
///
/// La captura va por el portapapeles (copiar → leer → restaurar) porque no hay
/// forma portable de leer la selección de otra app sin copiar; ver
/// [`crate::clipboard::leer_seleccion`], que garantiza la restauración (R5).
///
/// Todo el trabajo va a una tarea aparte: la acción del atajo NO puede bloquear
/// el hilo que atiende el teclado, o la pulsación se sentiría pegajosa.
struct LeerSeleccionAction;

/// ¿Hay una lectura EN CURSO pedida por el atajo?
///
/// No basta con preguntarle al motor si está hablando, y esa fue una carrera real
/// (encontrada el 30/07 probando con voz neuronal): entre pulsar el atajo y que
/// empiece a sonar, un motor neuronal tarda SEGUNDOS sintetizando, y en esa
/// ventana `hablando` es `false`. Pulsar otra vez para callar no callaba: capturaba
/// la selección otra vez y arrancaba una SEGUNDA lectura encima.
///
/// Esta bandera se levanta al pedir la lectura y se baja al terminarla, así que
/// cubre también el tramo de síntesis. El toggle pregunta por ella, no por el
/// motor.
static LEYENDO: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

impl ShortcutAction for LeerSeleccionAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        use std::sync::atomic::Ordering;

        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            let Some(tts) = app.try_state::<Arc<TtsManager>>() else {
                warn!("leer selección: el motor de voz no está inicializado");
                return;
            };
            let tts = tts.inner().clone();

            // Toggle. Se pregunta por NUESTRA bandera además del motor: la bandera
            // cubre el tramo de síntesis (donde el motor dice que no habla) y el
            // motor cubre el caso de que algo la dejara colgada.
            let leyendo =
                LEYENDO.load(Ordering::SeqCst) || matches!(tts.status(), Ok(e) if e.hablando);
            if leyendo {
                LEYENDO.store(false, Ordering::SeqCst);
                if let Err(e) = tts.stop() {
                    warn!("leer selección: no se pudo detener la lectura: {e}");
                }
                return;
            }
            LEYENDO.store(true, Ordering::SeqCst);

            // La bandera se baja en TODAS las salidas. Si una rama de error se
            // olvidara de bajarla, el atajo quedaría creyendo que sigue leyendo y
            // la siguiente pulsación intentaría callar algo que no suena: el atajo
            // quedaría muerto hasta reiniciar. El guard lo hace por construcción.
            struct BajarAlSalir;
            impl Drop for BajarAlSalir {
                fn drop(&mut self) {
                    LEYENDO.store(false, std::sync::atomic::Ordering::SeqCst);
                }
            }
            let _guard = BajarAlSalir;

            // Traza del camino completo. Una lectura correcta no escribía NADA en
            // el log, así que ante «no funciona» no había forma de saber en qué
            // paso se rompió. Sin contenido del dictado, solo longitudes (S3).
            let t0 = Instant::now();
            info!("[leer] atajo pulsado, capturando selección");

            let seleccion = match crate::clipboard::leer_seleccion(&app) {
                Ok(Some(t)) => {
                    info!(
                        "[leer] selección capturada: {} chars en {:?}",
                        t.chars().count(),
                        t0.elapsed()
                    );
                    t
                }
                Ok(None) => {
                    // Sin selección no se dice nada y no se molesta con un error:
                    // pulsar el atajo sin seleccionar es un accidente común.
                    info!("[leer] no había nada seleccionado ({:?})", t0.elapsed());
                    return;
                }
                Err(e) => {
                    warn!("[leer] no se pudo capturar la selección: {e}");
                    return;
                }
            };

            // DESENVOLVER antes de leer. Un PDF no guarda párrafos, guarda líneas
            // colocadas en la página: al copiar, cada línea VISUAL trae su salto y
            // el motor lo lee como fin de frase — una pausa cada seis palabras.
            // Word no sufre esto porque copia el párrafo entero en una línea, y por
            // eso ahí sonaba bien y en PDF no. Reportado el 30/07 leyendo una ley.
            let seleccion = crate::managers::escucha::preproceso::desenvolver_lineas(&seleccion);

            // Misma voz y mismos ajustes de velocidad/tono que el panel Escucha:
            // el atajo no es un modo aparte, es el mismo lector.
            let settings = get_settings(&app);
            let voz = settings.escucha_voz_prosa.clone();
            let velocidad = settings.tts_velocidad;
            let tono = settings.tts_tono;

            // `speak` bloquea hasta terminar de sintetizar Y de reproducir, así que
            // va a un hilo de bloqueo y no al ejecutor async. Cuando vuelve, la
            // lectura acabó y el guard baja la bandera.
            info!("[leer] hablando con voz {voz:?} tras {:?}", t0.elapsed());
            let _ = tauri::async_runtime::spawn_blocking(move || {
                if let Err(e) = tts.speak(seleccion, voz, Some(velocidad), Some(tono)) {
                    warn!("[leer] la síntesis falló: {e}");
                }
            })
            .await;
            info!("[leer] lectura terminada, total {:?}", t0.elapsed());
        });
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nada al soltar: el toggle vive en `start`. Si se detuviera aquí, un
        // atajo pulsado y soltado (lo normal) callaría al instante.
    }
}

impl ShortcutAction for CancelAction {
    fn start(&self, app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        utils::cancel_current_operation(app);
    }

    fn stop(&self, _app: &AppHandle, _binding_id: &str, _shortcut_str: &str) {
        // Nothing to do on stop for cancel
    }
}

// Test Action
struct TestAction;

impl ShortcutAction for TestAction {
    fn start(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Started - {} (App: {})", // Changed "Pressed" to "Started" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }

    fn stop(&self, app: &AppHandle, binding_id: &str, shortcut_str: &str) {
        log::info!(
            "Shortcut ID '{}': Stopped - {} (App: {})", // Changed "Released" to "Stopped" for consistency
            binding_id,
            shortcut_str,
            app.package_info().name
        );
    }
}

// Static Action Map
/// Acciones por id de atajo.
///
/// **INVARIANTE: todo id de `settings::bindings` necesita su entrada aquí.** Un
/// binding sin acción se registra en el sistema operativo y no hace nada: un
/// atajo global fantasma, que le roba la combinación al resto de las apps para
/// nada. Es peor que no tenerlo.
///
/// No hay test que lo compruebe, y no por descuido: referenciar `ACTION_MAP`
/// desde cualquier test de esta crate impide que arranque el binario de test en
/// Windows (`STATUS_ENTRYPOINT_NOT_FOUND`) porque arrastra dependencias nativas
/// que el ejecutable de test no tiene al lado. Se sostiene leyendo las dos listas.
pub static ACTION_MAP: Lazy<HashMap<String, Arc<dyn ShortcutAction>>> = Lazy::new(|| {
    let mut map = HashMap::new();
    map.insert(
        "transcribe".to_string(),
        Arc::new(TranscribeAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "cancel".to_string(),
        Arc::new(CancelAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "leer_seleccion".to_string(),
        Arc::new(LeerSeleccionAction) as Arc<dyn ShortcutAction>,
    );
    map.insert(
        "test".to_string(),
        Arc::new(TestAction) as Arc<dyn ShortcutAction>,
    );
    map
});

#[cfg(test)]
mod tests {
    use super::{complete_unless_cancelled, should_use_streaming_overlay};
    use crate::settings::OverlayStyle;
    use std::future;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn completed_operation_returns_its_output() {
        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::ready("done"),
            || false,
        ));

        assert_eq!(result, Some("done"));
    }

    #[test]
    fn pending_operation_stops_after_cancellation() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let cancelled_for_thread = Arc::clone(&cancelled);
        let cancel_thread = thread::spawn(move || {
            thread::sleep(Duration::from_millis(10));
            cancelled_for_thread.store(true, Ordering::Release);
        });

        let result = tauri::async_runtime::block_on(complete_unless_cancelled(
            future::pending::<()>(),
            || cancelled.load(Ordering::Acquire),
        ));

        cancel_thread.join().unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn live_overlay_uses_streaming_states_only_for_streaming_models() {
        assert!(should_use_streaming_overlay(OverlayStyle::Live, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::Live, false));
        assert!(!should_use_streaming_overlay(OverlayStyle::Minimal, true));
        assert!(!should_use_streaming_overlay(OverlayStyle::None, true));
    }
}
