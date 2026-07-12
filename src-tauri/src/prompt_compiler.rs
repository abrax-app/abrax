//! Compilador de Prompts — modo plantilla (F6, MVP sin LLM).
//!
//! Convierte el último dictado en un prompt estructurado (CONTEXTO / TAREA /
//! REQUISITOS / FORMATO) con un **linter de ambigüedades es-419**: cada
//! término vago («rápido», «bonito», «ojalá»…) genera una advertencia con una
//! pregunta concreta. Todo heurístico y local: la demo jamás depende de la
//! red. El CONTEXTO se enriquece con el Diccionario Vivo (proyecto activo y
//! términos del repo mencionados en el dictado).

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

use crate::audio_toolkit::build_match_key;

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct AmbiguityWarning {
    /// El término vago tal como apareció en el dictado.
    pub term: String,
    /// Pregunta concreta que lo desarma.
    pub question: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
pub struct CompiledPrompt {
    pub markdown: String,
    pub warnings: Vec<AmbiguityWarning>,
}

/// Léxico de vaguedad es-419: clave normalizada (minúsculas, sin tildes) →
/// pregunta que exige lo medible. Las variantes de género/número se listan
/// explícitas — el matching es por token completo, no por prefijo.
const VAGUE_TERMS: &[(&str, &str)] = &[
    (
        "rapido",
        "¿objetivo medible? (p. ej., latencia en ms o rps)",
    ),
    (
        "rapida",
        "¿objetivo medible? (p. ej., latencia en ms o rps)",
    ),
    (
        "rapidos",
        "¿objetivo medible? (p. ej., latencia en ms o rps)",
    ),
    (
        "rapidas",
        "¿objetivo medible? (p. ej., latencia en ms o rps)",
    ),
    (
        "bonito",
        "¿siguiendo qué referencia visual o sistema de diseño?",
    ),
    (
        "bonita",
        "¿siguiendo qué referencia visual o sistema de diseño?",
    ),
    (
        "lindo",
        "¿siguiendo qué referencia visual o sistema de diseño?",
    ),
    (
        "linda",
        "¿siguiendo qué referencia visual o sistema de diseño?",
    ),
    (
        "ojala",
        "¿es requisito duro o solo deseable? Decídelo antes de pedirlo.",
    ),
    ("mejor", "¿mejor en qué métrica exactamente?"),
    ("simple", "¿qué se puede omitir y qué no? Nómbralo."),
    ("sencillo", "¿qué se puede omitir y qué no? Nómbralo."),
    ("sencilla", "¿qué se puede omitir y qué no? Nómbralo."),
    ("luego", "¿para cuándo? Ponle fecha u orden concreto."),
    ("despues", "¿para cuándo? Ponle fecha u orden concreto."),
    ("eficiente", "¿eficiente en qué recurso: CPU, memoria, red?"),
    ("moderno", "¿qué stack o versión define «moderno» aquí?"),
    ("moderna", "¿qué stack o versión define «moderno» aquí?"),
    ("intuitivo", "¿intuitivo para quién? Describe al usuario."),
    ("intuitiva", "¿intuitivo para quién? Describe al usuario."),
    ("facil", "¿fácil medido cómo? (pasos, tiempo, clics)"),
    ("robusto", "¿robusto ante qué fallos concretos?"),
    ("robusta", "¿robusto ante qué fallos concretos?"),
    ("escalable", "¿escalable hasta qué carga o volumen?"),
    (
        "seguro",
        "¿seguro contra qué amenazas? Nombra el modelo de riesgo.",
    ),
    (
        "segura",
        "¿seguro contra qué amenazas? Nombra el modelo de riesgo.",
    ),
    ("optimo", "¿óptimo respecto a qué función de costo?"),
    ("optima", "¿óptimo respecto a qué función de costo?"),
    (
        "flexible",
        "¿flexible en qué ejes? Cambios esperados, no hipotéticos.",
    ),
    ("pronto", "¿para cuándo? Ponle fecha."),
    ("varios", "¿cuántos exactamente?"),
    ("varias", "¿cuántas exactamente?"),
    ("algunos", "¿cuántos y cuáles?"),
    ("algunas", "¿cuántas y cuáles?"),
];

/// Verbos que anclan la oración-tarea (núcleo imperativo del dictado).
const IMPERATIVE_ANCHORS: &[&str] = &[
    "haz",
    "hazme",
    "hagame",
    "crea",
    "creame",
    "escribe",
    "escribeme",
    "genera",
    "generame",
    "implementa",
    "agrega",
    "agregame",
    "anade",
    "corrige",
    "arregla",
    "repara",
    "valida",
    "refactoriza",
    "cambia",
    "muestra",
    "muestrame",
    "construye",
    "arma",
    "armame",
    "disena",
    "programa",
    "calcula",
    "convierte",
    "traduce",
    "documenta",
    "revisa",
    "optimiza",
    "necesito",
    "quiero",
];

/// Muletillas de arranque que se recortan del inicio de la tarea.
const LEADING_FILLERS: &[&str] = &[
    "ya", "po", "yapo", "oye", "bueno", "dale", "entonces", "mira", "a", "ver", "porfa",
    "porfavor", "por", "favor", "ahora", "primero",
];

fn is_task_anchor(key: &str) -> bool {
    IMPERATIVE_ANCHORS.contains(&key)
}

/// Divide el dictado en oraciones por puntuación fuerte.
fn split_sentences(text: &str) -> Vec<String> {
    text.split(['.', '!', '?', ';', '\n'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Encuentra las advertencias de vaguedad sobre el texto completo.
fn lint_ambiguities(text: &str) -> Vec<AmbiguityWarning> {
    let mut warnings: Vec<AmbiguityWarning> = Vec::new();
    for token in text.split_whitespace() {
        let key = build_match_key(token);
        if key.is_empty() {
            continue;
        }
        if let Some((_, question)) = VAGUE_TERMS.iter().find(|(term, _)| *term == key) {
            let clean = token.trim_matches(|c: char| !c.is_alphanumeric());
            if !warnings.iter().any(|w| build_match_key(&w.term) == key) {
                warnings.push(AmbiguityWarning {
                    term: clean.to_string(),
                    question: (*question).to_string(),
                });
            }
        }
    }
    warnings
}

/// Separa la oración-tarea de sus cláusulas de requisito («… y que …»).
fn split_task_and_requirements(sentence: &str) -> (String, Vec<String>) {
    let words: Vec<&str> = sentence.split_whitespace().collect();

    // Recorta muletillas de arranque hasta el ancla imperativa.
    let mut start = 0;
    for (i, w) in words.iter().enumerate() {
        let key = build_match_key(w);
        if is_task_anchor(&key) {
            start = i;
            break;
        }
        if !LEADING_FILLERS.contains(&key.as_str()) {
            break; // palabra con contenido antes del ancla: no recortar más
        }
        start = i + 1;
    }
    let trimmed = words[start..].join(" ");

    // Las cláusulas «y que …» / «pero que …» posteriores al primer «que»
    // se vuelven requisitos separados.
    let mut parts: Vec<String> = Vec::new();
    let mut task = trimmed.clone();
    for separator in [" y que ", " pero que ", " aunque sea "] {
        if let Some(idx) = task.find(separator) {
            let rest = task[idx + separator.len()..].to_string();
            task.truncate(idx);
            parts.push(format!("que {rest}"));
        }
    }
    (task.trim().to_string(), parts)
}

/// Tipo de artefacto pedido, para afinar la sección FORMATO.
fn detect_artifact(text: &str) -> Option<&'static str> {
    for token in text.split_whitespace() {
        match build_match_key(token).as_str() {
            "funcion" => return Some("una función con firma clara y casos borde cubiertos"),
            "componente" => return Some("un componente autocontenido con sus props tipadas"),
            "clase" => return Some("una clase con su interfaz pública documentada"),
            "script" => return Some("un script ejecutable con sus argumentos documentados"),
            "consulta" | "query" => return Some("la consulta con un ejemplo de resultado"),
            "test" | "tests" | "prueba" | "pruebas" => {
                return Some("tests con nombres que describan el caso")
            }
            _ => {}
        }
    }
    None
}

/// Compila el dictado crudo en un prompt estructurado + advertencias.
pub fn compile_text(
    transcript: &str,
    project_name: Option<&str>,
    project_terms_mentioned: &[String],
) -> CompiledPrompt {
    let warnings = lint_ambiguities(transcript);
    let sentences = split_sentences(transcript);

    // La oración-tarea: la primera que contenga un ancla imperativa; si no
    // hay, la primera oración.
    let task_sentence = sentences
        .iter()
        .find(|s| {
            s.split_whitespace()
                .any(|w| is_task_anchor(&build_match_key(w)))
        })
        .or_else(|| sentences.first())
        .cloned()
        .unwrap_or_default();

    let (task, mut requirements) = split_task_and_requirements(&task_sentence);

    // Las demás oraciones se vuelven requisitos adicionales.
    for s in &sentences {
        if *s != task_sentence {
            requirements.push(s.clone());
        }
    }

    let mut md = String::from("## CONTEXTO\n");
    match project_name {
        Some(name) => md.push_str(&format!("- Proyecto activo: {name}\n")),
        None => md.push_str("- Proyecto: (sin proyecto activo en el Diccionario Vivo)\n"),
    }
    if !project_terms_mentioned.is_empty() {
        md.push_str(&format!(
            "- Términos del repo mencionados: {}\n",
            project_terms_mentioned.join(", ")
        ));
    }

    md.push_str("\n## TAREA\n");
    md.push_str(&task);
    md.push('\n');

    if !requirements.is_empty() {
        md.push_str("\n## REQUISITOS\n");
        for r in &requirements {
            let has_warning = r.split_whitespace().any(|w| {
                warnings
                    .iter()
                    .any(|wa| build_match_key(&wa.term) == build_match_key(w))
            });
            let mark = if has_warning { " ⚠" } else { "" };
            md.push_str(&format!("- {r}{mark}\n"));
        }
    }

    md.push_str("\n## FORMATO\n");
    match detect_artifact(transcript) {
        Some(artifact) => md.push_str(&format!("- Entrega {artifact}.\n")),
        None => md.push_str("- Entrega el resultado en un bloque, con una explicación breve.\n"),
    }
    md.push_str("- Si algo es ambiguo, pregunta antes de asumir.\n");

    if !warnings.is_empty() {
        md.push_str("\n## ⚠ AMBIGÜEDADES DETECTADAS\n");
        for w in &warnings {
            md.push_str(&format!("- «{}»: {}\n", w.term, w.question));
        }
    }

    CompiledPrompt {
        markdown: md,
        warnings,
    }
}

/// Comando: compila un dictado en prompt estructurado. El CONTEXTO se
/// enriquece con el proyecto activo del Diccionario Vivo y los términos del
/// repo que aparezcan en el texto.
#[tauri::command]
#[specta::specta]
pub fn compile_prompt(app: AppHandle, text: String) -> CompiledPrompt {
    let settings = crate::settings::get_settings(&app);

    let project_name = settings.dictionary_project.as_ref().map(|p| {
        std::path::Path::new(&p.path)
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| p.path.clone())
    });

    // Términos del diccionario mencionados en el dictado (clave normalizada).
    let mentioned = crate::dictionary::terms_mentioned_in(&text);

    compile_text(&text, project_name.as_deref(), &mentioned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_criterion_yapo_rut_rapida() {
        // «ya po hazme una función que valide el rut y que ojalá ande rápida»
        let result = compile_text(
            "Ya po hazme una función que valide el rut y que ojalá ande rápida",
            Some("repo-prueba"),
            &["validarRut".to_string()],
        );

        // ≥2 advertencias correctas: «ojalá» y «rápida».
        assert!(
            result.warnings.len() >= 2,
            "esperaba ≥2 ⚠, hubo {}: {:?}",
            result.warnings.len(),
            result.warnings
        );
        let terms: Vec<String> = result
            .warnings
            .iter()
            .map(|w| build_match_key(&w.term))
            .collect();
        assert!(terms.contains(&"ojala".to_string()));
        assert!(terms.contains(&"rapida".to_string()));

        // Estructura: tarea sin el «Ya po», requisito marcado, contexto del repo.
        assert!(result
            .markdown
            .contains("## TAREA\nhazme una función que valide el rut"));
        assert!(result.markdown.contains("- que ojalá ande rápida ⚠"));
        assert!(result.markdown.contains("repo-prueba"));
        assert!(result.markdown.contains("validarRut"));
        assert!(result.markdown.contains("una función con firma clara"));
    }

    #[test]
    fn no_warnings_on_precise_text() {
        let result = compile_text(
            "Crea un endpoint POST /usuarios que valide el correo con la RFC 5322",
            None,
            &[],
        );
        assert!(result.warnings.is_empty(), "{:?}", result.warnings);
        assert!(result.markdown.contains("## TAREA"));
    }

    #[test]
    fn vague_terms_dedupe_and_fold_accents() {
        let result = compile_text("que sea rápido, rápido y ojala simple", None, &[]);
        let keys: Vec<String> = result
            .warnings
            .iter()
            .map(|w| build_match_key(&w.term))
            .collect();
        // «rápido» solo una vez pese a repetirse; «ojala» sin tilde igual cae.
        assert_eq!(keys.iter().filter(|k| k.as_str() == "rapido").count(), 1);
        assert!(keys.contains(&"ojala".to_string()));
        assert!(keys.contains(&"simple".to_string()));
    }
}
