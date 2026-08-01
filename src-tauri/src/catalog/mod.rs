//! The bundled, offline model catalog.
//!
//! `catalog.json` is regenerated MANUALLY (offline, not as a build step) with
//! `scripts/gen_catalog.py` from the `handy-computer` Hugging Face org (card
//! `transcribe_cpp` capabilities + benchmarks, a GGUF header probe for
//! name/params, and local curation for the recommended set). The committed JSON
//! is compiled into the binary via `include_str!`, so Abrax ships a complete
//! model list with zero network access.
//!
//! Each entry is normalised into a [`ModelDescriptor`] — the same source-agnostic
//! shape every other producer (HF discovery, on-disk scans, the legacy table)
//! yields — so the catalog is "just another producer". Its explicit `capabilities`
//! map becomes a [`CapabilityProbe`] with confident `Some(..)` values; the runtime
//! `GgufHeaderProber` is the same shape with `None` where a header omits a key,
//! which is why the two are interchangeable (the catalog is a baked probe).

use std::collections::HashMap;

use once_cell::sync::Lazy;
use serde::Deserialize;

use crate::managers::model::{
    default_quant_file, EngineType, ModelDescriptor, ModelSource, QuantFile,
};
use crate::managers::model_capabilities::{CapabilityProbe, Compatibility};

#[derive(Deserialize)]
struct CatalogRoot {
    models: Vec<CatalogModel>,
}

/// One model as written in `catalog.json`. Only the fields the descriptor needs
/// are declared; serde ignores the rest (slug, family, license, …).
#[derive(Deserialize)]
struct CatalogModel {
    /// HF repo id, e.g. `handy-computer/whisper-small-gguf`.
    id: String,
    name: String,
    description: String,
    architecture: Option<String>,
    languages: Vec<String>,
    capabilities: CatalogCaps,
    speed_score: Option<f32>,
    accuracy_score: Option<f32>,
    files: Vec<QuantFile>,
    default_quant: Option<String>,
    recommended_rank: Option<u32>,
    /// Part of the small curated onboarding set (badged "Recommended"). Distinct
    /// from `recommended_rank`, which only orders the full list.
    #[serde(default)]
    recommended: bool,
}

#[derive(Deserialize)]
struct CatalogCaps {
    streaming: bool,
    translate: bool,
    lang_detect: bool,
    // `timestamps` (a string enum) is present in the catalog but has no
    // `CapabilityProbe` field yet — wire it through when the probe gains one.
}

impl From<CatalogModel> for ModelDescriptor {
    fn from(m: CatalogModel) -> Self {
        // The default download file. Its name is folded into the id so a catalog
        // entry collides (dedups) with the very same file later discovered in
        // the HF cache — both compute `"{repo_id}/{filename}"`.
        let default_filename = default_quant_file(&m.files, m.default_quant.as_deref())
            .map(|f| f.filename.clone())
            .unwrap_or_default();

        ModelDescriptor {
            id: format!("{}/{}", m.id, default_filename),
            source: ModelSource::HuggingFace {
                repo_id: m.id,
                revision: "main".to_string(),
            },
            name: m.name,
            description: m.description,
            engine_type: EngineType::TranscribeCpp,
            caps: CapabilityProbe {
                verdict: Compatibility::Compatible, // curated org models we ship support for
                display_name: None,
                architecture: m.architecture,
                variant: None,
                languages: Some(m.languages),
                supports_streaming: Some(m.capabilities.streaming),
                supports_translation: Some(m.capabilities.translate),
                supports_language_detect: Some(m.capabilities.lang_detect),
            },
            files: m.files,
            default_quant: m.default_quant,
            // catalog scores are 0–100; ModelInfo / the UI bars use 0.0–1.0.
            speed_score: m.speed_score.unwrap_or(0.0) / 100.0,
            accuracy_score: m.accuracy_score.unwrap_or(0.0) / 100.0,
            recommended_rank: m.recommended_rank,
            recommended: m.recommended,
        }
    }
}

/// The bundled catalog, parsed once and normalised into descriptors.
pub static CATALOG: Lazy<Vec<ModelDescriptor>> = Lazy::new(|| {
    let root: CatalogRoot = serde_json::from_str(include_str!("catalog.json"))
        .expect("bundled catalog.json is valid JSON matching the catalog schema");
    root.models.into_iter().map(ModelDescriptor::from).collect()
});

/// Editorial recommended rank keyed by descriptor id (the same id the model
/// registry uses). Built once from the catalog.
static RANK_BY_ID: Lazy<HashMap<String, u32>> = Lazy::new(|| {
    CATALOG
        .iter()
        .filter_map(|d| d.recommended_rank.map(|r| (d.id.clone(), r)))
        .collect()
});

/// Recommended rank for a model id (lower = higher priority). Returns
/// `u32::MAX` for unranked/unknown ids so they sort last in an ascending sort.
pub fn rank_of(model_id: &str) -> u32 {
    RANK_BY_ID.get(model_id).copied().unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::managers::model_capabilities::KNOWN_ARCHES;
    use std::collections::BTreeSet;

    /// `org/repo/archivo.gguf` -> `org/repo`.
    fn repo_de(id: &str) -> &str {
        id.rsplit_once('/').map(|(repo, _)| repo).unwrap_or(id)
    }

    #[test]
    fn catalog_parses_and_is_nonempty() {
        assert!(!CATALOG.is_empty(), "bundled catalog should contain models");
    }

    #[test]
    fn ids_are_unique() {
        let mut ids: Vec<&str> = CATALOG.iter().map(|d| d.id.as_str()).collect();
        ids.sort_unstable();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "catalog descriptor ids must be unique");
    }

    #[test]
    fn scores_are_normalised_0_to_1() {
        for d in CATALOG.iter() {
            assert!((0.0..=1.0).contains(&d.speed_score), "{} speed", d.id);
            assert!((0.0..=1.0).contains(&d.accuracy_score), "{} acc", d.id);
        }
    }

    /// ABRAX envía CINCO modelos curados, no los 65 de la organización upstream
    /// (D1). Este test es el que impide que la poda se deshaga sola: correr
    /// `scripts/gen_catalog.py` sin su lista blanca `ENVIADOS` devuelve el
    /// catálogo completo, y sin esta comprobación el binario saldría con 65
    /// modelos y nadie se enteraría hasta ver la pantalla.
    #[test]
    fn catalog_ships_the_curated_five() {
        // El id del descriptor es `org/repo/archivo.gguf`; comparamos por REPO
        // para que cambiar el quant por defecto no rompa este test.
        let ids: Vec<&str> = CATALOG.iter().map(|d| repo_de(&d.id)).collect();
        // Orden editorial cambiado el 31/07 por decisión de producto: primero el
        // que mejor transcribe, último el más rápido. El pequeño (Canary) se
        // comía frases cortas —medido el 29/07 sobre loopback y otra vez el
        // 31/07 sobre grabaciones reales de dictado: cadena VACÍA donde
        // Nemotron sí entendió—, así que dejar de ofrecerlo el primero.
        assert_eq!(
            ids,
            vec![
                "handy-computer/cohere-transcribe-03-2026-gguf",
                "handy-computer/whisper-large-v3-turbo-gguf",
                "handy-computer/whisper-large-v3-gguf",
                "handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf",
                "handy-computer/canary-180m-flash-gguf",
            ],
            "el catálogo enviado debe ser exactamente los 5 curados, en orden"
        );
        let ranks: Vec<Option<u32>> = CATALOG.iter().map(|d| d.recommended_rank).collect();
        assert_eq!(
            ranks,
            vec![Some(1), Some(2), Some(3), Some(4), Some(5)],
            "los rangos editoriales deben ser 1..5 sin huecos ni repetidos"
        );
    }

    /// Una insignia que llevan cinco de cinco no recomienda nada. La medición en
    /// equipo limpio pidió que la recomendación fuera evidente: exactamente una.
    #[test]
    fn exactly_one_model_carries_the_recommended_badge() {
        let badged: Vec<&str> = CATALOG
            .iter()
            .filter(|d| d.recommended)
            .map(|d| repo_de(&d.id))
            .collect();
        // La insignia pasó a Cohere el 31/07, con el orden editorial: es la que
        // decide qué ofrece PRIMERO la pantalla de bienvenida
        // (`Onboarding.tsx` separa `is_recommended` del resto), así que dejarla
        // en Canary habría enseñado el catálogo en un orden y recomendado el
        // contrario. Cuesta más descarga —1,6 GB frente a 208 MB— y el texto de
        // la tarjeta lo dice; lo que no se puede es recomendar el que devuelve
        // vacío con frases cortas.
        assert_eq!(
            badged,
            vec!["handy-computer/cohere-transcribe-03-2026-gguf"],
            "solo el modelo de arranque lleva insignia «Recomendado»"
        );
    }

    /// ABRAX vende dictado en español. Un modelo que no lo transcribe no tiene
    /// nada que hacer en un catálogo de cinco.
    #[test]
    fn every_shipped_model_transcribes_spanish() {
        for d in CATALOG.iter() {
            let languages = d
                .caps
                .languages
                .as_ref()
                .unwrap_or_else(|| panic!("{} no declara idiomas", d.id));
            assert!(
                languages.iter().any(|l| l == "es"),
                "{} no transcribe español: {:?}",
                d.id,
                languages
            );
        }
    }

    #[test]
    fn catalog_architectures_are_known_to_capability_probe() {
        let missing: BTreeSet<&str> = CATALOG
            .iter()
            .filter_map(|d| d.caps.architecture.as_deref())
            .filter(|arch| !KNOWN_ARCHES.contains(arch))
            .collect();

        assert!(
            missing.is_empty(),
            "catalog architecture(s) missing from KNOWN_ARCHES: {:?}",
            missing
        );
    }
}
