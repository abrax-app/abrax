//! Recomendación de motor. **Regla clave: ninguna rama recomienda la nube.**
//! Todos los motores son locales y CPU, así que la GPU ya no influye: se
//! recomienda Kokoro (premium local, mejor calidad + reparto de voces), y la
//! disponibilidad real (¿aprovisionado?) degrada a Piper y luego al sistema en
//! `resolve_engine`.

// Sin `advanced-tts` (build de entrega) `recommend_engine` no se usa (el activo
// es siempre Sistema); `resolve_engine` sí. Es esperado, no código muerto real.
#![cfg_attr(not(feature = "advanced-tts"), allow(dead_code))]

use super::engine::EngineId;
use super::hardware::HardwareInfo;

/// Motor **ideal** (sin considerar si está aprovisionado). Kokoro es la mejor
/// voz local; `_hw` se conserva por si futuras heurísticas lo necesitan.
pub fn recommend_engine(_hw: &HardwareInfo) -> EngineId {
    let rec = EngineId::Kokoro;
    debug_assert!(rec.is_local(), "la recomendación nunca debe ser un motor no-local");
    rec
}

/// Resuelve el motor **activo** combinando la recomendación con la
/// disponibilidad real. Si el recomendado no está disponible, degrada a Piper,
/// y si tampoco, a Voces del sistema (fallback universal: siempre local, nunca
/// silencio, nunca nube).
pub fn resolve_engine(recommended: EngineId, is_available: impl Fn(EngineId) -> bool) -> EngineId {
    if is_available(recommended) {
        recommended
    } else if is_available(EngineId::Piper) {
        EngineId::Piper
    } else {
        EngineId::System
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::tts::hardware::{GpuType, GpuVendor, HardwareInfo, OsKind};

    fn hw(vendor: GpuVendor, gpu_type: GpuType, vram_mb: Option<u32>) -> HardwareInfo {
        HardwareInfo {
            os: OsKind::Windows,
            gpu_vendor: vendor,
            gpu_name: "test".into(),
            gpu_type,
            vram_mb,
        }
    }

    #[test]
    fn recommends_kokoro_regardless_of_hardware() {
        for vendor in [
            GpuVendor::Nvidia,
            GpuVendor::Apple,
            GpuVendor::Amd,
            GpuVendor::Intel,
            GpuVendor::None,
        ] {
            let rec = recommend_engine(&hw(vendor, GpuType::Discrete, Some(12000)));
            assert_eq!(rec, EngineId::Kokoro);
        }
    }

    #[test]
    fn recommendation_is_always_local() {
        assert!(recommend_engine(&hw(GpuVendor::None, GpuType::Cpu, None)).is_local());
    }

    #[test]
    fn never_recommends_or_falls_back_to_online() {
        let h = hw(GpuVendor::Nvidia, GpuType::Discrete, Some(12000));
        assert_ne!(recommend_engine(&h), EngineId::Online);
        // Al degradar (recomendado no disponible) jamás se cae al motor online,
        // aunque sea lo único "disponible": el fallback es Piper → sistema.
        assert_ne!(
            resolve_engine(EngineId::Kokoro, |e| e == EngineId::Online),
            EngineId::Online
        );
    }

    #[test]
    fn resolve_falls_back_to_system_when_nothing_available() {
        assert_eq!(resolve_engine(EngineId::Kokoro, |_| false), EngineId::System);
    }

    #[test]
    fn resolve_falls_back_to_piper_when_recommended_unavailable() {
        let active = resolve_engine(EngineId::Kokoro, |e| e == EngineId::Piper);
        assert_eq!(active, EngineId::Piper);
    }

    #[test]
    fn resolve_keeps_recommended_when_available() {
        assert_eq!(resolve_engine(EngineId::Kokoro, |_| true), EngineId::Kokoro);
    }
}
