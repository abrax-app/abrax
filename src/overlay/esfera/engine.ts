/*
 * ESFERA «flor cósmica» — motor de render del overlay de grabación.
 *
 * Port del prototipo aprobado `assets/prototipos/esfera_v2_elegante.html`
 * (dirección de arte del 11/07, BLK-002): mismos shaders, misma geometría de
 * anillos concéntricos, mismo suavizado de audio. Se retira todo lo que no
 * pertenece a un overlay: chrome de UI, modos demo/micrófono (el espectro
 * llega del backend por el evento `spectrum`), fondo de nebulosas/estrellas
 * (el overlay es transparente) y arrastre con puntero (la ventana no roba
 * foco). Ciclo de vida explícito: `start`/`stop` controlan el rAF y
 * `dispose` libera geometría, materiales, texturas y contexto GL.
 *
 * prefers-reduced-motion (F11 del overlay): sin rAF continuo — un render
 * estático por cambio de estado, con el núcleo tintado según el estado.
 */
import * as THREE from "three";

export type EsferaState = "recording" | "transcribing" | "processing";

const BANDS = 32;

// Presupuesto de puntos para un escenario de ~220 px (el prototipo a pantalla
// completa usa 25k–120k; el overlay luce denso con mucho menos). El watchdog
// de FPS degrada densidad, nunca fluidez.
const LEVELS = { alta: 24000, media: 12000, baja: 6000 } as const;
type Level = keyof typeof LEVELS;
const LEVEL_ORDER: Level[] = ["alta", "media", "baja"];
const DPR_MAX = 1.75;

/**
 * Paleta de la esfera, leída de los tokens CSS de la raíz (theme.css):
 * stops del degradado de la membrana, núcleos por estado y colores del
 * escenario. La paleta activa (`data-ui-theme`) redefine los tokens y la
 * esfera se re-tiñe sin tocar el motor — cualquier shell (clásico, orbital,
 * retro) reutiliza esta inyección tal cual. Los respaldos son los valores
 * del prototipo v2 (paleta ABRAX).
 *
 * Cada color es un entero 0xRRGGBB en sRGB crudo: el shader consume los
 * componentes tal cual (como los vec3 literales del prototipo), sin pasar
 * por la gestión de color de three.
 */
export interface EsferaPalette {
  /** Exterior frío del degradado (ABRAX: cian). */
  stopA: number;
  /** Exterior cálido del degradado (ABRAX: violeta). */
  stopB: number;
  /** Anillo interior (ABRAX: magenta). */
  stopC: number;
  /** Base clara de picos y transición al núcleo. */
  base: number;
  /** Núcleo al grabar (late con la voz). */
  coreRecording: number;
  /** Núcleo al transcribir. */
  coreTranscribing: number;
  /** Núcleo al post-procesar. */
  coreProcessing: number;
  /** Halo suave alrededor del núcleo. */
  halo: number;
  /** Brillo central. */
  glow: number;
  /** Núcleo opaco que bloquea los puntos traseros. */
  fondo: number;
}

const PALETTE_TOKENS: Record<keyof EsferaPalette, [string, number]> = {
  stopA: ["--esfera-stop-a", 0x33bfff],
  stopB: ["--esfera-stop-b", 0x7326d9],
  stopC: ["--esfera-stop-c", 0xff40cc],
  base: ["--esfera-blanco", 0xfff7fc],
  coreRecording: ["--esfera-nucleo-grabando", 0xffffff],
  coreTranscribing: ["--esfera-nucleo-transcribiendo", 0x2fd9ff],
  coreProcessing: ["--esfera-nucleo-procesando", 0x8b5cf6],
  halo: ["--esfera-halo", 0xff78c8],
  glow: ["--esfera-glow", 0xc8f0ff],
  fondo: ["--esfera-fondo", 0x05060f],
};

/** `#rgb`/`#rrggbb` → entero 0xRRGGBB; null si el valor no es un hex. */
function parseHexColor(value: string): number | null {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(value.trim());
  if (!m) return null;
  const h =
    m[1].length === 3
      ? m[1]
          .split("")
          .map((c) => c + c)
          .join("")
      : m[1];
  return parseInt(h, 16);
}

/** Lee la paleta activa desde los tokens CSS computados de la raíz. */
export function readEsferaPalette(
  root: HTMLElement = document.documentElement,
): EsferaPalette {
  const cs = getComputedStyle(root);
  const palette = {} as EsferaPalette;
  for (const key of Object.keys(PALETTE_TOKENS) as (keyof EsferaPalette)[]) {
    const [token, fallback] = PALETTE_TOKENS[key];
    palette[key] = parseHexColor(cs.getPropertyValue(token)) ?? fallback;
  }
  return palette;
}

/** Componentes sRGB crudos [0,1] para los uniforms del shader. */
function hexToVec3(hex: number): THREE.Vector3 {
  return new THREE.Vector3(
    ((hex >> 16) & 255) / 255,
    ((hex >> 8) & 255) / 255,
    (hex & 255) / 255,
  );
}

// Si el backend deja de emitir (silencio VAD, fin de la grabación), la esfera
// decae a su respiración serena en vez de congelarse en el último espectro.
const DATA_STALE_MS = 350;

const VERT = `
#define PI 3.14159265
uniform float uTime;
uniform float uRMS;
uniform float uBass;
uniform float uSens;
uniform float uReduced;
uniform float uScale;
uniform float uSize;
uniform float uPushAngle;
uniform float uPushRing;
uniform float uPushAmt;
uniform float uBands[32];
attribute float aRing;
attribute float aAngle;
attribute float aRand;
varying float vEnergy;
varying float vWarm;
varying float vShade;
varying float vRR;
varying vec3  vWorld;

float bandAt(float r){
  float f = clamp(r, 0.0, 0.999) * 31.0;
  int i = int(floor(f));
  int j = i + 1; if (j > 31) j = 31;
  return mix(uBands[i], uBands[j], fract(f));
}

void main(){
  vec3 dir = normalize(position);
  float m = 1.0 - uReduced*0.85;                 // atenuador de movimiento reducido
  // 0 = centro visible, 1 = borde (ecuador). El hemisferio trasero espeja.
  float rr = aRing < 0.5 ? aRing*2.0 : (1.0-aRing)*2.0;
  float e  = bandAt(rr);

  // ── volado elegante: centro sereno, ondas que crecen hacia el borde ──
  float ramp = pow(smoothstep(0.22, 0.85, rr), 1.4);
  float ph = aRing*22.0;
  float ondas = sin(aAngle*5.0  + uTime*0.50 + ph)         * 0.48
              + sin(aAngle*9.0  - uTime*0.34 + ph*1.7+1.3) * 0.30
              + sin(aAngle*16.0 + uTime*0.72 - ph*0.6)     * 0.16
              + sin(aAngle*27.0 - uTime*0.90 + aRand*6.28) * 0.06;
  float ruffle = ramp * ondas * (0.085 + 0.10*uRMS*uSens) * m;

  // respiración sutil + ondas concéntricas del núcleo
  float breath = sin(uTime*0.55 + aRing*PI*2.0 + aRand*3.0)*0.006*m + 0.002;
  float centro = smoothstep(0.16, 0.0, rr);
  float ripple = centro * sin(rr*34.0 - uTime*2.2) * 0.010 * m;

  // audio: expansión por banda (más en el borde) + latido de graves
  float bandPush = e * (0.035 + 0.075*uSens) * (0.35 + ramp*0.65);
  float bassPush = uBass * 0.05 * uSens;

  // abultamiento de la frecuencia dominante: la membrana empujada desde dentro
  float da = abs(atan(sin(aAngle - uPushAngle), cos(aAngle - uPushAngle))) / PI;
  float dr = abs(rr - uPushRing);
  float bulge = uPushAmt * exp(-da*da*10.0 - dr*dr*16.0) * (0.4 + ramp*0.6);

  // TODO desplazamiento es radial: la esfera jamás se rompe en tentáculos
  float disp = breath + ripple + ruffle + bandPush + bassPush + bulge*0.16*uSens;
  disp = clamp(disp, -0.10, 0.30);
  vec3 pos = dir * (1.0 + disp);

  vShade  = clamp(0.55 + ruffle*4.2 + bulge*0.6, 0.18, 1.30); // crestas brillan, valles se apagan
  vEnergy = clamp(e*1.0 + bulge*1.3 + uRMS*0.22, 0.0, 1.5);
  vWarm   = clamp(bulge*1.5, 0.0, 1.0);
  vRR     = rr;
  vWorld  = (modelMatrix * vec4(pos, 1.0)).xyz;

  vec4 mv = modelViewMatrix * vec4(pos, 1.0);
  gl_Position = projectionMatrix * mv;
  float ps = uSize * (0.85 + vEnergy*0.6) * uScale / max(-mv.z, 0.1);
  gl_PointSize = clamp(ps, 1.0, 5.0);
}
`;

const FRAG = `
precision mediump float;
uniform vec3 uStopA;
uniform vec3 uStopB;
uniform vec3 uStopC;
uniform vec3 uBase;
varying float vEnergy;
varying float vWarm;
varying float vShade;
varying float vRR;
varying vec3  vWorld;
void main(){
  vec2 uv = gl_PointCoord*2.0 - 1.0;
  float d = dot(uv, uv);
  if (d > 1.0) discard;
  float alpha = exp(-d*3.2) * 0.9;

  // núcleo claro caliente → anillo interior → exterior B (abajo-izq) / A (arriba-der)
  // (paleta ABRAX: blanco → magenta → violeta/cian; los stops llegan por uniform)
  float t = clamp((vWorld.x + vWorld.y)*0.30 + 0.5, 0.0, 1.0);
  vec3 exterior = mix(uStopB, uStopA, t);
  vec3 col = mix(uStopC, exterior, smoothstep(0.18, 0.70, vRR));
  col = mix(uBase, col, smoothstep(0.02, 0.16, vRR));
  col *= vShade;                                              // sombreado del volado (tela)
  col = mix(col, vec3(1.0, 0.62, 0.42), vWarm*0.5);           // tinte cálido en la dominante
  col = mix(col, vec3(1.0), clamp(vEnergy*vEnergy*0.45, 0.0, 0.75)); // blanco en picos
  gl_FragColor = vec4(col * (0.42 + vEnergy*0.70), alpha);
}
`;

/** Textura radial suave para los sprites del núcleo (idéntica al prototipo). */
function radialTexture(hex: number, aCenter: number) {
  const r = (hex >> 16) & 255;
  const g = (hex >> 8) & 255;
  const b = hex & 255;
  const s = 256;
  const cv = document.createElement("canvas");
  cv.width = cv.height = s;
  const ctx = cv.getContext("2d")!;
  const gr = ctx.createRadialGradient(s / 2, s / 2, 0, s / 2, s / 2, s / 2);
  gr.addColorStop(0.0, `rgba(${r},${g},${b},${aCenter})`);
  gr.addColorStop(0.35, `rgba(${r},${g},${b},${aCenter * 0.35})`);
  gr.addColorStop(1.0, `rgba(${r},${g},${b},0)`);
  ctx.fillStyle = gr;
  ctx.fillRect(0, 0, s, s);
  return new THREE.CanvasTexture(cv);
}

/*
 * Anillos concéntricos (latitudes) con el polo hacia la cámara.
 * aRing = φ/π (0 = polo frontal, 0.5 = ecuador/borde visible, 1 = polo trasero).
 * Leve desplazamiento angular por anillo → espiral suave.
 */
function buildGeometry(total: number) {
  const rings = Math.max(90, Math.round(Math.sqrt(total) * 0.62));
  const phiMax = Math.PI * 0.97;
  const weights: number[] = [];
  let sum = 0;
  for (let i = 0; i < rings; i++) {
    const phi = ((i + 0.5) / rings) * phiMax;
    const w = Math.sin(phi) + 0.03;
    weights.push(w);
    sum += w;
  }
  const pos: number[] = [];
  const aR: number[] = [];
  const aA: number[] = [];
  const aRd: number[] = [];
  for (let i = 0; i < rings; i++) {
    const phi = ((i + 0.5) / rings) * phiMax;
    const n = Math.max(3, Math.round((total * weights[i]) / sum));
    const spiral = i * 0.37;
    const s = Math.sin(phi);
    const c = Math.cos(phi);
    for (let j = 0; j < n; j++) {
      const th = (j / n) * Math.PI * 2 + spiral + (Math.random() - 0.5) * 0.014;
      pos.push(s * Math.cos(th), s * Math.sin(th), c);
      aR.push(phi / Math.PI);
      aA.push(th);
      aRd.push(Math.random());
    }
  }
  const g = new THREE.BufferGeometry();
  g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
  g.setAttribute("aRing", new THREE.Float32BufferAttribute(aR, 1));
  g.setAttribute("aAngle", new THREE.Float32BufferAttribute(aA, 1));
  g.setAttribute("aRand", new THREE.Float32BufferAttribute(aRd, 1));
  return g;
}

function ease(current: number, target: number, up: number, down: number) {
  return target > current
    ? current + (target - current) * up
    : current + (target - current) * down;
}

export class EsferaEngine {
  private renderer: THREE.WebGLRenderer;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private holder = new THREE.Group();
  private spinner = new THREE.Group();
  private points: THREE.Points | null = null;
  private material: THREE.ShaderMaterial;
  private darkCore: THREE.Mesh;
  private coreGlow: THREE.Sprite;
  private pinkHalo: THREE.Sprite;
  private textures: THREE.Texture[] = [];

  private clock = new THREE.Clock();
  private raf = 0;
  private running = false;
  private disposed = false;
  private readonly reduced: boolean;
  private level: Level;
  private state: EsferaState = "recording";

  // Estado de audio suavizado (mismas constantes que el prototipo).
  private bands = new Float32Array(BANDS);
  private rms = 0;
  private bass = 0;
  private dom = BANDS * 0.35;
  private pushAmt = 0;
  private pushAngle = -0.6;
  private lastDataAt = 0;

  // Watchdog de FPS: degrada densidad de puntos, no fluidez.
  private framesThisSecond = 0;
  private lastSecondAt = 0;
  private lowSeconds = 0;

  private resizeObserver: ResizeObserver | null = null;

  private uniforms = {
    uTime: { value: 0 },
    uRMS: { value: 0 },
    uBass: { value: 0 },
    uSens: { value: 1 },
    uReduced: { value: 0 },
    uScale: { value: 300 },
    // El prototipo usa 0.0032 a pantalla completa; en un escenario de ~220 px
    // el punto proyectado cae bajo el clamp mínimo — se compensa aquí.
    uSize: { value: 0.01 },
    uPushAngle: { value: -0.6 },
    uPushRing: { value: 0.4 },
    uPushAmt: { value: 0 },
    uBands: { value: new Float32Array(BANDS) },
    uStopA: { value: new THREE.Vector3() },
    uStopB: { value: new THREE.Vector3() },
    uStopC: { value: new THREE.Vector3() },
    uBase: { value: new THREE.Vector3() },
  };

  private palette: EsferaPalette;

  constructor(
    private canvas: HTMLCanvasElement,
    palette: EsferaPalette = readEsferaPalette(),
  ) {
    this.palette = palette;
    this.reduced = window.matchMedia(
      "(prefers-reduced-motion: reduce)",
    ).matches;
    this.uniforms.uReduced.value = this.reduced ? 1 : 0;
    this.level = this.reduced ? "baja" : "alta";

    this.renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: false,
      alpha: true, // ventana de overlay transparente: el fondo lo pone el CSS
      powerPreference: "high-performance",
    });
    this.renderer.setClearColor(0x000000, 0);

    this.camera = new THREE.PerspectiveCamera(42, 1, 0.1, 60);
    // Más lejos que el prototipo (2.9): la esfera completa, con su
    // desplazamiento máximo (+0.30), debe caber en el escenario pequeño.
    this.camera.position.set(0, 0, 3.7);

    this.holder.add(this.spinner);
    this.scene.add(this.holder);

    // Núcleo oscuro y opaco: bloquea los puntos traseros para que los anillos
    // frontales se lean.
    this.darkCore = new THREE.Mesh(
      new THREE.SphereGeometry(0.86, 48, 48),
      new THREE.MeshBasicMaterial({ color: palette.fondo }),
    );
    this.spinner.add(this.darkCore);

    // Punto brillante central (late con la voz) — el núcleo de estado.
    const glowTex = radialTexture(palette.glow, 0.9);
    this.textures.push(glowTex);
    this.coreGlow = new THREE.Sprite(
      new THREE.SpriteMaterial({
        map: glowTex,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        transparent: true,
      }),
    );
    this.coreGlow.position.set(0, 0, 1.0);
    this.coreGlow.scale.setScalar(0.34);
    this.spinner.add(this.coreGlow);

    // Halo suave alrededor del núcleo (rosado en ABRAX; sigue a la paleta).
    const haloTex = radialTexture(palette.halo, 0.45);
    this.textures.push(haloTex);
    this.pinkHalo = new THREE.Sprite(
      new THREE.SpriteMaterial({
        map: haloTex,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        transparent: true,
      }),
    );
    this.pinkHalo.position.set(0, 0, 0.95);
    this.pinkHalo.scale.setScalar(1.25);
    this.spinner.add(this.pinkHalo);

    this.applyPaletteUniforms();
    // Tinte inicial del núcleo: setState() retorna temprano si el estado no
    // cambia, así que el estado de partida se tiñe aquí (el material por
    // defecto es blanco, que solo coincide con la paleta ABRAX).
    (this.coreGlow.material as THREE.SpriteMaterial).color.setHex(
      this.coreTint(this.state),
    );

    this.material = new THREE.ShaderMaterial({
      uniforms: this.uniforms,
      vertexShader: VERT,
      fragmentShader: FRAG,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      depthTest: true,
      transparent: true,
    });

    this.rebuild(this.level);
    this.resize();

    // Keep the bitmap in sync with the CSS box (mount can race the card's
    // layout, and the DPR can change when the overlay moves between monitors).
    this.resizeObserver = new ResizeObserver(() => {
      this.resize();
      if (this.reduced) this.renderOnce();
    });
    this.resizeObserver.observe(canvas);
  }

  /** Reconstruye la nube de puntos con el presupuesto del nivel dado. */
  private rebuild(level: Level) {
    this.level = level;
    if (this.points) {
      this.spinner.remove(this.points);
      this.points.geometry.dispose();
    }
    this.points = new THREE.Points(buildGeometry(LEVELS[level]), this.material);
    this.spinner.add(this.points);
  }

  private resize() {
    const w = this.canvas.clientWidth || 216;
    const h = this.canvas.clientHeight || 196;
    const dpr = Math.min(window.devicePixelRatio || 1, DPR_MAX);
    this.renderer.setPixelRatio(dpr);
    this.renderer.setSize(w, h, false);
    this.camera.aspect = w / h;
    this.camera.updateProjectionMatrix();
    this.uniforms.uScale.value =
      (h * dpr) / (2 * Math.tan((this.camera.fov * Math.PI) / 360));
  }

  /**
   * Datos externos del backend (evento `spectrum`): acepta N bandas y
   * re-muestrea a 32, con los mismos suavizados subida/bajada del prototipo.
   */
  setAudioData(bands: number[], rms?: number, bass?: number, domIdx?: number) {
    if (this.disposed || !bands || !bands.length) return;
    const n = bands.length;
    for (let k = 0; k < BANDS; k++) {
      const f = (k / (BANDS - 1)) * (n - 1);
      const i = Math.floor(f);
      const j = Math.min(n - 1, i + 1);
      const v = Math.max(
        0,
        Math.min(1, bands[i] + (bands[j] - bands[i]) * (f - i)),
      );
      this.bands[k] = ease(this.bands[k], v, 0.55, 0.12);
    }
    const rmsV =
      typeof rms === "number"
        ? rms
        : this.bands.reduce((a, b) => a + b, 0) / BANDS;
    this.rms = ease(this.rms, Math.min(1, rmsV), 0.4, 0.08);
    this.bass =
      typeof bass === "number"
        ? bass
        : (this.bands[0] + this.bands[1] + this.bands[2] + this.bands[3]) / 4;
    if (typeof domIdx === "number") {
      this.dom += ((domIdx / (n - 1)) * (BANDS - 1) - this.dom) * 0.1;
    }
    this.lastDataAt = performance.now();
    // Con movimiento reducido no hay rAF: la esfera es estática por diseño.
  }

  /** Tinte del núcleo para una fase del dictado, según la paleta activa. */
  private coreTint(state: EsferaState): number {
    switch (state) {
      case "recording":
        return this.palette.coreRecording;
      case "transcribing":
        return this.palette.coreTranscribing;
      case "processing":
        return this.palette.coreProcessing;
    }
  }

  /** Vuelca los stops de la paleta a los uniforms del shader. */
  private applyPaletteUniforms() {
    this.uniforms.uStopA.value.copy(hexToVec3(this.palette.stopA));
    this.uniforms.uStopB.value.copy(hexToVec3(this.palette.stopB));
    this.uniforms.uStopC.value.copy(hexToVec3(this.palette.stopC));
    this.uniforms.uBase.value.copy(hexToVec3(this.palette.base));
  }

  /**
   * Re-tiñe la esfera en caliente (membrana, núcleos, halo y escenario).
   * El overlay monta un motor nuevo por sesión de dictado, así que la vía
   * normal es el constructor; esta entrada queda lista para shells que
   * cambien de paleta con el motor vivo (orbital/retro).
   */
  setPalette(palette: EsferaPalette) {
    if (this.disposed) return;
    this.palette = palette;
    this.applyPaletteUniforms();
    (this.darkCore.material as THREE.MeshBasicMaterial).color.setHex(
      palette.fondo,
    );
    const glowMat = this.coreGlow.material as THREE.SpriteMaterial;
    const haloMat = this.pinkHalo.material as THREE.SpriteMaterial;
    for (const [mat, hex, alpha] of [
      [glowMat, palette.glow, 0.9],
      [haloMat, palette.halo, 0.45],
    ] as const) {
      const old = mat.map;
      const tex = radialTexture(hex, alpha);
      this.textures.push(tex);
      mat.map = tex;
      mat.needsUpdate = true;
      if (old) {
        const idx = this.textures.indexOf(old);
        if (idx !== -1) this.textures.splice(idx, 1);
        old.dispose();
      }
    }
    glowMat.color.setHex(this.coreTint(this.state));
    if (this.reduced) this.renderOnce();
  }

  /** Cambia el tinte del núcleo según la fase del dictado. */
  setState(state: EsferaState) {
    if (this.disposed || state === this.state) return;
    this.state = state;
    (this.coreGlow.material as THREE.SpriteMaterial).color.setHex(
      this.coreTint(state),
    );
    if (this.reduced) this.renderOnce();
  }

  start() {
    if (this.disposed || this.running) return;
    if (this.reduced) {
      // F11: esfera estática con núcleo de estado — un solo render.
      this.renderOnce();
      return;
    }
    this.running = true;
    this.clock.start();
    this.lastSecondAt = performance.now();
    this.framesThisSecond = 0;
    const tick = () => {
      if (!this.running) return;
      this.raf = requestAnimationFrame(tick);
      this.frame();
    };
    this.raf = requestAnimationFrame(tick);
  }

  /** Detiene el rAF de inmediato (cero trabajo con el overlay oculto). */
  stop() {
    this.running = false;
    if (this.raf) cancelAnimationFrame(this.raf);
    this.raf = 0;
  }

  private frame() {
    const t = this.clock.getElapsedTime();
    const now = performance.now();

    // Decaimiento a la calma cuando el backend deja de emitir (silencio o fin
    // de grabación): la esfera respira en vez de congelarse.
    if (now - this.lastDataAt > DATA_STALE_MS) {
      for (let k = 0; k < BANDS; k++)
        this.bands[k] += (0 - this.bands[k]) * 0.06;
      this.rms += (0 - this.rms) * 0.05;
      this.bass += (0 - this.bass) * 0.06;
    }

    // Dominante → empuje que migra suavemente por la membrana.
    let mi = 0;
    let mv = 0;
    for (let i = 0; i < BANDS; i++) {
      if (this.bands[i] > mv) {
        mv = this.bands[i];
        mi = i;
      }
    }
    this.dom += (mi - this.dom) * 0.07;
    const pushTarget = Math.max(0, Math.min(1, (mv - 0.3) * 1.7));
    this.pushAmt = ease(this.pushAmt, pushTarget, 0.1, 0.05);
    const angTarget =
      -Math.PI / 4 +
      (this.dom / (BANDS - 1) - 0.5) * 1.8 +
      Math.sin(t * 0.35) * 0.25;
    this.pushAngle += (angTarget - this.pushAngle) * 0.06;

    this.uniforms.uTime.value = t;
    this.uniforms.uRMS.value = this.rms;
    this.uniforms.uBass.value = this.bass;
    this.uniforms.uBands.value = this.bands;
    this.uniforms.uPushAmt.value = this.pushAmt;
    this.uniforms.uPushAngle.value = this.pushAngle;
    this.uniforms.uPushRing.value = 0.15 + (this.dom / (BANDS - 1)) * 0.75;

    // Rotación lenta de planeta + leve precesión.
    this.spinner.rotation.z = t * 0.05;
    this.holder.rotation.x = Math.sin(t * 0.3) * 0.05;
    this.holder.rotation.y = Math.cos(t * 0.23) * 0.06;

    // Núcleo: late con la voz al grabar; pulso sereno al transcribir/procesar.
    const working = this.state !== "recording";
    const beat = working
      ? 0.3 + 0.05 * Math.sin(t * 2.0)
      : 0.32 + this.rms * 0.45 + this.bass * 0.3;
    this.coreGlow.scale.setScalar(beat);
    (this.coreGlow.material as THREE.SpriteMaterial).opacity =
      0.55 + this.rms * 0.4;
    (this.pinkHalo.material as THREE.SpriteMaterial).opacity =
      0.32 + this.rms * 0.25;

    this.renderer.render(this.scene, this.camera);

    // Watchdog: 3 s consecutivos bajo 42 fps → bajar densidad de puntos.
    this.framesThisSecond++;
    if (now - this.lastSecondAt >= 1000) {
      const fps = this.framesThisSecond;
      this.framesThisSecond = 0;
      this.lastSecondAt = now;
      this.lowSeconds = fps < 42 ? this.lowSeconds + 1 : 0;
      if (this.lowSeconds >= 3) {
        this.lowSeconds = 0;
        const idx = LEVEL_ORDER.indexOf(this.level);
        if (idx < LEVEL_ORDER.length - 1) this.rebuild(LEVEL_ORDER[idx + 1]);
      }
    }
  }

  private renderOnce() {
    (this.coreGlow.material as THREE.SpriteMaterial).color.setHex(
      this.coreTint(this.state),
    );
    (this.coreGlow.material as THREE.SpriteMaterial).opacity = 0.75;
    this.renderer.render(this.scene, this.camera);
  }

  /** Libera todos los recursos GPU. El motor no puede reutilizarse después. */
  dispose() {
    if (this.disposed) return;
    this.stop();
    this.disposed = true;
    this.resizeObserver?.disconnect();
    this.resizeObserver = null;
    if (this.points) {
      this.spinner.remove(this.points);
      this.points.geometry.dispose();
      this.points = null;
    }
    this.material.dispose();
    this.darkCore.geometry.dispose();
    (this.darkCore.material as THREE.Material).dispose();
    (this.coreGlow.material as THREE.SpriteMaterial).dispose();
    (this.pinkHalo.material as THREE.SpriteMaterial).dispose();
    for (const tx of this.textures) tx.dispose();
    this.textures = [];
    this.renderer.dispose();
  }
}
