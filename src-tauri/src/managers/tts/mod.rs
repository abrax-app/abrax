//! Motor TTS adaptativo 100% local para Abrax (Fase 2 de "Escucha tu Código").
//!
//! Este módulo **envuelve** el manager de Escucha existente
//! (`managers::escucha`, motor del sistema por el crate `tts`) con un registro
//! multi-motor: detecta el hardware (`hardware`), recomienda el mejor motor
//! **local** que el equipo soporte (`recommend`) y define el contrato común de
//! los motores (`engine`). Reglas de oro: **todo es local, siempre hay un
//! fallback que funciona, ninguna rama cae a la nube.**

pub mod engine;
pub mod hardware;
pub mod recommend;
