import { listen } from "@tauri-apps/api/event";
import React, { useEffect, useRef } from "react";
import { EsferaEngine, EsferaState } from "./esfera/engine";

/** Payload del evento `spectrum` (overlay.rs::emit_spectrum). */
type SpectrumPayload = {
  bands: number[];
  rms: number;
  bass: number;
  dominant: number;
};

interface EsferaStageProps {
  state: EsferaState;
  /** Visible → motor corriendo; oculto → rAF detenido y GPU liberada. */
  active: boolean;
}

// Tras el fade-out del overlay (300 ms) se libera el motor por completo:
// "montar al mostrar, destruir al ocultar".
const DISPOSE_AFTER_FADE_MS = 400;

const EsferaStage: React.FC<EsferaStageProps> = ({ state, active }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const engineRef = useRef<EsferaEngine | null>(null);
  const disposeTimerRef = useRef<number | null>(null);

  // Ciclo de vida del motor, gobernado por la visibilidad del overlay.
  useEffect(() => {
    if (active) {
      if (disposeTimerRef.current !== null) {
        window.clearTimeout(disposeTimerRef.current);
        disposeTimerRef.current = null;
      }
      if (!engineRef.current && canvasRef.current) {
        engineRef.current = new EsferaEngine(canvasRef.current);
      }
      engineRef.current?.setState(state);
      engineRef.current?.start();
    } else if (engineRef.current) {
      // El rAF se corta ya (cero trabajo con el overlay oculto); el frame
      // congelado acompaña el fade y después se libera la GPU.
      engineRef.current.stop();
      disposeTimerRef.current = window.setTimeout(() => {
        engineRef.current?.dispose();
        engineRef.current = null;
        disposeTimerRef.current = null;
      }, DISPOSE_AFTER_FADE_MS);
    }
  }, [active, state]);

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

  return <canvas ref={canvasRef} aria-hidden="true" />;
};

export default EsferaStage;
