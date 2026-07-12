# Escucha — Fase 2: voz neural premium (Piper/ONNX)

> **Estado: SOLO DISEÑO.** No hay código de Fase 2 en el repo; el MVP de
> Fase 1 corre sobre el TTS del sistema operativo (crate `tts`: WinRT en
> Windows, AVSpeech en macOS, speech-dispatcher en Linux). Ver
> `src-tauri/src/managers/escucha/`.

## Por qué Piper

El TTS del SO cumple la promesa "cero descargas, 100% local", pero su calidad
depende del sistema del usuario: en Windows las voces OneCore españolas
(Pablo/Helena/Laura) son inteligibles pero robóticas; en Linux,
speech-dispatcher suele caer en espeak (calidad muy baja). Piper ofrece voces
neuronales (VITS) de calidad natural, corre en CPU en tiempo real, es MIT, y
encaja con la infraestructura que Abrax ya tiene: modelos ONNX descargables
con verificación de integridad.

## Principio rector

**Las voces Piper son "modelos" del ModelManager existente** — no un sistema de
descargas paralelo. Se reutiliza todo lo que ya está probado en
`src-tauri/src/managers/model.rs`:

- Descarga cancelable vía `hf-hub` (fork cancellable-downloads ya en Cargo).
- Verificación `sha256` por archivo (mecanismo `ModelDescriptor` con
  `sha256: Option<String>` ya existente).
- Cache compartido de Hugging Face para no re-descargar lo que otras
  herramientas ya bajaron.
- Eventos de progreso de descarga hacia el frontend (misma UI de progreso que
  los modelos de transcripción).

## Alcance propuesto

### Voces (es-419 primero)

| Voz | Repo HF | Tamaño aprox. | Uso |
|---|---|---|---|
| `es_MX-claude-high` (o equivalente comunitario) | `rhasspy/piper-voices` | ~60 MB | Prosa |
| `es_ES-davefx-medium` | `rhasspy/piper-voices` | ~40 MB | Prosa alternativa |
| `es_ES-sharvard-medium` | `rhasspy/piper-voices` | ~40 MB | Código (cadencia neutra) |

Cada voz Piper son 2 archivos: `voz.onnx` + `voz.onnx.json` (config). Ambos con
sha256 fijado en el catálogo (mismo patrón que `catalog/`).

### Arquitectura

```
managers/escucha/
├── mod.rs            (EscuchaManager: hilo + canal, sin cambios de API)
├── preproceso.rs     (sin cambios: es agnóstico del motor)
└── motor/
    ├── mod.rs        (trait MotorTts: hablar/detener/esta_hablando/voces)
    ├── sistema.rs    (backend crate `tts` — el MVP actual, renombrado)
    └── piper.rs      (backend ONNX Runtime, Fase 2)
```

- El `trait MotorTts` se extrae del worker actual; `escucha_speak` elige el
  backend según settings (`escucha_motor: Sistema | Piper`).
- Piper corre sobre **ONNX Runtime ya enlazado** en el proyecto
  (`transcribe-rs` features `onnx`): no se agrega runtime nuevo, solo el grafo
  VITS y el espeak-ng phonemizer (crate `espeak-rs` o `piper-rs`; evaluar
  licencia GPL de espeak-ng → alternativa: phonemizer puro Rust o empaquetar
  como sidecar opcional).
- Salida de audio por `rodio` (ya en Cargo) hacia el device de salida que el
  usuario ya elige en settings (`selected_output_device`).

### El riesgo a resolver ANTES de codificar

El fonemizador es la parte con licencia delicada: espeak-ng es **GPLv3**, y
Abrax es MIT. Opciones, en orden de preferencia:

1. `piper-rs` con phonemización eSpeak como **proceso sidecar** (GPL aislada en
   binario aparte, opt-in de descarga como las voces).
2. Voces entrenadas con phonemización por texto plano (algunas voces Piper
   "medium" aceptan grafemas — degrada calidad en números/abreviaturas, que el
   preprocesador ya expande de todos modos).
3. Portar el G2P español a Rust (esfuerzo alto; español es relativamente
   regular — factible como proyecto aparte).

Sin decisión de licencia, Fase 2 no arranca. Ese es el gate.

### Settings nuevos (aditivos)

- `escucha_motor: Sistema | Piper` (default `Sistema`; si Piper no tiene voz
  descargada, cae a Sistema con aviso).
- `escucha_voz_piper_prosa`, `escucha_voz_piper_codigo` (ids del catálogo).

### Sincronía mejorada (bonus de Fase 2)

Piper genera el audio completo antes de reproducir → se conoce la duración
exacta de cada oración. El resaltado puede pasar de "poll de is_speaking" a
una línea de tiempo precisa, incluso con karaoke por palabra usando los
alineamientos del modelo (VITS expone duraciones por fonema).

## Qué NO cambia

- `preproceso.rs` (la verbalización es del dominio, no del motor).
- Los comandos Tauri (`escucha_*`): misma API, otro backend.
- El panel Escucha: solo gana un selector de motor y el catálogo de voces
  descargables (reutilizando los componentes de la sección Models).
