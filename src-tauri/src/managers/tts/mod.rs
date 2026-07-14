//! Motor TTS adaptativo 100% local para Abrax (Fase 2 de "Escucha tu Código").
//!
//! **Envuelve** el manager de Escucha existente (`managers::escucha`, motor del
//! sistema por el crate `tts`) con un registro multi-motor: detecta el hardware
//! (`hardware`), recomienda el mejor motor **local** que el equipo soporte
//! (`recommend`), define el contrato común (`engine`) y enruta la síntesis
//! (`manager`). Motores: sistema (`system`), Piper (`piper`) y Kokoro
//! (`kokoro`, servidor Python local). La descarga con checksum vive en
//! `download` y la reproducción neuronal (normalizada a −3 dBFS) en `playback`.
//!
//! Reglas de oro: **todo es local, siempre hay un fallback que funciona, ninguna
//! rama cae a la nube.**

// Núcleo siempre compilado: contrato, enrutado, requisitos, motor del Sistema y
// los TIPOS de hardware (la detección con wgpu vive tras `advanced-tts`).
pub mod engine;
pub mod hardware;
pub mod manager;
pub mod recommend;
pub mod registry;
pub mod system;

// Motores neuronales + descarga/reproducción: solo con `advanced-tts` (OFF por
// defecto). El build de entrega no los compila. "Diferir, no borrar".
#[cfg(feature = "advanced-tts")]
pub mod download;
#[cfg(feature = "advanced-tts")]
pub mod kokoro;
#[cfg(feature = "advanced-tts")]
pub mod online;
#[cfg(feature = "advanced-tts")]
pub mod piper;
#[cfg(feature = "advanced-tts")]
pub mod playback;
#[cfg(feature = "advanced-tts")]
pub mod pyserver;
