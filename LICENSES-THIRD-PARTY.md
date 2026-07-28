# Licencias de terceros de Abrax

Abrax es MIT. Los componentes de terceros que descarga o ejecuta conservan sus
propias licencias. Este documento las declara y da las atribuciones exigidas.

Cubre dos familias, y ninguna viaja dentro del instalador: los **modelos de
reconocimiento de voz (ASR)** que Abrax descarga bajo demanda, y los **motores de
voz (TTS)** que se aprovisionan en el primer uso.

- [Modelos de reconocimiento de voz (ASR)](#modelos-de-reconocimiento-de-voz-asr)
- [Motores de voz (TTS)](#motores-de-voz-tts)

---

# Modelos de reconocimiento de voz (ASR)

Abrax **no empaqueta ningún modelo**: el instalador viaja sin pesos y cada modelo
se descarga desde Hugging Face cuando el usuario lo elige. Aun así, Abrax los
distribuye de facto al ofrecerlos y descargarlos, así que **cada uno conserva su
licencia y sus obligaciones**, y las que exigen atribución la reciben aquí.

Los cinco que Abrax ofrece hoy son builds GGUF publicados por la organización
`handy-computer`, derivados por cuantización de los modelos originales. **La
licencia que manda es la del modelo original**, y es la que se declara.

| Modelo (nombre en la app) | Autor original     | Modelo original                                                                                           | Build GGUF que se descarga                                                                                                          | Licencia                                           |
| ------------------------- | ------------------ | --------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------- |
| Canary 180M Flash         | NVIDIA Corporation | [`nvidia/canary-180m-flash`](https://huggingface.co/nvidia/canary-180m-flash)                             | [`handy-computer/canary-180m-flash-gguf`](https://huggingface.co/handy-computer/canary-180m-flash-gguf)                             | **CC-BY-4.0**                                      |
| Whisper Large v3 Turbo    | OpenAI             | [`openai/whisper-large-v3-turbo`](https://huggingface.co/openai/whisper-large-v3-turbo)                   | [`handy-computer/whisper-large-v3-turbo-gguf`](https://huggingface.co/handy-computer/whisper-large-v3-turbo-gguf)                   | Apache-2.0                                         |
| Whisper Large v3          | OpenAI             | [`openai/whisper-large-v3`](https://huggingface.co/openai/whisper-large-v3)                               | [`handy-computer/whisper-large-v3-gguf`](https://huggingface.co/handy-computer/whisper-large-v3-gguf)                               | Apache-2.0                                         |
| Nemotron Streaming 3.5    | NVIDIA Corporation | [`nvidia/nemotron-3.5-asr-streaming-0.6b`](https://huggingface.co/nvidia/nemotron-3.5-asr-streaming-0.6b) | [`handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf`](https://huggingface.co/handy-computer/nemotron-3.5-asr-streaming-0.6b-gguf) | [**OpenMDW-1.1**](https://openmdw.ai/license/1-1/) |
| Cohere Transcribe         | Cohere Labs        | [`CohereLabs/cohere-transcribe-03-2026`](https://huggingface.co/CohereLabs/cohere-transcribe-03-2026)     | [`handy-computer/cohere-transcribe-03-2026-gguf`](https://huggingface.co/handy-computer/cohere-transcribe-03-2026-gguf)             | Apache-2.0                                         |

## Atribución exigida por CC-BY-4.0

La licencia [Creative Commons Attribution 4.0 International](https://creativecommons.org/licenses/by/4.0/)
obliga a dar crédito al autor, enlazar la licencia e indicar si hubo cambios.
Para el modelo CC-BY que Abrax ofrece:

> **Canary 180M Flash** © NVIDIA Corporation, usado bajo
> [CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/).
> **Modificado:** Abrax descarga un build **cuantizado a GGUF** publicado por
> `handy-computer`, no los pesos originales. No se modificó el entrenamiento del
> modelo.
> Abrax **no está afiliado a NVIDIA Corporation ni cuenta con su respaldo**, y
> NVIDIA no avala este producto.

Esta misma atribución se muestra **dentro de la app**, en Ajustes → Acerca de →
Agradecimientos, junto al autor y la licencia de cada modelo.

## Notas sobre las otras licencias

- **OpenMDW-1.1** (Nemotron Streaming 3.5) es la licencia de modelo abierto de
  NVIDIA; su texto vive en <https://openmdw.ai/license/1-1/>. No exige atribución
  en la interfaz, pero se acredita igual por coherencia.
- **Apache-2.0** (Whisper ×2, Cohere Transcribe) exige conservar los avisos de
  copyright y licencia, que viajan dentro de cada repositorio de modelo. Abrax no
  redistribuye esos archivos porque no empaqueta los pesos.

## Límites declarados

- **Sin verificación de integridad de los pesos.** Las descargas de modelos ASR
  **no se comprueban por sha256** hoy (`QuantFile` solo lleva `size_bytes`), a
  diferencia de los runtimes TTS de más abajo, que sí van anclados. Está anotado
  como deuda en `docs/RELEASING.md` §Checksums; no se anuncia lo contrario en
  ninguna parte.
- **Los builds GGUF los publica un tercero** (`handy-computer`). Abrax declara la
  licencia del modelo **original**, que es la que obliga; si un build intermedio
  declarara otra distinta, manda la del origen.
- El registro heredado del upstream contiene más entradas de modelos que la
  interfaz **no ofrece** (no son seleccionables ni descargables desde la app).
  Esta sección cubre los cinco que Abrax sí ofrece. Si alguno de los heredados
  volviera a ofrecerse, entra aquí antes de enviarse.

---

# Motores de voz (TTS)

Los motores de voz neuronales usan componentes con licencias distintas. Esta
sección explica cómo se mantiene la separación, en especial del **fonemizador
espeak-ng (GPL-3.0)**, para que el repositorio y el instalador MIT **nunca
enlacen ni redistribuyan código GPL**.

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
  (Piper) o por HTTP en 127.0.0.1 (Kokoro).
- Resultado: **cero GPL en el repositorio MIT, cero GPL en el instalador.**

## Componentes por motor

### Piper (motor estándar, CPU)

| Componente                                  | Licencia                                                    | Cómo se distribuye                                                                                                                                                                                  |
| ------------------------------------------- | ----------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Ejecutable `piper` (rhasspy/piper)          | fuente MIT, **binario GPL-3.0 efectivo** (embebe espeak-ng) | **Descargado** en el 1.er uso desde el release oficial (`rhasspy/piper` 2023.11.14-2), verificado por sha256. Ejecutado como proceso separado (stdin→WAV). **No se incluye en el repo/instalador.** |
| `espeak-ng` + `espeak-ng-data`              | **GPL-3.0-or-later**                                        | Viene dentro del bundle de `piper` descargado. Nunca en el repo.                                                                                                                                    |
| ONNX Runtime (`onnxruntime.dll`)            | **MIT** (Microsoft)                                         | Dentro del bundle de `piper`. Redistribuible.                                                                                                                                                       |
| Voz `es_MX-ald-medium` (`.onnx`/`.json`)    | **Unlicense** (dominio público)                             | Descargada con sha256 desde `rhasspy/piper-voices` (commit anclado).                                                                                                                                |
| Voz `es_ES-davefx-medium` (`.onnx`/`.json`) | **CC0-1.0**                                                 | Igual.                                                                                                                                                                                              |

### Kokoro (motor premium, CPU)

| Componente                                                                   | Licencia       | Cómo se distribuye                                             |
| ---------------------------------------------------------------------------- | -------------- | -------------------------------------------------------------- |
| `kokoro-onnx` (paquete Python)                                               | **Apache-2.0** | En el venv aprovisionado con `uv`, fuera del repo.             |
| Fonemizador español de `kokoro-onnx` (`espeakng-loader` / `phonemizer-fork`) | **GPL-3.0**    | Dentro del venv (proceso servidor separado). Nunca en el repo. |
| Pesos `kokoro-v1.0.onnx` + `voices-v1.0.bin`                                 | **Apache-2.0** | Descargados con sha256 desde el release de `kokoro-onnx`.      |

### Voz online (edge-tts) — ⚠️ NO LOCAL, opt-in

**Excepción deliberada** al principio "100% local" (decisión de producto). Este
motor **envía el texto a los servidores de Microsoft** (edge-tts usa el endpoint
de lectura en voz alta de Edge). Va **desactivado por defecto** y marcado en la UI
con «requiere internet · tu voz sale del equipo».

| Componente                            | Licencia                    | Cómo se distribuye                                                                                                     |
| ------------------------------------- | --------------------------- | ---------------------------------------------------------------------------------------------------------------------- |
| `edge-tts`                            | **GPL-3.0-or-later**        | En el venv aprovisionado con `uv`, fuera del repo (proceso separado, "mere aggregation"). Nunca en el repo/instalador. |
| `miniaudio` (decodifica MP3→PCM)      | **MIT-0 / dominio público** | En el venv.                                                                                                            |
| Voces neuronales de Microsoft (Azure) | servicio de Microsoft       | Se sintetizan **en la nube**; el audio vuelve por 127.0.0.1. Para producción correspondería Azure TTS con clave.       |

### Voces del sistema (fallback)

Voces del sistema operativo (SAPI en Windows, AVSpeech en macOS,
speech-dispatcher en Linux). Provistas por el SO; sin licencia adicional que
Abrax redistribuya.

## Scripts de servidor (código de Abrax, MIT)

`resources/tts/kokoro_server.py` y `resources/tts/online_server.py` son código
propio (MIT), embebidos en el binario y escritos al `runtime_dir` al arrancar.
Solo orquestan el runtime aprovisionado; no contienen código GPL.

## Verificación de integridad

Todas las descargas (runtimes con GPL y pesos limpios) se verifican por
**sha256 anclado** antes de usarse; un desajuste borra el archivo y aborta.
