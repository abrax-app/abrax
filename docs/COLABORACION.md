# Colaboración

Cómo trabajamos varias personas, en varias máquinas, sobre este mismo repo sin
pisarnos. Todo lo de aquí aplica igual para cualquiera que tenga acceso.

## Puesta en marcha en una máquina nueva

```bash
gh auth login
gh repo clone <owner>/<repo> abrax
cd abrax
bun install
```

Los prerrequisitos de compilación (Rust, Bun, Tauri y, en Windows, CMake +
Vulkan SDK + rutas largas + `CARGO_TARGET_DIR` corto) están en
[BUILD.md](../BUILD.md). Dos avisos que ahorran sustos:

- **La primera compilación de Rust demora ~15 minutos.** No está colgada.
- En Windows, sin un `CARGO_TARGET_DIR` corto el build **falla** por el límite
  de 260 caracteres del sistema. Es obligatorio, no opcional.

Para levantar la app:

```bash
bun run tauri dev                          # build liviano (solo Voces del Sistema)
bun run tauri dev --features advanced-tts  # completo (motores neuronales + detección de hardware)
```

### El token necesita el scope `workflow`

El repo tiene workflows en `.github/`, y GitHub rechaza cualquier push que los
toque si al token le falta ese permiso:

```bash
gh auth refresh -h github.com -s workflow
```

## Modelo de ramas

`rebrand/product` es la **línea de integración**. La regla de oro:

> Nadie commitea directo a la línea de integración. Todo entra por PR.

```bash
# Antes de empezar, siempre:
git checkout rebrand/product
git pull --rebase origin rebrand/product

# Trabajo en rama propia:
git checkout -b feat/lo-que-sea
# ...commits...
git push -u origin feat/lo-que-sea

# Integrar:
gh pr create --base rebrand/product --title "feat: ..." --body "..."
```

Por qué PR y no merge directo: **todos vemos qué está entrando antes de que
entre.** Un merge sorpresa sobre el trabajo de otra persona es justo lo que
esto evita.

Además:

- **Nunca `push --force`** sobre ramas compartidas.
- Quien mergea, avisa.
- Si dos personas trabajan el mismo día, mejor sobre archivos distintos.

## Antes de abrir un PR

```bash
cargo test                     # desde src-tauri/
cargo clippy                   # sin warnings nuevos
cargo fmt
bun run lint
bun run check:translations     # paridad de los 22 locales
bun run build
```

## Archivos que siempre chocan

- **`src/bindings.ts`** — **no lo resuelvas a mano nunca.** Se genera solo al
  arrancar en modo debug. Si hay conflicto: toma cualquiera de las dos
  versiones, arranca la app en debug y quedará regenerado y correcto.
- **Los 22 `src/i18n/locales/*/translation.json`** — auto-fusionan bien
  mientras cada rama agregue claves distintas. Mantén siempre la paridad entre
  los 22 (lo verifica `check:translations`).

## Convenciones de commit

Commits atómicos y convencionales: `feat:` · `fix:` · `chore:` · `docs:`.
Un commit = un cambio con sentido propio.

## Identidad en el historial

Este repo será público. Para que el historial no arrastre datos personales de
nadie, cada quien configura su identidad **antes de su primer commit**:

```bash
# En GitHub: Settings → Emails → "Keep my email addresses private"
git config user.name "<nombre o alias sin apellido>"
git config user.email "<id>+<usuario>@users.noreply.github.com"
```

Y en los mensajes de commit: nada de nombres completos, correos personales,
direcciones ni datos identificatorios. Aplica a todo el equipo por igual.

## Secretos

- **Ningún secreto entra al repo.** El `.gitignore` ya bloquea los directorios
  de secretos, pero la responsabilidad es de quien commitea.
- Las **claves de firma de releases** se distribuyen lo mínimo posible y jamás
  se copian sueltas entre máquinas: van por gestor de secretos o como secreto
  de CI.

## Documentos de trabajo locales

Si mantienes notas o documentos de trabajo en tu copia local que **no** deben
entrar al repo, agrégalos a `.git/info/exclude`.

> ⚠️ **`.git/info/exclude` no viaja con el clon.** Al clonar en una máquina
> nueva hay que volver a aplicarlo. Si trabajas con documentos locales, este es
> el primer paso después de clonar — antes del primer `git add`.

## Permisos

Los roles granulares (`admin`, `maintain`, `triage`) **solo existen en repos de
organización**. En un repo de cuenta personal, los colaboradores solo pueden
tener `write`: alcanza para ramas, commits, push y PRs, pero no para settings,
protección de ramas ni visibilidad del repo.
