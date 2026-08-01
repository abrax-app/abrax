//! Recomendación de motor. **Regla clave: ninguna rama recomienda la nube.**
//!
//! Todos los motores locales corren en CPU, así que la GPU no interviene: lo que
//! decide es la **CPU y la RAM**. Kokoro es el premium local (mejor calidad y
//! reparto de voces) pero el más pesado; Piper es el liviano; las Voces del
//! sistema son el suelo universal, siempre disponibles y sin descargas.
//!
//! La disponibilidad real (¿está aprovisionado el motor?) se aplica después, en
//! `resolve_engine`, que degrada Kokoro → Piper → sistema.

// Sin `advanced-tts` (build de entrega) `recommend_engine` no se usa (el activo
// es siempre Sistema); `resolve_engine` sí. Es esperado, no código muerto real.
#![cfg_attr(not(feature = "advanced-tts"), allow(dead_code))]

use super::engine::EngineId;
use super::hardware::HardwareInfo;

/// RAM y hilos mínimos para que Kokoro (el motor local más pesado) rinda sin
/// ahogar el equipo mientras se dicta.
const KOKORO_MIN_RAM_MB: u32 = 4096;
const KOKORO_MIN_THREADS: u32 = 4;
/// Piper es liviano, pero por debajo de esto conviene el motor del sistema.
const PIPER_MIN_RAM_MB: u32 = 2048;
const PIPER_MIN_THREADS: u32 = 2;

/// Motor **ideal** para este equipo, sin considerar si está aprovisionado.
///
/// Un dato ausente (`None`) **no cuenta como restricción**: la detección es
/// best-effort y no leer la RAM no significa que falte. Solo se degrada ante
/// evidencia real de un equipo modesto.
pub fn recommend_engine(hw: &HardwareInfo) -> EngineId {
    // `None` => sin evidencia de límite => no penaliza.
    let meets = |ram_min: u32, threads_min: u32| {
        hw.ram_mb.is_none_or(|r| r >= ram_min) && hw.cpu_threads.is_none_or(|t| t >= threads_min)
    };

    let rec = if meets(KOKORO_MIN_RAM_MB, KOKORO_MIN_THREADS) {
        EngineId::Kokoro
    } else if meets(PIPER_MIN_RAM_MB, PIPER_MIN_THREADS) {
        EngineId::Piper
    } else {
        EngineId::System
    };

    debug_assert!(
        rec.is_local(),
        "la recomendación nunca debe ser un motor no-local"
    );
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
            cpu_threads: Some(8),
            ram_mb: Some(16384),
        }
    }

    /// Ajusta CPU/RAM sobre el equipo base (que es holgado).
    fn hw_cpu_ram(threads: Option<u32>, ram_mb: Option<u32>) -> HardwareInfo {
        HardwareInfo {
            cpu_threads: threads,
            ram_mb,
            ..hw(GpuVendor::None, GpuType::Unknown, None)
        }
    }

    /// La GPU no interviene: todos los motores locales son CPU.
    #[test]
    fn gpu_does_not_change_the_recommendation() {
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
    fn degrades_by_cpu_and_ram() {
        // Equipo holgado: el premium local.
        assert_eq!(
            recommend_engine(&hw_cpu_ram(Some(8), Some(16384))),
            EngineId::Kokoro
        );
        // Poca RAM para Kokoro, suficiente para Piper.
        assert_eq!(
            recommend_engine(&hw_cpu_ram(Some(4), Some(3072))),
            EngineId::Piper
        );
        // Pocos hilos para Kokoro, suficientes para Piper.
        assert_eq!(
            recommend_engine(&hw_cpu_ram(Some(2), Some(8192))),
            EngineId::Piper
        );
        // Equipo muy modesto: el suelo universal.
        assert_eq!(
            recommend_engine(&hw_cpu_ram(Some(1), Some(1024))),
            EngineId::System
        );
    }

    /// La detección es best-effort: no poder leer un dato no debe penalizar.
    #[test]
    fn unknown_hardware_does_not_degrade() {
        assert_eq!(recommend_engine(&hw_cpu_ram(None, None)), EngineId::Kokoro);
        assert_eq!(
            recommend_engine(&hw_cpu_ram(None, Some(16384))),
            EngineId::Kokoro
        );
        assert_eq!(
            recommend_engine(&hw_cpu_ram(Some(8), None)),
            EngineId::Kokoro
        );
    }

    /// Ni el equipo más modesto puede empujar la recomendación fuera de lo local.
    #[test]
    fn weakest_hardware_still_recommends_local() {
        assert!(recommend_engine(&hw_cpu_ram(Some(1), Some(256))).is_local());
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
        assert_eq!(
            resolve_engine(EngineId::Kokoro, |_| false),
            EngineId::System
        );
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
