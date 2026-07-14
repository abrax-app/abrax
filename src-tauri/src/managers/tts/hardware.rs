//! Detección de hardware para recomendar el mejor motor TTS **local**.
//!
//! El identificador de fabricante se resuelve por **PCI vendor ID**, nunca por
//! string (los nombres de driver cambian entre versiones y localizaciones):
//! NVIDIA `0x10DE`, AMD `0x1002`, Intel `0x8086`, Apple `0x106B`.
//!
//! `wgpu` da, cross-platform, el vendor y el tipo (discreta / integrada) de cada
//! adaptador vía `Instance::enumerate_adapters` — sin crear device ni renderizar.
//! La VRAM total **no** la expone `wgpu`; en Windows se intenta leer por DXGI
//! (`IDXGIAdapter1::GetDesc1().DedicatedVideoMemory`), y si no se puede queda en
//! `None` — jamás se inventa un número. En otros SO también queda `None`.

// Sin `advanced-tts` (build de entrega) no se enlaza wgpu: los IDs de fabricante
// PCI y el mapeo por PCI solo los usa la detección con wgpu (y su test), así que
// quedan sin uso en el build de entrega. Es esperado, no código muerto real.
#![cfg_attr(not(feature = "advanced-tts"), allow(dead_code))]

use serde::{Deserialize, Serialize};
use specta::Type;

/// PCI vendor IDs conocidos (fuente: pcisig / listas públicas).
pub const VENDOR_NVIDIA: u32 = 0x10DE;
pub const VENDOR_AMD: u32 = 0x1002;
pub const VENDOR_INTEL: u32 = 0x8086;
/// Apple usa `0x106B` en Metal/wgpu para su GPU integrada de Apple Silicon.
pub const VENDOR_APPLE: u32 = 0x106B;

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum OsKind {
    Windows,
    MacOs,
    Linux,
    Other,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum GpuVendor {
    Nvidia,
    Apple,
    Amd,
    Intel,
    Unknown,
    /// No se detectó ninguna GPU (ni integrada).
    None,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum GpuType {
    Discrete,
    Integrated,
    Virtual,
    Cpu,
    Unknown,
}

/// Instantánea del equipo, best-effort. Nota: `has_internet` NO forma parte de
/// esto — la elección de motor es 100% local; internet solo se consulta aparte
/// para saber si se PUEDE descargar un modelo, nunca para elegir motor.
#[derive(Serialize, Deserialize, Debug, Clone, Type)]
pub struct HardwareInfo {
    pub os: OsKind,
    pub gpu_vendor: GpuVendor,
    pub gpu_name: String,
    pub gpu_type: GpuType,
    /// VRAM dedicada en MB. `None` = no se pudo leer (best-effort), nunca inventada.
    pub vram_mb: Option<u32>,
}

/// Fabricante a partir del PCI vendor ID (no del string del driver).
pub fn vendor_from_pci_id(vendor_id: u32) -> GpuVendor {
    match vendor_id {
        VENDOR_NVIDIA => GpuVendor::Nvidia,
        VENDOR_AMD => GpuVendor::Amd,
        VENDOR_INTEL => GpuVendor::Intel,
        VENDOR_APPLE => GpuVendor::Apple,
        _ => GpuVendor::Unknown,
    }
}

fn detect_os() -> OsKind {
    #[cfg(target_os = "windows")]
    {
        OsKind::Windows
    }
    #[cfg(target_os = "macos")]
    {
        OsKind::MacOs
    }
    #[cfg(target_os = "linux")]
    {
        OsKind::Linux
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
    {
        OsKind::Other
    }
}

#[cfg(feature = "advanced-tts")]
fn gpu_type_from_wgpu(device_type: wgpu::DeviceType) -> GpuType {
    match device_type {
        wgpu::DeviceType::DiscreteGpu => GpuType::Discrete,
        wgpu::DeviceType::IntegratedGpu => GpuType::Integrated,
        wgpu::DeviceType::VirtualGpu => GpuType::Virtual,
        wgpu::DeviceType::Cpu => GpuType::Cpu,
        wgpu::DeviceType::Other => GpuType::Unknown,
    }
}

/// Un adaptador candidato ya normalizado a nuestros tipos.
#[cfg(feature = "advanced-tts")]
struct GpuCandidate {
    vendor: GpuVendor,
    gpu_type: GpuType,
    name: String,
    vendor_id: u32,
    device_id: u32,
}

/// Prioridad para elegir "la GPU que importa" cuando hay varias (p.ej. una
/// laptop con Intel integrada + NVIDIA discreta): gana la discreta, y entre
/// iguales gana el fabricante con motor neuronal fuerte (NVIDIA/Apple/AMD).
#[cfg(feature = "advanced-tts")]
fn candidate_rank(c: &GpuCandidate) -> (u8, u8) {
    let type_rank = match c.gpu_type {
        GpuType::Discrete => 3,
        GpuType::Integrated => 2,
        GpuType::Virtual => 1,
        GpuType::Cpu | GpuType::Unknown => 0,
    };
    let vendor_rank = match c.vendor {
        GpuVendor::Nvidia | GpuVendor::Apple => 3,
        GpuVendor::Amd => 2,
        GpuVendor::Intel => 1,
        GpuVendor::Unknown | GpuVendor::None => 0,
    };
    (type_rank, vendor_rank)
}

/// Enumera adaptadores con `wgpu` y elige el más relevante. Aislado y sin
/// `unwrap`: si `wgpu` no encuentra nada, devuelve `None`.
#[cfg(feature = "advanced-tts")]
fn best_gpu_candidate() -> Option<GpuCandidate> {
    let instance = wgpu::Instance::default();
    let mut best: Option<GpuCandidate> = None;
    // `enumerate_adapters` es async en wgpu 30 aunque en nativo resuelve al
    // instante; `pollster` bloquea sobre ese future ya listo.
    let adapters = pollster::block_on(instance.enumerate_adapters(wgpu::Backends::all()));
    for adapter in adapters {
        let info = adapter.get_info();
        let candidate = GpuCandidate {
            vendor: vendor_from_pci_id(info.vendor),
            gpu_type: gpu_type_from_wgpu(info.device_type),
            name: info.name,
            vendor_id: info.vendor,
            device_id: info.device,
        };
        // El backend software (p.ej. WARP/lavapipe) reporta Cpu: no debe ganarle
        // a una GPU real, pero sirve de último recurso.
        best = match best {
            Some(prev) if candidate_rank(&prev) >= candidate_rank(&candidate) => Some(prev),
            _ => Some(candidate),
        };
    }
    best
}

/// VRAM dedicada en MB para un adaptador identificado por (vendor, device).
/// Solo Windows (DXGI); en el resto de SO devuelve `None`.
#[cfg(all(feature = "advanced-tts", target_os = "windows"))]
fn read_vram_mb(vendor_id: u32, device_id: u32) -> Option<u32> {
    use windows::Win32::Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1};

    unsafe {
        let factory: IDXGIFactory1 = CreateDXGIFactory1().ok()?;
        // Primera pasada: match exacto por vendor + device. Segunda pasada:
        // match solo por vendor (por si el device id no cuadra entre APIs).
        let mut vendor_only: Option<u32> = None;
        let mut idx = 0u32;
        loop {
            let adapter = match factory.EnumAdapters1(idx) {
                Ok(a) => a,
                Err(_) => break,
            };
            // En windows 0.61, `GetDesc1` no toma out-param: devuelve el desc.
            if let Ok(desc) = adapter.GetDesc1() {
                let bytes = desc.DedicatedVideoMemory as u64;
                if bytes > 0 {
                    let mb = (bytes / (1024 * 1024)) as u32;
                    if desc.VendorId == vendor_id && desc.DeviceId == device_id {
                        return Some(mb);
                    }
                    if desc.VendorId == vendor_id && vendor_only.is_none() {
                        vendor_only = Some(mb);
                    }
                }
            }
            idx += 1;
        }
        vendor_only
    }
}

#[cfg(all(feature = "advanced-tts", not(target_os = "windows")))]
fn read_vram_mb(_vendor_id: u32, _device_id: u32) -> Option<u32> {
    None
}

/// Build de entrega (sin `advanced-tts`): no se enlaza wgpu. Reporta solo el SO;
/// la GPU queda "desconocida" — la recomendación no la usa (siempre Sistema).
#[cfg(not(feature = "advanced-tts"))]
pub fn detect_hardware() -> HardwareInfo {
    HardwareInfo {
        os: detect_os(),
        gpu_vendor: GpuVendor::None,
        gpu_name: String::new(),
        gpu_type: GpuType::Unknown,
        vram_mb: None,
    }
}

/// Detecta el equipo. Nunca hace panic ni `unwrap`: cualquier fallo de `wgpu`
/// o DXGI degrada a valores "desconocidos"/`None`, jamás inventados.
#[cfg(feature = "advanced-tts")]
pub fn detect_hardware() -> HardwareInfo {
    let os = detect_os();
    match best_gpu_candidate() {
        Some(c) => {
            let vram_mb = read_vram_mb(c.vendor_id, c.device_id);
            HardwareInfo {
                os,
                gpu_vendor: c.vendor,
                gpu_name: c.name,
                gpu_type: c.gpu_type,
                vram_mb,
            }
        }
        None => HardwareInfo {
            os,
            gpu_vendor: GpuVendor::None,
            gpu_name: String::new(),
            gpu_type: GpuType::Unknown,
            vram_mb: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vendor_ids_map_by_pci_not_string() {
        assert_eq!(vendor_from_pci_id(0x10DE), GpuVendor::Nvidia);
        assert_eq!(vendor_from_pci_id(0x1002), GpuVendor::Amd);
        assert_eq!(vendor_from_pci_id(0x8086), GpuVendor::Intel);
        assert_eq!(vendor_from_pci_id(0x106B), GpuVendor::Apple);
        assert_eq!(vendor_from_pci_id(0xBEEF), GpuVendor::Unknown);
    }

    #[cfg(feature = "advanced-tts")]
    #[test]
    fn candidate_rank_prefers_discrete_then_strong_vendor() {
        let discrete_nvidia = GpuCandidate {
            vendor: GpuVendor::Nvidia,
            gpu_type: GpuType::Discrete,
            name: "RTX".into(),
            vendor_id: VENDOR_NVIDIA,
            device_id: 1,
        };
        let integrated_intel = GpuCandidate {
            vendor: GpuVendor::Intel,
            gpu_type: GpuType::Integrated,
            name: "UHD".into(),
            vendor_id: VENDOR_INTEL,
            device_id: 2,
        };
        assert!(candidate_rank(&discrete_nvidia) > candidate_rank(&integrated_intel));
    }

    /// Imprime el `HardwareInfo` real del equipo (correr con `--nocapture`).
    /// No asevera sobre la GPU (varía por máquina/CI), solo que no paniquea y
    /// que el SO es coherente con el build.
    #[test]
    fn detect_hardware_prints_real_info() {
        let hw = detect_hardware();
        println!("\n[detect_hardware] => {hw:#?}\n");
        #[cfg(target_os = "windows")]
        assert_eq!(hw.os, OsKind::Windows);
        #[cfg(target_os = "macos")]
        assert_eq!(hw.os, OsKind::MacOs);
    }
}
