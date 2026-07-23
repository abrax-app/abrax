# Changelog del rebranding — ABRAX

Estado consolidado al **2026-07-12**. Toda afirmación de este documento está respaldada por el historial de git de este repositorio.

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

| Hallazgo                                                                                                   | Commit                | Cómo se verificó                                                                       |
| ---------------------------------------------------------------------------------------------------------- | --------------------- | -------------------------------------------------------------------------------------- |
| **R1 (P1)** — el dictado podía quedar colgado de forma permanente si el micrófono dejaba de entregar audio | `0e4108f`             | Tests unitarios nuevos del recorder; suite Rust completa en verde                      |
| **S7 (P1)** — un build del fork habría aceptado actualizaciones del proyecto original                      | `e02c6a9`             | Configuración sin endpoint ni clave del upstream; arranque real verificado             |
| **S3 (P2)** — el texto dictado quedaba escrito en el log por defecto                                       | `37c9fc2`             | El log por defecto registra solo la longitud; el contenido solo aparece en nivel trace |
| **R5 (P2)** — dictar con método portapapeles destruía el contenido no-texto copiado                        | `c7b1df3`             | Restaura texto e imágenes del portapapeles                                             |
| **R6/R7 (P2)** — clientes HTTP (LLM y descargas de modelos) sin timeout                                    | `4315030`             | Timeouts de conexión y lectura configurados; build OK                                  |
| **F2 (P2)** — fuga de listeners de eventos en el overlay de grabación                                      | `2e7a4bc`             | Cleanup registrado en el efecto; lint y build OK                                       |
| **S11 (P3)** — encabezados HTTP anunciaban la marca del upstream                                           | `e02c6a9`             | Búsqueda de referencias en el árbol de código                                          |
| **F23 (P3)** — enlaces de About/actualizador apuntaban al proyecto original                                | `e02c6a9` + `a948b2c` | Búsqueda de URLs; atribución al proyecto original conservada en About (22 idiomas)     |
| **F14 (P3)** — versión de respaldo falsa ("0.1.2") si fallaba la lectura de versión                        | `3a1e1c8`             | Fallback vacío; sin datos inventados                                                   |
| **R13 (P3)** — comandos IPC que fallarían al ser invocados                                                 | `c661c45`             | Eliminados junto con C1 (no tenían llamadores)                                         |
| Marca visible remanente (título de ventana, tooltip de bandeja, CLI, icono interno)                        | `2a2e6a3`             | Barrido de la palabra en todo el árbol + arranque real                                 |

## 4. Funciones nuevas o liberadas

- **Filtro de muletillas personalizable, ahora con interfaz** (`34c3715`). El motor de transcripción ya sabía filtrar muletillas con lista personalizada (`custom_filler_words`, con tests), pero la opción no tenía UI ni comando: solo se podía editar a mano el archivo de configuración. Ahora vive en Ajustes → Avanzado con tres estados siempre visibles (lista por defecto del idioma / lista propia / filtro desactivado), chips editables, soporte de frases multi-palabra ("o sea") y un **preset es-419** de 10 muletillas frecuentes que se combina sin duplicar ni borrar entradas del usuario. Textos en los 22 idiomas. Incluye test nuevo de frases multi-palabra; suite Rust 120/120.
- **Esfera de audio en el overlay** (`e78ded4`). Estilo de overlay nuevo «Esfera»: visualización 3D reactiva a 32 bandas de espectro (70–8000 Hz) bajo modelo de suscripción — con cero suscriptores no se computa ni emite nada. Respeta `prefers-reduced-motion`, degrada densidad ante carga y jamás roba el foco. three.js local (sin red).
- **Errores visibles** (`cc5ecf3`, `e1be4f0`). Canal único de alertas con registro reciente, toast localizado y **notificación nativa** cuando la ventana está oculta; banner «Errores recientes» al reabrir; estados de error con causa y Reintentar en modelos, onboarding e historial (F1, F3, F4, F7).
- **Diccionario Vivo por proyecto** (`63da2be`, `6d6ef19`). ABRAX aprende los términos de tu repositorio (identificadores, ramas, archivos; respeta .gitignore) y corrige el dictado hacia ellos; reemplazos exactos definidos por el usuario; todo local (el índice vive en tu disco).
- **Compilador de Prompts, modo plantilla** (`c6a4ef8`). Estructura el dictado crudo en CONTEXTO/TAREA/REQUISITOS/FORMATO con linter de ambigüedades es-419 — sin ningún LLM, instantáneo y offline; botón varita en el Historial.
- **Paletas de color: ABRAX e Imperial** (`87da2de`). Selector «Paleta» en Acerca de, independiente del modo claro/oscuro: **ABRAX** (cian/violeta/magenta de marca) o **Imperial** (oro/ámbar/rojo sobre tinta cálida). Imperial es un tema oscuro por diseño, así que fuerza el modo oscuro mientras está activo y conserva tu preferencia de claro/oscuro para cuando vuelvas a ABRAX. Toda la interfaz sigue la paleta activa vía tokens de tema —botones, toggles, sliders, enlaces, foco, el lockup (que pasa a variante monocroma en Imperial) y **la esfera del overlay**, que late en los colores de la paleta con su núcleo de grabación teñido—. La preferencia se persiste y se aplica al instante, sin reiniciar. Textos en los 22 idiomas (los nombres de paleta no se traducen).
- **Escucha — ABRAX lee tu código** (rama `feat/escucha`, **fusionada el 12/07** en `b6427e6`). Sección nueva: lectura en voz alta con TTS del sistema (crate `tts`, MIT — cero descargas, 100% local), preprocesador markdown/código es-419 (símbolos verbalizados, identificadores partidos, encabezados anunciados), voces y velocidades independientes para prosa y código, resaltado de la línea en lectura, «Leer portapapeles» y Esc para detener. La pausa opera al final de la oración en curso (documentado en el propio panel, 22 idiomas). 19 tests nuevos; diseño de voz neural en `docs/ESCUCHA_FASE2.md` (solo documento).
- **Formas de ventana: Clásico, Orbital y Retro** (`1b3b898`, `60bb55e`). Un eje nuevo, independiente del tema de color, para elegir la **forma** de la ventana desde Acerca de. **Clásico** es la ventana de ajustes de siempre (default y respaldo). **Orbital** convierte la app en una esfera flotante sin marco: el menú son nodos en órbita y, al abrir una sección, la esfera se abre como iris y muestra dentro esa misma sección — sin ventanas ni duplicación. **Retro** es un skin inspirado en los reproductores clásicos (cromo biselado, ventanas apilables y arrastrables, LEDs), con reloj de dictado, analizador de espectro, historial como lista de reproducción y un menú clásico que navega a las secciones reales. Las tres formas leen la paleta activa por tokens, así que **Orbital e Imperial** laten en oro/rojo y **Retro e Imperial** visten cromo negro-oro. La esfera del home reacciona al dictado real sin escuchar en reposo (respeta el modelo de suscripción del espectro). El cambio de forma se aplica al instante; el marco transparente de la ventana termina de aplicarse al reiniciar. Textos en los 22 idiomas.

- **Sonidos de marca** (rama `feat/sonidos-abrax`). Tema de sonido «Abrax» nuevo y por defecto: dos gestos cortos con identidad de orbe —quinta ascendente (La4→Mi5) al empezar a dictar, quinta descendente al terminar—, timbre vítreo de parciales casi-armónicos sintetizado desde cero (sin samples de terceros). Generador reproducible en `scripts/gen_sonidos_abrax.ts`; Marimba, Pop y Custom siguen disponibles en el selector.
- **Memoria de correcciones** (rama `feat/memoria-correcciones`). ABRAX aprende de las ediciones del usuario: al corregir una transcripción en el Historial (lápiz nuevo por entrada), un diff por tokens detecta las sustituciones («ábrax» → «Abrax», «use auth store» → `useAuthStore`) y las guarda como pares que los dictados siguientes aplican como **reemplazo exacto por frase** antes de las capas difusas. Puertas de seguridad: no aprende inserciones/borrados puros, ni frases largas, ni palabras comunes es/en (stoplist), ni reescrituras sin parecido (Levenshtein); enseñar lo contrario retira el par viejo (sin ping-pong) y hay tope de 200 pares. Sección propia en Ajustes → Avanzado (junto a las muletillas): toggle, lista `de → a` con contador, olvidar individual y total. Todo local; textos en los 22 idiomas; 18 tests nuevos (suite 312/312).
- **Dictado con micrófonos de nivel bajo** (rama `fix/dictado-nivel-bajo`). Dos capas contra el caso «en Discord sí me escuchan, ABRAX no me transcribe»: (1) **AGC de una pasada** antes del STT — las apps de llamadas compensan el nivel con su propio control de ganancia y ABRAX recibía la señal cruda (RMS ≤ -34 dBFS hace degenerar a Whisper); solo actúa bajo la zona sana del veredicto del micrófono (-28 dBFS), levanta hacia -24 dBFS con tope +18 dB y limitado por pico. (2) **Guardia anti-repetición**: la salida degenerada del motor (bucles «qqqq…» con audio casi inaudible) se recorta antes de llegar al editor — rachas de un mismo carácter >10 se eliminan y palabras repetidas >4 veces seguidas se colapsan; «jajaja», «1111» y «sí sí sí» quedan intactos. 8 tests nuevos (suite 294/294).
- **Carpeta de modelos, fase C: mudanza** (rama `feat/disco-mudanza`). Al cambiar la carpeta de modelos (elección de disco, PR #18), la app ahora ofrece **mover lo ya descargado** —transcripción y Pulido— con barra de progreso, o dejar solo las descargas nuevas en el destino. Mudanza segura por archivo: renombrar si es el mismo volumen; si no, copiar → verificar tamaño → recién borrar el origen, así una interrupción nunca pierde nada (los restos completos se aprovechan y los incompletos se recopian). Chequeo previo de espacio libre cuando el destino es otro volumen y candado contra mudanzas simultáneas. Evento tipado `mudanza-progreso`; textos en los 22 idiomas; 3 tests Rust nuevos (suite 286/286).

## 5. Limpieza

- **10 comandos IPC muertos eliminados** (`c661c45` + `971c3fd`): −117 líneas Rust y −83 líneas de bindings generados. Superficie IPC: 107 → 97 comandos (98 hoy, con el comando nuevo del filtro de muletillas).
- **CLI interna sin target de build** (369 líneas) y dependencia `rdev` sin usos, eliminadas (`3a1e1c8`).
- **5 componentes React muertos** (~272 líneas, uno con datos falsos hardcodeados) y un `console.log` residual, eliminados (`3a1e1c8`).
- **Componentes de logo del upstream** eliminados tras el reemplazo (−115 líneas; `a948b2c`, `2a2e6a3`).
- **Iconos móviles (Android/iOS) del upstream retirados** (`a948b2c`): no se compila para móvil y se regeneran desde el kit propio si llegaran a necesitarse.
- Warning de compilación `unused_assignments` eliminado (`3a1e1c8`); normalización de fin de línea con `.gitattributes` (`380595d`).
- **Tail de higiene** (`74bbea7`): clippy a cero (12 avisos), evento interno sin oyentes `settings-changed` eliminado (C2, 7 emisores), color hardcodeado del reproductor de audio migrado al token de marca (F22), `baseUrl` deprecado fuera de tsconfig, y el workflow de CI ya no depende de una variable de entorno implícita para la identidad de firma.

## 6. Pendiente declarado

- Fuentes de descarga de modelos: **11 de 16 migradas a Hugging Face** con URL anclada por commit y sha256 verificado por descarga real (los 3 Whisper GGML en el repo oficial de ggerganov; el resto en mirrors byte-idénticos). Las 5 restantes (Moonshine base/tiny, GigaAM, Canary ×2) no existen fuera del hosting del upstream: se migran a un espejo propio (archivos ya verificados y preservados localmente) — fecha objetivo 22/07.
- Adaptación de los workflows de CI para builds propios (falta el build macOS sin firma e instrucciones Gatekeeper).
- Rebranding del README y documentación de privacidad (PRIVACY.md, incluida la nota del índice local del Diccionario).
- Suite al día de hoy: **161 tests en verde (+1 smoke de audio que se corre aparte)** · clippy sin avisos · 22 idiomas en paridad.

## 7. Atribución

ABRAX es un fork de **Handy**, creado por **CJ Pais** y publicado bajo licencia **MIT**. Gran parte del mérito de que esta app exista —el pipeline de dictado, el soporte multiplataforma, la base de i18n— es del proyecto original y de sus contribuidores. Conservamos `LICENSE`, `ATTRIBUTION.md` y `UPSTREAM.md`, y el About de la app reconoce explícitamente al proyecto original. ABRAX no está afiliado a Handy ni a su autor.
