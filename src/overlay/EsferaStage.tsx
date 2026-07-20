import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef } from "react";
import { events } from "@/bindings";
import { EsferaEngine, EsferaState, readEsferaPalette } from "./esfera/engine";
import type { SpectrumPayload } from "@/lib/types/events";

/** Payload del evento `spectrum` (overlay.rs::emit_spectrum). */
interface EsferaStageProps {
  state: EsferaState;
  /** Visible → motor corriendo; oculto → rAF detenido y GPU liberada. */
  active: boolean;
}

// Tras el fade-out del overlay (300 ms) se libera el motor por completo:
// "montar al mostrar, destruir al ocultar".
const DISPOSE_AFTER_FADE_MS = 400;

// Modo «palabras»: el overlay dosifica la llegada de palabras para que una
// ráfaga (el bloque entero de un modelo por lotes) igual "vaya llegando" de a
// una. 130 ms es rápido para no rezagarse del habla en vivo (streaming) y
// suficiente para escalonar una ráfaga. Tope de cola por seguridad.
const WORD_RELEASE_MS = 130;
const WORD_QUEUE_CAP = 400;

const EsferaStage: React.FC<EsferaStageProps> = ({ state, active }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<EsferaEngine | null>(null);
  const disposeTimerRef = useRef<number | null>(null);
  // Estado más reciente sin volverlo dependencia del efecto de visibilidad:
  // así crear/re-mostrar el motor arranca en la fase correcta, pero un cambio
  // de fase NO re-ejecuta ese efecto (evita reconstruir texturas por fase).
  const stateRef = useRef(state);
  stateRef.current = state;
  // Palabras transcritas pendientes de inyectar a la esfera (modo «palabras»).
  const wordQueueRef = useRef<string[]>([]);

  // Ciclo de vida del motor, gobernado por la visibilidad del overlay.
  useEffect(() => {
    if (active) {
      if (disposeTimerRef.current !== null) {
        window.clearTimeout(disposeTimerRef.current);
        disposeTimerRef.current = null;
      }
      if (!engineRef.current && canvasRef.current) {
        // El motor lee la paleta activa (tokens CSS) al nacer.
        engineRef.current = new EsferaEngine(canvasRef.current);
      } else {
        // Motor reciclado (re-show antes del dispose diferido): la paleta
        // pudo cambiar mientras estaba oculto — re-leer los tokens.
        engineRef.current?.setPalette(readEsferaPalette());
      }
      // Dictado nuevo: la esfera arranca sin palabras de la sesión anterior.
      wordQueueRef.current = [];
      engineRef.current?.vaciarPalabras();
      engineRef.current?.setState(stateRef.current);
      engineRef.current?.start();
    } else if (engineRef.current) {
      // El rAF se corta ya (cero trabajo con el overlay oculto); el frame
      // congelado acompaña el fade y después se libera la GPU.
      wordQueueRef.current = [];
      engineRef.current.stop();
      disposeTimerRef.current = window.setTimeout(() => {
        engineRef.current?.dispose();
        engineRef.current = null;
        disposeTimerRef.current = null;
      }, DISPOSE_AFTER_FADE_MS);
    }
  }, [active]);

  // Cambios de fase: solo re-tintan el núcleo, sin tocar la geometría/texturas.
  useEffect(() => {
    engineRef.current?.setState(state);
  }, [state]);

  // Espectro del backend → esfera. El listener vive con el componente; la
  // suscripción backend (start/stop_spectrum) la maneja RecordingOverlay.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    listen<SpectrumPayload>("spectrum", (event) => {
      const { bands, rms, bass, dominant } = event.payload;
      engineRef.current?.setAudioData(bands, rms, bass, dominant);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
      if (disposeTimerRef.current !== null) {
        window.clearTimeout(disposeTimerRef.current);
      }
      engineRef.current?.dispose();
      engineRef.current = null;
    };
  }, []);

  // Palabras transcritas → esfera (modo «palabras»). El backend solo emite este
  // evento cuando el modo está activo, así que en modo «audio» la cola queda
  // vacía y no pasa nada. Un intervalo dosifica la inyección.
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | null = null;
    events.transcriptWordsEvent
      .listen((event) => {
        const words = event.payload.words;
        if (!words?.length) return;
        const q = wordQueueRef.current;
        q.push(...words);
        // Tope de seguridad: descarta las más viejas si se desborda.
        if (q.length > WORD_QUEUE_CAP) q.splice(0, q.length - WORD_QUEUE_CAP);
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });

    const release = window.setInterval(() => {
      const q = wordQueueRef.current;
      if (!q.length) return;
      const engine = engineRef.current;
      if (!engine) {
        // Overlay oculto: no acumular palabras para el próximo dictado.
        q.length = 0;
        return;
      }
      engine.agregarPalabra(q.shift()!);
    }, WORD_RELEASE_MS);

    return () => {
      cancelled = true;
      unlisten?.();
      window.clearInterval(release);
    };
  }, []);

  return <canvas ref={canvasRef} aria-hidden="true" />;
};

export default EsferaStage;
