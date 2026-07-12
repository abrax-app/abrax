# Changelog del rebranding — ABRAX

Estado consolidado al **2026-07-11**. Toda afirmación de este documento está respaldada por el historial de git de este repositorio.

## 1. Resumen ejecutivo

- **Base upstream:** fork de [Handy](https://github.com/cjpais/Handy) v0.9.1, commit `3882576`.
- **Trabajo propio:** 15 commits · 139 archivos tocados · +1.573 / −1.383 líneas.
- **Auditoría interna previa** (74 hallazgos catalogados por severidad): **17 resueltos** — 2 de los 3 P1, 5 de 25 P2 y 10 de 46 P3. Otros 11 se absorben por diseño en el rediseño de UI en curso (accesibilidad y theming de serie); el resto está agendado o se documenta como decisión.
- **Verificación al cierre (2026-07-11):** suite Rust **120/120** · ESLint **0 errores** · **22 idiomas** en paridad completa · build de frontend OK (3.4 s) · la app compila, arranca y corre como **Abrax** con carpeta de datos propia.

## 2. Identidad ABRAX

- Nombre de producto **Abrax**, identificador `cl.abrax.app`, binario `abrax`.
- **Carpeta de datos propia** (`cl.abrax.app`): una instalación de ABRAX convive con una de Handy sin tocar sus datos.
- **Iconos** de aplicación (ico/icns/PNG en todos los tamaños) y de bandeja (reposo/grabando/transcribiendo × tema claro/oscuro) generados desde el kit de logo propio; **paleta de marca** aplicada vía tokens de tema (violeta `#8B5CF6` como color primario de UI).
- Lockup e isotipo ABRAX en barra lateral, onboarding y About; título de ventana, tooltip de bandeja y ayuda de CLI con la marca nueva.
- **330+ cadenas de interfaz** re-marcadas en los **22 idiomas** soportados, manteniendo paridad completa de traducciones.
- **Actualizador automático deshabilitado** en esta etapa: el fork no consulta el servicio de actualizaciones del proyecto original ni acepta sus releases.
- Los encabezados HTTP del cliente LLM opcional identifican al fork con su propia marca.

## 3. Correcciones

| Hallazgo | Commit | Cómo se verificó |
|---|---|---|
| **R1 (P1)** — el dictado podía quedar colgado de forma permanente si el micrófono dejaba de entregar audio | `0e4108f` | Tests unitarios nuevos del recorder; suite Rust completa en verde |
| **S7 (P1)** — un build del fork habría aceptado actualizaciones del proyecto original | `e02c6a9` | Configuración sin endpoint ni clave del upstream; arranque real verificado |
| **S3 (P2)** — el texto dictado quedaba escrito en el log por defecto | `37c9fc2` | El log por defecto registra solo la longitud; el contenido solo aparece en nivel trace |
| **R5 (P2)** — dictar con método portapapeles destruía el contenido no-texto copiado | `c7b1df3` | Restaura texto e imágenes del portapapeles |
| **R6/R7 (P2)** — clientes HTTP (LLM y descargas de modelos) sin timeout | `4315030` | Timeouts de conexión y lectura configurados; build OK |
| **F2 (P2)** — fuga de listeners de eventos en el overlay de grabación | `2e7a4bc` | Cleanup registrado en el efecto; lint y build OK |
| **S11 (P3)** — encabezados HTTP anunciaban la marca del upstream | `e02c6a9` | Búsqueda de referencias en el árbol de código |
| **F23 (P3)** — enlaces de About/actualizador apuntaban al proyecto original | `e02c6a9` + `a948b2c` | Búsqueda de URLs; atribución al proyecto original conservada en About (22 idiomas) |
| **F14 (P3)** — versión de respaldo falsa ("0.1.2") si fallaba la lectura de versión | `3a1e1c8` | Fallback vacío; sin datos inventados |
| **R13 (P3)** — comandos IPC que fallarían al ser invocados | `c661c45` | Eliminados junto con C1 (no tenían llamadores) |
| Marca visible remanente (título de ventana, tooltip de bandeja, CLI, icono interno) | `2a2e6a3` | Barrido de la palabra en todo el árbol + arranque real |

## 4. Funciones nuevas o liberadas

- **Filtro de muletillas personalizable, ahora con interfaz** (`34c3715`). El motor de transcripción ya sabía filtrar muletillas con lista personalizada (`custom_filler_words`, con tests), pero la opción no tenía UI ni comando: solo se podía editar a mano el archivo de configuración. Ahora vive en Ajustes → Avanzado con tres estados siempre visibles (lista por defecto del idioma / lista propia / filtro desactivado), chips editables, soporte de frases multi-palabra ("o sea") y un **preset es-419** de 10 muletillas frecuentes que se combina sin duplicar ni borrar entradas del usuario. Textos en los 22 idiomas. Incluye test nuevo de frases multi-palabra; suite Rust 120/120.

## 5. Limpieza

- **10 comandos IPC muertos eliminados** (`c661c45` + `971c3fd`): −117 líneas Rust y −83 líneas de bindings generados. Superficie IPC: 107 → 97 comandos (98 hoy, con el comando nuevo del filtro de muletillas).
- **CLI interna sin target de build** (369 líneas) y dependencia `rdev` sin usos, eliminadas (`3a1e1c8`).
- **5 componentes React muertos** (~272 líneas, uno con datos falsos hardcodeados) y un `console.log` residual, eliminados (`3a1e1c8`).
- **Componentes de logo del upstream** eliminados tras el reemplazo (−115 líneas; `a948b2c`, `2a2e6a3`).
- **Iconos móviles (Android/iOS) del upstream retirados** (`a948b2c`): no se compila para móvil y se regeneran desde el kit propio si llegaran a necesitarse.
- Warning de compilación `unused_assignments` eliminado (`3a1e1c8`); normalización de fin de línea con `.gitattributes` (`380595d`).

## 6. Pendiente declarado

- Migración de las 17 fuentes de descarga de modelos a hosting con checksums anclados — fecha objetivo 22/07.
- Superficie de error visible cuando la ventana está oculta (F1) — scopeado, en cola.
- Estados de carga/vacío/error con reintento en modelos y onboarding (F3/F4) — scopeado, en cola.
- Visualizador de audio del overlay — en diseño; se activará solo bajo demanda (nunca emisión continua).
- Limpieza del evento interno sin oyentes `settings-changed` (C2) — trivial, agendada.
- Higiene final pre-publicación: `clippy --fix`, renormalización LF y migración del `baseUrl` deprecado de tsconfig.
- Color hardcodeado en el reproductor de audio del historial (F22) — agendado con el rediseño de UI.
- Adaptación de los workflows de CI para builds propios.

## 7. Atribución

ABRAX es un fork de **Handy**, creado por **CJ Pais** y publicado bajo licencia **MIT**. Gran parte del mérito de que esta app exista —el pipeline de dictado, el soporte multiplataforma, la base de i18n— es del proyecto original y de sus contribuidores. Conservamos `LICENSE`, `ATTRIBUTION.md` y `UPSTREAM.md`, y el About de la app reconoce explícitamente al proyecto original. ABRAX no está afiliado a Handy ni a su autor.
