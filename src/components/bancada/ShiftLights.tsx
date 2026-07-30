import React, { forwardRef, useImperativeHandle, useMemo, useRef } from "react";

// Tira de shift-lights (LEDs de cambio de marcha) del shell Bancada, estilo
// volante de F1: verde → ámbar → rojo por zonas. Se actualiza por REFS directas
// al DOM (nunca por estado React) para no re-renderizar por frame; el shell la
// alimenta con rev 0..1 desde su bucle rAF único. SVG/DOM (no WebGL) → captura
// CDP fiel para el video.

const N = 15;

export interface ShiftLightsHandle {
  setRev(rev: number): void;
  /** Barrido de "formación" (lights-out) al arrancar el dictado. */
  flash(): void;
}

function zone(i: number): "v" | "a" | "r" {
  const t = i / (N - 1);
  return t < 0.55 ? "v" : t < 0.82 ? "a" : "r";
}

export const ShiftLights = forwardRef<
  ShiftLightsHandle,
  { className?: string }
>(({ className }, ref) => {
  const wrapRef = useRef<HTMLDivElement>(null);
  const leds = useRef<(HTMLSpanElement | null)[]>([]);
  const reduce = useRef(
    typeof window !== "undefined" &&
      !!window.matchMedia?.("(prefers-reduced-motion: reduce)").matches,
  );

  useImperativeHandle(ref, () => ({
    setRev(rev: number) {
      const r = rev < 0 ? 0 : rev > 1 ? 1 : rev;
      const on = Math.round(r * N);
      for (let i = 0; i < N; i++) {
        const el = leds.current[i];
        if (el) el.classList.toggle("on", i < on);
      }
      wrapRef.current?.classList.toggle("redline", r > 0.85 && !reduce.current);
    },
    flash() {
      const el = wrapRef.current;
      if (!el || reduce.current) return;
      el.classList.remove("formation");
      // reflow para reiniciar la animación
      void el.offsetWidth;
      el.classList.add("formation");
      window.setTimeout(() => el.classList.remove("formation"), 700);
    },
  }));

  const cells = useMemo(() => Array.from({ length: N }, (_, i) => zone(i)), []);

  return (
    <div
      ref={wrapRef}
      className={`bnc-shift${className ? ` ${className}` : ""}`}
      aria-hidden="true"
    >
      {cells.map((z, i) => (
        <span
          key={i}
          ref={(el) => {
            leds.current[i] = el;
          }}
          className={`bnc-led z-${z}`}
          style={{ ["--i" as string]: String(i) }}
        />
      ))}
    </div>
  );
});

ShiftLights.displayName = "ShiftLights";
