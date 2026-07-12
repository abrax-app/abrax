# INFORME — Sesión paralela ESCUCHA (F7) · domingo 12/07/2026

## Resumen ejecutivo

ABRAX ahora **lee tu código**: la sección Escucha abre un archivo (o el
portapapeles), lo segmenta en oraciones, lee la prosa con voz natural del
sistema y el código con voz técnica que verbaliza símbolos en es-419,
resaltando la línea en lectura. MVP 100% local sobre el TTS del SO (crate
`tts`, MIT), sin descargas. La voz neural premium (Piper) quedó diseñada en
`docs/ESCUCHA_FASE2.md` — solo documento, cero código.

## Aislamiento

- **Worktree**: `workspace/handy-escucha`, rama `feat/escucha`.
- **Hash base**: `afcdcb95fcf4990fa25a6a9a5f060891a0317394` (HEAD de
  `rebrand/product` al momento de crear el worktree; commit "chore(version):
  ABRAX inicia su propia línea de versiones — v0.1.0").
- Cero commits fuera de este worktree. `handy-rebrand/` no se tocó.
- **Puerto vite 1421 + devUrl 1421**: cambiado SOLO en el working tree,
  deliberadamente SIN commitear (es una medida de aislamiento de la sesión, no
  parte de la feature; así la rama no lleva conflicto de puertos a la fusión).
- **CARGO_TARGET_DIR=C:\tmp\escucha-target**: los builds Rust de este worktree
  usan un target dir corto y propio. Dos razones: (1) no compartir el lock de
  target con la maratón, (2) la ruta del worktree + rutas internas de CMake de
  transcribe-cpp superan MAX_PATH (260) en Windows y MSBuild revienta con
  FTK1011 — con target dir corto compila limpio.
- Sin `tauri dev` en toda la sesión (single-instance): desarrollo con
  `cargo test` + binario headless (`--list-models`, modo portable con datos en
  `C:\tmp\escucha-target\debug\Data`) para regenerar `bindings.ts` sin abrir
  ventana ni tocar el app-data compartido.

## Commits (rama feat/escucha)

| Hash | Mensaje | Naturaleza |
|---|---|---|
| `4cc1238` | feat(escucha): manager TTS del sistema y preprocesador es-419 con tests | Archivos nuevos (backend) |
| `1ccab3a` | feat(escucha): registro en archivos compartidos — bloques [ESCUCHA] aditivos | **Calientes** backend (Cargo.toml/lock, lib.rs, settings.rs, mods) |
| `c1d6986` | chore(escucha): regenerar bindings.ts | Generado |
| `b8c9d97` | feat(escucha): panel de lectura en voz alta (componente nuevo) | Archivo nuevo (frontend) |
| `43efc7c` | feat(escucha): entrada de sidebar + i18n en los 22 locales | **Calientes** frontend + i18n |
| `a611044` | style(escucha): formato prettier del panel y el sidebar | Formato |
| *(este commit)* | docs(escucha): informe, diseño Fase 2, ideas y changelog | Docs |

Los cambios a archivos calientes viven SOLO en `1ccab3a` y `43efc7c`, para
que en la fusión del lunes se auditen en dos diffs pequeños y aditivos.

## Estado de checks EN ESTA RAMA (punta, tras el último commit de código)

| Check | Resultado |
|---|---|
| `cargo test` | **138 passed / 0 failed / 1 ignored** (el ignorado es el smoke de audio; corrido aparte: **passed**, habla es-ES real) |
| `bun run lint` (eslint) | **0 errores** |
| `bun run check:translations` | **22/22 idiomas en paridad** |
| `bun run build` (tsc + vite) | **OK** (5,7 s) |
| `cargo fmt` / prettier | Archivos de la sesión limpios (`llm_client.rs` venía sin formatear de base; se dejó intacto) |

## Decisiones y hallazgos técnicos

### Motor (E1)

- **Crate `tts` v0.26.3, licencia MIT** ✓ (verificada en el registry). Backend
  WinRT en Windows, AVSpeech en macOS, speech-dispatcher en Linux.
- **GATE E1 superado en ~15 min**: spike aislado compiló y habló "Hola, soy
  ABRAX. Escucha tu código." con voz española en Windows.
- **Voces encontradas (Windows 11 de esta máquina)**: 6 voces OneCore — 3 en
  español: Microsoft **Pablo**, **Helena**, **Laura** (es-ES); 3 en inglés
  (David, Zira, Mark — en-US).
- **Features del backend WinRT**: `is_speaking` ✓, `stop` ✓, `rate` ✓
  (min 0.5 / normal 1.0 / max 6.0 — el rate nativo es un multiplicador, así
  que nuestro parámetro "1.0 = normal" pasa directo), `utterance_callbacks` ✓
  (disponibles; el MVP usa polling de 100 ms igualmente por simplicidad
  multiplataforma — anotado en IDEAS.md).
- **El crate NO expone pause/resume** (no existe en su API). La pausa se
  implementa a nivel de cola: pausar corta la oración actual y recuerda el
  índice; continuar relee desde esa oración. Honesto y suficiente.
- **Threading**: `tts::Tts` no es `Send` en todas las plataformas → el motor
  vive en un hilo propio (`escucha-tts`) con canal mpsc y respuestas con
  timeout de 5 s: un fallo del motor nunca cuelga el hilo IPC de Tauri.
- Motor de inicialización perezosa: se crea en el primer comando, no en el
  arranque de la app.

### Preprocesador (E2) — el corazón

- `managers/escucha/preproceso.rs`: módulo **puro** (sin Tauri, sin TTS).
- Segmentación markdown con `pulldown-cmark` 0.13 (MIT, default-features off)
  usando `into_offset_iter()` para mapear cada oración a líneas 1-based del
  documento (resaltado sincronizado).
- Verbalización es-419 con `TablaSimbolos` configurable: `&&`→"y y",
  `||`→"o o", `=>`/`->`→"flecha", `==`→"igual igual", `/`→"slash",
  `_`→"guión bajo", `-`→"guión", `.`→"punto", `#`→"gato", `$`→"peso",
  `"`→"comillas", etc.
- Identificadores: camelCase, siglas y fronteras letra↔dígito se parten
  ("useAuthStore"→"use Auth Store", "HTTPServer"→"HTTP Server"); dígito suelto
  se pronuncia ("v0.1.0"→"v cero punto uno punto cero"), runs multi-dígito se
  dejan al TTS ("error404"→"error 404").
- Verbosidad **Natural** (calla cierres de paréntesis/llaves/corchetes) vs
  **Literal** ("abre/cierra" en cada uno) — setting persistido.
- Encabezados anuncian nivel: "Sección: X" (h1), "Subsección: X" (h2),
  "Apartado: X" (h3+).
- Código inline en prosa se verbaliza dentro de la oración (voz de prosa).
- Heurística `parece_codigo` para "Leer portapapeles" (modo auto).
- **17 tests unitarios** cubren los 3 ejemplos de la spec, el bloque markdown
  mixto completo con verificación de voces y líneas, versiones/números,
  Natural vs Literal, listas, inline code y la heurística.

### UI (E3)

- Sección **Escucha** en el sidebar (icono `AudioLines`, set lucide).
- Panel: Abrir archivo… (diálogo nativo; md/txt/código) · Leer portapapeles ·
  transporte ▶/⏸/⏹ · voz + velocidad independientes para Prosa y Código ·
  selector de verbosidad de símbolos · visor con números de línea y
  **resaltado de la línea en lectura** con auto-scroll.
- Cola de oraciones en el frontend: habla una oración, espera fin por poll de
  `escucha_status` (100 ms), avanza resaltado, sigue. Cambios de voz/velocidad
  aplican a partir de la siguiente oración.
- **Esc detiene la lectura**; al desmontar el panel también se detiene.
- `prefers-reduced-motion`: sin transición de color en el resaltado y
  auto-scroll instantáneo (no smooth).
- Lectura de archivo y portapapeles vía comandos Rust propios
  (`escucha_read_file` con tope 2 MB, `escucha_read_clipboard`): cero cambios
  en el `capabilities/default.json` compartido.

### Settings (E4)

- Bloque aditivo `// [ESCUCHA]` al final de `AppSettings`:
  `escucha_voz_prosa`, `escucha_voz_codigo`, `escucha_rate_prosa` (1.0),
  `escucha_rate_codigo` (0.9 — el código verbalizado se sigue mejor un poco
  más lento), `escucha_verbosidad_simbolos` (Natural).
- Persistencia vía comando propio `escucha_update_settings` (debounce 400 ms
  en el panel) — **no** se tocó `shortcut/mod.rs` ni `settingsStore.ts`
  compartidos.
- i18n: 24 claves nuevas × 22 locales (`sidebar.escucha` + bloque `escucha.*`),
  `check:translations` verde.

## Archivos calientes compartidos — qué se tocó y cómo

| Archivo | Cambio | Naturaleza |
|---|---|---|
| `src-tauri/Cargo.toml` | +2 deps (`tts`, `pulldown-cmark`) con comentario `# [ESCUCHA]` | Aditivo puro |
| `src-tauri/src/lib.rs` | +9 comandos en bloque `// [ESCUCHA]` al final de `collect_commands!` + 1 `manage()` | Aditivo puro |
| `src-tauri/src/settings.rs` | Bloque de 5 campos al final del struct + 2 default fns + 5 líneas en `get_default_settings()` | Aditivo puro |
| `src/bindings.ts` | **Regenerado** (nunca a mano) | Generado |
| 22 × `translation.json` | +24 claves (`sidebar.escucha` + `escucha.*`) | Unión de claves |
| `src/components/Sidebar.tsx` | +1 import, +1 entrada en `SECTIONS_CONFIG` marcada `// [ESCUCHA]` | Aditivo |
| `src/components/settings/index.ts` | +1 export marcado | Aditivo |

Todo lo demás es archivo nuevo (`managers/escucha/`, `commands/escucha.rs`,
`components/settings/escucha/`, docs).

## Limitaciones conocidas

1. **Pausa a nivel de oración**: pausar corta la oración en curso; continuar
   la relee desde el inicio de esa oración (el crate `tts` no tiene pause).
2. **Resaltado por rango de línea de la oración**, no por palabra; en párrafos
   multi-oración el rango es aproximado (checkpoint por evento de texto).
3. La **verbosidad** aplica al preprocesar: cambiar el selector afecta al
   próximo contenido que se cargue (anotado en IDEAS.md).
4. Voces = las instaladas en el SO. En un Windows sin voces españolas, se usa
   la primera voz disponible (el selector permite corregir). Calidad de voz =
   la del SO (la premium es Fase 2).
5. Líneas de código extremadamente largas se leen como una sola oración
   (sin partir; anotado en IDEAS.md).
6. Smoke visual del panel pendiente de ventana coordinada (single-instance):
   la lógica de motor está cubierta por el smoke test de backend
   (`cargo test escucha_smoke -- --ignored --nocapture` habla por los
   parlantes) y el resto por tests + build.

## Guion de demo (15 s) — Avance 5

1. Abrir ABRAX → sección **Escucha** (icono de onda).
2. "Abrir archivo…" → elegir `README.md` del proyecto → ▶.
3. *Se escucha* (voz Pablo/Helena): «Sección: Abrax. Abrax es dictado por voz
   local…» — prosa fluida, línea resaltada avanzando.
4. Al llegar al bloque de instalación cambia la cadencia (voz código, más
   lenta): «bun install **y y** bun run dev».
5. Copiar un snippet cualquiera → "Leer portapapeles" → lo lee al instante.
   Esc corta. Fin.

## Fusión del lunes

Protocolo en §F del brief. Notas para quien fusione:
- `bindings.ts`: regenerar tras el rebase (jamás resolver a mano) — basta
  `cargo build` debug + `abrax --list-models` desde `src-tauri`.
- `settings.rs` / `lib.rs` / `Cargo.toml`: los bloques `[ESCUCHA]` son
  aditivos y están al final de sus secciones — el conflicto esperable es de
  contexto, se resuelve conservando ambos lados.
- i18n: unión de claves (las nuestras cuelgan de `escucha.*` y
  `sidebar.escucha`).
- El cambio de puerto vite (1421) NO está commiteado — nada que resolver.
