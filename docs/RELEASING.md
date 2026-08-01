# Releases de escritorio — Abrax

> Estado: rebrand técnico en curso (rama `rebrand/desconexion`). **Cero binarios
> públicos** hasta cerrar la desconexión del upstream y sanear el repo.

## Desconexión del upstream — estado

| Punto                            | Estado           | Detalle                                                                                                                                                   |
| -------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| productName / identifier         | ✅ Hecho         | `Abrax` / `cl.abrax.app` en `src-tauri/tauri.conf.json`                                                                                                   |
| Binario                          | ✅ Hecho         | `abrax` (`src-tauri/Cargo.toml` `name`/`default-run`)                                                                                                     |
| Carpeta de datos                 | ✅ Hecho         | Derivada del identifier → `%APPDATA%/cl.abrax.app` (macOS `~/Library/Application Support/cl.abrax.app`). Convive con una instalación de Handy sin pisarla |
| Updater endpoint + pubkey        | ✅ Desconectado  | Pubkey NUEVO propio + `update_checks_enabled=false` por defecto. Ya no apunta a `cjpais/Handy`                                                            |
| signCommand (Azure de cjpais)    | ✅ Quitado       | Ver "Firma" abajo                                                                                                                                         |
| Headers LLM (Referer/UA/X-Title) | ✅ Hecho         | `Abrax` / `abrax.app` en `src-tauri/src/llm_client.rs`                                                                                                    |
| URLs UI (About, Updater)         | ✅ Hecho         | `github.com/abrax-app/abrax` — owner definitivo (organización `abrax-app`)                                                                                |
| Nombre del producto en i18n      | ✅ Hecho         | 330 ocurrencias → `Abrax` en 22 locales; paridad verificada                                                                                               |
| **Fuentes de modelos**           | ⏳ **PENDIENTE** | 5 tarballs `blob.handy.computer` en `model.rs` (moonshine ×2, giga-am, canary ×2) + 1 mirror de CI (onnxruntime macOS Intel). Ver abajo                   |
| Nombre de lib Rust               | ⏳ Deuda menor   | `handy_app_lib` (interno, invisible al usuario). Renombrar a `abrax_app_lib` en una versión futura toca `main.rs` + fuerza recompilación total            |
| Release notes / test fixtures    | ⏳ Menor         | `src/content/release-notes/0.9.0.md` (link a cjpais/issues) y fixture `settings.rs:1199`                                                                  |

### Fuentes de modelos (tarea dedicada, NO cambiar a ciegas)

Estado 2026-07-20: quedan **5 tarballs** de modelos legacy en
`blob.handy.computer` (`model.rs`: moonshine-base, moonshine-tiny-streaming,
giga-am-v3-int8, canary-180m-flash, canary-1b-v2 — todos con `sha256` presente,
así que la integridad está protegida aunque el hosting sea de terceros). En CI,
onnxruntime Linux y Windows ya bajan del **release oficial de Microsoft**; el de
**macOS Intel sigue en el blob** porque Microsoft dejó de publicar binarios
osx-x86_64 (v1.24.2 solo trae arm64) — ese requiere hosting propio sí o sí.

Migrar a fuentes propias **con verificación de checksum**:

1. Para cada tarball, localizar el equivalente en Hugging Face (la org
   `handy-computer` ya publica varios; el catálogo bundled ya usa la ruta HF) o
   re-subirlo a la org HF propia cuando exista la cuenta.
2. Reemplazar `ModelSource::Url { url, sha256 }` por la URL de HF **conservando el
   `sha256`** ya presente (la verificación SHA-256 previa a extracción ya existe y
   debe mantenerse — hallazgo S4 de la auditoría).
3. Verificar una descarga real de un modelo tiny/base antes de dar por buena la
   migración (no romper la demo).
4. Alternativa: hospedar los blobs en infraestructura propia (R2/S3) con los
   mismos checksums.

Hasta cerrar esto, la app sigue descargando de `blob.handy.computer`: **no
distribuir binarios públicos** (viola "cero dependencia de servidores del autor").

## Updater — reactivación (Fase 11)

Par de claves minisign **nuevo** ya generado (2026-07-11):

- Clave pública: en `src-tauri/tauri.conf.json` → `plugins.updater.pubkey`
  (huella `4F1D4C081B707F14`, distinta de la de upstream `BAB72095206601F9`).
- Clave privada: **fuera del repo**, en `handy-rebrand-claude-code-kit/.secrets-abrax/abrax-updater.key`
  (sin contraseña; para producción, regenerar con contraseña). **Nunca** commitear.

**`bundle.createUpdaterArtifacts` está en `false`** (2026-07-26). Con la pubkey
presente y ese flag en `true`, `tauri build` intenta firmar los artefactos del
updater al final del empaquetado y, si no encuentra `TAURI_SIGNING_PRIVATE_KEY`,
**sale con código 1 — después de haber generado los instaladores**. Los binarios
quedan completos y usables, pero un job de CI lo lee como fracaso: no publica el
release y quema la cuota igual (los runners de macOS cuentan ×10). Con el updater
apagado esos artefactos no los puede usar nadie, así que no se generan.

Para reactivar el auto-update al publicar:

1. El owner/repo definitivo es `github.com/abrax-app/abrax` (organización
   `abrax-app`); `plugins.updater.endpoints` ya apunta ahí.
2. **Volver a poner `bundle.createUpdaterArtifacts` en `true`** en
   `src-tauri/tauri.conf.json`. Sin esto no se generan `.sig` ni `latest.json`.
3. En CI, exportar `TAURI_SIGNING_PRIVATE_KEY` (contenido de la clave privada) y
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`. **Los pasos 2 y 3 van juntos**: el flag
   en `true` sin la clave en el entorno es exactamente el fallo descrito arriba.
4. `bun run tauri build` firma los artefactos y genera `latest.json`.
5. Publicar `latest.json` + artefactos en el endpoint.
6. Poner `update_checks_enabled` por defecto en `true` solo cuando el canal exista.

## 🚦 GATE: la landing y los binarios se publican JUNTOS

La landing tiene una sección **«Antes de instalar»** (`landing/index.html`,
`id="avisos"`) escrita para binarios **SIN FIRMAR**: explica el aviso de
SmartScreen, el «editor desconocido» del UAC, el bloqueo de Gatekeeper y la ruta
por Ajustes del Sistema.

**Esa sección y el estado de firma de los binarios tienen que coincidir. Las dos
combinaciones cruzadas mienten:**

| Binario                  | Sección «Antes de instalar» | Resultado                                                                                             |
| ------------------------ | --------------------------- | ----------------------------------------------------------------------------------------------------- |
| Sin firmar / ad-hoc      | **Presente**                | ✅ Correcto                                                                                           |
| Sin firmar / ad-hoc      | Quitada                     | ❌ El usuario ve dos avisos de seguridad sin contexto y cierra la ventana                             |
| Firmado **y** notarizado | **Presente**                | ❌ Miente al revés: anuncia bloqueos que no van a ocurrir y siembra una desconfianza que ya no aplica |
| Firmado **y** notarizado | Quitada                     | ✅ Correcto                                                                                           |

**Estado a 27/07/2026 — se publica SIN FIRMAR, la sección SE QUEDA.** El `.dmg`
de la entrega se construye **en local** en el Mac Apple Silicon: firma **ad-hoc**
(`signingIdentity: "-"`), sin certificado Apple (la inscripción no ha llegado) y
con el CI bloqueado por facturación. Windows tampoco va firmado.

**Si el certificado llegara antes de publicar** y se firmara **y notarizara** de
verdad — las dos cosas, firmar sin notarizar NO quita el bloqueo —, hay que
**quitar la sección entera** y también los dos enlaces `href="#avisos"` de las
tarjetas de compatibilidad. Notarizar y dejar la sección puesta es el error más
caro de los cuatro: es el único que resta credibilidad a un producto que ya
cumple.

### Lo que un usuario de macOS ve hoy, y por qué está escrito así

- **«Clic derecho → Abrir» YA NO FUNCIONA.** Apple eliminó ese atajo en **macOS
  15 Sequoia** ([Developer News, 6/8/2024](https://developer.apple.com/news/?id=saqachfa):
  «users will no longer be able to Control-click to override Gatekeeper… They'll
  need to visit System Settings > Privacy & Security»). Cualquier instrucción
  que lo mencione manda al usuario a un callejón sin salida y le hace concluir
  que la app está rota. La ruta válida es **Ajustes del Sistema → Privacidad y
  seguridad → Abrir de todas formas**, con dos avisos que el sistema no da: hay
  que **intentar abrir la app antes** de que el botón aparezca, y el botón
  **caduca en cosa de una hora**.
- **Con firma ad-hoc, macOS puede decir que la app «está dañada»** y en ese caso
  **no ofrece ningún botón para abrir**. La única salida es
  `xattr -dr com.apple.quarantine /Applications/Abrax.app`. Por eso ese comando
  está en la landing y no solo aquí.
- De los diálogos se citan **los botones, no el texto completo**: las cadenas
  exactas varían entre builds y versiones, y una cita que no coincide con lo que
  el usuario tiene en pantalla destruye la confianza que la sección construye.

## Firma

- **Windows:** el `signCommand` de Azure Trusted Signing de cjpais fue **eliminado**.
  Local: `bun run tauri build --no-bundle` o NSIS sin firma. Producción: configurar
  firma propia (Azure Trusted Signing o certificado EV) y documentar el secreto.
- **macOS (prioridad alta):** sin certificado Apple Developer.
  Opciones: firma ad-hoc (`codesign -s -`) + instrucciones de Gatekeeper
  (`xattr -dr com.apple.quarantine /Applications/Abrax.app`), o notarización real
  con cuenta Apple. Verificar antes si el fix de compilación macOS 26 está en el
  commit base `3882576`; cherry-pick si falta, antes del primer build CI macOS.

## Builds por plataforma

### Windows

- Arquitecturas: x86_64 (verificado en dev), aarch64 (CI)
- Firma: pendiente (ver arriba)
- Instalador/portable: NSIS (`nsis/installer.nsi`) + MSI; portable via marcador `portable`
- Artefactos: `abrax.exe`, `Abrax_0.9.1_x64-setup.exe`

### macOS

- Arquitecturas: aarch64 + x86_64
- Firma / Notarización: pendiente (ver arriba)
- Artefactos: `Abrax.app`, `Abrax_0.9.1_*.dmg`

### Linux

- Formatos: deb, rpm, AppImage
- Dependencias: gtk-layer-shell, openblas
- Wayland/X11: overlay desactivado por defecto en Linux (documentado por upstream)

## Secretos requeridos

| Secreto                              | Plataforma      | Dónde configurar | No incluir en |
| ------------------------------------ | --------------- | ---------------- | ------------- |
| `TAURI_SIGNING_PRIVATE_KEY`          | Todas (updater) | CI secret        | Repo          |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Todas (updater) | CI secret        | Repo          |
| Certificado de firma Windows         | Windows         | CI secret        | Repo          |
| Credenciales Apple (notarización)    | macOS           | CI secret        | Repo          |

## Checksums

**Estado real 27/07/2026: NO se genera ni se publica ningún `SHA256SUMS.txt`.**
Ningún paso de CI lo produce (verificado sobre `.github/workflows/`). La landing
lo anunciaba —«publicado junto a cada release firmado»— y era doblemente falso:
ni existe el archivo ni se firma nada. Esa frase se quitó el 27/07 y se sustituyó
por lo que sí es cierto: el código está entero y se puede compilar una copia
propia.

Pendiente para cuando haya release de verdad: añadir un paso de CI que genere
`SHA256SUMS.txt` con los artefactos y lo suba al release. **Solo entonces** se
vuelve a anunciar en la landing — no antes.

Ojo con el matiz que sí es cierto hoy y conviene no perder: **las voces de TTS sí
verifican sha256** al descargarse (`src-tauri/src/managers/tts/download.rs`), y
los modelos legacy por URL también (`model.rs`, campo `sha256`). Los **cinco
modelos del catálogo** bajan de Hugging Face por una ruta que **no** comprueba
hash. La landing lo dice así, separado.
