# Corrección de dictado — evaluación de modelos locales pequeños

> **Estado: EVALUACIÓN CERRADA.** Ningún LLM local pequeño resultó viable como
> capa de corrección. La corrección de ABRAX es determinista; ver
> `FEATURE_DECISIONS.md` y `src-tauri/src/correccion/`. Este documento registra
> qué se probó y midió, para no repetir el trabajo.

Fuentes web consultadas el **2026-07-19**. Marcas: **[MEDIDO]** = experimento
propio en este equipo (CPU x86-64, Windows, sin GPU para el LLM); **[DOC]** =
documentación oficial; **[ESTIMACIÓN]** = inferencia no medida.

## Contexto

La capa de corrección (`src-tauri/src/correccion/`) transforma la transcripción
cruda antes de insertarla: resuelve autocorrecciones habladas, quita muletillas,
restaura tildes y protege datos (fechas, números, URLs, correos, negaciones). La
pregunta evaluada fue si un LLM local pequeño podía **mejorar** esa capa
manteniéndose ligero (CPU, poca RAM, sin dependencias externas obligatorias).

La arquitectura del módulo es agnóstica del motor: `camino_modelo` en
`correccion/mod.rs` protege el texto, llama a un fraseador, restaura y valida,
con degradación a reglas ante cualquier fallo. Un motor local encajaría detrás
de esa misma interfaz.

## 1. Modelos decoder-only probados [MEDIDO]

Todos en GGUF Q4_K_M, CPU, temperatura 0, vía `llama-server` (llama.cpp b10068)
como sidecar en loopback.

### 1a. Reescritura libre (el modelo devuelve la frase corregida)

| Modelo                                      | Resultado                                                           |
| ------------------------------------------- | ------------------------------------------------------------------- |
| Qwen2.5-0.5B-Instruct                       | Conserva marcadores; no resuelve autocorrecciones; acentúa a medias |
| Qwen2.5-1.5B-Instruct                       | Igual que 0.5B; ~3× más lento                                       |
| Qwen2.5-3B-Instruct                         | Igual; a veces devuelve la entrada intacta; parte mal las oraciones |
| Salamandra-2B-instruct (español-first, BSC) | **Idéntico a Qwen** — la especialización en español no cambió nada  |

Patrón consistente: con prompts conservadores los modelos **devuelven la entrada
casi intacta**; con prompts largos/detallados empeoran (se saturan). Latencia
medida: 0.5B ≈ 1.1 s, 1.5B ≈ 3.3 s, 3B ≈ 3.7–6.3 s por frase corta.

### 1b. Ediciones estructuradas (el modelo devuelve operaciones, no texto)

Hipótesis: en vez de la frase, pedir una lista JSON de operaciones
(`delete`/`replace` por índice de **palabra**, no de subtoken), validada y
aplicada por código. Modelo probado: **Qwen3-1.7B** (Apache-2.0, GGUF oficial),
salida restringida por `json_schema`.

Resultados contra los criterios de aceptación (dos estrategias de prompt):

| Métrica               | Umbral  | Conservador                           | Forzado + few-shot |
| --------------------- | ------- | ------------------------------------- | ------------------ |
| `invalid_json_rate`   | ~0      | 0 % ✓                                 | 0 % ✓              |
| `self_repair_recall`  | ≥ 0.40  | **0.00** ✗                            | **0.00** ✗         |
| `disfluency_recall`   | ≥ 0.40  | **0.00** ✗                            | **0.00** ✗         |
| `accent_recall`       | —       | 0.00                                  | 0.00               |
| `meaning_change_rate` | ≤ 1 %   | 10 % ✗                                | 10 % ✗             |
| `latency_p95`         | ≤ 3 s   | 5.4 s ✗                               | 11 s ✗             |
| `marker_preservation` | = 100 % | lo sostuvo el validador, no el modelo | ídem               |

Con prompt conservador el modelo devuelve `edits: []` (no hace nada) en los casos
que sí requerían corrección. Con prompt forzado + ejemplos, **copia mecánicamente
el patrón del ejemplo** sobre frases que no lo necesitan (la mayoría de las
operaciones las rechaza el validador). En un caso adversarial intentó **borrar
una fecha real** alucinando una autocorrección inexistente; solo el validador
determinista lo frenó.

**Conclusión medida:** el formato se puede forzar (JSON válido siempre), pero la
**decisión** del modelo falla igual que en reescritura libre. El problema es de
capacidad, no de formato ni de prompt.

## 2. Salida restringida en llama.cpp [DOC]

- `llama-server` (~b10068) soporta `response_format: {type:"json_schema"}` y
  gramáticas GBNF (`grammar`). **Verificado [MEDIDO]**: `json_schema` funciona en
  `/v1/chat/completions` en b10068 (el bug ggml-org/llama.cpp#11988 no aplica a
  este build).
- La restricción se implementa por enmascarado de tokens (constrained decoding):
  el formato queda garantizado. Un array de objetos con `enum` es de los casos
  mejor soportados; `minimum`/`maximum` acota índices enteros.
- **Formato ≠ decisión.** La literatura lo respalda: _"Let Me Speak Freely?"_
  (arXiv 2408.02442, EMNLP 2024) — las restricciones de formato no mejoran, y a
  veces degradan, el razonamiento; _"When Correct Isn't Usable"_ (arXiv
  2605.02363) — el constrained decoding fuerza validez pero degrada la tarea y
  multiplica la latencia.
- **Índices de token:** los LLM pequeños cuentan mal por tokenización BPE
  (off-by-one, índices inventados) — arXiv 2410.19730, 2402.14903. Mitigación
  aplicada en la prueba: numerar **palabras**, no subtokens, y validar en código.

## 3. Ruta encoder-decoder [DOC]

Los modelos de corrección gramatical (GEC) fuertes suelen ser encoder-decoder
(seq2seq), que **no corren en llama.cpp**. Evaluada una ruta aislada con otro
runtime:

- **Runtime recomendado:** CTranslate2 (binding Rust `ct2rs`). Soporta T5/mT5/BART,
  int8 en CPU, Windows/macOS/Linux (Apple Silicon vía Accelerate), sin Python en
  runtime, **no usa ggml** (sin conflicto con whisper.cpp). ONNX Runtime (`ort`)
  es buena alternativa pero obliga a escribir el bucle de decodificación seq2seq.
  OpenVINO no aporta y penaliza en Apple Silicon.
- **Modelo candidato:** `SkitCon/gec-spanish-BARTO-COWS-L2H` (BART español,
  ~0.1B, base Apache-2.0). Es el único GEC-ES seq2seq entrenado y descargable.
- **Descartes:** T5Gemma/T5Gemma 2 (licencia Gemma con fricción de
  redistribución); mT5/Flan-T5 (sin fine-tune de GEC).

**Matiz decisivo:** los corpus de GEC son **español escrito** (COWS-L2H son textos
de aprendices). Cubren tildes/gramática — que el corrector determinista ya hace —
pero **no** disfluencias habladas (muletillas, autocorrecciones), que es el hueco
real. La arquitectura _correcta_ (tagger estilo GECToR, encoder-only) **no existe
preentrenada para español** (verificado; MultiGEC-2025 no incluye español). La
ruta encoder-decoder es, con rigor, un bonus marginal.

## 4. Corrección de tildes: diccionario > LLM [DOC + MEDIDO]

Para tildes/ortografía del español en CPU, un enfoque determinista de diccionario
es **más fiable** que cualquier LLM ≤ 3B (que puede alucinar o cambiar palabras).
Implementado en `src-tauri/src/correccion/tildes.rs`: mapa de 13.468 entradas que
solo restaura la tilde cuando la forma sin tilde no es una palabra válida
(`codigo`→`código`), dejando intactas las diacríticas dependientes de contexto
(`esta`/`está`, `mas`/`más`). Cero falsos positivos en la auditoría del mapa.

## Resumen de evidencia

**5 modelos × 3 formulaciones, todos fallan en la decisión:**

- Qwen2.5-0.5B / 1.5B / 3B, Salamandra-2B → reescritura libre → devuelven la entrada.
- Qwen3-1.7B → ediciones estructuradas (2 prompts) → recall 0 / alucina.

La corrección fiable, ligera y sin descargas es la **determinista**. Un LLM solo
aporta en reformulación libre, que es lo más riesgoso y lo que peor hicieron.

## Fuentes (consultadas 2026-07-19)

- llama.cpp server / GBNF / json_schema: `github.com/ggml-org/llama.cpp` (tools/server/README.md, grammars/README.md, issue #11988).
- Salida estructurada y fiabilidad: arXiv 2408.02442, 2605.02363, 2411.15100 (XGrammar), 2501.10868 (JSONSchemaBench).
- Qwen3: `qwenlm.github.io/blog/qwen3/`, arXiv 2505.09388, model cards `Qwen/Qwen3-{0.6B,1.7B}-GGUF`.
- Tagging/edición: GECToR (BEA-2020), arXiv 2410.16473, CoEdIT (2305.09857), 2305.11862.
- Tokenización/conteo: arXiv 2410.19730, 2402.14903, 2406.11687.
- Encoder-decoder / runtimes: CTranslate2 (`github.com/OpenNMT/CTranslate2`), `ct2rs` (docs.rs), `ort` (docs.rs/ort), T5Gemma (`developers.googleblog.com/en/t5gemma/`), `SkitCon/gec-spanish-BARTO-COWS-L2H`.
- Diccionario de tildes: RLA-ES (`github.com/LibreOffice/dictionaries`, MPL/GPL/LGPL), hermitdave/FrequencyWords (MIT). Ver `ATTRIBUTION.md`.
