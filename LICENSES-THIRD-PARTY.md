# Licencias de terceros — motores de voz (TTS) de Abrax

Abrax es MIT. Los motores de voz neuronales usan componentes con licencias
distintas. Este documento explica cómo se mantiene la separación, en especial
del **fonemizador espeak-ng (GPL-3.0)**, para que el repositorio y el instalador
MIT **nunca enlacen ni redistribuyan código GPL**.

## Principio: "mere aggregation" (agregación simple)

El fonemizador **espeak-ng** (GPL-3.0-or-later) es el conversor grafema→fonema
de facto para español y **contamina el binario de cualquier runtime que lo
enlace**. La FSF permite agregar programas GPL y no-GPL en el mismo sistema
siempre que se comuniquen como **procesos separados** y no formen un solo
ejecutable. Abrax aplica exactamente eso:

- El código de Abrax (Rust + los scripts de servidor Python que shippeamos) es
  **MIT** y **no importa, enlaza ni vendoriza** ningún componente GPL.
- Los runtimes que contienen GPL se **descargan/aprovisionan en el primer uso**
  en `<datadir>/tts/runtime/…`, **fuera del repositorio y del instalador**, y se
  ejecutan como **procesos separados** con los que Abrax habla por CLI/stdin
  (Piper) o por HTTP en 127.0.0.1 (Chatterbox/Kokoro).
- Resultado: **cero GPL en el repositorio MIT, cero GPL en el instalador.**

## Componentes por motor

### Piper (motor estándar, CPU)

| Componente | Licencia | Cómo se distribuye |
|---|---|---|
| Ejecutable `piper` (rhasspy/piper) | fuente MIT, **binario GPL-3.0 efectivo** (embebe espeak-ng) | **Descargado** en el 1.er uso desde el release oficial (`rhasspy/piper` 2023.11.14-2), verificado por sha256. Ejecutado como proceso separado (stdin→WAV). **No se incluye en el repo/instalador.** |
| `espeak-ng` + `espeak-ng-data` | **GPL-3.0-or-later** | Viene dentro del bundle de `piper` descargado. Nunca en el repo. |
| ONNX Runtime (`onnxruntime.dll`) | **MIT** (Microsoft) | Dentro del bundle de `piper`. Redistribuible. |
| Voz `es_MX-ald-medium` (`.onnx`/`.json`) | **Unlicense** (dominio público) | Descargada con sha256 desde `rhasspy/piper-voices` (commit anclado). |
| Voz `es_ES-davefx-medium` (`.onnx`/`.json`) | **CC0-1.0** | Igual. |

### Kokoro (motor premium, CPU)

| Componente | Licencia | Cómo se distribuye |
|---|---|---|
| `kokoro-onnx` (paquete Python) | **Apache-2.0** | En el venv aprovisionado con `uv`, fuera del repo. |
| Fonemizador español de `kokoro-onnx` (`espeakng-loader` / `phonemizer-fork`) | **GPL-3.0** | Dentro del venv (proceso servidor separado). Nunca en el repo. |
| Pesos `kokoro-v1.0.onnx` + `voices-v1.0.bin` | **Apache-2.0** | Descargados con sha256 desde el release de `kokoro-onnx`. |

### Chatterbox (motor premium, GPU NVIDIA/Apple)

| Componente | Licencia | Cómo se distribuye |
|---|---|---|
| `chatterbox-tts` (Resemble AI) | **MIT** | En el venv aprovisionado con `uv`, fuera del repo. |
| PyTorch (`torch`, `torchaudio`) | **BSD-3-Clause** | En el venv. |
| Modelo Chatterbox | licencia del modelo (ver su tarjeta) | Descargado por el propio paquete en la 1.ª síntesis, cacheado localmente. |

### Voces del sistema (fallback)

Voces del sistema operativo (SAPI en Windows, AVSpeech en macOS,
speech-dispatcher en Linux). Provistas por el SO; sin licencia adicional que
Abrax redistribuya.

## Scripts de servidor (código de Abrax, MIT)

`resources/tts/chatterbox_server.py` y `resources/tts/kokoro_server.py` son
código propio (MIT), embebidos en el binario y escritos al `runtime_dir` al
arrancar. Solo orquestan el runtime aprovisionado; no contienen código GPL.

## Verificación de integridad

Todas las descargas (runtimes con GPL y pesos limpios) se verifican por
**sha256 anclado** antes de usarse; un desajuste borra el archivo y aborta.
