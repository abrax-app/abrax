/**
 * retroSpectrum — analizador de espectro del shell retro (canvas 2D).
 *
 * Reutiliza el MISMO stream R8 de 32 bandas del overlay (no inventa otro): el
 * shell se suscribe con `start_spectrum` solo mientras se dicta y alimenta este
 * motor con las bandas reales. En reposo (sin dictado) las barras decaen a una
 * línea base sintética baja — sin audio, sin escucha ambiental (privacidad).
 *
 * Lee la paleta activa de los tokens `--esfera-stop-*`, así retiñe con
 * `ui_theme` (Imperial = espectro ardiendo en oro→rojo) sin hardcodear color.
 */

export type EspectroHandle = {
  /** Bandas reales (0..1) del evento `spectrum`; marca el frame como "vivo". */
  push(bands: number[]): void;
  /** Estado de dictado: en reposo las barras decaen a la base sintética. */
  setGrabando(v: boolean): void;
  /** Re-lee los tokens de paleta (al cambiar `ui_theme`). */
  setPalette(): void;
  destroy(): void;
};

const NB = 19; // barras visibles (las 32 bandas R8 se agrupan a 19)

function tokenColor(root: HTMLElement, token: string, fb: string): string {
  const v = getComputedStyle(root).getPropertyValue(token).trim();
  return v || fb;
}

export function montarEspectro(cv: HTMLCanvasElement): EspectroHandle {
  const ctx = cv.getContext("2d")!;
  const W = cv.width;
  const H = cv.height;
  const reducido = matchMedia("(prefers-reduced-motion: reduce)").matches;

  let col = readCols();
  let grabando = false;
  let vivo = true;
  let ultimoFrame = 0; // timestamp del último push real
  const alturas = new Array(NB).fill(2);
  const objetivo = new Array(NB).fill(2);
  const picos = new Array(NB).fill(2);

  function readCols() {
    const r = document.documentElement;
    return {
      a: tokenColor(r, "--esfera-stop-a", "#2fd9ff"),
      b: tokenColor(r, "--esfera-stop-b", "#8b5cf6"),
      c: tokenColor(r, "--esfera-stop-c", "#f23dc4"),
    };
  }

  function frame(now: number) {
    if (!vivo) return;
    const fresco = now - ultimoFrame < 250; // llegaron bandas hace poco
    ctx.clearRect(0, 0, W, H);
    const step = W / NB;
    const barW = Math.max(3, step - 4);
    for (let i = 0; i < NB; i++) {
      // Sin frames reales frescos: base sintética baja (reposo, sin audio).
      if (!fresco) {
        const base = grabando ? 8 : 2;
        objetivo[i] =
          base +
          (reducido
            ? 0
            : Math.abs(Math.sin(now / 500 + i * 0.7)) * (grabando ? 10 : 3));
      }
      alturas[i] += (objetivo[i] - alturas[i]) * (reducido ? 1 : 0.35);
      picos[i] = Math.max(picos[i] - 0.6, alturas[i]);
      const x = i * step + (step - barW) / 2;
      const h = Math.min(H - 2, Math.max(2, alturas[i]));
      const g = ctx.createLinearGradient(0, H, 0, H - h);
      g.addColorStop(0, col.a);
      g.addColorStop(0.7, col.b);
      g.addColorStop(1, col.c);
      ctx.fillStyle = g;
      // Segmentos tipo LED (barras discontinuas).
      for (let y = 0; y < h; y += 4) ctx.fillRect(x, H - 2 - y, barW, 3);
      // Pico que cae.
      ctx.fillStyle = col.c;
      const py = Math.min(H - 2, Math.max(2, picos[i]));
      ctx.fillRect(x, H - 2 - py, barW, 2);
    }
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  return {
    push: (bands) => {
      ultimoFrame = performance.now();
      // Agrupar 32 bandas → NB barras (media por bucket).
      const n = bands.length || NB;
      for (let i = 0; i < NB; i++) {
        const lo = Math.floor((i * n) / NB);
        const hi = Math.max(lo + 1, Math.floor(((i + 1) * n) / NB));
        let s = 0;
        for (let j = lo; j < hi; j++) s += bands[j] ?? 0;
        const avg = s / (hi - lo);
        objetivo[i] = 2 + avg * (H - 6);
      }
    },
    setGrabando: (v) => {
      grabando = v;
    },
    setPalette: () => {
      col = readCols();
    },
    destroy: () => {
      vivo = false;
    },
  };
}
