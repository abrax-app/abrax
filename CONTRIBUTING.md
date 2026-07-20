# Contribuir a ABRAX

¡Gracias por tu interés en contribuir a ABRAX! Esta guía te ayuda a partir con el pie derecho.

> ABRAX es un fork amistoso de [Handy](https://github.com/cjpais/Handy) (MIT), de CJ Pais.
> Si tu corrección aplica también al núcleo original (audio, transcripción, plataforma),
> considera aportarla además al upstream — mantener sana esa base nos beneficia a todos.

## 📖 Filosofía

- **Local y privado**: todo el procesamiento ocurre en tu equipo. Nada sale sin una acción explícita tuya.
- **Español de primera clase**: la interfaz, la documentación y las funciones (muletillas, diccionario, lectura en voz alta) se diseñan primero en es-419, con 22 idiomas en paridad.
- **AI-nativo**: los asistentes de código son contribuidores de primera clase (ver [AGENTS.md](AGENTS.md)); la transparencia sobre su uso es parte de la cultura del proyecto.
- **Simplicidad heredada**: preferimos código claro y mantenible por sobre soluciones ingeniosas — es la tradición del proyecto original y la conservamos.

## 🚀 Preparar el entorno

Prerrequisitos: [Rust](https://rustup.rs/) (estable), [Bun](https://bun.sh/) y las herramientas de tu plataforma (ver [BUILD.md](BUILD.md)).

1. **Haz fork y clona**:

   ```bash
   git clone git@github.com:TU_USUARIO/abrax.git
   cd abrax
   git remote add upstream git@github.com:abrax-app/abrax.git
   ```

2. **Instala dependencias**:

   ```bash
   bun install
   ```

3. **Corre en modo desarrollo** (el modelo de VAD ya viene trackeado en el
   repo, no hay que descargar nada):

   ```bash
   bun run tauri dev
   # En macOS, si cmake da error:
   CMAKE_POLICY_VERSION_MINIMUM=3.5 bun run tauri dev
   ```

Para la arquitectura del código, ver [AGENTS.md](AGENTS.md) (backend Rust, frontend React/TypeScript, patrones y flujo).

## 🐛 Reportar bugs

Antes de reportar: busca en los issues existentes, prueba la última versión y activa el modo debug (`Cmd/Ctrl+Shift+D`) para juntar diagnóstico.

Usa la [plantilla de bug](.github/ISSUE_TEMPLATE/bug_report.md) e incluye:

- **Sistema**: versión de la app, sistema operativo, CPU y GPU.
- **Detalle**: descripción clara, pasos para reproducir, comportamiento esperado vs. real, capturas o logs (y lo que arroje el modo debug).

## 💡 Proponer funciones

Las ideas de funciones van a las **Discussions** del repositorio, no a los issues (los issues se reservan para bugs y tareas accionables). Describe el problema que quieres resolver, tu propuesta, alternativas consideradas y cómo calza con la filosofía del proyecto. Las PRs de funciones con interés demostrado de la comunidad tienen prioridad.

## 🔧 Contribuir código

### Antes de empezar

1. **Busca en issues y PRs** (abiertos Y cerrados): puede que ya exista, o que haya una razón por la que se cerró.
2. **Si retomas algo cerrado**: trae un argumento sólido y junta feedback en Discussions primero.
3. **Para funciones**: abre una Discussion antes o junto con tu PR.

### Flujo de trabajo

1. Crea una rama: `git checkout -b feature/tu-feature` o `fix/tu-fix`.
2. Haz cambios limpios y enfocados; sigue el estilo existente.
3. Prueba a fondo en tu(s) plataforma(s), incluido el modo debug.
4. Commits convencionales: `feat:` · `fix:` · `docs:` · `refactor:` · `test:` · `chore:` — el mensaje explica el _porqué_.
5. Mantén tu fork al día (`git fetch upstream && git rebase upstream/main`) y abre la PR llenando **toda** la plantilla.

### Declaración de asistencia de IA

**Las PRs asistidas por IA son bienvenidas** — ABRAX se construye así. Sé transparente en la descripción de tu PR:

- Si usaste IA (sí/no), qué herramientas (p. ej. "Claude Code", "Copilot") y con qué profundidad (boilerplate, debugging, la mayor parte del código).

### Estilo de código

**Rust**: `cargo fmt` + `cargo clippy` sin warnings; errores manejados explícitamente (sin `unwrap` en rutas de producción); doc comments en APIs públicas.

**TypeScript/React**: TypeScript estricto (sin `any`), componentes funcionales con hooks, Tailwind para estilos, componentes chicos y enfocados.

**i18n**: toda cadena visible pasa por `t()` (ESLint lo exige) y se agrega a los **22 idiomas** — ver [CONTRIBUTING_TRANSLATIONS.md](CONTRIBUTING_TRANSLATIONS.md). El copy fuente se escribe en es-419 neutro.

### Verificar antes de la PR

```bash
bun run lint                # ESLint
bun run format:check        # Prettier + cargo fmt
bun run check:translations  # paridad de los 22 idiomas
cd src-tauri && cargo test  # suite Rust
bun run tauri build         # build de producción
```

## 📝 Documentación

Mejoras a README, BUILD, esta guía, comentarios de código, tutoriales y mensajes de error son muy valoradas.

## 🤝 Convivencia

Sé respetuoso e inclusivo, paciente (equipo chico), constructivo y colaborativo. Busca antes de crear.

## 📜 Licencia

Al contribuir a ABRAX aceptas que tu contribución se licencie bajo MIT — ver [LICENSE](LICENSE). El proyecto conserva la atribución al upstream en [ATTRIBUTION.md](ATTRIBUTION.md) y [UPSTREAM.md](UPSTREAM.md).

---

**¡Gracias por contribuir a ABRAX!** Tu esfuerzo hace que dictar en español —y construir software con la voz— sea más accesible, privado y extensible para todos.
