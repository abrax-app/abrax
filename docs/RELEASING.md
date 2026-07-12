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
| URLs UI (About, Updater)         | ✅ Hecho         | Placeholder `github.com/wmarquezz/abrax` (confirmar owner real antes de publicar)                                                                         |
| Nombre del producto en i18n      | ✅ Hecho         | 330 ocurrencias → `Abrax` en 22 locales; paridad verificada                                                                                               |
| **Fuentes de modelos**           | ⏳ **PENDIENTE** | 17 URLs `blob.handy.computer` en `src-tauri/src/managers/model.rs` (líneas 493–1025). Ver abajo                                                           |
| Nombre de lib Rust               | ⏳ Deuda menor   | `handy_app_lib` (interno, invisible al usuario). Renombrar a `abrax_app_lib` en una versión futura toca `main.rs` + fuerza recompilación total            |
| Release notes / test fixtures    | ⏳ Menor         | `src/content/release-notes/0.9.0.md` (link a cjpais/issues) y fixture `settings.rs:1199`                                                                  |

### Fuentes de modelos (tarea dedicada, NO cambiar a ciegas)

Los modelos legacy se descargan de `blob.handy.computer` (infra del autor original).
Migrar a fuentes propias **con verificación de checksum**:

1. Para cada modelo (17), localizar el equivalente en Hugging Face (la org
   `handy-computer` ya publica varios; el catálogo bundled ya usa la ruta HF).
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

Para reactivar el auto-update al publicar:

1. Confirmar el owner/repo real y ajustar `plugins.updater.endpoints` (hoy
   placeholder `github.com/wmarquezz/abrax`).
2. En CI, exportar `TAURI_SIGNING_PRIVATE_KEY` (contenido de la clave privada) y
   `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`.
3. `bun run tauri build` firma los artefactos y genera `latest.json`.
4. Publicar `latest.json` + artefactos en el endpoint.
5. Poner `update_checks_enabled` por defecto en `true` solo cuando el canal exista.

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

Generar `SHA256SUMS.txt` por release y publicarlo en la landing junto a las descargas.
