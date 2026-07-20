/* ═══════════════════════════════════════════════════════════════════════
   ESFERA «palabras vivas» · adaptación para la landing de ABRAX
   Origen: assets/prototipos/esfera_con_palabras.html (prototipo aprobado).
   Cambios respecto al prototipo:
   - init({canvas,...}) con tamaño de contenedor + ResizeObserver (antes fullscreen).
   - Sin UI propia: el estado se notifica por callback onEstado(texto).
   - El feed de palabras y el micrófono se controlan desde afuera (API).
   - Poda de chispas: cada sistema de puntos vive CHISPA_VIDA_S segundos y se
     libera (el prototipo las acumulaba para siempre — fuga de memoria/draw calls).
   - Colores alineados a la marca: #2FD9FF / #8B5CF6 / #F23DC4, fondo #05060F.
   - El bucle se pausa fuera del viewport y con la pestaña oculta.
   API pública: window.ESFERA — init, setAudioData, agregarPalabra, vaciarPalabras,
   startDemo, setSensitivity, setQuality, iniciarFeed, detenerFeed, toggleMic,
   micActivo, getFps.
   ═══════════════════════════════════════════════════════════════════════ */
(function () {
  "use strict";

  const BANDAS = 32;
  const NIVELES = {
    alta: { puntos: 120000, dpr: 1.75 },
    media: { puntos: 60000, dpr: 1.5 },
    baja: { puntos: 25000, dpr: 1.25 },
  };
  const CHISPA_VIDA_S = 14; // vida total de un sistema de chispas
  const CHISPA_FUNDE_S = 2.5; // últimos segundos: fundido a cero antes de liberar
  const reducirMotion = window.matchMedia(
    "(prefers-reduced-motion: reduce)",
  ).matches;

  function nivelAuto() {
    const tosco = window.matchMedia("(pointer: coarse)").matches;
    if (tosco || innerWidth < 820) return "baja";
    return devicePixelRatio > 1.6 ? "media" : "alta";
  }

  /* estado del módulo (se llena en init) */
  let listo = false;
  let canvas, contenedor;
  let onEstado = () => {}; // siempre invocable, aunque init no haya corrido
  let renderer, escena, camara, holder, spinner;
  let nucleoGlow, haloRosa;
  let uniforms, materialPuntos, texChispa;
  let nivelActual, modoCalidad, puntos;
  let grupoPalabras, grupoChispas;
  const palabras = [];
  const chispas = [];
  let reloj;
  /* perillas de afinado en vivo (ESFERA.tune) */
  let ajusteNucleo = 1,
    ajusteHalo = 1,
    ajusteGiro = 1,
    ajusteInterior = 1;
  let interiorMat, polvo;
  let anillo1, anillo2;
  const anillosCruzados = [];
  let ajusteAnillo = 0;
  let ajusteCruzados = 0;

  /* ── texturas útiles (canvas 2D) ── */
  function texturaRadial(r, g, b, aCentro) {
    const s = 256,
      cv = document.createElement("canvas");
    cv.width = cv.height = s;
    const ctx = cv.getContext("2d");
    const gr = ctx.createRadialGradient(s / 2, s / 2, 0, s / 2, s / 2, s / 2);
    gr.addColorStop(0, "rgba(" + r + "," + g + "," + b + "," + aCentro + ")");
    gr.addColorStop(
      0.35,
      "rgba(" + r + "," + g + "," + b + "," + aCentro * 0.35 + ")",
    );
    gr.addColorStop(1, "rgba(" + r + "," + g + "," + b + ",0)");
    ctx.fillStyle = gr;
    ctx.fillRect(0, 0, s, s);
    return new THREE.CanvasTexture(cv);
  }

  function spriteGlow(r, g, b, alfa, escala, x, y, z) {
    const m = new THREE.SpriteMaterial({
      map: texturaRadial(r, g, b, alfa),
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      transparent: true,
    });
    const sp = new THREE.Sprite(m);
    sp.scale.setScalar(escala);
    sp.position.set(x, y, z);
    return sp;
  }

  /* ── geometría: anillos concéntricos con el polo hacia la cámara —
     la estructura original del prototipo (ordenada, con espiral suave) ── */
  function construirGeometria(total) {
    const anillos = Math.max(90, Math.round(Math.sqrt(total) * 0.62));
    const phiMax = Math.PI * 0.995;
    const pesos = [];
    let suma = 0;
    for (let i = 0; i < anillos; i++) {
      const phi = ((i + 0.5) / anillos) * phiMax;
      const w = Math.sin(phi) + 0.03;
      pesos.push(w);
      suma += w;
    }
    const pos = [],
      aR = [],
      aA = [],
      aRd = [];
    for (let i = 0; i < anillos; i++) {
      const phi = ((i + 0.5) / anillos) * phiMax;
      const n = Math.max(3, Math.round((total * pesos[i]) / suma));
      const espiral = i * 0.37;
      const s = Math.sin(phi),
        c = Math.cos(phi);
      for (let j = 0; j < n; j++) {
        const th =
          (j / n) * Math.PI * 2 + espiral + (Math.random() - 0.5) * 0.014;
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

  /* ── shaders ── */
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
uniform float uOrganico;
uniform float uRuido;
uniform float uRuidoEsc;
uniform float uRuidoVel;
uniform float uVenas;
uniform float uVenaEsc;
uniform float uBands[32];
attribute float aRing;
attribute float aAngle;
attribute float aRand;
varying float vEnergy;
varying float vWarm;
varying float vShade;
varying float vRR;
varying float vVena;
varying vec3  vWorld;

float bandAt(float r){
  float f = clamp(r, 0.0, 0.999) * 31.0;
  int i = int(floor(f));
  int j = i + 1; if (j > 31) j = 31;
  return mix(uBands[i], uBands[j], fract(f));
}

/* ── ruido simplex 3D (Ashima / IQ, dominio público) ──
   deforma la estructura con ruido real: nada de redondez de compás */
vec3 mod289(vec3 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
vec4 mod289(vec4 x){ return x - floor(x * (1.0/289.0)) * 289.0; }
vec4 permute(vec4 x){ return mod289(((x*34.0)+1.0)*x); }
vec4 taylorInvSqrt(vec4 r){ return 1.79284291400159 - 0.85373472095314 * r; }
float snoise(vec3 v){
  const vec2 C = vec2(1.0/6.0, 1.0/3.0);
  const vec4 D = vec4(0.0, 0.5, 1.0, 2.0);
  vec3 i  = floor(v + dot(v, C.yyy));
  vec3 x0 = v - i + dot(i, C.xxx);
  vec3 g = step(x0.yzx, x0.xyz);
  vec3 l = 1.0 - g;
  vec3 i1 = min(g.xyz, l.zxy);
  vec3 i2 = max(g.xyz, l.zxy);
  vec3 x1 = x0 - i1 + C.xxx;
  vec3 x2 = x0 - i2 + C.yyy;
  vec3 x3 = x0 - D.yyy;
  i = mod289(i);
  vec4 p = permute(permute(permute(
      i.z + vec4(0.0, i1.z, i2.z, 1.0))
    + i.y + vec4(0.0, i1.y, i2.y, 1.0))
    + i.x + vec4(0.0, i1.x, i2.x, 1.0));
  float n_ = 0.142857142857;
  vec3 ns = n_ * D.wyz - D.xzx;
  vec4 j = p - 49.0 * floor(p * ns.z * ns.z);
  vec4 x_ = floor(j * ns.z);
  vec4 y_ = floor(j - 7.0 * x_);
  vec4 x = x_ * ns.x + ns.yyyy;
  vec4 y = y_ * ns.x + ns.yyyy;
  vec4 h = 1.0 - abs(x) - abs(y);
  vec4 b0 = vec4(x.xy, y.xy);
  vec4 b1 = vec4(x.zw, y.zw);
  vec4 s0 = floor(b0)*2.0 + 1.0;
  vec4 s1 = floor(b1)*2.0 + 1.0;
  vec4 sh = -step(h, vec4(0.0));
  vec4 a0 = b0.xzyw + s0.xzyw*sh.xxyy;
  vec4 a1 = b1.xzyw + s1.xzyw*sh.zzww;
  vec3 p0 = vec3(a0.xy, h.x);
  vec3 p1 = vec3(a0.zw, h.y);
  vec3 p2 = vec3(a1.xy, h.z);
  vec3 p3 = vec3(a1.zw, h.w);
  vec4 norm = taylorInvSqrt(vec4(dot(p0,p0), dot(p1,p1), dot(p2,p2), dot(p3,p3)));
  p0 *= norm.x; p1 *= norm.y; p2 *= norm.z; p3 *= norm.w;
  vec4 m = max(0.6 - vec4(dot(x0,x0), dot(x1,x1), dot(x2,x2), dot(x3,x3)), 0.0);
  m = m * m;
  return 42.0 * dot(m*m, vec4(dot(p0,x0), dot(p1,x1), dot(p2,x2), dot(p3,x3)));
}

void main(){
  vec3 dir = normalize(position);
  float m = 1.0 - uReduced*0.85;
  float rr = aRing < 0.5 ? aRing*2.0 : (1.0-aRing)*2.0;
  float e  = bandAt(rr);

  float ramp = pow(smoothstep(0.22, 0.85, rr), 1.4);
  float ph = aRing*22.0;
  float ondas = sin(aAngle*5.0  + uTime*0.50 + ph)         * 0.48
              + sin(aAngle*9.0  - uTime*0.34 + ph*1.7+1.3) * 0.30
              + sin(aAngle*16.0 + uTime*0.72 - ph*0.6)     * 0.16
              + sin(aAngle*27.0 - uTime*0.90 + aRand*6.28) * 0.06;
  float ruffle = ramp * ondas * (0.085 + 0.10*uRMS*uSens) * m;

  float breath = sin(uTime*0.55 + aRing*PI*2.0 + aRand*3.0)*0.006*m + 0.002;
  float centro = smoothstep(0.16, 0.0, rr);
  float ripple = centro * sin(rr*34.0 - uTime*2.2) * 0.010 * m;

  float bandPush = e * (0.035 + 0.075*uSens) * (0.35 + ramp*0.65);
  float bassPush = uBass * 0.05 * uSens;

  float da = abs(atan(sin(aAngle - uPushAngle), cos(aAngle - uPushAngle))) / PI;
  float dr = abs(rr - uPushRing);
  float bulge = uPushAmt * exp(-da*da*10.0 - dr*dr*16.0) * (0.4 + ramp*0.6);

  /* bultos orgánicos de baja frecuencia: rompen la redondez perfecta de la
     silueta, lentos como una membrana viva (0 = esfera matemática) */
  float lump = (
      sin(aAngle*2.0 + uTime*0.16 + ph*0.35) * 0.55 +
      sin(aAngle*5.0 - uTime*0.11 + aRing*9.0) * 0.30 +
      sin(aAngle*9.0 + uTime*0.07 - ph*1.2) * 0.15
    ) * uOrganico * 0.09 * m;

  /* estructura de ruido: la masa entera respira con simplex 3D — la
     silueta deja de ser un círculo y el volumen gana vetas y grietas */
  float ruido = 0.0;
  if (uRuido > 0.001) {
    vec3 pR = dir * uRuidoEsc + vec3(0.0, uTime * uRuidoVel * 0.14, uTime * uRuidoVel * 0.09);
    ruido = (snoise(pR) * 0.72 + snoise(pR * 2.3 + 11.7) * 0.28) * uRuido * m;
  }

  float disp = breath + ripple + ruffle + lump + ruido + (bandPush + bassPush + bulge*0.16*uSens) * m;
  disp = clamp(disp, -0.10 - uRuido*0.55, 0.30 + uRuido*0.65);
  vec3 pos = dir * (1.0 + disp);

  /* venas: iso-líneas del campo de ruido — filamentos luminosos que
     serpentean por la superficie, migran lento y pulsan con la voz */
  float vena = 0.0;
  if (uVenas > 0.001) {
    vec3 pV = dir * uVenaEsc + vec3(uTime*0.045, -uTime*0.036, uTime*0.028) * m;
    float v1 = 1.0 - smoothstep(0.0, 0.09, abs(snoise(pV)));
    float v2 = 1.0 - smoothstep(0.0, 0.13, abs(snoise(pV * 0.45 + 7.31)));
    vena = max(v1, v2 * 0.55) * uVenas * (0.70 + uRMS * uSens * 0.7);
  }
  vVena = vena;

  vShade  = clamp(0.55 + ruffle*4.2 + bulge*0.6 + ruido*2.2, 0.18, 1.30);
  vEnergy = clamp(e*1.0 + bulge*1.3 + uRMS*0.22, 0.0, 1.5);
  vWarm   = clamp(bulge*1.5, 0.0, 1.0);
  vRR     = rr;
  vWorld  = (modelMatrix * vec4(pos, 1.0)).xyz;

  vec4 mv = modelViewMatrix * vec4(pos, 1.0);
  gl_Position = projectionMatrix * mv;
  float ps = uSize * (0.85 + vEnergy*0.6 + vena*0.7) * uScale / max(-mv.z, 0.1);
  gl_PointSize = clamp(ps, 1.0, 5.5);
}
`;

  /* colores de marca: cian #2FD9FF · violeta #8B5CF6 · magenta #F23DC4 */
  const FRAG = `
precision mediump float;
uniform float uBlanco;
varying float vEnergy;
varying float vWarm;
varying float vShade;
varying float vRR;
varying float vVena;
varying vec3  vWorld;
void main(){
  vec2 uv = gl_PointCoord*2.0 - 1.0;
  float d = dot(uv, uv);
  if (d > 1.0) discard;
  float alpha = exp(-d*3.2) * 0.9;

  float t = clamp((vWorld.x + vWorld.y)*0.30 + 0.5, 0.0, 1.0);
  vec3 cian    = vec3(0.184, 0.851, 1.000);
  vec3 violeta = vec3(0.545, 0.361, 0.965);
  vec3 magenta = vec3(0.949, 0.239, 0.769);
  vec3 blanco  = vec3(1.00, 0.97, 0.99);
  vec3 exterior = mix(violeta, cian, t);
  vec3 col = mix(magenta, exterior, smoothstep(0.18, 0.70, vRR));
  col = mix(blanco, col, smoothstep(0.02, 0.16, vRR));
  col *= vShade;
  col = mix(col, vec3(1.0, 0.62, 0.42), vWarm*0.5);
  col = mix(col, vec3(1.0), clamp(vEnergy*vEnergy*0.45, 0.0, 0.75) * uBlanco);
  /* limbo luminoso: el borde de la esfera arde suave, como eclipse */
  float limbo = smoothstep(0.80, 0.97, vRR);
  col += exterior * limbo * 0.28;

  /* las venas tiñen a cian luminoso y suben el brillo del punto */
  float vn = clamp(vVena, 0.0, 1.0);
  col = mix(col, vec3(0.58, 0.92, 1.0), vn * 0.85);
  gl_FragColor = vec4(col * (0.42 + vEnergy*0.70 + vn*0.55), alpha);
}
`;

  function reconstruir(nivel) {
    nivelActual = nivel;
    if (puntos) {
      spinner.remove(puntos);
      puntos.geometry.dispose();
    }
    puntos = new THREE.Points(
      construirGeometria(NIVELES[nivel].puntos),
      materialPuntos,
    );
    spinner.add(puntos);
    ajustarTamano();
  }

  /* ── motor de audio (demo / micrófono / externo) ── */
  const audio = {
    modo: "demo",
    bandas: new Float32Array(BANDAS),
    rms: 0,
    bass: 0,
    dom: BANDAS * 0.35,
    pushAmt: 0,
    pushAngle: -0.6,
    sens: 1,
    ctx: null,
    analyser: null,
    stream: null,
    freq: null,
    tiempo: null,
    picos: new Float32Array(BANDAS).fill(0.18),
    rangos: null,
  };

  function suavizar(actual, objetivo, subida, bajada) {
    return objetivo > actual
      ? actual + (objetivo - actual) * subida
      : actual + (objetivo - actual) * bajada;
  }

  function demoTick(t) {
    const c1 = (Math.sin(t * 0.21) + 1) / 2;
    const c2 = (Math.sin(t * 0.13 + 2.1) + 1) / 2;
    for (let i = 0; i < BANDAS; i++) {
      const x = i / (BANDAS - 1);
      const g1 =
        Math.exp(-Math.pow(x - c1, 2) / 0.015) *
        (0.6 + 0.4 * Math.sin(t * 1.9));
      const g2 =
        Math.exp(-Math.pow(x - c2, 2) / 0.03) *
        (0.5 + 0.5 * Math.sin(t * 2.7 + 1.0));
      const objetivo = 0.1 + 0.3 * g1 + 0.2 * g2;
      audio.bandas[i] = suavizar(audio.bandas[i], objetivo, 0.3, 0.08);
    }
    audio.rms = suavizar(audio.rms, 0.11 + 0.05 * Math.sin(t * 0.9), 0.2, 0.06);
    audio.bass =
      (audio.bandas[0] + audio.bandas[1] + audio.bandas[2] + audio.bandas[3]) /
      4;
    actualizarDominante(t, 0.55);
  }

  let micActivando = false;
  let micCancelar = false;
  async function activarMic() {
    if (micActivando || audio.modo === "mic") return audio.modo === "mic";
    micActivando = true;
    try {
      audio.stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          echoCancellation: false,
          noiseSuppression: false,
          autoGainControl: true,
        },
      });
      audio.ctx = new (window.AudioContext || window.webkitAudioContext)();
      await audio.ctx.resume();
      const src = audio.ctx.createMediaStreamSource(audio.stream);
      audio.analyser = audio.ctx.createAnalyser();
      audio.analyser.fftSize = 1024;
      audio.analyser.smoothingTimeConstant = 0.72;
      src.connect(audio.analyser);
      audio.freq = new Uint8Array(512);
      audio.tiempo = new Uint8Array(1024);
      const nyq = audio.ctx.sampleRate / 2;
      const fLo = 70,
        fHi = Math.min(7800, nyq * 0.95);
      audio.rangos = [];
      for (let k = 0; k < BANDAS; k++) {
        const f0 = fLo * Math.pow(fHi / fLo, k / BANDAS);
        const f1 = fLo * Math.pow(fHi / fLo, (k + 1) / BANDAS);
        const b0 = Math.max(0, Math.floor((f0 / nyq) * 512));
        const b1 = Math.min(511, Math.max(b0 + 1, Math.ceil((f1 / nyq) * 512)));
        audio.rangos.push([b0, b1]);
      }
      if (micCancelar) {
        /* el usuario canceló mientras el permiso estaba pendiente */
        micCancelar = false;
        audio.stream.getTracks().forEach((t) => t.stop());
        audio.stream = null;
        audio.ctx.close().catch(() => {});
        audio.ctx = null;
        audio.analyser = null;
        onEstado("modo demo · respiración sintética");
        return false;
      }
      audio.modo = "mic";
      onEstado("micrófono activo · procesamiento 100% en este navegador");
      return true;
    } catch (err) {
      onEstado("sin permiso de micrófono · la esfera sigue en modo demo");
      console.warn("Micrófono no disponible:", err);
      return false;
    } finally {
      micActivando = false;
    }
  }

  function detenerMic() {
    if (audio.stream) {
      audio.stream.getTracks().forEach((t) => t.stop());
      audio.stream = null;
    }
    if (audio.ctx) {
      audio.ctx.close().catch(() => {});
      audio.ctx = null;
    }
    audio.analyser = null;
    audio.modo = "demo";
    onEstado("modo demo · respiración sintética");
  }

  function micTick(t) {
    const an = audio.analyser;
    if (!an) return;
    an.getByteFrequencyData(audio.freq);
    an.getByteTimeDomainData(audio.tiempo);
    for (let k = 0; k < BANDAS; k++) {
      const [b0, b1] = audio.rangos[k];
      let s = 0;
      for (let b = b0; b < b1; b++) s += audio.freq[b];
      let v = s / (b1 - b0) / 255;
      audio.picos[k] = Math.max(audio.picos[k] * 0.996, v, 0.18);
      v = Math.min(1, v / audio.picos[k]);
      audio.bandas[k] = suavizar(audio.bandas[k], v, 0.55, 0.1);
    }
    let acc = 0;
    for (let i = 0; i < 1024; i++) {
      const d = (audio.tiempo[i] - 128) / 128;
      acc += d * d;
    }
    const rmsCrudo = Math.sqrt(acc / 1024) * 2.2;
    audio.rms = suavizar(audio.rms, Math.min(1, rmsCrudo), 0.4, 0.07);
    audio.bass =
      (audio.bandas[0] + audio.bandas[1] + audio.bandas[2] + audio.bandas[3]) /
      4;
    actualizarDominante(t, 1.0);
  }

  function actualizarDominante(t, fuerza) {
    let mi = 0,
      mv = 0;
    for (let i = 0; i < BANDAS; i++) {
      if (audio.bandas[i] > mv) {
        mv = audio.bandas[i];
        mi = i;
      }
    }
    audio.dom += (mi - audio.dom) * 0.07;
    const objetivo = Math.max(0, Math.min(1, (mv - 0.3) * 1.7)) * fuerza;
    audio.pushAmt = suavizar(audio.pushAmt, objetivo, 0.1, 0.05);
    const angObjetivo =
      -Math.PI / 4 +
      (audio.dom / (BANDAS - 1) - 0.5) * 1.8 +
      Math.sin(t * 0.35) * 0.25;
    audio.pushAngle += (angObjetivo - audio.pushAngle) * 0.06;
  }

  let externoTimeout = null;
  function setAudioData(bandas, rms, bass, domIdx) {
    if (!listo || !bandas || !bandas.length) return;
    if (audio.modo === "mic") detenerMic(); // no dejar el stream del mic abierto
    audio.modo = "externo";
    const n = bandas.length;
    for (let k = 0; k < BANDAS; k++) {
      const f = (k / (BANDAS - 1)) * (n - 1);
      const i = Math.floor(f),
        j = Math.min(n - 1, i + 1);
      const v = Math.max(
        0,
        Math.min(1, bandas[i] + (bandas[j] - bandas[i]) * (f - i)),
      );
      audio.bandas[k] = suavizar(audio.bandas[k], v, 0.55, 0.12);
    }
    const rmsV =
      typeof rms === "number"
        ? rms
        : audio.bandas.reduce((a, b) => a + b, 0) / BANDAS;
    audio.rms = suavizar(audio.rms, Math.min(1, rmsV), 0.4, 0.08);
    audio.bass =
      typeof bass === "number"
        ? bass
        : (audio.bandas[0] +
            audio.bandas[1] +
            audio.bandas[2] +
            audio.bandas[3]) /
          4;
    if (typeof domIdx === "number")
      audio.dom += ((domIdx / (n - 1)) * (BANDAS - 1) - audio.dom) * 0.1;
    clearTimeout(externoTimeout);
    externoTimeout = setTimeout(() => {
      if (audio.modo === "externo") iniciarDemo();
    }, 1500);
  }

  function iniciarDemo() {
    if (audio.modo === "mic") detenerMic();
    audio.modo = "demo";
    onEstado("modo demo · respiración sintética");
  }

  /* ── interacción: arrastre con inercia y retorno orgánico ── */
  const tilt = { x: 0, y: 0, vx: 0, vy: 0, activo: false, px: 0, py: 0 };
  function conectarArrastre() {
    canvas.addEventListener("pointerdown", (e) => {
      tilt.activo = true;
      tilt.px = e.clientX;
      tilt.py = e.clientY;
      canvas.classList.add("arrastrando");
      canvas.setPointerCapture(e.pointerId);
    });
    canvas.addEventListener("pointermove", (e) => {
      if (!tilt.activo) return;
      const dx = e.clientX - tilt.px,
        dy = e.clientY - tilt.py;
      tilt.px = e.clientX;
      tilt.py = e.clientY;
      tilt.vy = dx * 0.004;
      tilt.vx = dy * 0.004;
      tilt.y = Math.max(-1.1, Math.min(1.1, tilt.y + tilt.vy));
      tilt.x = Math.max(-1.1, Math.min(1.1, tilt.x + tilt.vx));
    });
    function soltar() {
      tilt.activo = false;
      canvas.classList.remove("arrastrando");
    }
    canvas.addEventListener("pointerup", soltar);
    canvas.addEventListener("pointercancel", soltar);
  }

  /* ── tamaño según contenedor ── */
  function ajustarTamano() {
    if (!listo) return;
    const w = contenedor.clientWidth || 1;
    const h = contenedor.clientHeight || 1;
    const dprMax = NIVELES[nivelActual].dpr;
    const dpr = Math.min(devicePixelRatio || 1, dprMax);
    renderer.setPixelRatio(dpr);
    renderer.setSize(w, h);
    camara.aspect = w / h;
    camara.updateProjectionMatrix();
    uniforms.uScale.value =
      (h * dpr) / (2 * Math.tan((camara.fov * Math.PI) / 360));
  }

  /* ── watchdog de FPS ── */
  let framesSeg = 0,
    ultimoSeg = 0,
    fpsProm = 60,
    segBajos = 0;
  function medirFps(ahora) {
    framesSeg++;
    if (ahora - ultimoSeg >= 1000) {
      fpsProm = framesSeg;
      framesSeg = 0;
      ultimoSeg = ahora;
      if (modoCalidad === "auto" && !reducirMotion) {
        segBajos = fpsProm < 42 ? segBajos + 1 : 0;
        if (segBajos >= 3) {
          segBajos = 0;
          if (nivelActual === "alta") reconstruir("media");
          else if (nivelActual === "media") reconstruir("baja");
        }
      }
    }
  }

  /* ── bucle principal, pausable ── */
  /* tAnim es un reloj que solo avanza mientras el bucle corre: las edades de
     palabras y chispas se miden contra él, así una palabra agregada con la
     esfera pausada (p. ej. lanzada desde el formulario mientras scrollea)
     empieza su vuelo recién al reanudarse — nunca "envejece" invisible */
  let animId = null,
    enVista = true,
    tAnim = 0,
    tFramePrev = 0;
  function frame() {
    animId = requestAnimationFrame(frame);
    const t = reloj.getElapsedTime();
    const ahora = performance.now();
    if (!tFramePrev) tFramePrev = ahora;
    tAnim += Math.min(ahora - tFramePrev, 100);
    tFramePrev = ahora;

    if (audio.modo === "demo") demoTick(t);
    else if (audio.modo === "mic") micTick(t);
    else if (audio.modo === "externo") actualizarDominante(t, 1.0);

    uniforms.uTime.value = t;
    uniforms.uRMS.value = audio.rms;
    uniforms.uBass.value = audio.bass;
    uniforms.uSens.value = audio.sens;
    uniforms.uPushAmt.value = audio.pushAmt;
    uniforms.uPushAngle.value = audio.pushAngle;
    uniforms.uPushRing.value = 0.15 + (audio.dom / (BANDAS - 1)) * 0.75;

    const velGiro = (reducirMotion ? 0.006 : 0.05) * ajusteGiro;
    spinner.rotation.z = t * velGiro;
    const wobX = reducirMotion ? 0 : Math.sin(t * 0.3) * 0.05;
    const wobY = reducirMotion ? 0 : Math.cos(t * 0.23) * 0.06;
    if (!tilt.activo) {
      tilt.y += tilt.vy;
      tilt.x += tilt.vx;
      tilt.vx *= 0.94;
      tilt.vy *= 0.94;
      tilt.x *= 0.985;
      tilt.y *= 0.985;
    }
    holder.rotation.x = wobX + tilt.x;
    holder.rotation.y = wobY + tilt.y;

    const llenado = Math.min((palabras.length + chispas.length) / 22, 1);
    const lat =
      0.32 + audio.rms * 0.45 * audio.sens + audio.bass * 0.3 + llenado * 0.18;
    nucleoGlow.scale.setScalar((reducirMotion ? 0.34 : lat) * ajusteNucleo);
    nucleoGlow.material.opacity =
      (0.55 + audio.rms * 0.4 + llenado * 0.2) * Math.min(ajusteNucleo, 1.2);
    haloRosa.material.opacity =
      (0.32 + audio.rms * 0.25 + llenado * 0.15) * ajusteHalo;
    if (polvo) {
      polvo.material.opacity =
        (0.13 + audio.rms * 0.1 + llenado * 0.05) * ajusteInterior;
      polvo.visible = ajusteInterior > 0.01;
      polvo.rotation.z = -t * (reducirMotion ? 0.004 : 0.03); /* contra-remolino sutil del polvo */
    }
    if (anillo1) {
      /* sin opacidad = sin draw call: visible solo si la perilla lo pide */
      const verAnillos = ajusteAnillo > 0.01;
      const verCruzados = ajusteCruzados > 0.01;
      anillo1.visible = verAnillos;
      anillo2.visible = verAnillos;
      for (let i = 0; i < anillosCruzados.length; i++)
        anillosCruzados[i].p.visible = verCruzados;
      const velReduc = reducirMotion ? 0.12 : 1; /* órbitas también en calma */
      if (verAnillos) {
        anillo1.rotation.z = t * 0.1 * ajusteGiro * velReduc;
        anillo2.rotation.z = -t * 0.065 * ajusteGiro * velReduc;
        anillo1.material.opacity = (0.2 + audio.rms * 0.3) * ajusteAnillo;
        anillo2.material.opacity = (0.14 + audio.rms * 0.22) * ajusteAnillo;
      }
      if (verCruzados)
        for (let i = 0; i < anillosCruzados.length; i++) {
          const a = anillosCruzados[i];
          a.p.rotation.z = t * a.vel * 1.2 * ajusteGiro * velReduc;
          a.p.material.opacity = (0.15 + audio.rms * 0.25) * ajusteCruzados;
        }
    }

    animarPalabras(tAnim);

    renderer.render(escena, camara);
    medirFps(ahora);
  }
  function arrancar() {
    if (listo && !animId && enVista && !document.hidden) {
      /* reiniciar la ventana del watchdog: una pausa larga no debe contar
         como "segundo bajo" y degradar la calidad sin motivo */
      ultimoSeg = performance.now();
      framesSeg = 0;
      frame();
    }
  }
  function parar() {
    if (animId) {
      cancelAnimationFrame(animId);
      animId = null;
      tFramePrev = 0; // congela tAnim: al reanudar no salta el tiempo pausado
    }
  }

  /* ── palabras que se vuelven la esfera ── */
  function texturaTexto(txt, tech) {
    const pad = 8,
      fs = 40;
    const cv = document.createElement("canvas");
    const ctx = cv.getContext("2d");
    ctx.font = `${tech ? 600 : 500} ${fs}px "JetBrains Mono", ui-monospace, monospace`;
    const w = Math.ceil(ctx.measureText(txt).width) + pad * 2;
    const h = fs + pad * 2;
    cv.width = w;
    cv.height = h;
    const c2 = cv.getContext("2d");
    c2.font = `${tech ? 600 : 500} ${fs}px "JetBrains Mono", ui-monospace, monospace`;
    c2.textAlign = "center";
    c2.textBaseline = "middle";
    c2.shadowColor = tech ? "rgba(47,217,255,0.9)" : "rgba(200,220,255,0.55)";
    c2.shadowBlur = tech ? 14 : 8;
    c2.fillStyle = tech ? "#7fe9ff" : "#e6eeff";
    c2.fillText(txt, w / 2, h / 2);
    const tx = new THREE.CanvasTexture(cv);
    tx.needsUpdate = true;
    return { tx, aspect: w / h };
  }

  function agregarPalabra(txt) {
    if (!listo) return;
    const tech =
      /[a-z][A-Z]/.test(txt) || (/^[a-z]+$/.test(txt) && txt.length > 7);
    const { tx, aspect } = texturaTexto(txt, tech);
    const mat = new THREE.SpriteMaterial({
      map: tx,
      transparent: true,
      depthWrite: false,
      depthTest: false,
      blending: THREE.NormalBlending,
      opacity: 0,
    });
    const sp = new THREE.Sprite(mat);
    sp.renderOrder = 10;

    let rx, ry, rz, d2;
    do {
      rx = Math.random() * 2 - 1;
      ry = Math.random() * 2 - 1;
      rz = Math.random() * 2 - 1;
      d2 = rx * rx + ry * ry + rz * rz;
    } while (d2 < 0.05 || d2 > 1);
    const len = Math.sqrt(d2);
    const dir = new THREE.Vector3(rx / len, ry / len, rz / len);
    const superficie = dir.clone().multiplyScalar(1.0);
    const legible = dir.clone().multiplyScalar(1.5);
    const entrada = dir.clone().multiplyScalar(2.9);

    sp.position.copy(reducirMotion ? legible : entrada);
    const escalaBase = 0.15 * aspect;
    sp.scale.set(escalaBase, 0.15, 1);
    grupoPalabras.add(sp);

    palabras.push({
      sp,
      txt,
      tech,
      dir,
      entrada,
      legible,
      superficie,
      escalaBase,
      nacido: tAnim,
      volarMs: reducirMotion ? 1 : 700,
      leerMs: 380,
    });
  }

  function disolverEnChispas(p) {
    const n = Math.max(10, Math.min(46, p.txt.length * 4));
    const pos = new Float32Array(n * 3);
    const home = new Float32Array(n * 3);
    const rnd = new Float32Array(n);
    for (let i = 0; i < n; i++) {
      const jitter = new THREE.Vector3(
        p.dir.x + (Math.random() - 0.5) * 0.5,
        p.dir.y + (Math.random() - 0.5) * 0.5,
        p.dir.z + (Math.random() - 0.5) * 0.5,
      ).normalize();
      home[i * 3] = jitter.x;
      home[i * 3 + 1] = jitter.y;
      home[i * 3 + 2] = jitter.z;
      pos[i * 3] = jitter.x;
      pos[i * 3 + 1] = jitter.y;
      pos[i * 3 + 2] = jitter.z;
      rnd[i] = Math.random();
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
    const m = new THREE.PointsMaterial({
      map: texChispa,
      size: 0.072,
      transparent: true,
      opacity: 0.8,
      depthWrite: false,
      blending: THREE.AdditiveBlending,
      sizeAttenuation: true,
      color: p.tech ? 0x7fe9ff : 0xbfe0ff,
    });
    const pts = new THREE.Points(g, m);
    grupoChispas.add(pts);
    chispas.push({
      pts,
      geo: g,
      home,
      rnd,
      n,
      nacido: tAnim,
    });
  }

  function liberarChispa(c) {
    grupoChispas.remove(c.pts);
    c.geo.dispose();
    c.pts.material.dispose(); // texChispa es compartida: material.dispose() no la toca
  }

  function vaciarPalabras() {
    for (const p of palabras) {
      grupoPalabras.remove(p.sp);
      p.sp.material.map.dispose();
      p.sp.material.dispose();
    }
    for (const c of chispas) liberarChispa(c);
    palabras.length = 0;
    chispas.length = 0;
  }

  function animarPalabras(now) {
    for (let i = palabras.length - 1; i >= 0; i--) {
      const p = palabras[i];
      const edad = now - p.nacido;
      if (edad < p.volarMs) {
        const k = edad / p.volarMs,
          e = 1 - Math.pow(1 - k, 3);
        p.sp.position.lerpVectors(p.entrada, p.legible, e);
        p.sp.material.opacity = Math.min(1, e * 1.2);
        const s = 0.5 + e * 0.5;
        p.sp.scale.set(p.escalaBase * s, 0.15 * s, 1);
      } else if (edad < p.volarMs + p.leerMs) {
        p.sp.position.copy(p.legible);
        p.sp.material.opacity = 1;
      } else {
        const k = Math.min((edad - p.volarMs - p.leerMs) / 260, 1);
        const e = k * k;
        p.sp.position.lerpVectors(p.legible, p.superficie, e);
        p.sp.material.opacity = 1 - e;
        const s = 1 - e * 0.5;
        p.sp.scale.set(p.escalaBase * s, 0.15 * s, 1);
        if (k >= 1) {
          disolverEnChispas(p);
          grupoPalabras.remove(p.sp);
          p.sp.material.map.dispose();
          p.sp.material.dispose();
          palabras.splice(i, 1);
        }
      }
    }
    /* chispas: empujan hacia afuera, se calman, y al final de su vida se liberan
       (poda que el prototipo no tenía — evita acumulación infinita) */
    for (let i = chispas.length - 1; i >= 0; i--) {
      const c = chispas[i];
      const edad = (now - c.nacido) / 1000;
      if (edad >= CHISPA_VIDA_S) {
        liberarChispa(c);
        chispas.splice(i, 1);
        continue;
      }
      const arr = c.geo.attributes.position.array;
      for (let k = 0; k < c.n; k++) {
        const hx = c.home[k * 3],
          hy = c.home[k * 3 + 1],
          hz = c.home[k * 3 + 2];
        const push = reducirMotion
          ? Math.exp(-edad * 0.9) * 0.04 /* asentamiento sin vibración */
          : Math.exp(-edad * 0.6) * 0.14 * Math.sin(edad * 6 + c.rnd[k] * 6.28) +
            Math.exp(-edad * 0.9) * 0.12;
        const r = 1.0 + push;
        arr[k * 3] = hx * r;
        arr[k * 3 + 1] = hy * r;
        arr[k * 3 + 2] = hz * r;
      }
      c.geo.attributes.position.needsUpdate = true;
      let op = 0.35 + 0.6 * Math.exp(-edad * 0.5);
      const resta = CHISPA_VIDA_S - edad;
      if (resta < CHISPA_FUNDE_S) op *= resta / CHISPA_FUNDE_S;
      c.pts.material.opacity = op;
    }
  }

  /* ── alimentación demo ── */
  const VOCAB = [
    "crea",
    "un",
    "useAuthStore",
    "que",
    "persista",
    "el",
    "token",
    "en",
    "localStorage",
    "importa",
    "desde",
    "store",
    "valida",
    "rut",
    "dígito",
    "verificador",
    "kubectl",
    "namespace",
    "staging",
    "deploy",
    "función",
    "async",
    "await",
    "const",
    "return",
    "middleware",
    "endpoint",
    "hola",
    "cómo",
    "estás",
    "reunión",
    "viernes",
    "gracias",
    "informe",
    "listo",
    "revisamos",
    "mañana",
    "dictado",
    "local",
    "privado",
    "español",
    "voz",
    "texto",
    "abrax",
    "di",
    "la",
    "palabra",
  ];
  let _vi = 0,
    _feedTimer = null;
  function iniciarFeed(cadenciaMs) {
    if (_feedTimer || !listo) return;
    _feedTimer = setInterval(() => {
      if (!animId) return; // pausada (fuera de vista / pestaña oculta): no acumular
      agregarPalabra(VOCAB[_vi % VOCAB.length]);
      _vi++;
    }, cadenciaMs || 900);
  }
  function detenerFeed() {
    clearInterval(_feedTimer);
    _feedTimer = null;
  }

  /* ── init ── */
  function init(opts) {
    if (listo) return;
    opts = opts || {};
    canvas = opts.canvas || document.getElementById("esfera");
    if (!canvas || typeof THREE === "undefined") return;
    contenedor = opts.contenedor || canvas.parentElement;
    onEstado = typeof opts.onEstado === "function" ? opts.onEstado : () => {};

    renderer = new THREE.WebGLRenderer({
      canvas,
      antialias: false,
      alpha: false,
      powerPreference: "high-performance",
    });
    renderer.setClearColor(0x05060f, 1);

    escena = new THREE.Scene();
    camara = new THREE.PerspectiveCamera(42, 1, 0.1, 60);
    camara.position.set(0, 0, opts.distancia || 2.9);

    holder = new THREE.Group();
    spinner = new THREE.Group();
    holder.add(spinner);
    holder.position.y = opts.desplazarY || 0;
    escena.add(holder);

    /* fondo: nebulosas de marca, destello, bokeh, estrellas */
    escena.add(spriteGlow(47, 217, 255, 0.12, 9.5, 2.6, 1.9, -6));
    escena.add(spriteGlow(242, 61, 196, 0.1, 10.5, -2.8, -2.2, -6.5));
    escena.add(spriteGlow(210, 235, 255, 0.26, 1.6, 2.1, 1.55, -2.5));
    for (let i = 0; i < 9; i++) {
      const frio = Math.random() < 0.5;
      escena.add(
        spriteGlow(
          frio ? 140 : 255,
          frio ? 210 : 150,
          frio ? 255 : 220,
          0.08 + Math.random() * 0.07,
          0.25 + Math.random() * 0.55,
          (Math.random() - 0.5) * 7.5,
          (Math.random() - 0.5) * 5.5,
          -4 - Math.random() * 4,
        ),
      );
    }
    (function estrellas() {
      const N = 220,
        pos = new Float32Array(N * 3);
      for (let i = 0; i < N; i++) {
        const R = 6 + Math.random() * 9;
        const a = Math.random() * Math.PI * 2,
          b = Math.acos(2 * Math.random() - 1);
        pos[i * 3] = R * Math.sin(b) * Math.cos(a);
        pos[i * 3 + 1] = R * Math.sin(b) * Math.sin(a);
        pos[i * 3 + 2] = -Math.abs(R * Math.cos(b)) - 2;
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
      const m = new THREE.PointsMaterial({
        size: 0.022,
        color: 0x8fb6d9,
        transparent: true,
        opacity: 0.4,
        depthWrite: false,
        blending: THREE.AdditiveBlending,
        sizeAttenuation: true,
      });
      escena.add(new THREE.Points(g, m));
    })();

    /* interior: ya no una bola negra plana — un cuenco (BackSide) con
       gradiente de paleta en la diagonal de marca, que ocluye los puntos
       traseros pero se funde con el universo en vez de leerse como hoyo */
    interiorMat = new THREE.ShaderMaterial({
      side: THREE.BackSide,
      uniforms: { uInterior: { value: 1 } },
      vertexShader: `
        varying vec3 vN;
        void main(){
          vN = normalize(normalMatrix * normal);
          gl_Position = projectionMatrix * modelViewMatrix * vec4(position, 1.0);
        }`,
      fragmentShader: `
        uniform float uInterior;
        varying vec3 vN;
        void main(){
          float fondo = abs(vN.z);                              /* 1 = centro del cuenco, 0 = borde */
          /* disolución granulada hacia el contorno: la oclusión se apaga
             pixel a pixel (dither), los puntos traseros reaparecen gradual
             y la silueta deja de ser un círculo dibujado */
          float velo = smoothstep(0.45, 0.97, 1.0 - fondo);
          /* hash seguro en fp16 y sin periodicidad visible (variante hash12) */
          vec2 q = fract(gl_FragCoord.xy * vec2(0.1031, 0.113));
          q += dot(q, q.yx + 13.33);
          float h = fract((q.x + q.y) * q.x);
          if (h < velo) discard;
          float t = clamp((vN.x + vN.y) * 0.5 + 0.5, 0.0, 1.0); /* diagonal de marca en pantalla */
          vec3 borde   = vec3(0.020, 0.024, 0.059);             /* tinta */
          vec3 violeta = vec3(0.090, 0.058, 0.180);
          vec3 cian    = vec3(0.028, 0.080, 0.125);
          vec3 centro  = mix(violeta, cian, t) * uInterior;
          vec3 col = borde + centro * pow(fondo, 1.5);
          gl_FragColor = vec4(col, 1.0);
        }`,
    });
    spinner.add(
      new THREE.Mesh(new THREE.SphereGeometry(0.78, 48, 48), interiorMat),
    );
    nucleoGlow = new THREE.Sprite(
      new THREE.SpriteMaterial({
        map: texturaRadial(200, 240, 255, 0.9),
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        transparent: true,
      }),
    );
    nucleoGlow.position.set(0, 0, 1.0);
    nucleoGlow.scale.setScalar(0.34);
    spinner.add(nucleoGlow);
    haloRosa = new THREE.Sprite(
      new THREE.SpriteMaterial({
        map: texturaRadial(255, 120, 200, 0.45),
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        transparent: true,
      }),
    );
    haloRosa.position.set(0, 0, 0.95);
    haloRosa.scale.setScalar(1.25);
    spinner.add(haloRosa);

    grupoPalabras = new THREE.Group();
    holder.add(grupoPalabras);
    grupoChispas = new THREE.Group();
    holder.add(grupoChispas);

    uniforms = {
      uTime: { value: 0 },
      uRMS: { value: 0 },
      uBass: { value: 0 },
      uSens: { value: 1 },
      uReduced: { value: reducirMotion ? 1 : 0 },
      uScale: { value: 300 },
      uSize: { value: 0.0032 },
      uPushAngle: { value: -0.6 },
      uPushRing: { value: 0.4 },
      uPushAmt: { value: 0 },
      uOrganico: { value: 0 },
      uBlanco: { value: 1 },
      uRuido: { value: 0 },
      uRuidoEsc: { value: 2.2 },
      uRuidoVel: { value: 1 },
      uVenas: { value: 0 },
      uVenaEsc: { value: 3.0 },
      uBands: { value: new Float32Array(BANDAS) },
    };
    materialPuntos = new THREE.ShaderMaterial({
      uniforms,
      vertexShader: VERT,
      fragmentShader: FRAG,
      blending: THREE.AdditiveBlending,
      depthWrite: false,
      depthTest: true,
      transparent: true,
    });
    uniforms.uBands.value = audio.bandas; /* referencia fija, se llena in-place */
    texChispa = texturaRadial(150, 220, 255, 1.0);

    /* polvo interior: partículas tenues llenando el volumen (denso hacia el
       núcleo) — el interior deja de ser vacío y gana la nube de la paleta */
    (function crearPolvo() {
      const N = 4200,
        pos = new Float32Array(N * 3);
      for (let i = 0; i < N; i++) {
        const a = Math.random() * Math.PI * 2,
          b = Math.acos(2 * Math.random() - 1);
        const r = 0.16 + 0.66 * Math.pow(Math.random(), 0.55);
        pos[i * 3] = r * Math.sin(b) * Math.cos(a);
        pos[i * 3 + 1] = r * Math.sin(b) * Math.sin(a);
        pos[i * 3 + 2] = r * Math.cos(b);
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
      polvo = new THREE.Points(
        g,
        new THREE.PointsMaterial({
          map: texChispa,
          size: 0.035,
          color: 0x9a7bff,
          transparent: true,
          opacity: 0.15,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          sizeAttenuation: true,
        }),
      );
      spinner.add(polvo);
    })();

    /* anillos del isotipo (2) + enjambre armilar (3): órbitas punteadas de
       marca. Apagados por defecto; se encienden con tune({anillo, cruzados}) */
    function crearAnillo(radio, n, color, tam) {
      const pos = new Float32Array(n * 3);
      for (let i = 0; i < n; i++) {
        const a = (i / n) * Math.PI * 2;
        pos[i * 3] = Math.cos(a) * radio;
        pos[i * 3 + 1] = Math.sin(a) * radio;
        pos[i * 3 + 2] = 0;
      }
      const g = new THREE.BufferGeometry();
      g.setAttribute("position", new THREE.Float32BufferAttribute(pos, 3));
      const p = new THREE.Points(
        g,
        new THREE.PointsMaterial({
          map: texChispa,
          size: tam,
          color: color,
          transparent: true,
          opacity: 0,
          depthWrite: false,
          blending: THREE.AdditiveBlending,
          sizeAttenuation: true,
        }),
      );
      p.rotation.x = 1.12;
      p.rotation.y = -0.34;
      holder.add(p);
      return p;
    }
    anillo1 = crearAnillo(1.38, 190, 0x7fd4ff, 0.042);
    anillo2 = crearAnillo(1.62, 150, 0xa98bff, 0.05);
    [
      { r: 1.5, n: 170, color: 0x8fd8ff, tam: 0.04, rx: -0.95, ry: 0.85, vel: 0.085 },
      { r: 1.56, n: 160, color: 0xc59bff, tam: 0.045, rx: 0.35, ry: 1.45, vel: -0.06 },
      { r: 1.68, n: 140, color: 0xf28bd8, tam: 0.05, rx: 1.85, ry: -0.55, vel: 0.048 },
    ].forEach(function (d) {
      const p = crearAnillo(d.r, d.n, d.color, d.tam);
      p.rotation.x = d.rx;
      p.rotation.y = d.ry;
      anillosCruzados.push({ p: p, vel: d.vel });
    });

    nivelActual = reducirMotion ? "baja" : nivelAuto();
    modoCalidad = "auto";
    reloj = new THREE.Clock();
    listo = true;

    reconstruir(nivelActual);
    conectarArrastre();

    if (typeof ResizeObserver !== "undefined")
      new ResizeObserver(ajustarTamano).observe(contenedor);
    addEventListener("resize", ajustarTamano);

    if (typeof IntersectionObserver !== "undefined") {
      new IntersectionObserver(
        (entradas) => {
          /* puede llegar más de un registro por callback: manda el último */
          enVista = entradas[entradas.length - 1].isIntersecting;
          enVista ? arrancar() : parar();
        },
        { threshold: 0.02 },
      ).observe(canvas);
    }
    document.addEventListener("visibilitychange", () =>
      document.hidden ? parar() : arrancar(),
    );

    ultimoSeg = performance.now();
    arrancar();
    onEstado("modo demo · respiración sintética");
    return true;
  }

  /* ── API pública ── */
  window.ESFERA = {
    init,
    setAudioData,
    agregarPalabra,
    vaciarPalabras,
    startDemo: iniciarDemo,
    iniciarFeed,
    detenerFeed,
    micActivo() {
      return audio.modo === "mic";
    },
    async toggleMic() {
      if (!listo) return false;
      if (audio.modo === "mic") {
        detenerMic();
        return false;
      }
      if (micActivando) {
        micCancelar = true; /* toggle durante el permiso pendiente = cancelar */
        return false;
      }
      return activarMic();
    },
    setSensitivity(v) {
      audio.sens = Math.max(0.2, Math.min(2.5, v));
    },
    setQuality(q) {
      if (!listo) return;
      if (q === "auto") {
        modoCalidad = "auto";
        reconstruir(nivelAuto());
      } else if (NIVELES[q]) {
        modoCalidad = q;
        reconstruir(q);
      }
    },
    getFps() {
      return fpsProm;
    },
    /* afinado en vivo: {nucleo, halo, blanco, organico, giro} — todos 0..~1.5,
       1 = como el prototipo. Pensado para el laboratorio (lab-esfera.html). */
    tune(o) {
      if (!listo || !o) return;
      const c = (v, max) => Math.max(0, Math.min(max, v));
      if (typeof o.nucleo === "number") ajusteNucleo = c(o.nucleo, 2);
      if (typeof o.halo === "number") ajusteHalo = c(o.halo, 2);
      if (typeof o.giro === "number") ajusteGiro = c(o.giro, 3);
      if (typeof o.anillo === "number") ajusteAnillo = c(o.anillo, 1.5);
      if (typeof o.cruzados === "number") ajusteCruzados = c(o.cruzados, 1.5);
      if (typeof o.interior === "number") {
        ajusteInterior = c(o.interior, 2);
        if (interiorMat) interiorMat.uniforms.uInterior.value = ajusteInterior;
      }
      if (typeof o.blanco === "number") uniforms.uBlanco.value = c(o.blanco, 2);
      if (typeof o.organico === "number")
        uniforms.uOrganico.value = c(o.organico, 1.5);
      if (typeof o.ruido === "number") uniforms.uRuido.value = c(o.ruido, 0.6);
      if (typeof o.ruidoEscala === "number")
        uniforms.uRuidoEsc.value = c(o.ruidoEscala, 5);
      if (typeof o.ruidoVel === "number")
        uniforms.uRuidoVel.value = c(o.ruidoVel, 3);
      if (typeof o.venas === "number") uniforms.uVenas.value = c(o.venas, 1.5);
      if (typeof o.venaEscala === "number")
        uniforms.uVenaEsc.value = c(o.venaEscala, 6);
    },
    getTune() {
      return {
        nucleo: ajusteNucleo,
        halo: ajusteHalo,
        giro: ajusteGiro,
        interior: ajusteInterior,
        anillo: ajusteAnillo,
        cruzados: ajusteCruzados,
        blanco: listo ? uniforms.uBlanco.value : 1,
        organico: listo ? uniforms.uOrganico.value : 0,
        ruido: listo ? uniforms.uRuido.value : 0,
        ruidoEscala: listo ? uniforms.uRuidoEsc.value : 2.2,
        ruidoVel: listo ? uniforms.uRuidoVel.value : 1,
        venas: listo ? uniforms.uVenas.value : 0,
        venaEscala: listo ? uniforms.uVenaEsc.value : 3.0,
      };
    },
  };
})();
