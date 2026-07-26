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
  interior con fundido granulado, ruido orgánico, limbo, anillos orbitales
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

## Deploy (Vercel)

Proyecto estático con esta carpeta como root; `404.html` y `.vercelignore` ya
están en su lugar.

## Pendiente antes de publicar

- [ ] Video demo embebido en la sección "Escúchalo y pruébalo".
- [ ] URLs de release reales + `SHA256SUMS.txt` publicado junto al binario.
- [ ] Dominio definitivo: convertir `og:image` en URL absoluta y añadir
      `canonical` + `og:url` (los scrapers de OG exigen URL absoluta).
- [ ] QA en iPad/Safari (AudioContext con gesto, touch, reduced-motion).
