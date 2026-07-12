# Ideas aparcadas

## [ESCUCHA] — sesión 12/07/2026 (rama feat/escucha)

- **Re-preprocesar al cambiar verbosidad**: hoy el cambio de
  Natural/Literal aplica al próximo archivo que se cargue; podría
  re-preprocesar el contenido ya abierto al vuelo.
- **Partir líneas de código kilométricas**: una línea minificada de 2000
  caracteres es una sola "oración" — partir en ~300 chars al espacio más
  cercano mantendría el resaltado ágil.
- **Callbacks de fin de locución en vez de polling**: el crate `tts` reporta
  `utterance_callbacks: true` en Windows; suscribirse a `on_utterance_end`
  y emitir un evento Tauri eliminaría el poll de 100 ms. (El poll es honesto
  y multiplataforma; el callback es una optimización.)
- **Leer desde una línea**: clic en una línea del visor → empezar la lectura
  desde la oración que la contiene.
- **Atajo global "leer portapapeles"** (stretch del brief): toca bindings
  compartidos; dejado fuera para mantener la fusión del lunes trivial.
- **Karaoke por palabra en Fase 2 (Piper)**: VITS expone duraciones por
  fonema → resaltado por palabra, no por línea. Ver docs/ESCUCHA_FASE2.md.
- **Tabla de símbolos editable por el usuario**: `TablaSimbolos` ya está
  aislada; exponerla en settings como lista de pares símbolo→palabra.
- **Saltar bloques de código o solo-prosa**: modo "solo léeme la prosa"
  (útil para READMEs largos) y su inverso "solo el código".
