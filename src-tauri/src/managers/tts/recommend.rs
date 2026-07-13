//! Árbol de decisión de la sección 4 del prompt: hardware → motor recomendado.
//! **Regla clave: NINGUNA rama recomienda la nube.** Todo motor devuelto es
//! local. La disponibilidad real (¿descargado? ¿falla?) y el fallback a Voces
//! del sistema se resuelven aparte en `resolve_engine`.

use super::engine::EngineId;
use super::hardware::{GpuType, GpuVendor, HardwareInfo};

/// VRAM mínima orientativa (best-effort) para ofrecer Chatterbox en NVIDIA.
/// El prompt dice "≥ ~6 GB". Si la VRAM es ilegible (`None`) se decide por
/// vendor + tipo (no se inventa un número).
pub const CHATTERBOX_MIN_VRAM_MB: u32 = 6000;

/// Devuelve el motor **ideal** para este hardware (sin considerar si está
/// descargado). Ver la tabla de la sección 4:
/// - NVIDIA discreta ≥ ~6 GB → Chatterbox
/// - Apple Silicon (M1+)      → Chatterbox (MPS)
/// - Solo CPU / GPU no compatible → Piper
pub fn recommend_engine(hw: &HardwareInfo) -> EngineId {
    // Apple Silicon corre Chatterbox en Metal (MPS), sin importar la VRAM.
    if hw.gpu_vendor == GpuVendor::Apple {
        return EngineId::Chatterbox;
    }

    // NVIDIA discreta: Chatterbox si hay VRAM suficiente, o si es ilegible
    // (`None`) decidimos por vendor+tipo (best-effort, sin inventar cifras).
    if hw.gpu_vendor == GpuVendor::Nvidia && hw.gpu_type == GpuType::Discrete {
        return match hw.vram_mb {
            Some(mb) if mb < CHATTERBOX_MIN_VRAM_MB => EngineId::Piper,
            _ => EngineId::Chatterbox,
        };
    }

    // Todo lo demás (solo CPU, AMD/Intel, integrada, desconocida): Piper es el
    // estándar neuronal local. Chatterbox solo corre en CUDA/MPS.
    EngineId::Piper
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
    fn nvidia_discrete_with_enough_vram_recommends_chatterbox() {
        let h = hw(GpuVendor::Nvidia, GpuType::Discrete, Some(12000));
        assert_eq!(recommend_engine(&h), EngineId::Chatterbox);
    }

    #[test]
    fn nvidia_discrete_unknown_vram_recommends_chatterbox() {
        // VRAM ilegible → best-effort por vendor+tipo, NO se inventa número.
        let h = hw(GpuVendor::Nvidia, GpuType::Discrete, None);
        assert_eq!(recommend_engine(&h), EngineId::Chatterbox);
    }

    #[test]
    fn nvidia_discrete_low_vram_falls_to_piper() {
        let h = hw(GpuVendor::Nvidia, GpuType::Discrete, Some(4000));
        assert_eq!(recommend_engine(&h), EngineId::Piper);
    }

    #[test]
    fn nvidia_integrated_recommends_piper() {
        // GPU NVIDIA pero integrada (raro): no es el caso Chatterbox del árbol.
        let h = hw(GpuVendor::Nvidia, GpuType::Integrated, None);
        assert_eq!(recommend_engine(&h), EngineId::Piper);
    }

    #[test]
    fn apple_silicon_recommends_chatterbox() {
        // Apple reporta integrada en wgpu, pero corre Chatterbox por MPS.
        let h = hw(GpuVendor::Apple, GpuType::Integrated, None);
        assert_eq!(recommend_engine(&h), EngineId::Chatterbox);
    }

    #[test]
    fn intel_integrated_recommends_piper() {
        let h = hw(GpuVendor::Intel, GpuType::Integrated, None);
        assert_eq!(recommend_engine(&h), EngineId::Piper);
    }

    #[test]
    fn amd_discrete_recommends_piper() {
        // AMD no tiene CUDA/MPS → Piper (no Chatterbox).
        let h = hw(GpuVendor::Amd, GpuType::Discrete, Some(16000));
        assert_eq!(recommend_engine(&h), EngineId::Piper);
    }

    #[test]
    fn cpu_only_recommends_piper() {
        let h = hw(GpuVendor::None, GpuType::Unknown, None);
        assert_eq!(recommend_engine(&h), EngineId::Piper);
    }

    #[test]
    fn no_branch_recommends_cloud() {
        // Barrido de todas las combinaciones: el motor recomendado SIEMPRE es local.
        for vendor in [
            GpuVendor::Nvidia,
            GpuVendor::Apple,
            GpuVendor::Amd,
            GpuVendor::Intel,
            GpuVendor::Unknown,
            GpuVendor::None,
        ] {
            for gpu_type in [
                GpuType::Discrete,
                GpuType::Integrated,
                GpuType::Virtual,
                GpuType::Cpu,
                GpuType::Unknown,
            ] {
                for vram in [None, Some(0), Some(4000), Some(8000), Some(24000)] {
                    let rec = recommend_engine(&hw(vendor, gpu_type, vram));
                    assert!(rec.is_local(), "recomendó un motor no-local: {rec:?}");
                }
            }
        }
    }

    #[test]
    fn resolve_falls_back_to_system_when_nothing_available() {
        // Recomendado = Chatterbox, nada disponible → System (nunca silencio/nube).
        let active = resolve_engine(EngineId::Chatterbox, |_| false);
        assert_eq!(active, EngineId::System);
    }

    #[test]
    fn resolve_falls_back_to_piper_when_recommended_unavailable() {
        let active = resolve_engine(EngineId::Chatterbox, |e| e == EngineId::Piper);
        assert_eq!(active, EngineId::Piper);
    }

    #[test]
    fn resolve_keeps_recommended_when_available() {
        let active = resolve_engine(EngineId::Chatterbox, |_| true);
        assert_eq!(active, EngineId::Chatterbox);
    }
}
