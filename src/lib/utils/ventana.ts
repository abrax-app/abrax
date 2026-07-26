import type { Window } from "@tauri-apps/api/window";
import { type as osType } from "@tauri-apps/plugin-os";

/**
 * Cierre de la ventana desde la X que pinta cada shell sin marco (Quiet, Retro,
 * Bancada).
 *
 * El shell Clásico usa la X nativa del marco, que dispara `CloseRequested` y por
 * tanto pasa por la decisión del backend. Los shells sin marco pintan su propia X
 * en React, así que tienen que enrutarse al MISMO punto a mano: `close()` emite
 * `CloseRequested` (a diferencia de `destroy()`, que no emite nada), y allí el
 * backend decide si ocultar —cuando hay bandeja— o salir de Abrax —cuando no la
 * hay, porque entonces ocultarse dejaría la app viva y sin ninguna superficie
 * desde la que volver ni salir.
 *
 * Antes las tres llamaban a `hide()` sin condición, que es justo lo que producía
 * ese estado. El botón dedicado de «enviar a la bandeja» NO usa esto: ese sigue
 * ocultando, y solo se muestra si hay bandeja.
 *
 * macOS queda fuera a propósito y conserva `hide()`: allí cerrar con la bandeja
 * apagada mantiene el icono del Dock y `RunEvent::Reopen` devuelve la ventana, así
 * que ya existe vía de vuelta y el comportamiento actual es el idiomático del
 * sistema. Es la misma exclusión que aplica el backend con `#[cfg]`.
 */
export function cerrarDesdeShell(win: Window): void {
  if (osType() === "macos") {
    void win.hide();
    return;
  }
  // Si falla (p. ej. faltara el permiso `core:window:allow-close`), la X quedaría
  // muerta en silencio: se registra para que se note en el log en vez de parecer
  // que el botón no existe.
  void win.close().catch((e) => {
    console.error("No se pudo cerrar la ventana:", e);
  });
}
