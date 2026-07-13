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

pub mod download;
pub mod engine;
pub mod hardware;
pub mod kokoro;
pub mod manager;
pub mod piper;
pub mod playback;
pub mod pyserver;
pub mod recommend;
pub mod registry;
pub mod system;
