# Landing de ABRAX

Sitio estático, sin build ni dependencias de red: three.js va self-hosteado y
las fuentes son la pila monoespaciada del sistema.

## Ver en local

Cualquier servidor estático sobre esta carpeta:

```bash
npx serve landing
# o
python -m http.server 8000 --directory landing
```

Parámetro de desarrollo: `?sin-acto` salta la portada (útil para QA y capturas).

## Piezas

- `index.html` — página completa (Acto I, hero con esfera, flujo, funciones,
  privacidad, demo, compatibilidad, comparación, FAQ, descarga, footer). El
  guion inline fija la receta visual oficial de la esfera vía `ESFERA.tune()`
  y carga three.js + motor **en diferido** (idle) para no bloquear el pintado.
- `js/esfera.js` — motor de la esfera «palabras vivas»: audio-reactiva
  (demo/micrófono/datos externos), palabras que vuelan y se disuelven,
  interior con fundido granulado, ruido orgánico, limbo, anillos en órbita
  opcionales (apagados por defecto) y API `tune()/getTune()`. Respeta
  `prefers-reduced-motion` en todas las capas y se pausa fuera del viewport.
- `js/cuadraditos.js` — transición de revelado (retícula radial con flashes
  del degradado de marca). Canvas 2D puro, sin dependencias.
- `js/esfera-loca.js` + `lab-loco.html` — fork del motor sin límites y su
  laboratorio de experimentos (solo desarrollo).
- `lab-esfera.html` — laboratorio del motor serio con perillas en vivo y el
  preset "Firma (landing)" que reproduce la receta oficial (solo desarrollo).
- `recetas-esfera.md` — recetas visuales guardadas de la esfera.
- `vendor/three.min.js` — three.js r128 minificado tal cual (no formatear:
  `landing/vendor/` está en `.prettierignore`).
- `assets/` — SVG oficiales de marca, capturas reales en WebP, clip MP4 de
  Escucha y `og.jpg` (tarjeta social 1200×630).
- `css/estilos.css` — tokens y componentes.
- `.vercelignore` — excluye del deploy los laboratorios, el fork, las recetas
  y este README.

## Deploy (Vercel) — `abrax.krafify.com`

Sitio 100 % estático: **sin build, sin dependencias, sin variables de entorno**.
Se sirve tal cual. `vercel.json`, `404.html` y `.vercelignore` ya están puestos.

### Configuración del proyecto (una sola vez, en el panel de Vercel)

| Ajuste             | Valor                          |
| ------------------ | ------------------------------ |
| Framework Preset   | **Other**                      |
| **Root Directory** | **`landing`** ← el crítico     |
| Build Command      | vacío (lo anula `vercel.json`) |
| Output Directory   | vacío                          |
| Install Command    | vacío                          |

**Root Directory es el ajuste que hay que acertar**: el repositorio es el de la
app y la landing vive en una subcarpeta. Si se deja en la raíz, Vercel intenta
construir la aplicación Tauri y falla.

`vercel.json` fija `buildCommand: null` (nada que compilar), cabeceras de
seguridad discretas y caché inmutable para `assets/` y `vendor/`. **No lleva
`Permissions-Policy`**: la demo de la esfera pide el micrófono con
`getUserMedia`, y una política mal puesta la mataría en silencio.

### Dominio (pasos en el registrador de `krafify.com`)

1. En Vercel → Project → Settings → Domains → añadir `abrax.krafify.com`.
2. Vercel devuelve el destino del CNAME (normalmente `cname.vercel-dns.com`).
3. En el DNS de `krafify.com`, crear un registro **CNAME**:
   `abrax` → el destino que dio Vercel. TTL bajo (300 s) el primer día.
4. Esperar propagación y confirmar el certificado TLS en el panel de Vercel.

**Si el dominio final NO fuera `abrax.krafify.com`**, hay que cambiar **tres
líneas juntas** en `index.html`: `canonical`, `og:url` y `og:image` — esta
última debe ser **absoluta** o la tarjeta social sale sin imagen.

### Botones de descarga: hay un interruptor

Mientras no exista release publicado, los dos botones **no son enlaces**: son
`<span class="btn btn--prim btn--espera" aria-disabled="true">` con el texto
«el viernes 31», y la única acción viva es «Ver el código». Enlazar a una página
de releases vacía sería un botón muerto: promete descarga y entrega una página
en blanco.

**Para publicar la descarga**, en la sección `#descarga`:

1. Sustituir cada `<span class="btn btn--prim btn--espera" aria-disabled="true">`
   por `<a class="btn btn--prim" href="URL">`, con la **URL directa del
   artefacto** (`.dmg` y `.exe`), no la de la página de releases.
2. Cerrar con `</a>` en lugar de `</span>`.
3. Borrar el párrafo `.cta-espera`.
4. Devolver «Ver el código» a `btn--ghost` (ya lo está) o quitarlo si estorba.

> 🚦 **Antes de publicar, comprobar el gate de firma de `docs/RELEASING.md`**: la
> sección «Antes de instalar» está escrita para binarios SIN FIRMAR. Si se llega
> a firmar **y** notarizar, esa sección se quita entera junto con sus dos
> enlaces `#avisos`.

### Plataformas

Se ofrecen **macOS y Windows**. **Linux queda fuera de esta entrega**: el código
compila, pero no está probado lo suficiente como para ofrecer una descarga. La
tarjeta de compatibilidad lo dice así, en vez de callarlo.

## Pendiente antes de publicar

- [ ] Video demo embebido en la sección "Escúchalo y pruébalo".
- [ ] URLs de release reales (ver «interruptor» arriba).
- [ ] QA en iPad/Safari (AudioContext con gesto, touch, reduced-motion).
- [ ] QA de la intro cinemática en navegador real.
