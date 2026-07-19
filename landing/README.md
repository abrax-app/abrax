# Landing de ABRAX

Sitio estático, sin build. Estructura y copy según `abrax-kit/kit/PROMPT_MAESTRO_V2.md`
FASE 8 (secciones) y FASE 7 (esfera); diseño base generado con Claude Design a partir del
design system "ABRAX — Design System" (claude.ai/design) e integrado aquí con el código real.

## Ver en local

Cualquier servidor estático sobre esta carpeta:

```bash
npx serve landing
# o
python -m http.server 8000 --directory landing
```

Abrir directamente `index.html` con doble clic también funciona (no usa módulos ES
ni fetch), pero un servidor reproduce mejor el entorno de Vercel.

## Piezas

- `index.html` — página completa (hero, flujo, funciones, privacidad, demo,
  compatibilidad, comparación, FAQ, descarga, footer).
- `js/esfera.js` — esfera «palabras vivas» adaptada del prototipo
  `assets/prototipos/esfera_con_palabras.html`: tamaño de contenedor, poda de
  chispas (el prototipo las filtraba para siempre), colores de marca exactos,
  pausa fuera del viewport, feed y micrófono controlados por API. El micrófono
  jamás se pide al cargar: solo con el botón de la sección demo.
- `js/cuadraditos.js` — transición de revelado portada de `web/index.html`
  (retícula radial con flashes del degradado de marca). Corre una vez por
  sesión; con `prefers-reduced-motion` se sustituye por un fundido.
- `vendor/three.min.js` — three.js r128 self-hosteado (misma versión que el
  prototipo; sin CDN por privacidad y por CSP).
- `assets/` — SVG oficiales del kit de marca.
- `css/estilos.css` — tokens y componentes.

## Deploy (Vercel)

Proyecto estático con esta carpeta como root. `404.html` ya está en su lugar.

## Pendiente (se llena antes del 25/07)

- [x] Capturas reales integradas (WebP en `assets/capturas/`, convertidas con ffmpeg
      desde `abrax-kit/capturas-maraton/`) + clip real de Escucha (mp4 116 KB, sin audio).
- [ ] Video demo ≤ 2:00 embebido (producción 27–29/07, sección demo).
- [ ] URLs de release reales + `SHA256SUMS.txt` (bloqueado por distribución).
- [ ] `og:image` como PNG 1200×630 (hoy apunta al SVG del ícono).
- [ ] Ratificar dominio (`abrax.krafify.com` según decisión 12/07 vs `abrax.app`
      del CLAUDE.md) y añadir `canonical`.
- [ ] QA en iPad Pro/Safari (AudioContext con gesto, touch, reduced-motion).
