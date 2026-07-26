# Brief para Claude Design — Landing de ABRAX

> Pega este documento completo como contexto en claude.ai/design antes de pedir la landing.
> La identidad de ABRAX **ya existe y está aprobada** — no inventes una dirección visual nueva:
> aplica esta al pie de la letra.

## Qué es ABRAX

Puesto de voz local para programar en español: hablas y escribe, y te lee de vuelta código y
prosa. Atajo global → dictas → el texto se inserta en la app activa. Gratis, open source,
local por defecto, español (es-419) de primera clase.

Posicionamiento: no es "para desarrolladores" — es **para los que le hablan a la IA todo el día**.
Frase estrella del compilador de prompts: _"Otros formatean. Esto compila."_
Cierre aprobado: _"Gratis. Open source. Local. Para siempre."_

## Identidad visual (innegociable)

**Tokens:**

```css
--cian: #2fd9ff; /* CIAN SEÑAL — acento */
--violeta: #8b5cf6; /* VIOLETA ABRAX — primario UI */
--magenta: #f23dc4; /* MAGENTA VIVO — fin del degradado */
--tinta: #05060f; /* fondo oscuro de página */
--panel: #0b0d1c; /* paneles/cards */
--texto: #c9d4e0; /* texto base */
--suave: #6b7688; /* texto secundario */
```

**Degradado de marca** (firma visual): `linear-gradient(225deg, #2FD9FF 0%, #8B5CF6 55%, #F23DC4 100%)`
— diagonal desde arriba-derecha (cian) hacia abajo-izquierda (magenta).
**Solo vive en el isotipo y en la X del wordmark.** Nunca en todo el wordmark, nunca en fondos
enteros, nunca en bloques de texto. Las letras A-B-R-A van en blanco/currentColor.

**Tipografía:** 100% monoespaciada, sin excepciones ni webfonts:
`ui-monospace, "Cascadia Code", "JetBrains Mono", Menlo, Consolas, monospace`.
Escalas de referencia: eyebrow 11px / tracking .28em / uppercase · títulos de sección 13px /
tracking .24em / uppercase con prefijo `◈ ` en violeta · lema 15px / tracking .34em.

**Lema (composición exacta, guillemets incluidos):** `« DI LA PALABRA. »` con **PALABRA** en cian.
Subtítulo opcional: _Del arameo «avra k'davra» — "creo mientras hablo". Nosotros lo hicimos software._

**Reglas duras del logo:** espacio de respeto de 4 radios del núcleo alrededor del lockup; no
estirar, no rotar, no cambiar el orden del degradado. Sobre fondos claros o de un solo color:
versión monocroma o wordmark todo blanco/tinta.

## El hero: la esfera es la protagonista

El hero lo domina una **esfera 3D de partículas en vivo** (WebGL, ~120k puntos) que respira y
hace volar palabras dictadas hacia su superficie. **Claude Design no puede renderizarla: deja un
contenedor reservado** (60–70% del alto del hero, centrado o levemente descentrado) y represéntala
como placeholder: orbe luminoso cian→magenta con núcleo blanco, sobre fondo casi negro (#05060F)
con nebulosa tenue cian/magenta y estrellas/bokeh sutil. El canvas real lo integra Claude Code
después.

En el hero además: wordmark + lema, propuesta de valor de una frase, CTA de descarga (primario),
badges de compatibilidad (macOS · Windows · Linux), "gratuito y open source", y una línea de
privacidad honesta.

También se integrará después una **transición de entrada con cuadrícula de cuadraditos** (canvas,
~2 s, cortina que se disuelve en onda radial con flashes del degradado). Diseña el hero para que
funcione como destino de esa revelación.

## Estructura de la página (orden obligatorio)

1. **Hero** — como arriba.
2. **Flujo** — 4 pasos: Presiona → Habla → Transcribe → Compila (o pega).
3. **Funciones** — Dictado local (Whisper/Parakeet, filtro de muletillas es-419) · Escucha (TTS
   con voz de prosa y voz de código) · Diccionario Vivo (indexa tu repo: «use auth store» →
   `useAuthStore`) · Compilador de prompts (CONTEXTO/TAREA/REQUISITOS/FORMATO, sin LLM) ·
   2 formas de ventana (Clásico/Quiet) · 2 paletas (ABRAX/IMPERIAL).
4. **Privacidad** — qué es local, qué es opcional online (opt-in), qué no se recopila, cómo borrar.
5. **Demo interactiva de audio** — placeholder; el micrófono NUNCA se pide automáticamente.
6. **Compatibilidad** — macOS/Windows/Linux con limitaciones honestas.
7. **Comparación** — sin atacar a rivales.
8. **FAQ**.
9. **CTA final** — descarga con checksums.
10. **Footer** — licencia MIT, atribución a Handy ("No implica afiliación ni respaldo"), enlace
    al código, "un producto de Krafify".

## Reglas de copy (críticas)

- **Nunca "100% local" a secas.** Redacción segura: _local por defecto_ o _"100% local, con
  opción online opt-in"_. Lo literalmente cierto: el audio del micrófono nunca sale del equipo.
- Español es-419, directo, técnico, sobrio. **Cero superlativos de marketing.**
- Honestidad radical: limitaciones con la misma prominencia que las virtudes.
- **Sin nombres personales ni ciudad** (nada de "Winston + Antonio" ni "La Serena") — eso es solo
  del material interno.
- Copy terminado, sin lorem ipsum.

## Anti-patrones (no hacer)

- Plantilla SaaS genérica: hero centrado con 3 tarjetas idénticas, glassmorphism, degradados
  púrpura sobre blanco.
- Fotos de stock de gente, testimonios inventados, screenshots falsos — solo habrá capturas
  reales de la app y un video demo real.
- Titulares vagos ("Bienvenido al futuro"), "Enviar" como CTA, navegación con muchos enlaces.
- Inventar otra identidad: nada de Vibe Discovery — el vibe ya se llama ABRAX.

## Requisitos técnicos

Responsive mobile-first (QA en 1440×900, 1280×800, 1024×768 y 390×844), `prefers-reduced-motion`
respetado, navegable por teclado, SEO + Open Graph + favicon (SVG del isotipo) + 404, carga < 2 s,
analítica desactivada o respetuosa. Deploy: Vercel.

## Assets reales que existen (no inventar sustitutos)

- 6 SVG oficiales del logo (lockup, isotipo, wordmark, app-icon, favicon, tray).
- 38 capturas/clips reales de la app (shells, esfera, Diccionario Vivo, Escucha, tema IMPERIAL).
- Video demo ≤ 2:00 (se produce el 27–29/07, embebido en la landing).
- La esfera y la transición de cuadraditos: código real que integra Claude Code tras el diseño.
