# Mapa de la repo

Guía de navegación: qué vive dónde y por dónde pasa el dictado. Una o dos
líneas por carpeta; para el porqué de las decisiones, ver
[FEATURE_DECISIONS.md](../FEATURE_DECISIONS.md).

## Vista general

```text
src-tauri/          backend Rust (Tauri 2) — el producto vive aquí
src/                frontend React/TypeScript (ventana de ajustes, overlay, shells)
scripts/            utilidades de desarrollo (checks de i18n/nix, generador del catálogo)
tests/              spec de Playwright (smoke del dev server)
docs/               documentación técnica y decisiones
landing/            página web estática (independiente de la app)
.github/workflows/  CI (tests, calidad, nix, playwright, build/release)
.nix / nix/         empaquetado Nix para Linux (heredado, autocontenido)
```

## Backend (`src-tauri/src/`)

| Módulo                                  | Qué es                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `main.rs` → `lib.rs`                    | Arranque: parsea CLI, registra ~150 comandos, monta logging, managers y ventana según shell. `lib.rs` incluye además el runner headless de CLI.                                                                                                                                                                                                                                                                                                                                              |
| `managers/audio.rs`                     | Grabación: micrófono, VAD (Silero), modo always-on, mute-al-grabar, espectro.                                                                                                                                                                                                                                                                                                                                                                                                                |
| `audio_toolkit/`                        | Captura cpal, resampler, VAD, guardado WAV — y `text.rs` (fuzzy matching y filtros de texto; el nombre de la carpeta es herencia del upstream).                                                                                                                                                                                                                                                                                                                                              |
| `managers/transcription.rs`             | Motores de transcripción (transcribe-cpp GGUF y transcribe-rs ONNX), streaming, post-proceso de texto (reemplazos, custom words, diccionario, muletillas) y eventos hacia la UI.                                                                                                                                                                                                                                                                                                             |
| `managers/model.rs` + `catalog/`        | Registro/catálogo de modelos de transcripción, descargas con progreso y sha256, descubrimiento en disco. El catálogo va embebido (`include_str!`).                                                                                                                                                                                                                                                                                                                                           |
| `actions.rs`                            | Orquestación del dictado: start/stop, overlay/tray/sonidos, cadena de transformación del texto e inserción. Punto de enganche de la corrección.                                                                                                                                                                                                                                                                                                                                              |
| `correccion/`                           | **Corrección determinista** (mayormente pura, sin Tauri): `reglas.rs` (autocorrecciones habladas, espacios, mayúsculas), `tildes.rs` (restauración segura de tildes), `simbolos.rs` («dos slash tres» → `2/3`), `protegidos.rs` (datos frágiles → marcadores `__TIPO_n__` y restauración), `validador.rs` (el significado no cambia), `fraseador.rs` (cliente del Pulido: Ollama o sidecar, solo loopback), `modelos.rs` + `motor_sidecar.rs` (catálogo y proceso aislado del LLM opcional). |
| `dictionary.rs`                         | Diccionario vivo por proyecto (reemplazos del usuario).                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| `clipboard.rs`                          | Inserción del texto: portapapeles + simulación de teclado por plataforma.                                                                                                                                                                                                                                                                                                                                                                                                                    |
| `shortcut/`                             | Atajos globales (registro, backends por SO) — y, hoy, también la mayoría de comandos de settings (deuda señalada en el mapa de refactors).                                                                                                                                                                                                                                                                                                                                                   |
| `settings.rs`                           | Fuente de verdad de configuración: tipos, defaults, carga con salvage y migraciones.                                                                                                                                                                                                                                                                                                                                                                                                         |
| `overlay.rs`, `tray.rs`, `tray_i18n.rs` | Overlay de grabación (regla: jamás roba el foco), bandeja e i18n de la bandeja (generado en build).                                                                                                                                                                                                                                                                                                                                                                                          |
| `managers/history.rs`                   | Historial de transcripciones (SQLite).                                                                                                                                                                                                                                                                                                                                                                                                                                                       |
| `managers/tts/`, `managers/escucha/`    | Lectura por voz ("Escucha"): motor del sistema siempre; motores neuronales tras la feature `advanced-tts` (OFF por defecto, patrón «diferir, no borrar»).                                                                                                                                                                                                                                                                                                                                    |
| `commands/`                             | Superficie de comandos Tauri por área (audio, modelos, corrección, discos, historial…).                                                                                                                                                                                                                                                                                                                                                                                                      |
| `portable.rs`                           | Modo portable (marcador junto al ejecutable).                                                                                                                                                                                                                                                                                                                                                                                                                                                |

## Frontend (`src/`)

| Área                                       | Qué es                                                                                                                                                    |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `App.tsx` + `components/Sidebar.tsx`       | Shell clásico y tabla de secciones de la app.                                                                                                             |
| `components/settings/`                     | Páginas de ajustes (general, modelos, historial, avanzado, corrección, escucha…).                                                                         |
| `components/quiet/`                        | Shell Quiet: panel minimalista con inicio, esfera y últimas transcripciones.                                                                              |
| `components/retro/`, `components/bancada/` | Shells alternativos completos, **fuera del selector de la interfaz** (ver CHANGELOG_REBRAND).                                                             |
| `overlay/`                                 | Overlay de grabación y motor WebGL de la esfera (`overlay/esfera/engine.ts`).                                                                             |
| `stores/`, `hooks/`                        | Estado (Zustand) y acceso a settings/modelos.                                                                                                             |
| `i18n/locales/`                            | 22 locales con paridad verificada (`bun run check:translations`). **Anclado al build Rust**: `build.rs` genera las traducciones de la bandeja desde aquí. |
| `bindings.ts`                              | **Generado** por specta al arrancar la app en debug — jamás editar a mano.                                                                                |
| `lib/`                                     | Utilidades transversales (formato, teclado, idiomas de transcripción, tema).                                                                              |

## El flujo del dictado (módulos reales)

```text
atajo global            shortcut/ → transcription_coordinator.rs → actions.rs
captura + VAD           managers/audio.rs → audio_toolkit/ (cpal, resampler, silero)
transcripción           managers/transcription.rs (transcribe-cpp | transcribe-rs)
post-proceso de texto   transcription.rs::post_process_transcription_text
                        (reemplazos → custom words → diccionario → muletillas)
corrección              actions.rs → correccion::procesar
                        (protegidos → reglas/tildes/simbolos → validador;
                         Pulido opcional vía fraseador → sidecar u Ollama, con
                         degradación a reglas ante cualquier fallo)
inserción               actions.rs → clipboard.rs (paste en la app activa)
historial               managers/history.rs
```

## Anclas del build (no mover sin tocar el ancla)

- `build.rs` lee `../src/i18n/locales/*/translation.json` (locale `en` con
  sección `tray` es obligatorio) y stagea `transcribe-libs/` (consumido por
  `tauri.windows.conf.json`, `tauri.conf.json` y la CI).
- `tauri.conf.json` empaqueta `resources/**` — hay strings Rust que apuntan a
  esos nombres exactos (tray PNGs, VAD, sonidos, vocab GigaAM).
- `swift/` (puente Apple Intelligence, macOS ARM), `icons/`, `nsis/installer.nsi`,
  `Entitlements.plist`.
- `lib.rs` exporta `../src/bindings.ts` al arrancar en debug.
- El identifier `cl.abrax.app` define el datadir del usuario: cambiarlo
  huerfaniza settings/historial existentes.
