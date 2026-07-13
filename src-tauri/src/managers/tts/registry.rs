//! Requisitos estáticos por motor, para que la UI componga el "por qué no está
//! disponible" (i18n) sin strings de copy en el backend.

use super::engine::{EngineId, EngineRequirements};

pub fn requirements_for(id: EngineId) -> EngineRequirements {
    match id {
        EngineId::System => EngineRequirements {
            needs_gpu: false,
            needs_download: false,
            needs_internet: false,
        },
        EngineId::Piper => EngineRequirements {
            needs_gpu: false,
            needs_download: true,
            needs_internet: false,
        },
        EngineId::Kokoro => EngineRequirements {
            needs_gpu: false,
            needs_download: true,
            needs_internet: false,
        },
        EngineId::Online => EngineRequirements {
            needs_gpu: false,
            needs_download: true,
            needs_internet: true,
        },
    }
}
