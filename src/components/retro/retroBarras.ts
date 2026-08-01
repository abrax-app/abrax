/**
 * retroBarras — "NIVEL DE ENTRADA" del skin ABRAX (canvas 2D): barras de
 * espectro (izquierda) + dos columnas VU L/R (derecha). Se alimenta de las
 * bandas R8 reales (solo mientras se dicta); en reposo, base baja sintética.
 * Color vía --esfera-stop-* (cian/violeta abrax; oro/rojo imperial).
 */

export type BarrasHandle = {
  push(bands: number[]): void;
  setGrabando(v: boolean): void;
  setPalette(): void;
  destroy(): void;
};

function tok(root: HTMLElement, t: string, fb: string): string {
  const v = getComputedStyle(root).getPropertyValue(t).trim();
  return v || fb;
}

export function montarBarras(cv: HTMLCanvasElement): BarrasHandle {
  const ctx = cv.getContext("2d")!;
  const W = cv.width;
  const H = cv.height;
  const reducido = matchMedia("(prefers-reduced-motion: reduce)").matches;

  const NB = 13; // barras del espectro
  const specW = Math.floor(W * 0.62);
  const alturas = new Array(NB).fill(2);
  const objetivo = new Array(NB).fill(2);
  let col = readCol();
  let grabando = false;
  let vivo = true;
  let ultimo = 0;
  let vu = 0; // nivel VU global 0..1

  function readCol() {
    const r = document.documentElement;
    return {
      a: tok(r, "--esfera-stop-a", "#2fd9ff"),
      b: tok(r, "--esfera-stop-b", "#8b5cf6"),
      c: tok(r, "--esfera-stop-c", "#f23dc4"),
    };
  }

  function frame(now: number) {
    if (!vivo) return;
    const fresco = now - ultimo < 240;
    ctx.clearRect(0, 0, W, H);

    // ── barras de espectro ──
    const step = specW / NB;
    const bw = Math.max(2, step - 3);
    for (let i = 0; i < NB; i++) {
      if (!fresco) {
        const base = grabando ? 7 : 2;
        objetivo[i] =
          base +
          (reducido
            ? 0
            : Math.abs(Math.sin(now / 480 + i * 0.7)) * (grabando ? 9 : 2));
      }
      alturas[i] += (objetivo[i] - alturas[i]) * (reducido ? 1 : 0.32);
      const x = i * step + (step - bw) / 2;
      const h = Math.min(H - 2, Math.max(2, alturas[i]));
      const g = ctx.createLinearGradient(0, H, 0, H - h);
      g.addColorStop(0, col.a);
      g.addColorStop(1, col.b);
      ctx.fillStyle = g;
      for (let y = 0; y < h; y += 3) ctx.fillRect(x, H - 2 - y, bw, 2);
    }

    // ── VU L/R (dos columnas de segmentos) ──
    const objetivoVu = fresco
      ? Math.min(1, alturas.reduce((s, v) => s + v, 0) / (NB * (H * 0.7)))
      : grabando
        ? 0.5
        : 0.08;
    vu += (objetivoVu - vu) * 0.3;
    const cols = [
      { x: specW + (W - specW) * 0.28, v: vu },
      {
        x: specW + (W - specW) * 0.62,
        v: vu * (0.8 + 0.2 * Math.abs(Math.sin(now / 300))),
      },
    ];
    const segN = 12;
    const segH = (H - 4) / segN;
    const cw = 9;
    for (const c of cols) {
      for (let s = 0; s < segN; s++) {
        const on = s / segN < c.v;
        const frac = s / segN;
        const color = frac > 0.82 ? col.c : frac > 0.6 ? "#ffb43a" : col.a;
        ctx.fillStyle = on ? color : "rgba(255,255,255,0.06)";
        const y = H - 2 - (s + 1) * segH + 1;
        ctx.fillRect(c.x - cw / 2, y, cw, segH - 1.5);
      }
    }

    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  return {
    push: (bands) => {
      ultimo = performance.now();
      const n = bands.length || NB;
      for (let i = 0; i < NB; i++) {
        const lo = Math.floor((i * n) / NB);
        const hi = Math.max(lo + 1, Math.floor(((i + 1) * n) / NB));
        let s = 0;
        for (let j = lo; j < hi; j++) s += bands[j] ?? 0;
        objetivo[i] = 2 + (s / (hi - lo)) * (H - 6);
      }
    },
    setGrabando: (v) => {
      grabando = v;
    },
    setPalette: () => {
      col = readCol();
    },
    destroy: () => {
      vivo = false;
    },
  };
}
