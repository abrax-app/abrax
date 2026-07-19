//! Catálogo de modelos LLM OPCIONALES para el "Pulido con IA" local.
//!
//! Estos modelos no vienen con ABRAX ni son necesarios: la corrección por
//! defecto es determinista y ligera (`reglas`, `tildes`, `simbolos`). Quien
//! quiera reformulación con IA local y tenga hardware para ello puede
//! descargar uno de estos y correrlo en el sidecar aislado (ver
//! `motor_sidecar`), sin depender de Ollama ni de la nube.
//!
//! Solo datos aquí: el registro (qué se puede descargar, con qué sha256 y
//! tamaño). La descarga, el runtime y el enganche viven en otros módulos.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

/// Una entrada del catálogo de modelos de corrección descargables.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Type)]
pub struct ModeloCorreccion {
    /// Identificador estable (se usa en ajustes y como nombre de archivo).
    pub id: String,
    pub nombre: String,
    pub descripcion: String,
    /// Repo de Hugging Face y archivo GGUF concreto a descargar.
    pub repo_hf: String,
    pub archivo: String,
    /// SHA-256 del archivo, para verificar integridad tras la descarga.
    pub sha256: String,
    pub tamano_bytes: u64,
    /// RAM (o VRAM en GPU) aproximada recomendada para que corra con soltura.
    pub ram_min_mb: u64,
    pub licencia: String,
    /// Sugerido para la mayoría (mejor equilibrio calidad/tamaño).
    pub recomendado: bool,
}

/// El catálogo. Todos son GGUF Q4_K_M de archivo único, verificados por sha256.
/// Ampliable sin tocar el resto del subsistema.
pub fn catalogo() -> Vec<ModeloCorreccion> {
    vec![
        ModeloCorreccion {
            id: "qwen2.5-7b-instruct-q4km".into(),
            nombre: "Qwen2.5 7B Instruct".into(),
            descripcion: "Equilibrio calidad/tamaño y muy buen español. \
                          Punto dulce para hardware de gama media."
                .into(),
            repo_hf: "bartowski/Qwen2.5-7B-Instruct-GGUF".into(),
            archivo: "Qwen2.5-7B-Instruct-Q4_K_M.gguf".into(),
            sha256: "65b8fcd92af6b4fefa935c625d1ac27ea29dcb6ee14589c55a8f115ceaaa1423".into(),
            tamano_bytes: 4_683_074_240,
            ram_min_mb: 6_144,
            licencia: "Apache-2.0".into(),
            recomendado: true,
        },
        ModeloCorreccion {
            id: "llama-3.1-8b-instruct-q4km".into(),
            nombre: "Llama 3.1 8B Instruct".into(),
            descripcion: "Alternativa fuerte en español, algo más grande. \
                          Buen seguimiento de instrucciones."
                .into(),
            repo_hf: "bartowski/Meta-Llama-3.1-8B-Instruct-GGUF".into(),
            archivo: "Meta-Llama-3.1-8B-Instruct-Q4_K_M.gguf".into(),
            sha256: "7b064f5842bf9532c91456deda288a1b672397a54fa729aa665952863033557c".into(),
            tamano_bytes: 4_920_739_232,
            ram_min_mb: 7_168,
            licencia: "Llama 3.1 Community".into(),
            recomendado: false,
        },
        ModeloCorreccion {
            id: "qwen2.5-14b-instruct-q4km".into(),
            nombre: "Qwen2.5 14B Instruct".into(),
            descripcion: "La mejor calidad de pulido del catálogo. Pide \
                          hardware potente (GPU con bastante VRAM)."
                .into(),
            repo_hf: "bartowski/Qwen2.5-14B-Instruct-GGUF".into(),
            archivo: "Qwen2.5-14B-Instruct-Q4_K_M.gguf".into(),
            sha256: "e47ad95dad6ff848b431053b375adb5d39321290ea2c638682577dafca87c008".into(),
            tamano_bytes: 8_988_110_976,
            ram_min_mb: 12_288,
            licencia: "Apache-2.0".into(),
            recomendado: false,
        },
    ]
}

/// Busca un modelo del catálogo por su `id`.
pub fn por_id(id: &str) -> Option<ModeloCorreccion> {
    catalogo().into_iter().find(|m| m.id == id)
}

/// URL de descarga directa (resolve por rama `main` de Hugging Face).
pub fn url_descarga(m: &ModeloCorreccion) -> String {
    format!(
        "https://huggingface.co/{}/resolve/main/{}",
        m.repo_hf, m.archivo
    )
}

/// Carpeta donde viven los modelos de corrección descargados, aislada del árbol
/// de modelos de transcripción.
pub fn carpeta(datadir: &Path) -> PathBuf {
    datadir.join("models").join("correccion")
}

/// Ruta al archivo GGUF de un modelo (exista o no en disco).
pub fn ruta_gguf(datadir: &Path, m: &ModeloCorreccion) -> PathBuf {
    carpeta(datadir).join(&m.archivo)
}

/// `true` si el modelo ya está descargado (archivo presente).
pub fn esta_descargado(datadir: &Path, m: &ModeloCorreccion) -> bool {
    ruta_gguf(datadir, m).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn el_catalogo_esta_bien_formado() {
        let c = catalogo();
        assert!(!c.is_empty());
        // ids únicos
        let mut ids: Vec<&str> = c.iter().map(|m| m.id.as_str()).collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), n, "hay ids de modelo duplicados");
        // exactamente un recomendado
        assert_eq!(
            c.iter().filter(|m| m.recomendado).count(),
            1,
            "debe haber exactamente un modelo recomendado"
        );
        for m in &c {
            assert_eq!(m.sha256.len(), 64, "sha256 inválido en {}", m.id);
            assert!(m.sha256.chars().all(|ch| ch.is_ascii_hexdigit()));
            assert!(
                m.tamano_bytes > 1_000_000_000,
                "tamaño sospechoso en {}",
                m.id
            );
            assert!(m.archivo.ends_with(".gguf"));
            assert!(!m.repo_hf.is_empty() && !m.licencia.is_empty());
        }
    }

    #[test]
    fn por_id_encuentra_y_falla_bien() {
        assert!(por_id("qwen2.5-7b-instruct-q4km").is_some());
        assert!(por_id("inexistente").is_none());
    }

    #[test]
    fn la_url_apunta_a_hugging_face() {
        let m = por_id("qwen2.5-7b-instruct-q4km").unwrap();
        assert_eq!(
            url_descarga(&m),
            "https://huggingface.co/bartowski/Qwen2.5-7B-Instruct-GGUF/resolve/main/Qwen2.5-7B-Instruct-Q4_K_M.gguf"
        );
    }
}
