# Abrax

Dictado por voz **100% local**. Abrax escucha, transcribe y escribe el texto en
la aplicación que tengas activa — sin cuentas, sin nube y sin que el audio
salga de tu equipo.

- **Local por defecto.** La transcripción (Whisper y otros modelos) corre en tu
  máquina. Las funciones que usan red son opcionales, explícitas y con el
  procesamiento local como respaldo.
- **Corrección inteligente determinista.** Una capa de corrección para es-419
  que resuelve autocorrecciones habladas («el martes, perdón, el miércoles»),
  restaura tildes seguras (`codigo` → `código`), normaliza símbolos dictados
  («dos slash tres» → `2/3`) y protege datos frágiles (fechas, números, URLs,
  correos, negaciones) para que ninguna transformación los altere.
- **Pulido con IA opcional.** Reformulación con un modelo local (vía Ollama o
  un modelo descargable que corre en un proceso aislado). Si falla o no está,
  el dictado sigue funcionando igual: la cadena siempre degrada a las reglas
  deterministas.
- **Multiplataforma.** Windows, macOS y Linux (Tauri 2: backend Rust +
  frontend React/TypeScript).

## Ejecutar en desarrollo

Requisitos: [Bun](https://bun.sh), Rust (MSVC en Windows) y las dependencias de
plataforma de Tauri 2 (ver [BUILD.md](BUILD.md)).

```bash
bun install
bun run tauri dev
```

No hay pasos de descarga de modelos para desarrollar: el modelo de VAD viene en
el repo y los modelos de transcripción se descargan desde la propia app.

## Build de producción

```bash
bun run tauri build
```

Detalles por plataforma, firma y empaquetado: [BUILD.md](BUILD.md) y
[docs/RELEASING.md](docs/RELEASING.md).

## Verificaciones

```bash
bun run lint                # ESLint (incluye la regla de i18n: sin literales en JSX)
bun run format:check        # Prettier + cargo fmt
bun run check:translations  # paridad de claves en los 22 locales
cd src-tauri && cargo test  # tests del backend
```

## Arquitectura en una línea

```text
audio (captura + VAD) → transcripción local → post-proceso de texto
→ corrección determinista (con protección de datos) → Pulido opcional (IA local)
→ inserción en la app activa
```

El mapa completo de carpetas y el flujo con módulos reales está en
[docs/architecture/repository-map.md](docs/architecture/repository-map.md).
Las decisiones técnicas importantes (por qué la corrección es determinista, por
qué no hay LLM embebido, por qué los sidecars van aislados) están en
[docs/FEATURE_DECISIONS.md](docs/FEATURE_DECISIONS.md).

## Convenciones

- Copy y UI en **es-419**; toda cadena visible pasa por `t()` (i18next, 22
  locales con paridad verificada).
- Commits atómicos convencionales (`feat:`/`fix:`/`chore:`/`docs:`).
- Flujo de colaboración y ramas: [docs/COLABORACION.md](docs/COLABORACION.md).
- Guía de contribución: [CONTRIBUTING.md](CONTRIBUTING.md) · Traducciones:
  [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md).

## Origen y licencias

Abrax es un producto independiente construido sobre el trabajo del proyecto
open source [Handy](https://github.com/cjpais/Handy). No implica afiliación,
respaldo ni continuidad oficial con sus mantenedores. Ver
[ATTRIBUTION.md](ATTRIBUTION.md), [UPSTREAM.md](UPSTREAM.md),
[LICENSE](LICENSE) y [LICENSES-THIRD-PARTY.md](LICENSES-THIRD-PARTY.md).

Los **modelos de reconocimiento de voz** no se empaquetan: se descargan bajo
demanda y cada uno conserva su licencia. El recomendado por defecto,
**Canary 180M Flash** © NVIDIA Corporation, es
[CC-BY-4.0](https://creativecommons.org/licenses/by/4.0/) y exige atribución;
esa atribución y la de los demás modelos están en
[LICENSES-THIRD-PARTY.md](LICENSES-THIRD-PARTY.md), que además viaja dentro del
instalador y se muestra en la app (Ajustes → Acerca de → Agradecimientos).
Abrax no está afiliado a NVIDIA, OpenAI ni Cohere, ni cuenta con su respaldo.
