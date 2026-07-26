// Medidor HUD moderno (canvas-2D) del shell Bancada. Anillo fino con gradiente
// que se LLENA con la energía de la voz (rev 0..1) y brilla; en el centro, un
// número GRANDE con la métrica real: palabras por minuto. Plano y limpio (nada
// de aguja/ticks skeuomórficos). Sin rAF propio: lo llama el bucle del shell.
// Canvas-2D a propósito (captura CDP fiel). Colores desde tokens --bnc-*.

export interface TacoHandle {
  render(rev: number, readout: number): void;
  setUnit(unit: string): void;
  resize(): void;
  setPalette(): void;
  destroy(): void;
}

const clamp01 = (v: number) => (v < 0 ? 0 : v > 1 ? 1 : v);

// arco de 270° abierto abajo
const A0 = Math.PI * 0.75;
const SWEEP = Math.PI * 1.5;

export function montarTacometro(canvas: HTMLCanvasElement): TacoHandle {
  const ctx = canvas.getContext("2d");
  if (!ctx) {
    return {
      render: () => {},
      setUnit: () => {},
      resize: () => {},
      setPalette: () => {},
      destroy: () => {},
    };
  }
  const g = ctx;
  let dpr = 1;
  let w = 0;
  let h = 0;
  let lastRev = 0;
  let lastReadout = 0;
  let unit = "PAL/MIN";
  const col = {
    accent: "#ee3f2c",
    accent2: "#ff9a3d",
    text: "#f4f5f7",
    dim: "#8a8b92",
    track: "rgba(255,255,255,0.08)",
  };

  const setPalette = () => {
    const cs = getComputedStyle(canvas);
    const get = (v: string, d: string) => cs.getPropertyValue(v).trim() || d;
    col.accent = get("--bnc-accent", col.accent);
    col.accent2 = get("--bnc-accent2", col.accent2);
    col.text = get("--bnc-text", col.text);
    col.dim = get("--bnc-dim", col.dim);
    col.track = get("--bnc-track", col.track);
  };

  const setUnit = (u: string) => {
    unit = u || unit;
    render(lastRev, lastReadout);
  };

  function render(rev: number, readout: number) {
    lastRev = rev;
    lastReadout = readout;
    if (w === 0 || h === 0) return;
    const cx = w / 2;
    const cy = h / 2;
    const lw = Math.max(6, Math.min(w, h) * 0.075);
    const R = Math.min(w, h) / 2 - lw / 2 - 2;
    g.clearRect(0, 0, w, h);
    g.lineCap = "round";

    // track (fondo del anillo)
    g.lineWidth = lw;
    g.strokeStyle = col.track;
    g.beginPath();
    g.arc(cx, cy, R, A0, A0 + SWEEP);
    g.stroke();

    // progreso (se llena con la voz) con gradiente cálido + glow
    const p = clamp01(rev);
    if (p > 0.001) {
      const grad = g.createLinearGradient(cx - R, cy, cx + R, cy);
      grad.addColorStop(0, col.accent);
      grad.addColorStop(1, col.accent2);
      g.strokeStyle = grad;
      g.shadowColor = col.accent;
      g.shadowBlur = lw * 1.1;
      g.beginPath();
      g.arc(cx, cy, R, A0, A0 + SWEEP * p);
      g.stroke();
      g.shadowBlur = 0;
    }

    // número grande (métrica real)
    const val = Math.max(0, Math.round(readout));
    g.fillStyle = col.text;
    g.textAlign = "center";
    g.font = `700 ${Math.round(R * 0.62)}px ui-sans-serif, system-ui, -apple-system, "Segoe UI", sans-serif`;
    g.textBaseline = "alphabetic";
    g.fillText(String(val), cx, cy + R * 0.14);

    // etiqueta de unidad
    g.fillStyle = col.dim;
    g.font = `600 ${Math.round(R * 0.17)}px ui-sans-serif, system-ui, sans-serif`;
    g.textBaseline = "top";
    g.fillText(unit, cx, cy + R * 0.28);
  }

  const resize = () => {
    const r = canvas.getBoundingClientRect();
    dpr = Math.max(1, window.devicePixelRatio || 1);
    w = Math.max(1, Math.round(r.width));
    h = Math.max(1, Math.round(r.height));
    canvas.width = Math.round(w * dpr);
    canvas.height = Math.round(h * dpr);
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    render(lastRev, lastReadout);
  };

  setPalette();
  resize();
  return { render, setUnit, resize, setPalette, destroy: () => {} };
}
