# Privacidad — Abrax

> Principio: **procesamiento local por defecto**. El dictado se transcribe en el
> equipo; nada del audio, del texto dictado ni del proyecto indexado sale a la
> red salvo por una acción explícita del usuario, enumerada abajo.

## Qué se procesa localmente

- **Dictado (STT):** transcripción 100% en el equipo con `transcribe-cpp`
  (familia Whisper) y `transcribe-rs` (Parakeet/Moonshine/…). No hay endpoint de
  STT en la nube en el código. Motor y catálogo en
  `src-tauri/src/managers/model.rs`.
- **Corrección de dictado:** el módulo `correccion` es local por diseño; la única
  red que toca es el loopback (`http://127.0.0.1:11434`, Ollama, solo si el
  usuario lo tiene y optó). `src-tauri/src/correccion/mod.rs:1-3`,
  `src-tauri/src/correccion/fraseador.rs:24-27`. Por defecto el motor está
  `Desactivado` (passthrough exacto). `src-tauri/src/settings.rs:785-786`.
- **Diccionario Vivo:** indexa el proyecto activo respetando `.gitignore`; nada
  del repositorio sale del equipo, el índice vive en el datadir.
  `src-tauri/src/dictionary.rs:11-12, 26`.
- **Memoria de correcciones «en el sitio»:** lee el campo enfocado vía API de
  accesibilidad local; no persiste ni registra el contenido leído, solo los pares
  aprendidos. `src-tauri/src/memoria_en_sitio.rs:11-17`.
- **Catálogo de modelos:** compilado en el binario (`include_str!`), listar
  modelos no toca la red. `src-tauri/src/catalog/mod.rs:6-9`.

## Qué puede salir a la red — y bajo qué acción explícita

Estas son las **únicas** salidas de red del producto. Todas son opt-in o
dependen de una acción del usuario, con verificación cuando aplica.

| Salida | Cuándo | Payload | Verificación / estado |
| --- | --- | --- | --- |
| Descarga de modelos STT / de pulido | El usuario elige descargar un modelo | Petición HTTP a Hugging Face o al blob de origen | **SHA-256** previo a extracción en la ruta `Url`; `model.rs:496-501`, `correccion/modelos.rs:24-27` |
| Post-proceso LLM (BYOK) | Opt-in; `post_process_enabled` = **false** por defecto | La transcripción + la API key del usuario, al `base_url` del proveedor elegido | `settings.rs:535, 789-791`; `actions.rs:101-176`; `llm_client.rs`. Ver S10 abajo |
| Fraseador local (Ollama) | Opt-in, solo si Ollama está instalado | Texto al **loopback** `127.0.0.1:11434`; nunca sale del equipo | `correccion/fraseador.rs:24-27` |
| Actualizaciones (updater) | Opt-in; `update_checks_enabled` = **false** por defecto | Consulta al endpoint de releases propio, firma minisign nueva | `settings.rs:669-673`; `tauri.conf.json:79-84` |

## Qué se guarda en disco y dónde

Todo bajo `app_data_dir` (o junto al ejecutable en **modo portable**, si existe el
marcador `portable`). `src-tauri/src/portable.rs`; `managers/model.rs:464-466`.

| Archivo | Contenido | Referencia |
| --- | --- | --- |
| `history.db` (SQLite) | Historial de dictados: `transcription_text`, `post_processed_text`, `post_process_prompt`, título, marca de tiempo, `saved` | `managers/history.rs:20-33, 79` |
| `recordings/*.wav` | Audio de cada dictado | `managers/history.rs:70-89, 689` |
| `settings_store.json` | Todos los ajustes: `custom_words`, `custom_replacements`, `memoria_correcciones` y **las API keys de post-proceso** | `settings.rs:1006` |
| `dictionary_index.json` | Índice del Diccionario Vivo | `dictionary.rs:26` |
| `models/` | Modelos STT y GGUF de pulido (carpeta reubicable a otro disco) | `commands/discos.rs` |
| `webview/` | Data directory del webview | `overlay.rs:310-312` |

El texto dictado y el audio se persisten **en el equipo** como función de
Historial (no en logs). Es comportamiento local-first esperado; ver borrado y
retención abajo.

## Logs: el contenido dictado queda fuera por defecto

El contenido de un dictado solo se emite a nivel `trace`; el nivel de log por
defecto no lo captura. A `info`/`debug` solo se registran longitudes y conteos,
nunca el texto:

- Transcripción: a `info` solo `{n} chars`; el texto completo solo en `trace`.
  `managers/transcription.rs:1600-1609`.
- Post-transcripción: se registran tiempos y longitud, nunca el texto dictado por
  defecto. `actions.rs:809-818`.
- Fraseador y validador: solo método/longitudes/conteos, ni una palabra del
  dictado. `correccion/fraseador.rs:16-18`; `correccion/mod.rs:156, 200-201`.

(Hallazgo **S3** de la auditoría — «transcripción fuera de logs por defecto» —
atendido en esta línea de trabajo.)

## Secretos / API keys

**Estado actual, declarado con honestidad:** las API keys de post-proceso se
guardan en `settings_store.json` en **texto plano** (el store no está cifrado, y
`SecretMap` serializa transparente). La única defensa presente es la redacción en
`Debug`/logs: `SecretMap::fmt` sustituye los valores por `[REDACTED]`, de modo que
ni el volcado de ajustes ni el diagnóstico filtran la key.

- Almacenamiento en claro: `settings.rs:540-541, 1329-1335`; store sin cifrar
  `lib.rs:836`.
- Redacción en logs: `settings.rs:395-408`.

No se usa el almacenamiento seguro de plataforma (keychain/keyring). La key queda
protegida solo por los permisos del sistema de archivos del usuario. Pendiente de
endurecer (hallazgo **S2**): mover las keys a almacenamiento seguro o cifrarlas en
reposo.

## Borrado y cancelación

- **Borrar una entrada del historial** (audio + texto): `delete_history_entry`.
  `commands/history.rs:49-51`.
- **Retención automática de audio:** por defecto `PreserveLimit` — conserva tantas
  grabaciones como `history_limit` (por defecto 5). `settings.rs:232-238, 522`.
  Las entradas marcadas `saved=1` no caducan (hallazgo **S8**: documentar y ofrecer
  un «borrar todo» que las incluya).
- **Borrar un modelo:** `delete_model`. `commands/models.rs:58`.
- **Cancelación:** operación de dictado/grabación con `cancel_operation` (atajo
  `escape`); descargas con `cancel_download`. `commands/mod.rs:22-25`;
  `commands/models.rs:181-186`.

## Riesgos conocidos y estado

Trazabilidad con la auditoría (`docs/CODE_AUDIT.md`). Ninguno bloquea el flujo
principal de dictado, que es 100% local.

| ID | Área | Resumen | Estado |
| --- | --- | --- | --- |
| S2 | Secretos | API keys de post-proceso en claro en `settings_store.json`; solo redactadas en logs | Documentado; endurecimiento pendiente (keychain/cifrado) |
| S3 | Logs | El texto dictado no va a logs por defecto (solo `trace`) | Atendido |
| S4 | Descargas | La ruta Hugging Face no fija hash propio (`revision:"main"` mutable); la ruta `Url` sí verifica SHA-256 | Pendiente: fijar `sha256`/revisión por commit |
| S6 | Micrófono | Disparos por IPC/segunda instancia/señales pueden iniciar la grabación sin validación adicional | Modelo de confianza: un proceso local del mismo usuario; documentado |
| S8 | Datos locales | WAVs y transcripciones en claro (local-first); las guardadas no caducan | Documentado |
| S10 | Red / LLM | Con un proveedor de post-proceso `custom`, el texto y la key van al host que el usuario configure | El host es responsabilidad del usuario; conviene avisar en la UI |

## Verificación

- `settings_store.json`, `history.db` e índices se inspeccionan siempre sobre
  **copia**, nunca editando el datadir a mano.
- Egreso de red comprobable revisando los cuatro casos de la tabla «Qué puede
  salir a la red»; fuera de ellos el procesamiento es local.
