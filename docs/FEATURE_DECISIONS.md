# Decisiones de la capa de corrección

Registro de decisiones técnicas del módulo `src-tauri/src/correccion/`. La
evidencia que las sustenta está en `COMPETITIVE_RESEARCH.md`.

## D1 — La corrección es determinista por defecto

**Decisión:** la corrección de dictado se hace con reglas deterministas, no con
un LLM. El motor por defecto (`CorreccionMotor::Desactivado`) es passthrough byte
a byte; los modos deterministas (`SoloReglas`, y `Auto`/`Modelo` cuando degradan)
componen:

1. Protección de datos frágiles → marcadores `__TIPO_n__` (`protegidos.rs`).
2. Autocorrecciones habladas ancladas (`reglas.rs::autocorreccion_hablada`).
3. Restauración de tildes por diccionario (`tildes.rs`).
4. Normalización de símbolos inequívocos (`simbolos.rs`).
5. Espacios y mayúsculas de oración (`reglas.rs`).
6. Validación de significado (`validador.rs`) en el camino del modelo.

**Por qué:** cinco modelos locales pequeños (Qwen2.5 0.5/1.5/3B, Salamandra-2B,
Qwen3-1.7B) por tres formulaciones (reescritura libre, ediciones estructuradas
con dos prompts) fallaron todos en la **decisión** de qué corregir. Las reglas
deterministas hacen esa tarea de forma fiable, instantánea y sin descargas. Ver
`COMPETITIVE_RESEARCH.md`.

## D2 — No se embebe llama.cpp en el proceso principal

**Decisión:** ningún motor de inferencia LLM se enlaza en el mismo binario que
la transcripción.

**Por qué:** whisper.cpp (vía `transcribe-cpp`) y llama.cpp exportan los mismos
símbolos `ggml_*`, y ggml no está versionado. Dos copias en el mismo proceso →
riesgo de símbolo duplicado o incompatibilidad de ABI que puede romper el
dictado, que es la función central. Cualquier LLM local futuro correría como
**sidecar de proceso aislado** (patrón de `managers/tts/pyserver.rs`: HTTP en
loopback, puerto efímero, handshake de arranque, `Drop`→`kill`), con descarga del
binario en runtime y verificación sha256 (patrón de `managers/tts/download.rs`),
detrás de una feature y apagado por defecto.

## D3 — Ollama es un camino opcional, nunca obligatorio

**Decisión:** el "Pulido con IA" vía Ollama en loopback (`fraseador.rs`) queda
como opción para quien ya lo tenga instalado. Nunca es dependencia del dictado ni
del arranque; si no está, la corrección degrada a reglas sin ruido.

## D4 — No hay tienda de descarga de modelos LLM de corrección

**Decisión:** no se construye un catálogo de modelos LLM descargables para la
corrección.

**Por qué:** la infraestructura de descarga (`managers/model.rs`,
`ModelSource`, progreso, sha256, elección de disco) se podría reutilizar, pero
solo tendría sentido si algún modelo superara el umbral de aceptación (D5), y
ninguno lo hizo. Ofrecer modelos que no mejoran de forma fiable la corrección no
aporta valor.

## D5 — Umbral de aceptación para reconsiderar un modelo local

Si en el futuro se reevalúa un modelo local para la corrección, debe superar
**todos** estos criterios en un banco de frases reales, con seguridad por encima
de recall (una edición peligrosa penaliza mucho más que una corrección omitida):

| Criterio | Umbral |
|---|---|
| `marker_preservation` | = 100 % (bloqueante) |
| `protected_data_preservation` | = 100 % (bloqueante) |
| `meaning_change_rate` | ≤ 1 % (bloqueante) |
| `self_repair_recall` | ≥ 0.40 |
| `unnecessary_edit_rate` | ≤ 10 % |
| `latency_p95` | ≤ 3 s en CPU |

`invalid_json_rate` no es criterio de éxito: la gramática lo fuerza a ~0.

## D6 — Alcance del corrector de tildes

Solo se restaura la tilde cuando la forma sin tilde **no es una palabra válida
del español** (`codigo`→`código`, `perdon`→`perdón`). Las tildes diacríticas
dependientes de contexto (`esta`/`está`, `mas`/`más`, `que`/`qué`,
`numero`/`número`) se dejan intactas, y la `ñ` no se restaura (`ano`/`año` es
ambiguo). Mapa generado offline (`scripts/gen_tildes.mjs`) con un diccionario
como oráculo de validez. Corre en `Limpio` y `Pulido`, fuera de `Literal`.

## D7 — Alcance de la normalización de símbolos

Solo se convierten patrones que **no pueden confundirse con habla normal**:
`N slash N` → `N/M`, `N dividido/partido por/entre N` → `N/M`, `N por ciento` →
`N%`, con un número a cada lado. No se tocan `más`, `menos`, `por` ni `igual`
sueltos (son palabras corrientes). Los números de varias palabras
(`mil doscientos`) quedan fuera: convertir todo número a dígitos es una decisión
de estilo distinta, no adoptada. Corre en `Limpio` y `Pulido`.
