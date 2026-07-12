use natural::phonetics::soundex;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use strsim::levenshtein;

/// Builds an n-gram string by cleaning and concatenating words
///
/// Strips punctuation from each word, lowercases, and joins without spaces.
/// This allows matching "Charge B" against "ChargeBee".
fn build_ngram(words: &[&str]) -> String {
    words
        .iter()
        .map(|w| build_match_key(w))
        .collect::<Vec<_>>()
        .concat()
}

/// Pliega tildes latinas a su vocal base (F5.2): "cachái" y "cachai" deben
/// compararse iguales. La ñ se conserva — es letra distinta en español, no
/// una vocal acentuada.
fn fold_accent(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' => 'a',
        'é' | 'è' | 'ë' | 'ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' => 'u',
        _ => c,
    }
}

/// Clave de comparación normalizada (F5.2): solo alfanuméricos, minúsculas y
/// sin tildes. Muletillas y diccionario comparan siempre sobre esta clave.
pub fn build_match_key(word: &str) -> String {
    word.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .map(fold_accent)
        .collect()
}

/// Núcleo de un token sin su puntuación adyacente (prefijo/sufijo).
fn token_core(word: &str) -> &str {
    let (prefix, suffix) = extract_punctuation(word);
    &word[prefix.len()..word.len() - suffix.len()]
}

/// Reemplazos exactos por token (F5.1) — la vía inmune a colisiones del
/// Diccionario Vivo. Matching por token EXACTO y case-sensitive sobre el
/// núcleo del token (la puntuación adyacente se conserva): "Ruth" → "rut"
/// dispara solo con el token "Ruth"; "ruta" es otro token y jamás se toca.
pub fn apply_custom_replacements(text: &str, replacements: &[(String, String)]) -> String {
    if replacements.is_empty() {
        return text.to_string();
    }

    text.split_whitespace()
        .map(|word| {
            let core = token_core(word);
            for (from, to) in replacements {
                if core == from {
                    let (prefix, suffix) = extract_punctuation(word);
                    return format!("{prefix}{to}{suffix}");
                }
            }
            word.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Join multi-token (F5.4) — el killer Spanglish del Diccionario Vivo.
///
/// Ventanas de 2–4 tokens transcritos que, concatenados y normalizados sin
/// separadores, coinciden con la clave de un término indexado, se reemplazan
/// por el término EXACTO del repo (sus separadores/case se respetan):
/// "use auth store" → `useAuthStore` · "hotfix token expiry" →
/// `hotfix/token-expiry`. El lookup llega ya normalizado (clave fusionada →
/// término original). La puntuación al interior de la ventana rompe la frase
/// (no se cruza una coma), y una clave fusionada corta (<6) nunca dispara.
pub fn apply_multi_token_join(text: &str, joined_lookup: &HashMap<String, String>) -> String {
    if joined_lookup.is_empty() {
        return text.to_string();
    }

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result: Vec<String> = Vec::with_capacity(words.len());
    let mut i = 0;

    while i < words.len() {
        let mut matched = false;

        for n in (2..=4usize).rev() {
            if i + n > words.len() {
                continue;
            }
            // Una coma/punto en un token intermedio marca límite de frase.
            if (0..n - 1).any(|j| !extract_punctuation(words[i + j]).1.is_empty()) {
                continue;
            }
            let joined: String = words[i..i + n].iter().map(|w| build_match_key(w)).collect();
            if joined.len() < 6 {
                continue;
            }
            if let Some(term) = joined_lookup.get(&joined) {
                let (prefix, _) = extract_punctuation(words[i]);
                let (_, suffix) = extract_punctuation(words[i + n - 1]);
                result.push(format!("{prefix}{term}{suffix}"));
                i += n;
                matched = true;
                break;
            }
        }

        if !matched {
            result.push(words[i].to_string());
            i += 1;
        }
    }

    result.join(" ")
}

/// Corrección difusa de tokens individuales contra los términos indexados del
/// proyecto (F5.3): misma métrica que custom_words (Levenshtein normalizada +
/// impulso Soundex, mismo umbral), con dos diferencias deliberadas:
/// - `is_protected` (stoplist es/en) veta candidatos que son palabras comunes:
///   "ruta" es español válido y JAMÁS se corrige a "rut", aunque la métrica dé.
/// - El reemplazo es el término EXACTO del repo (sin adaptar el case): el
///   valor está en producir `useAuthStore`, no "Useauthstore".
pub fn apply_dictionary_fuzzy(
    text: &str,
    terms: &[String],
    threshold: f64,
    is_protected: &dyn Fn(&str) -> bool,
) -> String {
    if terms.is_empty() {
        return text.to_string();
    }

    let term_keys: Vec<CustomWordMatchKey> = terms
        .iter()
        .enumerate()
        .flat_map(|(index, term)| build_custom_word_match_keys(term, index))
        .collect();

    text.split_whitespace()
        .map(|word| {
            let key = build_match_key(word);
            if key.len() < 3 || is_protected(&key) {
                return word.to_string();
            }
            if let Some((term, _score)) = find_best_match(&key, terms, &term_keys, threshold) {
                let (prefix, suffix) = extract_punctuation(word);
                return format!("{prefix}{term}{suffix}");
            }
            word.to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

struct CustomWordMatchKey {
    word_index: usize,
    key: String,
}

fn build_custom_word_match_keys(word: &str, word_index: usize) -> Vec<CustomWordMatchKey> {
    let primary_key = build_match_key(word);
    let mut keys = Vec::with_capacity(2);

    if !primary_key.is_empty() {
        keys.push(CustomWordMatchKey {
            word_index,
            key: primary_key.clone(),
        });
    }

    if word.contains('&') {
        let expanded_key = build_match_key(&word.replace('&', " and "));
        if !expanded_key.is_empty() && expanded_key != primary_key {
            keys.push(CustomWordMatchKey {
                word_index,
                key: expanded_key,
            });
        }
    }

    keys
}

/// Finds the best matching custom word for a candidate string
///
/// Uses Levenshtein distance and Soundex phonetic matching to find
/// the best match above the given threshold.
///
/// Limitación conocida (nota de diseño F5-fase2): Soundex es fonética
/// INGLESA — con español es-419 produce agrupaciones pobres (ll/y, rr, j/g).
/// Al abrir la fase 2 del Diccionario: evaluar una alternativa fonética
/// española o desactivar el impulso ×0.3 para términos del índice.
///
/// # Arguments
/// * `candidate` - The cleaned/lowercased candidate string to match
/// * `custom_words` - Original custom words (for returning the replacement)
/// * `custom_word_match_keys` - Normalized custom-word keys for comparison
/// * `threshold` - Maximum similarity score to accept
///
/// # Returns
/// The best matching custom word and its score, if any match was found
fn find_best_match<'a>(
    candidate: &str,
    custom_words: &'a [String],
    custom_word_match_keys: &[CustomWordMatchKey],
    threshold: f64,
) -> Option<(&'a String, f64)> {
    if candidate.is_empty() || candidate.len() > 50 {
        return None;
    }

    let mut best_match: Option<&String> = None;
    let mut best_score = f64::MAX;

    for custom_word_key in custom_word_match_keys {
        // Skip if lengths are too different (optimization + prevents over-matching)
        // Use percentage-based check: max 25% length difference (prevents n-grams from
        // matching significantly shorter custom words, e.g., "openaigpt" vs "openai")
        let len_diff = (candidate.len() as i32 - custom_word_key.key.len() as i32).abs() as f64;
        let max_len = candidate.len().max(custom_word_key.key.len()) as f64;
        let max_allowed_diff = (max_len * 0.25).max(2.0); // At least 2 chars difference allowed
        if len_diff > max_allowed_diff {
            continue;
        }

        // Calculate Levenshtein distance (normalized by length)
        let levenshtein_dist = levenshtein(candidate, &custom_word_key.key);
        let max_len = candidate.len().max(custom_word_key.key.len()) as f64;
        let levenshtein_score = if max_len > 0.0 {
            levenshtein_dist as f64 / max_len
        } else {
            1.0
        };

        // Calculate phonetic similarity using Soundex
        let phonetic_match = soundex(candidate, &custom_word_key.key);

        // Combine scores: favor phonetic matches, but also consider string similarity
        let combined_score = if phonetic_match {
            levenshtein_score * 0.3 // Give significant boost to phonetic matches
        } else {
            levenshtein_score
        };

        // Accept if the score is good enough (configurable threshold)
        if combined_score < threshold && combined_score < best_score {
            best_match = Some(&custom_words[custom_word_key.word_index]);
            best_score = combined_score;
        }
    }

    best_match.map(|m| (m, best_score))
}

/// Applies custom word corrections to transcribed text using fuzzy matching
///
/// This function corrects words in the input text by finding the best matches
/// from a list of custom words using a combination of:
/// - Levenshtein distance for string similarity
/// - Soundex phonetic matching for pronunciation similarity
/// - N-gram matching for multi-word speech artifacts (e.g., "Charge B" -> "ChargeBee")
///
/// # Arguments
/// * `text` - The input text to correct
/// * `custom_words` - List of custom words to match against
/// * `threshold` - Maximum similarity score to accept (0.0 = exact match, 1.0 = any match)
///
/// # Returns
/// The corrected text with custom words applied
pub fn apply_custom_words(text: &str, custom_words: &[String], threshold: f64) -> String {
    if custom_words.is_empty() {
        return text.to_string();
    }

    // Pre-compute normalized comparison keys to avoid repeated allocations.
    let custom_word_match_keys: Vec<CustomWordMatchKey> = custom_words
        .iter()
        .enumerate()
        .flat_map(|(index, word)| build_custom_word_match_keys(word, index))
        .collect();

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut result = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let mut matched = false;

        // Try n-grams from longest (3) to shortest (1) - greedy matching
        for n in (1..=3).rev() {
            if i + n > words.len() {
                continue;
            }

            let ngram_words = &words[i..i + n];
            let ngram = build_ngram(ngram_words);

            if let Some((replacement, _score)) =
                find_best_match(&ngram, custom_words, &custom_word_match_keys, threshold)
            {
                // Extract punctuation from first and last words of the n-gram
                let (prefix, _) = extract_punctuation(ngram_words[0]);
                let (_, suffix) = extract_punctuation(ngram_words[n - 1]);

                // Preserve case from first word
                let corrected = preserve_case_pattern(ngram_words[0], replacement);

                result.push(format!("{}{}{}", prefix, corrected, suffix));
                i += n;
                matched = true;
                break;
            }
        }

        if !matched {
            result.push(words[i].to_string());
            i += 1;
        }
    }

    result.join(" ")
}

/// Preserves the case pattern of the original word when applying a replacement
fn preserve_case_pattern(original: &str, replacement: &str) -> String {
    if original.chars().all(|c| c.is_uppercase()) {
        replacement.to_uppercase()
    } else if original.chars().next().is_some_and(|c| c.is_uppercase()) {
        let mut chars: Vec<char> = replacement.chars().collect();
        if let Some(first_char) = chars.get_mut(0) {
            *first_char = first_char.to_uppercase().next().unwrap_or(*first_char);
        }
        chars.into_iter().collect()
    } else {
        replacement.to_string()
    }
}

/// Extracts punctuation prefix and suffix from a word.
///
/// Byte-safe con puntuación multibyte ("¿", "…", "—"): la versión anterior
/// usaba conteos de caracteres como índices de bytes y panickeaba con "¿El".
fn extract_punctuation(word: &str) -> (&str, &str) {
    let prefix_end = word
        .char_indices()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, _)| i)
        .unwrap_or(word.len());
    let suffix_start = word
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_alphanumeric())
        .map(|(i, c)| i + c.len_utf8())
        .unwrap_or(prefix_end);

    (&word[..prefix_end], &word[suffix_start..])
}

/// Returns filler words appropriate for the given language code.
///
/// Some words like "um" and "ha" are real words in certain languages
/// (e.g., Portuguese "um" = "a/an", Spanish "ha" = "has"), so we only
/// include them as fillers for languages where they are truly fillers.
fn get_filler_words_for_language(lang: &str) -> &'static [&'static str] {
    let base_lang = lang.split(&['-', '_'][..]).next().unwrap_or(lang);

    match base_lang {
        "en" => &[
            "uh", "um", "uhm", "umm", "uhh", "uhhh", "ah", "hmm", "hm", "mmm", "mm", "mh", "eh",
            "ehh", "ha",
        ],
        "es" => &["ehm", "mmm", "hmm", "hm"],
        "pt" => &["ahm", "hmm", "mmm", "hm"],
        "fr" => &["euh", "hmm", "hm", "mmm"],
        "de" => &["äh", "ähm", "hmm", "hm", "mmm"],
        "it" => &["ehm", "hmm", "mmm", "hm"],
        "cs" => &["ehm", "hmm", "mmm", "hm"],
        "pl" => &["hmm", "mmm", "hm"],
        "tr" => &["hmm", "mmm", "hm"],
        "ru" => &["хм", "ммм", "hmm", "mmm"],
        "uk" => &["хм", "ммм", "hmm", "mmm"],
        "ar" => &["hmm", "mmm"],
        "ja" => &["hmm", "mmm"],
        "ko" => &["hmm", "mmm"],
        "vi" => &["hmm", "mmm", "hm"],
        "zh" => &["hmm", "mmm"],
        // Conservative universal fallback (no "um", "eh", "ha")
        _ => &[
            "uh", "uhm", "umm", "uhh", "uhhh", "ah", "hmm", "hm", "mmm", "mm", "mh", "ehh",
        ],
    }
}

static MULTI_SPACE_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s{2,}").unwrap());

/// Collapses repeated words (3+ repetitions) to a single instance.
/// E.g., "wh wh wh wh" -> "wh", "I I I I" -> "I"
fn collapse_stutters(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() {
        return text.to_string();
    }

    let mut result: Vec<&str> = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let word = words[i];
        let word_lower = word.to_lowercase();

        if word_lower.chars().all(|c| c.is_alphabetic()) {
            // Count consecutive repetitions (case-insensitive)
            let mut count = 1;
            while i + count < words.len() && words[i + count].to_lowercase() == word_lower {
                count += 1;
            }

            // If 3+ repetitions, collapse to single instance
            if count >= 3 {
                result.push(word);
                i += count;
            } else {
                result.push(word);
                i += 1;
            }
        } else {
            result.push(word);
            i += 1;
        }
    }

    result.join(" ")
}

/// Una muletilla preparada para el matching normalizado (F5.2): la secuencia
/// de claves por token y, para frases multi-token, la variante fusionada que
/// producen los ASR ("o sea" → "osea"/"ósea").
struct FillerChip {
    token_keys: Vec<String>,
    fused_key: Option<String>,
}

fn build_filler_chips(words: &[&str]) -> Vec<FillerChip> {
    let mut chips: Vec<FillerChip> = words
        .iter()
        .filter_map(|chip| {
            let token_keys: Vec<String> = chip
                .split_whitespace()
                .map(build_match_key)
                .filter(|k| !k.is_empty())
                .collect();
            if token_keys.is_empty() {
                return None;
            }
            let fused_key = if token_keys.len() > 1 {
                Some(token_keys.concat())
            } else {
                None
            };
            Some(FillerChip {
                token_keys,
                fused_key,
            })
        })
        .collect();
    // Las frases más largas se prueban primero (matching codicioso).
    chips.sort_by_key(|chip| std::cmp::Reverse(chip.token_keys.len()));
    chips
}

/// Filters transcription output by removing filler words and stutter artifacts.
///
/// This function cleans up raw transcription text by:
/// 1. Removing filler words based on the app language (or custom list),
///    comparando claves normalizadas (minúsculas, sin tildes, sin puntuación
///    adyacente — F5.2): "Osea," al inicio de frase cae con el chip "o sea",
///    y "cachai" sin tilde matchea el chip "cachái".
/// 2. Collapsing repeated word stutters (e.g., "wh wh wh" -> "wh")
/// 3. Cleaning up excess whitespace
///
/// # Arguments
/// * `text` - The raw transcription text to filter
/// * `lang` - The app language code (e.g., "en", "pt-BR") used to select filler words
/// * `custom_filler_words` - Optional user-provided filler word list. `Some(vec)` overrides
///   language defaults; `Some(empty vec)` disables filtering; `None` uses language defaults.
///
/// # Returns
/// The filtered text with filler words and stutters removed
pub fn filter_transcription_output(
    text: &str,
    lang: &str,
    custom_filler_words: &Option<Vec<String>>,
) -> String {
    let chips = match custom_filler_words {
        Some(words) => build_filler_chips(&words.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
        None => build_filler_chips(get_filler_words_for_language(lang)),
    };

    let words: Vec<&str> = text.split_whitespace().collect();
    let mut kept: Vec<&str> = Vec::with_capacity(words.len());
    let mut i = 0;

    'outer: while i < words.len() {
        let word_key = build_match_key(words[i]);

        for chip in &chips {
            let n = chip.token_keys.len();
            // Frase multi-token literal ("o sea"): los tokens intermedios no
            // pueden cargar puntuación de cierre (límite de frase).
            if n > 1
                && i + n <= words.len()
                && (0..n - 1).all(|j| extract_punctuation(words[i + j]).1.is_empty())
                && (0..n).all(|j| build_match_key(words[i + j]) == chip.token_keys[j])
            {
                i += n;
                continue 'outer;
            }
            // Token único, o la frase fusionada por el ASR ("osea"/"ósea").
            if !word_key.is_empty()
                && (chip.token_keys.len() == 1 && chip.token_keys[0] == word_key
                    || chip.fused_key.as_deref() == Some(word_key.as_str()))
            {
                i += 1;
                continue 'outer;
            }
        }

        kept.push(words[i]);
        i += 1;
    }

    let filtered = kept.join(" ");

    // Collapse repeated 1-2 letter words (stutter artifacts like "wh wh wh wh")
    let filtered = collapse_stutters(&filtered);

    // Clean up multiple spaces to single space (defensa por si llega texto raro)
    let filtered = MULTI_SPACE_PATTERN.replace_all(&filtered, " ").to_string();

    // Trim leading/trailing whitespace
    filtered.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_custom_words_exact_match() {
        let text = "hello world";
        let custom_words = vec!["Hello".to_string(), "World".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_apply_custom_words_fuzzy_match() {
        let text = "helo wrold";
        let custom_words = vec!["hello".to_string(), "world".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_preserve_case_pattern() {
        assert_eq!(preserve_case_pattern("HELLO", "world"), "WORLD");
        assert_eq!(preserve_case_pattern("Hello", "world"), "World");
        assert_eq!(preserve_case_pattern("hello", "WORLD"), "WORLD");
    }

    #[test]
    fn test_extract_punctuation() {
        assert_eq!(extract_punctuation("hello"), ("", ""));
        assert_eq!(extract_punctuation("!hello?"), ("!", "?"));
        assert_eq!(extract_punctuation("...hello..."), ("...", "..."));
    }

    #[test]
    fn test_empty_custom_words() {
        let text = "hello world";
        let custom_words = vec![];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_filter_filler_words() {
        let text = "So uhm I was thinking uh about this";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "So I was thinking about this");
    }

    #[test]
    fn test_filter_filler_words_case_insensitive() {
        let text = "UHM this is UH a test";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "this is a test");
    }

    #[test]
    fn test_filter_filler_words_with_punctuation() {
        let text = "Well, uhm, I think, uh. that's right";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Well, I think, that's right");
    }

    #[test]
    fn test_filter_cleans_whitespace() {
        let text = "Hello    world   test";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Hello world test");
    }

    #[test]
    fn test_filter_trims() {
        let text = "  Hello world  ";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Hello world");
    }

    #[test]
    fn test_filter_combined() {
        let text = "  Uhm, so I was, uh, thinking about this  ";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "so I was, thinking about this");
    }

    #[test]
    fn test_filter_preserves_valid_text() {
        let text = "This is a completely normal sentence.";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "This is a completely normal sentence.");
    }

    #[test]
    fn test_filter_stutter_collapse() {
        let text = "w wh wh wh wh wh wh wh wh wh why";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "w wh why");
    }

    #[test]
    fn test_filter_stutter_short_words() {
        let text = "I I I I think so so so so";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "I think so");
    }

    #[test]
    fn test_filter_stutter_longer_words() {
        let text = "Check data doc doc doc doc documentation.";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "Check data doc documentation.");
    }

    #[test]
    fn test_filter_stutter_mixed_case() {
        let text = "No NO no NO no";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "No");
    }

    #[test]
    fn test_filter_stutter_preserves_two_repetitions() {
        let text = "no no is fine";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "no no is fine");
    }

    #[test]
    fn test_filter_english_removes_um() {
        let text = "um I think um this is good";
        let result = filter_transcription_output(text, "en", &None);
        assert_eq!(result, "I think this is good");
    }

    #[test]
    fn test_filter_portuguese_preserves_um() {
        // "um" means "a/an" in Portuguese
        let text = "um gato bonito";
        let result = filter_transcription_output(text, "pt", &None);
        assert_eq!(result, "um gato bonito");
    }

    #[test]
    fn test_filter_spanish_preserves_ha() {
        // "ha" means "has" in Spanish
        let text = "ha sido un buen día";
        let result = filter_transcription_output(text, "es", &None);
        assert_eq!(result, "ha sido un buen día");
    }

    #[test]
    fn test_filter_language_code_with_region() {
        // "pt-BR" should normalize to "pt"
        let text = "um gato bonito";
        let result = filter_transcription_output(text, "pt-BR", &None);
        assert_eq!(result, "um gato bonito");
    }

    #[test]
    fn test_filter_custom_filler_words_override() {
        let custom = Some(vec!["okay".to_string(), "right".to_string()]);
        let text = "okay so I think right this works";
        let result = filter_transcription_output(text, "en", &custom);
        assert_eq!(result, "so I think this works");
    }

    #[test]
    fn test_filter_custom_filler_words_empty_disables() {
        let custom = Some(vec![]);
        let text = "So uhm I was thinking uh about this";
        let result = filter_transcription_output(text, "en", &custom);
        // No filler words removed since custom list is empty
        assert_eq!(result, "So uhm I was thinking uh about this");
    }

    #[test]
    fn test_filter_custom_filler_multiword_phrase() {
        // El preset es-419 incluye frases con espacio ("o sea"). La frase
        // también debe matchear FUSIONADA ("osea"/"ósea", como la producen
        // los ASR reales).
        let custom = Some(vec!["o sea".to_string(), "eh".to_string()]);
        let text = "Eso o sea funciona eh y osea también cae";
        let result = filter_transcription_output(text, "es", &custom);
        assert_eq!(result, "Eso funciona y también cae");
    }

    #[test]
    fn test_filter_fused_phrase_with_accent_and_punctuation() {
        // "Osea," al inicio de frase: mayúscula + tilde + coma adyacente — la
        // clave normalizada la alcanza igual (F5.2).
        let custom = Some(vec!["o sea".to_string()]);
        let result = filter_transcription_output("Ósea, esto funciona", "es", &custom);
        assert_eq!(result, "esto funciona");
        let result = filter_transcription_output("Osea, esto funciona", "es", &custom);
        assert_eq!(result, "esto funciona");
    }

    #[test]
    fn test_filter_accent_folding_matches_chip() {
        // "cachai" sin tilde debe caer con el chip "cachái" (y viceversa).
        let custom = Some(vec!["cachái".to_string()]);
        let result = filter_transcription_output("bueno cachai eso es", "es", &custom);
        assert_eq!(result, "bueno eso es");
        let result = filter_transcription_output("bueno cachái eso es", "es", &custom);
        assert_eq!(result, "bueno eso es");
    }

    #[test]
    fn test_filter_multiword_not_across_punctuation() {
        // "o. sea" — el punto cierra la frase: no es la muletilla "o sea".
        let custom = Some(vec!["o sea".to_string()]);
        let result = filter_transcription_output("esto o. sea claro", "es", &custom);
        assert_eq!(result, "esto o. sea claro");
    }

    // ───────────────────────── F5.1: reemplazos exactos ─────────────────────────

    fn replacements(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(f, t)| (f.to_string(), t.to_string()))
            .collect()
    }

    #[test]
    fn test_replacement_exact_token_applies_and_ruta_intact() {
        // El caso estrella del 11/07: "rut"→ASR escribe "Ruth". El reemplazo
        // exacto lo corrige, y "ruta" — otro token — es intocable por diseño.
        let reps = replacements(&[("Ruth", "rut")]);
        let result = apply_custom_replacements("valida el Ruth de la empresa", &reps);
        assert_eq!(result, "valida el rut de la empresa");
        let result = apply_custom_replacements("muéstrame la ruta del archivo", &reps);
        assert_eq!(result, "muéstrame la ruta del archivo");
    }

    #[test]
    fn test_replacement_is_case_sensitive() {
        let reps = replacements(&[("Ruth", "rut")]);
        assert_eq!(
            apply_custom_replacements("dijo ruth en voz baja", &reps),
            "dijo ruth en voz baja"
        );
    }

    #[test]
    fn test_replacement_preserves_adjacent_punctuation() {
        let reps = replacements(&[("Ruth", "rut"), ("yapo", "ya po")]);
        assert_eq!(
            apply_custom_replacements("¿El Ruth, cierto? yapo.", &reps),
            "¿El rut, cierto? ya po."
        );
    }

    // ───────────────────────── F5.4: join multi-token ─────────────────────────

    fn join_lookup(terms: &[&str]) -> HashMap<String, String> {
        terms
            .iter()
            .map(|t| (build_match_key(t), t.to_string()))
            .collect()
    }

    #[test]
    fn test_multi_token_join_camel_case() {
        let lookup = join_lookup(&["useAuthStore"]);
        assert_eq!(
            apply_multi_token_join("importa use auth store aquí", &lookup),
            "importa useAuthStore aquí"
        );
    }

    #[test]
    fn test_multi_token_join_branch_with_separators() {
        // El término conserva sus separadores originales (rama git).
        let lookup = join_lookup(&["hotfix/token-expiry"]);
        assert_eq!(
            apply_multi_token_join("cámbiate a hotfix token expiry ahora", &lookup),
            "cámbiate a hotfix/token-expiry ahora"
        );
    }

    #[test]
    fn test_multi_token_join_no_false_positive() {
        // "el uso del store" no contiene ninguna ventana que fusione a un
        // término del diccionario: no se toca.
        let lookup = join_lookup(&["useAuthStore", "hotfix/token-expiry"]);
        assert_eq!(
            apply_multi_token_join("el uso del store es correcto", &lookup),
            "el uso del store es correcto"
        );
    }

    #[test]
    fn test_multi_token_join_keeps_trailing_punctuation() {
        let lookup = join_lookup(&["useAuthStore"]);
        assert_eq!(
            apply_multi_token_join("llama a use auth store.", &lookup),
            "llama a useAuthStore."
        );
    }

    #[test]
    fn test_multi_token_join_not_across_punctuation() {
        // La coma dentro de la ventana rompe la frase: no hay join.
        let lookup = join_lookup(&["useAuthStore"]);
        assert_eq!(
            apply_multi_token_join("use auth, store aparte", &lookup),
            "use auth, store aparte"
        );
    }

    // ───────────────────── F5.3: corrección difusa protegida ─────────────────────

    #[test]
    fn test_dictionary_fuzzy_corrects_but_respects_stoplist() {
        let terms = vec!["Parakeet".to_string(), "rut".to_string()];
        let protected = |key: &str| key == "ruta" || key == "uso";
        // "parakit" se acerca fonéticamente a "Parakeet" → término exacto del repo.
        let result = apply_dictionary_fuzzy("configura el parakit ahora", &terms, 0.3, &protected);
        assert_eq!(result, "configura el Parakeet ahora");
        // "ruta" está protegida por la stoplist: jamás se corrige a "rut",
        // aunque la métrica (Levenshtein 1 + Soundex igual) diría que sí.
        let result = apply_dictionary_fuzzy("muéstrame la ruta", &terms, 0.3, &protected);
        assert_eq!(result, "muéstrame la ruta");
    }

    #[test]
    fn test_filter_unknown_language_uses_fallback() {
        let text = "uh I think uhm this works";
        let result = filter_transcription_output(text, "xx", &None);
        assert_eq!(result, "I think this works");
    }

    #[test]
    fn test_filter_fallback_does_not_remove_um() {
        // Fallback (unknown language) should not remove "um" since it's a real word in some languages
        let text = "um I think this works";
        let result = filter_transcription_output(text, "xx", &None);
        assert_eq!(result, "um I think this works");
    }

    #[test]
    fn test_apply_custom_words_ngram_two_words() {
        let text = "il cui nome è Charge B, che permette";
        let custom_words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("ChargeBee,"));
        assert!(!result.contains("Charge B"));
    }

    #[test]
    fn test_apply_custom_words_ngram_three_words() {
        let text = "use Chat G P T for this";
        let custom_words = vec!["ChatGPT".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("ChatGPT"));
    }

    #[test]
    fn test_apply_custom_words_prefers_longer_ngram() {
        let text = "Open AI GPT model";
        let custom_words = vec!["OpenAI".to_string(), "GPT".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert_eq!(result, "OpenAI GPT model");
    }

    #[test]
    fn test_apply_custom_words_ngram_preserves_case() {
        let text = "CHARGE B is great";
        let custom_words = vec!["ChargeBee".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("CHARGEBEE"));
    }

    #[test]
    fn test_apply_custom_words_ngram_with_spaces_in_custom() {
        // Custom word with space should also match against split words
        let text = "using Mac Book Pro";
        let custom_words = vec!["MacBook Pro".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        assert!(result.contains("MacBook"));
    }

    #[test]
    fn test_apply_custom_words_trailing_number_not_doubled() {
        // Verify that trailing non-alpha chars (like numbers) aren't double-counted
        // between build_ngram stripping them and extract_punctuation capturing them
        let text = "use GPT4 for this";
        let custom_words = vec!["GPT-4".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.5);
        // Should NOT produce "GPT-44" (double-counting the trailing 4)
        assert!(
            !result.contains("GPT-44"),
            "got double-counted result: {}",
            result
        );
    }

    #[test]
    fn test_apply_custom_words_matches_ampersand_word() {
        let text = "send it to RD for review";
        let custom_words = vec!["R&D".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.18);
        assert_eq!(result, "send it to R&D for review");
    }

    #[test]
    fn test_apply_custom_words_matches_spoken_ampersand_word() {
        let text = "send it to R and D for review";
        let custom_words = vec!["R&D".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.18);
        assert_eq!(result, "send it to R&D for review");
    }

    #[test]
    fn test_apply_custom_words_preserves_ampersand_word() {
        let text = "send it to R&D for review";
        let custom_words = vec!["R&D".to_string()];
        let result = apply_custom_words(text, &custom_words, 0.18);
        assert_eq!(result, "send it to R&D for review");
    }
}
