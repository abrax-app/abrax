// Sonidos de marca ABRAX (bonus #2): dos gestos cortos con identidad "orbe".
// start = quinta ascendente (la esfera se enciende), stop = quinta descendente
// (la esfera se apaga). Timbre vítreo: parciales casi-armónicos + sub suave,
// nada de "ding" genérico de notificación.
//
// Uso: bun gen-sonidos-abrax.ts <dirSalida>

const SR = 44100;

type NoteSpec = {
  freq: number;
  at: number; // segundos
  dur: number; // segundos (incluida la cola)
  gain: number;
  bright: number; // 0..1 — cuánta energía en los parciales altos
};

// Parciales de campana de vidrio: ligeramente inarmónicos para que suene a
// "orbe" y no a piano. Amplitudes relativas moduladas por `bright`.
const PARTIALS: Array<[number, number]> = [
  [1.0, 1.0],
  [2.0, 0.42],
  [2.99, 0.22],
  [4.16, 0.1],
  [5.43, 0.05],
];

function renderNote(buf: Float64Array, n: NoteSpec) {
  const start = Math.floor(n.at * SR);
  const len = Math.min(Math.floor(n.dur * SR), buf.length - start);
  const attack = Math.floor(0.008 * SR);
  const tau = n.dur / 4.5; // decaimiento exponencial suave
  for (let i = 0; i < len; i++) {
    const t = i / SR;
    // Glide corto de entrada: la nota "orbita" hacia su altura (0.97f → f en 30 ms)
    const glide = 1 - 0.03 * Math.exp(-t / 0.012);
    const env =
      (i < attack ? i / attack : 1) * Math.exp(-Math.max(0, t - 0.008) / tau);
    let s = 0;
    for (const [ratio, amp] of PARTIALS) {
      const a = ratio === 1 ? amp : amp * n.bright;
      // Los parciales altos decaen más rápido que la fundamental (campana real)
      const partialEnv = ratio === 1 ? 1 : Math.exp(-t * ratio * 1.6);
      // Par desafinado ±1.5 cents para dar cuerpo sin estéreo
      const f = n.freq * ratio * glide;
      s +=
        a *
        partialEnv *
        0.5 *
        (Math.sin(2 * Math.PI * f * 1.00087 * t) +
          Math.sin(2 * Math.PI * f * 0.99913 * t));
    }
    // Sub una octava abajo, muy suave, solo cuerpo
    s += 0.18 * Math.sin(2 * Math.PI * (n.freq / 2) * t) * Math.exp(-t / (tau * 0.8));
    buf[start + i] += n.gain * env * s;
  }
}

function renderGesture(notes: NoteSpec[], totalDur: number): Float64Array {
  const buf = new Float64Array(Math.ceil(totalDur * SR));
  for (const n of notes) renderNote(buf, n);
  // Fade final de 20 ms contra clicks
  const fade = Math.floor(0.02 * SR);
  for (let i = 0; i < fade; i++) buf[buf.length - 1 - i] *= i / fade;
  // Normalizar a -6 dBFS aprox (0.5 peak)
  let peak = 0;
  for (const v of buf) peak = Math.max(peak, Math.abs(v));
  const k = peak > 0 ? 0.5 / peak : 1;
  for (let i = 0; i < buf.length; i++) buf[i] *= k;
  return buf;
}

function toWav(samples: Float64Array): Uint8Array {
  const n = samples.length;
  const data = new Int16Array(n);
  for (let i = 0; i < n; i++) {
    const v = Math.max(-1, Math.min(1, samples[i]));
    data[i] = Math.round(v * 32767);
  }
  const byteLen = n * 2;
  const buf = new ArrayBuffer(44 + byteLen);
  const dv = new DataView(buf);
  const w = (off: number, s: string) => {
    for (let i = 0; i < s.length; i++) dv.setUint8(off + i, s.charCodeAt(i));
  };
  w(0, "RIFF");
  dv.setUint32(4, 36 + byteLen, true);
  w(8, "WAVE");
  w(12, "fmt ");
  dv.setUint32(16, 16, true);
  dv.setUint16(20, 1, true); // PCM
  dv.setUint16(22, 1, true); // mono
  dv.setUint32(24, SR, true);
  dv.setUint32(28, SR * 2, true);
  dv.setUint16(32, 2, true);
  dv.setUint16(34, 16, true);
  w(36, "data");
  dv.setUint32(40, byteLen, true);
  new Uint8Array(buf, 44).set(new Uint8Array(data.buffer));
  return new Uint8Array(buf);
}

const A4 = 440;
const E5 = 659.255;

// start: A4 → E5, brillante, la esfera se enciende
const start = renderGesture(
  [
    { freq: A4, at: 0, dur: 0.42, gain: 0.85, bright: 0.9 },
    { freq: E5, at: 0.085, dur: 0.5, gain: 1.0, bright: 1.0 },
  ],
  0.62,
);

// stop: E5 → A4, más corto, más opaco, la esfera se apaga y resuelve
const stop = renderGesture(
  [
    { freq: E5, at: 0, dur: 0.3, gain: 0.8, bright: 0.55 },
    { freq: A4, at: 0.07, dur: 0.42, gain: 1.0, bright: 0.4 },
  ],
  0.52,
);

const outDir = process.argv[2];
if (!outDir) throw new Error("falta el directorio de salida");
await Bun.write(`${outDir}/abrax_start.wav`, toWav(start));
await Bun.write(`${outDir}/abrax_stop.wav`, toWav(stop));
console.log(
  `ok: abrax_start.wav ${(start.length / SR).toFixed(2)}s, abrax_stop.wav ${(stop.length / SR).toFixed(2)}s en ${outDir}`,
);
