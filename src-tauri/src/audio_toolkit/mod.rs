pub mod audio;
pub mod constants;
pub mod text;
pub mod utils;
pub mod vad;

pub use audio::{
    is_microphone_access_denied, is_no_input_device_error, list_input_devices, list_output_devices,
    read_wav_samples, save_wav_file, verify_wav_file, AudioRecorder, CpalDeviceInfo, SpectrumFrame,
    VadPolicy,
};
pub use text::{
    apply_custom_replacements, apply_custom_words, apply_dictionary_fuzzy, apply_multi_token_join,
    build_match_key, filter_transcription_output, recortar_repeticion_degenerada,
};
pub use utils::{get_cpal_host, normalizar_nivel_para_stt};
pub use vad::{SileroVad, VoiceActivityDetector};
