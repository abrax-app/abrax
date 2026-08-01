//! Diccionario Vivo (F5): ABRAX aprende la jerga de tu código.
//!
//! Los modelos de voz no conocen tus identificadores ni tus ramas; este
//! módulo indexa el proyecto activo (respetando `.gitignore` vía la crate
//! `ignore`) y alimenta el post-proceso con tres capas:
//! 1. Reemplazos exactos por token (settings, `text::apply_custom_replacements`).
//! 2. Join multi-token: "use auth store" → `useAuthStore` (`apply_multi_token_join`).
//! 3. Corrección difusa de tokens sueltos contra los términos indexados, con
//!    stoplist es/en como veto: "ruta" es español válido y jamás se corrige.
//!
//! Privacidad: el índice vive en el datadir local (`dictionary_index.json`);
//! nada del repo sale del equipo.

use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use specta::Type;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::AppHandle;

use crate::audio_toolkit::{apply_dictionary_fuzzy, apply_multi_token_join, build_match_key};
use crate::settings::{self, AppSettings, DictionaryProject};

const INDEX_FILE: &str = "dictionary_index.json";
const INDEX_VERSION: u32 = 1;
const MAX_TERMS: usize = 5000;
const MIN_TERM_LEN: usize = 3;
/// Archivos de código más grandes que esto se saltan (minificados, generados).
const MAX_FILE_BYTES: u64 = 1_000_000;
const MAX_FILES: usize = 20_000;
const SAMPLE_SIZE: usize = 30;

/// Extensiones que el indexador considera código fuente.
const CODE_EXTENSIONS: &[&str] = &[
    "rs", "ts", "tsx", "js", "jsx", "mjs", "cjs", "py", "go", "java", "kt", "kts", "cs", "c", "h",
    "cpp", "hpp", "cc", "hh", "rb", "php", "swift", "m", "mm", "vue", "svelte", "scala", "dart",
    "lua", "sql", "sh", "bash", "zsh", "ps1", "psm1", "proto", "graphql", "prisma", "toml",
];

static IDENTIFIER_RE: Lazy<regex::Regex> =
    Lazy::new(|| regex::Regex::new(r"[A-Za-z_][A-Za-z0-9_]{2,}").unwrap());

/// Palabras comunes del español (claves normalizadas: minúsculas, sin tildes).
/// Un token protegido jamás se corrige hacia un término del diccionario:
/// aquí vive la defensa de "ruta", "uso", "casa"…
const STOPLIST_ES: &[&str] = &[
    // «si»/«no» cubren también «sí» (build_match_key pliega la tilde): la
    // memoria de correcciones no debe aprender orígenes hechos de estas.
    "si",
    "no",
    "los",
    "las",
    "una",
    "unos",
    "unas",
    "del",
    "que",
    "por",
    "para",
    "con",
    "sin",
    "sobre",
    "entre",
    "hasta",
    "desde",
    "como",
    "cuando",
    "donde",
    "quien",
    "cual",
    "esto",
    "esta",
    "este",
    "estos",
    "estas",
    "eso",
    "esa",
    "ese",
    "esos",
    "esas",
    "aquel",
    "aquella",
    "ser",
    "estar",
    "haber",
    "tener",
    "hacer",
    "poder",
    "decir",
    "ver",
    "dar",
    "saber",
    "querer",
    "llegar",
    "pasar",
    "deber",
    "poner",
    "parecer",
    "quedar",
    "creer",
    "hablar",
    "llevar",
    "dejar",
    "seguir",
    "encontrar",
    "llamar",
    "venir",
    "pensar",
    "salir",
    "volver",
    "tomar",
    "conocer",
    "vivir",
    "sentir",
    "tratar",
    "mirar",
    "contar",
    "empezar",
    "esperar",
    "buscar",
    "existir",
    "entrar",
    "trabajar",
    "escribir",
    "perder",
    "producir",
    "ocurrir",
    "entender",
    "pedir",
    "recibir",
    "recordar",
    "terminar",
    "permitir",
    "aparecer",
    "conseguir",
    "comenzar",
    "servir",
    "sacar",
    "necesitar",
    "mantener",
    "resultar",
    "leer",
    "caer",
    "cambiar",
    "presentar",
    "crear",
    "abrir",
    "considerar",
    "son",
    "era",
    "fue",
    "estan",
    "esta",
    "hay",
    "tiene",
    "tienen",
    "hace",
    "hacen",
    "puede",
    "pueden",
    "dice",
    "dicen",
    "vamos",
    "voy",
    "vas",
    "van",
    "muy",
    "mas",
    "menos",
    "tambien",
    "ademas",
    "pero",
    "porque",
    "aunque",
    "mientras",
    "entonces",
    "luego",
    "ahora",
    "antes",
    "despues",
    "siempre",
    "nunca",
    "aqui",
    "alli",
    "ahi",
    "casa",
    "cosa",
    "cosas",
    "vida",
    "tiempo",
    "dia",
    "dias",
    "vez",
    "veces",
    "parte",
    "forma",
    "manera",
    "ejemplo",
    "caso",
    "casos",
    "lugar",
    "momento",
    "persona",
    "personas",
    "gente",
    "mundo",
    "pais",
    "ciudad",
    "nombre",
    "numero",
    "palabra",
    "palabras",
    "texto",
    "archivo",
    "carpeta",
    "ruta",
    "rutas",
    "uso",
    "usos",
    "datos",
    "dato",
    "valor",
    "valores",
    "todo",
    "toda",
    "todos",
    "todas",
    "nada",
    "algo",
    "alguien",
    "nadie",
    "cada",
    "otro",
    "otra",
    "otros",
    "otras",
    "mismo",
    "misma",
    "nuevo",
    "nueva",
    "bueno",
    "buena",
    "gran",
    "grande",
    "pequeno",
    "mejor",
    "peor",
    "primero",
    "primera",
    "ultimo",
    "ultima",
    "solo",
    "sola",
    "bien",
    "mal",
    "favor",
    "gracias",
    "hola",
    "chao",
    "listo",
    "lista",
    "claro",
    "obvio",
];

/// Palabras comunes del inglés + palabras clave de programación frecuentes
/// (aparecen millones de veces en cualquier repo; corrigen hacia ellas = ruido).
const STOPLIST_EN: &[&str] = &[
    "the",
    "and",
    "for",
    "are",
    "but",
    "not",
    "you",
    "all",
    "can",
    "had",
    "her",
    "was",
    "one",
    "our",
    "out",
    "day",
    "get",
    "has",
    "him",
    "his",
    "how",
    "man",
    "new",
    "now",
    "old",
    "see",
    "two",
    "way",
    "who",
    "boy",
    "did",
    "its",
    "let",
    "put",
    "say",
    "she",
    "too",
    "use",
    "that",
    "with",
    "have",
    "this",
    "will",
    "your",
    "from",
    "they",
    "know",
    "want",
    "been",
    "good",
    "much",
    "some",
    "time",
    "very",
    "when",
    "come",
    "here",
    "just",
    "like",
    "long",
    "make",
    "many",
    "more",
    "only",
    "over",
    "such",
    "take",
    "than",
    "them",
    "well",
    "were",
    "what",
    "which",
    "while",
    "would",
    "there",
    "their",
    "about",
    "other",
    "into",
    "could",
    "first",
    "after",
    "should",
    "because",
    "these",
    "those",
    "then",
    "also",
    "each",
    "most",
    "must",
    "before",
    "does",
    "doing",
    "done",
    "going",
    "things",
    "thing",
    "still",
    "even",
    "back",
    "function",
    "return",
    "returns",
    "const",
    "let",
    "var",
    "class",
    "import",
    "export",
    "default",
    "public",
    "private",
    "protected",
    "static",
    "void",
    "int",
    "float",
    "double",
    "string",
    "str",
    "bool",
    "boolean",
    "true",
    "false",
    "null",
    "none",
    "nil",
    "self",
    "this",
    "new",
    "delete",
    "mod",
    "pub",
    "impl",
    "struct",
    "enum",
    "match",
    "async",
    "await",
    "fn",
    "type",
    "types",
    "interface",
    "extends",
    "implements",
    "super",
    "try",
    "catch",
    "finally",
    "throw",
    "throws",
    "switch",
    "case",
    "break",
    "continue",
    "while",
    "loop",
    "else",
    "elif",
    "end",
    "begin",
    "print",
    "println",
    "log",
    "logs",
    "debug",
    "info",
    "warn",
    "warning",
    "error",
    "errors",
    "test",
    "tests",
    "assert",
    "main",
    "master",
    "index",
    "value",
    "values",
    "data",
    "item",
    "items",
    "list",
    "lists",
    "array",
    "object",
    "objects",
    "key",
    "keys",
    "name",
    "names",
    "file",
    "files",
    "path",
    "paths",
    "line",
    "lines",
    "code",
    "codes",
    "app",
    "apps",
    "user",
    "users",
    "result",
    "results",
    "option",
    "options",
    "some",
    "okay",
    "state",
    "props",
    "async",
    "sync",
    "get",
    "set",
    "add",
    "remove",
    "update",
    "create",
    "read",
    "write",
    "open",
    "close",
    "start",
    "stop",
    "run",
    "runs",
    "call",
    "calls",
    "size",
    "len",
    "length",
    "count",
    "total",
    "next",
    "prev",
    "last",
    "current",
    "temp",
    "tmp",
    "src",
    "dist",
    "build",
    "target",
    "node",
    "modules",
];

static STOPLIST: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    STOPLIST_ES
        .iter()
        .chain(STOPLIST_EN.iter())
        .copied()
        .collect()
});

/// ¿La clave normalizada corresponde a una palabra común es/en?
pub fn is_stopword(key: &str) -> bool {
    STOPLIST.contains(key)
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Type)]
#[serde(rename_all = "snake_case")]
pub enum TermSource {
    Code,
    Branch,
    Path,
}

#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct DictTerm {
    pub term: String,
    pub source: TermSource,
    pub count: u32,
}

#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct DictionaryIndex {
    pub version: u32,
    pub project_path: String,
    pub indexed_at_ms: f64,
    pub terms: Vec<DictTerm>,
}

#[derive(Serialize, Deserialize, Clone, Debug, Type)]
pub struct DictionaryStats {
    pub project_path: String,
    pub term_count: u32,
    pub indexed_at_ms: f64,
    pub sample: Vec<DictTerm>,
}

/// Índice precompilado para el post-proceso: lookup de claves fusionadas para
/// el join multi-token, y el subconjunto de términos aptos para fuzzy.
pub struct LoadedDictionary {
    /// Términos "distintivos" (compuestos, con mayúsculas/dígitos, o largos):
    /// solo hacia estos corrige el fuzzy. Un identificador que es una palabra
    /// llana en minúsculas ("facturas") NUNCA atrae correcciones — evitaría
    /// que "factura" dictada terminara en "facturas".
    pub fuzzy_terms: Vec<String>,
    pub joined_lookup: HashMap<String, String>,
    /// Todos los términos indexados, para consultas de mención (compilador).
    pub all_terms: Vec<String>,
}

static ACTIVE: Lazy<RwLock<Option<Arc<LoadedDictionary>>>> = Lazy::new(|| RwLock::new(None));

/// ¿El término tiene ≥2 partes (camelCase, snake_case, kebab, slash, dígito)?
fn term_has_parts(term: &str) -> bool {
    let mut parts = 1u32;
    let mut prev_lower = false;
    for c in term.chars() {
        if matches!(c, '_' | '-' | '/' | '.') {
            parts += 1;
            prev_lower = false;
            continue;
        }
        if c.is_ascii_digit() {
            return true;
        }
        if c.is_uppercase() && prev_lower {
            parts += 1;
        }
        prev_lower = c.is_lowercase();
    }
    parts >= 2
}

/// ¿El término es candidato del fuzzy? Ver doc de `LoadedDictionary`.
fn is_distinctive(term: &str) -> bool {
    term_has_parts(term) || term.chars().any(|c| c.is_uppercase()) || term.len() >= 10
}

fn compile(index: &DictionaryIndex) -> LoadedDictionary {
    let mut joined_lookup: HashMap<String, String> = HashMap::new();
    let mut fuzzy_terms = Vec::new();

    for t in &index.terms {
        if is_distinctive(&t.term) {
            fuzzy_terms.push(t.term.clone());
        }
        if term_has_parts(&t.term) {
            let key = build_match_key(&t.term);
            if key.len() >= 6 {
                // Ante colisión de claves gana el más frecuente (el índice
                // llega ordenado por relevancia).
                joined_lookup.entry(key).or_insert_with(|| t.term.clone());
            }
        }
    }

    LoadedDictionary {
        fuzzy_terms,
        joined_lookup,
        all_terms: index.terms.iter().map(|t| t.term.clone()).collect(),
    }
}

/// Términos del diccionario activo mencionados en el texto (comparación por
/// clave normalizada). Alimenta el CONTEXTO del compilador de prompts.
pub fn terms_mentioned_in(text: &str) -> Vec<String> {
    let Some(dict) = ACTIVE.read().unwrap().clone() else {
        return Vec::new();
    };
    let keys: HashSet<String> = text
        .split_whitespace()
        .map(build_match_key)
        .filter(|k| k.len() >= MIN_TERM_LEN)
        .collect();
    let mut mentioned: Vec<String> = dict
        .all_terms
        .iter()
        .filter(|term| keys.contains(&build_match_key(term)))
        .cloned()
        .collect();
    mentioned.sort();
    mentioned.dedup();
    mentioned
}

fn index_path(app: &AppHandle) -> Result<PathBuf, String> {
    crate::portable::app_data_dir(app)
        .map(|d| d.join(INDEX_FILE))
        .map_err(|e| format!("app data dir: {e}"))
}

fn save_index(app: &AppHandle, index: &DictionaryIndex) -> Result<(), String> {
    let path = index_path(app)?;
    let json = serde_json::to_string(index).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| format!("writing {}: {e}", path.display()))
}

fn load_index(app: &AppHandle) -> Option<DictionaryIndex> {
    let path = index_path(app).ok()?;
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Carga (o descarga) el índice activo según settings. Se llama al arrancar y
/// tras indexar/activar/desactivar.
pub fn refresh_active(app: &AppHandle) {
    let settings = settings::get_settings(app);
    let loaded = match &settings.dictionary_project {
        Some(DictionaryProject { path, enabled, .. }) if *enabled => load_index(app)
            .filter(|idx| idx.project_path == *path)
            .map(|idx| Arc::new(compile(&idx))),
        _ => None,
    };
    let count = loaded.as_ref().map(|d| d.fuzzy_terms.len()).unwrap_or(0);
    *ACTIVE.write().unwrap() = loaded;
    log::debug!("dictionary: active index refreshed ({count} fuzzy terms)");
}

/// Aplica el Diccionario Vivo al texto transcrito (no-op si está apagado):
/// primero el join multi-token exacto, luego el fuzzy protegido por stoplist.
pub fn apply_active(text: &str, settings: &AppSettings) -> String {
    let enabled = settings
        .dictionary_project
        .as_ref()
        .is_some_and(|p| p.enabled);
    if !enabled {
        return text.to_string();
    }
    let Some(dict) = ACTIVE.read().unwrap().clone() else {
        return text.to_string();
    };
    let joined = apply_multi_token_join(text, &dict.joined_lookup);
    apply_dictionary_fuzzy(
        &joined,
        &dict.fuzzy_terms,
        settings.word_correction_threshold,
        &|key| is_stopword(key),
    )
}

/// Lee las ramas locales del repo: `refs/heads/**` sueltas + `packed-refs`.
/// Soporta worktrees (`.git` como archivo con `gitdir:`); si no hay git,
/// devuelve vacío sin error.
fn read_git_branches(project: &Path) -> Vec<String> {
    let dot_git = project.join(".git");
    let git_dir: PathBuf = if dot_git.is_file() {
        match std::fs::read_to_string(&dot_git) {
            Ok(content) => {
                let Some(rest) = content.trim().strip_prefix("gitdir:") else {
                    return Vec::new();
                };
                let p = PathBuf::from(rest.trim());
                if p.is_absolute() {
                    p
                } else {
                    project.join(p)
                }
            }
            Err(_) => return Vec::new(),
        }
    } else if dot_git.is_dir() {
        dot_git
    } else {
        return Vec::new();
    };

    let mut branches: Vec<String> = Vec::new();

    fn walk_heads(dir: &Path, prefix: &str, out: &mut Vec<String>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let joined = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let path = entry.path();
            if path.is_dir() {
                walk_heads(&path, &joined, out);
            } else {
                out.push(joined);
            }
        }
    }
    walk_heads(&git_dir.join("refs").join("heads"), "", &mut branches);

    // Las ramas empaquetadas por `git gc` viven en packed-refs. Un worktree
    // guarda sus refs en el gitdir común (commondir), pero el caso base cubre
    // el MVP.
    if let Ok(packed) = std::fs::read_to_string(git_dir.join("packed-refs")) {
        for line in packed.lines() {
            if let Some(idx) = line.find(" refs/heads/") {
                branches.push(line[idx + " refs/heads/".len()..].trim().to_string());
            }
        }
    }

    branches.sort();
    branches.dedup();
    branches.retain(|b| b.len() >= MIN_TERM_LEN && !is_stopword(&build_match_key(b)));
    branches
}

/// Recorre el proyecto y construye el índice: identificadores del código
/// (ranking por frecuencia), ramas git y nombres de archivo. Respeta
/// `.gitignore`/`.git/info/exclude`/global gitignore y omite ocultos.
pub fn build_index(project_path: &str) -> Result<DictionaryIndex, String> {
    let project = Path::new(project_path);
    if !project.is_dir() {
        return Err(format!("No es una carpeta: {project_path}"));
    }

    // término → (fuente, conteo); la clave es el término tal cual (case-sensitive).
    let mut counts: HashMap<String, (TermSource, u32)> = HashMap::new();
    let mut files_indexed = 0usize;

    let walker = ignore::WalkBuilder::new(project)
        .follow_links(false)
        // Respetar .gitignore aunque la carpeta no sea (aún) un repo git.
        .require_git(false)
        .build();

    for entry in walker.flatten() {
        if files_indexed >= MAX_FILES {
            break;
        }
        let path = entry.path();
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }

        // Nombres de archivo como términos (RecordingOverlay.tsx → RecordingOverlay).
        if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
            if stem.len() >= MIN_TERM_LEN
                && IDENTIFIER_RE.is_match(stem)
                && !is_stopword(&build_match_key(stem))
            {
                counts
                    .entry(stem.to_string())
                    .or_insert((TermSource::Path, 0))
                    .1 += 1;
            }
        }

        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_lowercase();
        if !CODE_EXTENSIONS.contains(&ext.as_str()) {
            continue;
        }
        if entry.metadata().map(|m| m.len()).unwrap_or(u64::MAX) > MAX_FILE_BYTES {
            continue;
        }
        let Ok(content) = std::fs::read_to_string(path) else {
            continue; // binario o no-UTF8: fuera
        };
        files_indexed += 1;

        for m in IDENTIFIER_RE.find_iter(&content) {
            let ident = m.as_str();
            let key = build_match_key(ident);
            if key.len() < MIN_TERM_LEN || is_stopword(&key) {
                continue;
            }
            let entry = counts
                .entry(ident.to_string())
                .or_insert((TermSource::Code, 0));
            entry.0 = TermSource::Code;
            entry.1 += 1;
        }
    }

    // Ramas git: pocas y valiosas — entran siempre, por delante del cap.
    let mut terms: Vec<DictTerm> = read_git_branches(project)
        .into_iter()
        .map(|b| DictTerm {
            term: b,
            source: TermSource::Branch,
            count: 1,
        })
        .collect();

    let mut code_terms: Vec<DictTerm> = counts
        .into_iter()
        .map(|(term, (source, count))| DictTerm {
            term,
            source,
            count,
        })
        .collect();
    code_terms.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.term.cmp(&b.term)));

    let remaining = MAX_TERMS.saturating_sub(terms.len());
    code_terms.truncate(remaining);
    terms.extend(code_terms);

    let indexed_at_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as f64;

    Ok(DictionaryIndex {
        version: INDEX_VERSION,
        project_path: project_path.to_string(),
        indexed_at_ms,
        terms,
    })
}

fn stats_from(index: &DictionaryIndex) -> DictionaryStats {
    DictionaryStats {
        project_path: index.project_path.clone(),
        term_count: index.terms.len() as u32,
        indexed_at_ms: index.indexed_at_ms,
        sample: index.terms.iter().take(SAMPLE_SIZE).cloned().collect(),
    }
}

// ───────────────────────────── Comandos IPC ─────────────────────────────

/// Indexa (o re-indexa) el proyecto y lo deja como diccionario activo.
#[tauri::command]
#[specta::specta]
pub async fn index_project(app: AppHandle, path: String) -> Result<DictionaryStats, String> {
    let index = {
        let path = path.clone();
        tauri::async_runtime::spawn_blocking(move || build_index(&path))
            .await
            .map_err(|e| format!("index task: {e}"))??
    };

    save_index(&app, &index)?;

    let mut settings = settings::get_settings(&app);
    settings.dictionary_project = Some(DictionaryProject {
        path,
        enabled: true,
        last_indexed_ms: Some(index.indexed_at_ms),
    });
    settings::write_settings(&app, settings);

    refresh_active(&app);
    log::info!(
        "dictionary: indexed {} terms from {}",
        index.terms.len(),
        index.project_path
    );
    Ok(stats_from(&index))
}

/// Estadísticas del índice activo para la UI (contador + muestra de chips).
#[tauri::command]
#[specta::specta]
pub fn get_dictionary_stats(app: AppHandle) -> Option<DictionaryStats> {
    let settings = settings::get_settings(&app);
    let project = settings.dictionary_project.as_ref()?;
    load_index(&app)
        .filter(|idx| idx.project_path == project.path)
        .map(|idx| stats_from(&idx))
}

/// Enciende/apaga «aprender de este proyecto» sin perder el índice.
#[tauri::command]
#[specta::specta]
pub fn set_dictionary_enabled(app: AppHandle, enabled: bool) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    match settings.dictionary_project.as_mut() {
        Some(project) => project.enabled = enabled,
        None => return Err("No hay proyecto de diccionario configurado".into()),
    }
    settings::write_settings(&app, settings);
    refresh_active(&app);
    Ok(())
}

/// Actualiza la lista de reemplazos exactos (espejo de update_custom_words).
#[tauri::command]
#[specta::specta]
pub fn update_custom_replacements(
    app: AppHandle,
    replacements: Vec<settings::CustomReplacement>,
) -> Result<(), String> {
    let mut settings = settings::get_settings(&app);
    settings.custom_replacements = replacements;
    settings::write_settings(&app, settings);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_project(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("abrax-dict-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn indexes_identifiers_branches_and_respects_gitignore() {
        let dir = temp_project("walk");
        fs::write(
            dir.join("auth.ts"),
            "export const useAuthStore = create(); function validarRut(rutValue) { return rutValue; }",
        )
        .unwrap();
        fs::create_dir_all(dir.join("dist")).unwrap();
        fs::write(
            dir.join("dist").join("bundle.js"),
            "const secretIgnored = 1;",
        )
        .unwrap();
        fs::write(dir.join(".gitignore"), "dist/\n").unwrap();
        // repo git falso: rama suelta + packed-refs (la crate `ignore` solo
        // honra .gitignore dentro de un repo git real)
        fs::create_dir_all(dir.join(".git").join("refs").join("heads").join("hotfix")).unwrap();
        fs::write(
            dir.join(".git")
                .join("refs")
                .join("heads")
                .join("hotfix")
                .join("token-expiry"),
            "0000000000000000000000000000000000000000\n",
        )
        .unwrap();
        fs::write(
            dir.join(".git").join("packed-refs"),
            "# pack-refs with: peeled fully-peeled sorted\n0000000000000000000000000000000000000000 refs/heads/feature/esfera-viva\n",
        )
        .unwrap();
        fs::write(dir.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();

        let index = build_index(dir.to_str().unwrap()).unwrap();
        let terms: Vec<&str> = index.terms.iter().map(|t| t.term.as_str()).collect();

        assert!(
            terms.contains(&"useAuthStore"),
            "identificador camelCase: {terms:?}"
        );
        assert!(terms.contains(&"validarRut"), "identificador de función");
        assert!(terms.contains(&"hotfix/token-expiry"), "rama suelta");
        assert!(terms.contains(&"feature/esfera-viva"), "rama packed-refs");
        assert!(
            !terms.contains(&"secretIgnored"),
            "dist/ está gitignored: {terms:?}"
        );
        // "const"/"function"/"return" son stopwords de programación
        assert!(!terms.contains(&"const"));
        assert!(!terms.contains(&"function"));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn compiled_dictionary_joins_and_protects() {
        let index = DictionaryIndex {
            version: 1,
            project_path: "x".into(),
            indexed_at_ms: 0.0,
            terms: vec![
                DictTerm {
                    term: "useAuthStore".into(),
                    source: TermSource::Code,
                    count: 10,
                },
                DictTerm {
                    term: "hotfix/token-expiry".into(),
                    source: TermSource::Branch,
                    count: 1,
                },
                DictTerm {
                    term: "facturas".into(), // palabra llana: fuera del fuzzy
                    source: TermSource::Code,
                    count: 50,
                },
            ],
        };
        let dict = compile(&index);
        assert_eq!(
            dict.joined_lookup.get("useauthstore").map(String::as_str),
            Some("useAuthStore")
        );
        assert_eq!(
            dict.joined_lookup
                .get("hotfixtokenexpiry")
                .map(String::as_str),
            Some("hotfix/token-expiry")
        );
        assert!(dict.fuzzy_terms.contains(&"useAuthStore".to_string()));
        assert!(
            !dict.fuzzy_terms.contains(&"facturas".to_string()),
            "una palabra llana en minúsculas no atrae correcciones"
        );

        // El flujo completo del criterio estrella: join exacto + ruta protegida.
        let text = "usa use auth store y muéstrame la ruta";
        let joined = apply_multi_token_join(text, &dict.joined_lookup);
        let out =
            crate::audio_toolkit::apply_dictionary_fuzzy(&joined, &dict.fuzzy_terms, 0.18, &|k| {
                is_stopword(k)
            });
        assert_eq!(out, "usa useAuthStore y muéstrame la ruta");
    }
}
