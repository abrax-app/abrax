/**
 * esferaHome — motor de la esfera del home orbital (canvas 2D, liviano).
 *
 * Puerto fiel del prototipo `assets/prototipos/abrax_mockup_app_v6.html`: una
 * vista polar de puntos que respira, un anillo orbital con física de "sector
 * dominante" (los puntos migran hacia el nodo enfocado) y un núcleo que late.
 * Al abrir una sección la esfera se abre como iris (los puntos migran al borde
 * y el interior queda libre para el contenido).
 *
 * Reglas del proyecto respetadas aquí:
 *  - Es una esfera DISTINTA de la del overlay (WebGL, R8): esta es canvas 2D y
 *    NO se suscribe a audio; en reposo solo respira. El estado `grabando` lo
 *    empuja el shell desde el estado real de dictado (sin abrir una segunda
 *    captura de micrófono ni emitir espectro 24/7).
 *  - No hardcodea color: lee la paleta activa (`--esfera-*`) por getComputedStyle,
 *    igual que el motor del overlay. Cambiar `ui_theme` la retiñe vía setPalette.
 *  - Respeta `prefers-reduced-motion`: congela el shimmer y el iris salta sin
 *    animar.
 */

export type EsferaHandle = {
  /** Estado de dictado real: sube la energía y cambia el núcleo. */
  setGrabando(v: boolean): void;
  /** true = iris abierto (los puntos migran al borde). */
  setApertura(v: boolean): void;
  /** Ángulo del sector enfocado (grados desde arriba) o null. */
  setSector(deg: number | null): void;
  /** Re-lee los tokens de paleta y re-tiñe todo (al cambiar `ui_theme`). */
  setPalette(): void;
  destroy(): void;
};

type RGB = [number, number, number];

export type OrbitalPalette = {
  c1: RGB; // stop-a (frío / oro)
  c2: RGB; // stop-b (cálido / ámbar)
  c3: RGB; // stop-c (anillo interior / rojo)
  blanco: RGB; // base clara de picos y núcleo en reposo
  rec: string; // relleno del núcleo al grabar
  coreGlow: string; // halo del núcleo en reposo
  recGlow: string; // halo del núcleo al grabar
};

const FALLBACK: OrbitalPalette = {
  c1: [51, 191, 255],
  c2: [115, 38, 217],
  c3: [255, 64, 204],
  blanco: [255, 247, 252],
  rec: "#ffffff",
  coreGlow: "rgba(200,240,255,0.85)",
  recGlow: "rgba(255,120,200,0.9)",
};

/** Parsea `#rrggbb`/`#rgb`/`rgb(...)` a RGB; null si no reconoce. */
function parseColor(raw: string): RGB | null {
  const s = raw.trim();
  if (!s) return null;
  if (s[0] === "#") {
    let hex = s.slice(1);
    if (hex.length === 3) {
      hex = hex[0] + hex[0] + hex[1] + hex[1] + hex[2] + hex[2];
    }
    if (hex.length < 6) return null;
    const n = parseInt(hex.slice(0, 6), 16);
    if (Number.isNaN(n)) return null;
    return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
  }
  const m = s.match(/rgba?\(([^)]+)\)/);
  if (m) {
    const parts = m[1].split(",").map((v) => parseFloat(v));
    if (parts.length >= 3) return [parts[0], parts[1], parts[2]];
  }
  return null;
}

const rgba = (c: RGB, a: number) => `rgba(${c[0]},${c[1]},${c[2]},${a})`;

/** Lee la paleta orbital desde los tokens CSS de la raíz (o de un root dado). */
export function readOrbitalPalette(
  root: HTMLElement = document.documentElement,
): OrbitalPalette {
  const cs = getComputedStyle(root);
  const get = (token: string, fb: RGB): RGB =>
    parseColor(cs.getPropertyValue(token)) ?? fb;
  const c1 = get("--esfera-stop-a", FALLBACK.c1);
  const c2 = get("--esfera-stop-b", FALLBACK.c2);
  const c3 = get("--esfera-stop-c", FALLBACK.c3);
  const blanco = get("--esfera-blanco", FALLBACK.blanco);
  const rec = parseColor(cs.getPropertyValue("--esfera-nucleo-grabando"));
  const halo = get("--esfera-halo", [255, 120, 200]);
  const glow = get("--esfera-glow", [200, 240, 255]);
  return {
    c1,
    c2,
    c3,
    blanco,
    rec: rec ? rgba(rec, 1) : FALLBACK.rec,
    coreGlow: rgba(glow, 0.85),
    recGlow: rgba(halo, 0.9),
  };
}

const mix = (a: RGB, b: RGB, k: number): RGB => [
  Math.round(a[0] + (b[0] - a[0]) * k),
  Math.round(a[1] + (b[1] - a[1]) * k),
  Math.round(a[2] + (b[2] - a[2]) * k),
];

/**
 * Color del degradado diagonal por ángulo (cian arriba-derecha → magenta
 * abajo-izquierda), leído de la paleta. Exportado para que los nodos del menú
 * usen exactamente la misma fórmula que el anillo, y todo retiñe junto.
 */
export function colorDeg(deg: number, pal: OrbitalPalette): string {
  const t = (Math.sin(((deg - 45) * Math.PI) / 180) + 1) / 2;
  const [a, b, k] =
    t > 0.5
      ? ([pal.c2, pal.c1, (t - 0.5) * 2] as const)
      : ([pal.c3, pal.c2, t * 2] as const);
  const c = mix(a, b, k);
  return `rgb(${c[0]},${c[1]},${c[2]})`;
}

const SZ = 740;
const C = SZ / 2;
const RMAX = 252; // radio de la esfera
const RAP = 210; // radio del iris abierto
const R_ORB = 300; // radio del anillo orbital
const N_ORB = 176;

type Punto = { rr: number; r: number; a: number; fase: number; col: string };
type OrbPunto = { ang: number; f: number; fase: number; col: string };

const angDist = (a: number, b: number) => {
  const d = Math.abs(a - b) % 360;
  return d > 180 ? 360 - d : d;
};

export function montarEsfera(cv: HTMLCanvasElement): EsferaHandle {
  const ctx = cv.getContext("2d")!;
  cv.width = SZ;
  cv.height = SZ;
  const reducido = matchMedia("(prefers-reduced-motion: reduce)").matches;

  let pal = readOrbitalPalette();
  let grabando = false;
  let aper = 0;
  let aperObj = 0;
  let sector: number | null = null;
  let vivo = true;

  const colorPunto = (rr: number, a: number): string => {
    const x = Math.cos(a);
    const y = Math.sin(a);
    const t = Math.min(1, Math.max(0, (x - y) * 0.5 + 0.5));
    let c = mix(pal.c3, t > 0.5 ? pal.c1 : pal.c2, Math.abs(t - 0.5) * 2 * 0.9);
    c = mix(pal.blanco, c, Math.min(1, rr * 4.2));
    return `rgb(${c[0]},${c[1]},${c[2]})`;
  };

  // Semilla de la esfera (anillos concéntricos de puntos).
  const puntos: Punto[] = [];
  const NR = 30;
  for (let i = 1; i <= NR; i++) {
    const rr = i / NR;
    const r = 10 + rr * (RMAX - 10);
    const n = Math.max(9, Math.floor((2 * Math.PI * r) / 5.6));
    for (let j = 0; j < n; j++) {
      const a = (j / n) * 2 * Math.PI;
      puntos.push({
        rr,
        r,
        a,
        fase: Math.random() * 6.28,
        col: colorPunto(rr, a),
      });
    }
  }

  // Semilla del anillo orbital.
  const orbPts: OrbPunto[] = [];
  for (let i = 0; i < N_ORB; i++) {
    const ang = (i * 360) / N_ORB;
    orbPts.push({
      ang,
      f: 0,
      fase: Math.random() * 6.28,
      col: colorDeg(-90 + ang, pal),
    });
  }

  const reColor = () => {
    for (const p of puntos) p.col = colorPunto(p.rr, p.a);
    for (const p of orbPts) p.col = colorDeg(-90 + p.ang, pal);
  };

  const t0 = performance.now();
  function frame(now: number) {
    if (!vivo) return;
    const t = (now - t0) / 1000;
    aper += (aperObj - aper) * (reducido ? 1 : 0.09);
    ctx.clearRect(0, 0, SZ, SZ);
    const boost = grabando ? 1 : 0.34;

    // Esfera.
    for (const p of puntos) {
      const ramp = Math.pow(Math.max(0, p.rr - 0.22) / 0.78, 1.3);
      const ondas =
        Math.sin(p.a * 5 + t * 0.9 + p.rr * 22) * 0.5 +
        Math.sin(p.a * 9 - t * 0.6 + p.rr * 37) * 0.3 +
        Math.sin(p.a * 16 + t * 1.3) * 0.2;
      let dom = 0;
      if (grabando) {
        const d = ((p.a * 57.29578 - ((t * 40) % 360) + 540) % 360) - 180;
        dom = Math.exp(-(d * d) / 1156);
      }
      const disp =
        (ondas * 9 * ramp + dom * 12) * boost +
        (reducido ? 0 : Math.sin(t * 1.2 + p.fase) * 0.6);
      const rIris = RAP + (p.r * (RMAX - RAP)) / RMAX;
      const R = p.r + (rIris - p.r) * aper + disp;
      const x = C + R * Math.cos(p.a);
      const y = C + R * Math.sin(p.a);
      const luz =
        0.42 +
        Math.max(0, ondas) * 0.4 * ramp * boost +
        dom * 0.6 +
        (p.rr < 0.12 ? 0.5 : 0);
      ctx.globalAlpha = Math.min(1, luz) * (1 - aper * 0.15);
      ctx.fillStyle = p.col;
      ctx.fillRect(x - 1, y - 1, 2, 2);
    }

    // Anillo orbital con física de sector dominante.
    const objetivo = sector;
    for (const p of orbPts) {
      const fObj =
        objetivo === null
          ? 0
          : Math.exp(-Math.pow(angDist(p.ang, objetivo) / 20, 2));
      p.f += (fObj - p.f) * (reducido ? 1 : 0.14);
      const shimmer = reducido
        ? 0
        : Math.sin(t * 1.3 + p.fase) * (grabando ? 1.1 : 0.5);
      const r = R_ORB + p.f * 16 + shimmer;
      const rad = ((p.ang - 90) * Math.PI) / 180;
      const x = C + r * Math.cos(rad);
      const y = C + r * Math.sin(rad);
      let op =
        0.4 + p.f * 0.6 + (reducido ? 0 : 0.08 * Math.sin(t * 1.1 + p.fase));
      if (aper > 0.5) {
        op *= objetivo !== null && angDist(p.ang, objetivo) < 26 ? 1 : 0.6;
      }
      const rad2 = 2 + p.f * 2;
      ctx.globalAlpha = Math.min(1, Math.max(0, op));
      ctx.fillStyle = p.col;
      ctx.beginPath();
      ctx.arc(x, y, rad2, 0, 7);
      ctx.fill();
    }

    // Núcleo (se disuelve al abrirse el iris).
    ctx.globalAlpha = Math.max(0, 1 - aper);
    const rN =
      6.5 + (grabando ? Math.sin(t * 6) * 2.4 : Math.sin(t * 1.4) * 0.8);
    const g = ctx.createRadialGradient(C, C, 0, C, C, 30);
    g.addColorStop(0, grabando ? pal.recGlow : pal.coreGlow);
    g.addColorStop(1, "rgba(0,0,0,0)");
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(C, C, 30, 0, 7);
    ctx.fill();
    ctx.fillStyle = grabando
      ? pal.rec
      : `rgb(${pal.blanco[0]},${pal.blanco[1]},${pal.blanco[2]})`;
    ctx.beginPath();
    ctx.arc(C, C, rN, 0, 7);
    ctx.fill();

    ctx.globalAlpha = 1;
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);

  return {
    setGrabando: (v) => {
      grabando = v;
    },
    setApertura: (v) => {
      aperObj = v ? 1 : 0;
    },
    setSector: (deg) => {
      sector = deg;
    },
    setPalette: () => {
      pal = readOrbitalPalette();
      reColor();
    },
    destroy: () => {
      vivo = false;
    },
  };
}
