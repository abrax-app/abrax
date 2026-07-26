//! Diarización de hablantes OFFLINE, 100% local. Pipeline validado (spike Python
//! + prototipo Rust standalone): segmentación pyannote-3.0 (ONNX) → fbank 80-dim
//! (Rust puro, sin C++/libclang) → embeddings de locutor CAM++ (ONNX) →
//! clustering aglomerativo coseno. Reusa el `ort` (ONNX Runtime) ya presente en
//! el árbol (transcribe-rs) — misma versión, sin segundo runtime.
//!
//! Trabaja sobre el MISMO buffer 16 kHz mono post-VAD que ve el modelo de
//! transcripción, así los tiempos quedan consistentes con los t0_ms/t1_ms de
//! whisper para alinear texto↔hablante aguas arriba (ver managers/actions.rs).

use anyhow::{Context, Result};
use ort::session::Session;
use ort::value::TensorRef;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::path::Path;
use std::sync::Arc;

const SR: f32 = 16000.0;
const WIN: usize = 160_000; // ventana de segmentación pyannote: 10 s @ 16 kHz
const FRAME_LEN: usize = 400; // fbank: 25 ms
const FRAME_SHIFT: usize = 160; // fbank: 10 ms
const N_FFT: usize = 512;
const N_MELS: usize = 80;
const LOW_HZ: f32 = 20.0;
const HIGH_HZ: f32 = 7600.0;
const PREEMPH: f32 = 0.97;
/// powerset pyannote(3 hablantes, máx 2 simultáneos): clase → hablantes activos.
const PS: [&[usize]; 7] = [&[], &[0], &[1], &[2], &[0, 1], &[0, 2], &[1, 2]];
const CLUST_TH: f32 = -0.25; // umbral de similitud AS-Norm (z-space); calibrado (rango estable -0.3..-0.2 → 2 hablantes en audio real de 2 voces)
const MERGE_GAP_S: f32 = 0.75; // fusiona tramos contiguos del mismo hablante

/// Un tramo de habla atribuido a un hablante. `speaker` es un id 1..=k local a
/// ESTA grabación (no persiste entre grabaciones distintas).
#[derive(Clone, Debug)]
pub struct SpeakerSegment {
    pub speaker: usize,
    pub t0_ms: i64,
    pub t1_ms: i64,
}

/// Diarizador cargado (dos sesiones ONNX + tablas de fbank). Reutilizable entre
/// grabaciones; crear una vez y llamar `diarize` por toma.
pub struct Diarizer {
    seg: Session,
    emb: Session,
    fb: Vec<Vec<f32>>,
    win: Vec<f32>,
    fft: Arc<dyn Fft<f32>>,
}

impl Diarizer {
    pub fn new(seg_path: &Path, emb_path: &Path) -> Result<Self> {
        let seg = Session::builder()?
            .commit_from_file(seg_path)
            .with_context(|| format!("cargando segmentación {:?}", seg_path))?;
        let emb = Session::builder()?
            .commit_from_file(emb_path)
            .with_context(|| format!("cargando embeddings {:?}", emb_path))?;
        Ok(Self {
            seg,
            emb,
            fb: mel_filters(),
            win: povey_window(),
            fft: FftPlanner::<f32>::new().plan_fft_forward(N_FFT),
        })
    }

    /// Diariza un buffer 16 kHz mono. `num_speakers` fuerza EXACTAMENTE N hablantes
    /// (pista del usuario — la vía más fiable); `None` = auto por umbral coseno.
    /// Devuelve tramos con hablante y tiempos (ms), ordenados por t0. Vacío si no
    /// hay habla suficiente.
    pub fn diarize(
        &mut self,
        samples: &[f32],
        num_speakers: Option<usize>,
        meeting: bool,
    ) -> Result<Vec<SpeakerSegment>> {
        // Paso 1: máscara de VOZ por frame (unión de hablantes-locales de pyannote),
        // ventanas de 10 s no solapadas — para saber DÓNDE hay voz.
        let n_frames = samples.len() / FRAME_SHIFT + 1;
        let mut speech = vec![false; n_frames];
        let mut pos = 0;
        while pos < samples.len() {
            let end = (pos + WIN).min(samples.len());
            let real = end - pos;
            let mut chunk = samples[pos..end].to_vec();
            chunk.resize(WIN, 0.0);
            let (f, logits): (usize, Vec<f32>) = {
                let out = self.seg.run(ort::inputs![
                    "x" => TensorRef::from_array_view(([1_i64, 1, WIN as i64], chunk.as_slice()))?
                ])?;
                let (osh, data) = out["y"].try_extract_tensor::<f32>()?;
                (osh[1] as usize, data.to_vec())
            };
            let step = WIN as f32 / f as f32;
            for fi in 0..f {
                let base = fi * 7;
                let (mut best, mut bv) = (0usize, f32::MIN);
                for c in 0..7 {
                    let v = logits[base + c];
                    if v > bv {
                        bv = v;
                        best = c;
                    }
                }
                let a = pos + (fi as f32 * step) as usize;
                if !PS[best].is_empty() && a < pos + real {
                    let idx = a / FRAME_SHIFT;
                    if idx < speech.len() {
                        speech[idx] = true;
                    }
                }
            }
            pos += WIN;
        }

        // Paso 2: embeddings de sub-segmentos sobre zonas con voz. La longitud es
        // FINA en reunión (capta turnos breves como una colega que habla poco, a
        // costa de algún cruce que el usuario fusiona en el panel) y estándar en
        // dictado. Override dev por env `ABRAX_DIARIZE_SUB_MS`.
        struct Item {
            emb: Vec<f32>,
            t0: f32,
            t1: f32,
        }
        let default_sub = if meeting { 16_000 } else { 32_000 }; // 1.0 s vs 2.0 s
        let sub_len = std::env::var("ABRAX_DIARIZE_SUB_MS")
            .ok()
            .and_then(|v| v.trim().parse::<usize>().ok())
            .map(|ms| (ms * 16).max(FRAME_LEN))
            .unwrap_or(default_sub);
        let sub_hop = (sub_len / 2).max(1);
        let voiced_min = 0.5f32;
        let mut items: Vec<Item> = Vec::new();
        let mut start = 0;
        while start + sub_len <= samples.len() {
            let (f0, f1) = (start / FRAME_SHIFT, (start + sub_len) / FRAME_SHIFT);
            let total = (f1 - f0).max(1);
            let voiced = (f0..f1.min(speech.len())).filter(|&i| speech[i]).count();
            if voiced as f32 / total as f32 >= voiced_min {
                let e = self.embed(&samples[start..start + sub_len])?;
                items.push(Item {
                    emb: e,
                    t0: start as f32 / SR,
                    t1: (start + sub_len) as f32 / SR,
                });
            }
            start += sub_hop;
        }
        if items.is_empty() {
            return Ok(Vec::new());
        }

        // Paso 3: clustering AVERAGE-linkage (robusto al encadenamiento del
        // single-linkage, que fundía dos voces parecidas en un mismo id). Con
        // `num_speakers` corta en exactamente N; si no, corta por umbral coseno.
        let embs: Vec<&[f32]> = items.iter().map(|it| it.emb.as_slice()).collect();
        // AS-Norm: normaliza el coseno de cada par contra la cohorte (el resto de
        // segmentos) — cancela la coloración común del micrófono, clave same-mic.
        let sim = asnorm_similarity(&embs);
        // `num_speakers` = TOPE (auto hasta N); None = auto sin tope. Umbral
        // calibrable por env `ABRAX_DIARIZE_TH` (dev), si no el default z-space.
        let th = std::env::var("ABRAX_DIARIZE_TH")
            .ok()
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(CLUST_TH);
        let labels = cluster_sim(&sim, num_speakers, th);

        // Paso 4: línea de tiempo — un tramo por sub-segmento — remapeando a 1..=k
        // en orden de aparición y fusionando contiguos del mismo hablante.
        let mut remap = std::collections::HashMap::new();
        let mut next = 1usize;
        let mut segs: Vec<SpeakerSegment> = items
            .iter()
            .zip(&labels)
            .map(|(it, &l)| {
                let g = *remap.entry(l).or_insert_with(|| {
                    let v = next;
                    next += 1;
                    v
                });
                SpeakerSegment {
                    speaker: g,
                    t0_ms: (it.t0 * 1000.0) as i64,
                    t1_ms: (it.t1 * 1000.0) as i64,
                }
            })
            .collect();
        segs.sort_by_key(|s| s.t0_ms);
        let mut merged: Vec<SpeakerSegment> = Vec::new();
        for s in segs {
            if let Some(last) = merged.last_mut() {
                if last.speaker == s.speaker
                    && (s.t0_ms - last.t1_ms) < (MERGE_GAP_S * 1000.0) as i64
                {
                    last.t1_ms = s.t1_ms.max(last.t1_ms);
                    continue;
                }
            }
            merged.push(s);
        }
        Ok(merged)
    }

    fn embed(&mut self, samps: &[f32]) -> Result<Vec<f32>> {
        let feats = fbank(samps, &self.fb, &self.win, &*self.fft);
        if feats.is_empty() {
            anyhow::bail!("fbank vacío");
        }
        let flat: Vec<f32> = feats.iter().flatten().copied().collect();
        let shape = [1_i64, feats.len() as i64, N_MELS as i64];
        // ResNet293-LM (wespeaker): input "feats" → output "embs" (256-dim). El
        // CAM++ anterior usaba "x"/"embedding" (192-dim).
        let out = self.emb.run(ort::inputs![
            "feats" => TensorRef::from_array_view((shape, flat.as_slice()))?
        ])?;
        Ok(out["embs"].try_extract_tensor::<f32>()?.1.to_vec())
    }
}

/// Similitud AS-Norm (self-cohort): normaliza el coseno de cada par contra la
/// distribución de scores de cada segmento al RESTO (cohorte = los demás),
/// `z[i][j] = ½·((s−μ_i)/σ_i + (s−μ_j)/σ_j)`. Cancela la coloración común del
/// micrófono (baseline de similitud alto same-channel) → los pares del mismo
/// hablante quedan sobre 0 y los de distinto bajo 0, separando lo que el coseno
/// crudo confunde.
fn asnorm_similarity(embs: &[&[f32]]) -> Vec<Vec<f32>> {
    let n = embs.len();
    let mut s = vec![vec![0f32; n]; n];
    for i in 0..n {
        for j in i + 1..n {
            let c = cosine(embs[i], embs[j]);
            s[i][j] = c;
            s[j][i] = c;
        }
    }
    let (mut mu, mut sd) = (vec![0f32; n], vec![1f32; n]);
    if n >= 2 {
        for i in 0..n {
            let m = (0..n).filter(|&j| j != i).map(|j| s[i][j]).sum::<f32>() / (n - 1) as f32;
            let v = (0..n)
                .filter(|&j| j != i)
                .map(|j| (s[i][j] - m) * (s[i][j] - m))
                .sum::<f32>()
                / (n - 1) as f32;
            mu[i] = m;
            sd[i] = v.sqrt().max(1e-6);
        }
    }
    let mut z = vec![vec![0f32; n]; n];
    for i in 0..n {
        for j in 0..n {
            z[i][j] = 0.5 * ((s[i][j] - mu[i]) / sd[i] + (s[i][j] - mu[j]) / sd[j]);
        }
    }
    z
}

/// AVERAGE-linkage sobre una matriz de similitud pre-calculada. `max_speakers`:
/// `Some(k)` = TOPE de k hablantes (auto HASTA k — nunca más de k, pero puede dar
/// menos si el audio no los tiene); `None` = auto sin tope. Corta cuando quedan
/// ≤ tope grupos Y la mejor similitud media entre grupos cae bajo `th`.
fn cluster_sim(sim: &[Vec<f32>], max_speakers: Option<usize>, th: f32) -> Vec<usize> {
    let n = sim.len();
    if n == 0 {
        return Vec::new();
    }
    let cap = max_speakers.map(|v| v.max(1)).unwrap_or(usize::MAX);
    let mut clusters: Vec<Vec<usize>> = (0..n).map(|i| vec![i]).collect();
    while clusters.len() > 1 {
        let (mut best, mut ba, mut bb) = (f32::MIN, 0usize, 1usize);
        for a in 0..clusters.len() {
            for b in a + 1..clusters.len() {
                let mut acc = 0.0;
                for &i in &clusters[a] {
                    for &j in &clusters[b] {
                        acc += sim[i][j];
                    }
                }
                let avg = acc / (clusters[a].len() * clusters[b].len()) as f32;
                if avg > best {
                    best = avg;
                    ba = a;
                    bb = b;
                }
            }
        }
        // Sigue fusionando si estamos por encima del tope, o si aún son similares.
        if clusters.len() <= cap && best <= th {
            break;
        }
        let m = clusters.remove(bb);
        clusters[ba].extend(m);
    }
    let mut label = vec![0usize; n];
    for (ci, cl) in clusters.iter().enumerate() {
        for &i in cl {
            label[i] = ci;
        }
    }
    label
}

fn hz_to_mel(f: f32) -> f32 {
    1127.0 * (1.0 + f / 700.0).ln()
}
fn mel_to_hz(m: f32) -> f32 {
    700.0 * ((m / 1127.0).exp() - 1.0)
}

fn mel_filters() -> Vec<Vec<f32>> {
    let nbins = N_FFT / 2 + 1;
    let (ml, mh) = (hz_to_mel(LOW_HZ), hz_to_mel(HIGH_HZ));
    let pts: Vec<f32> = (0..N_MELS + 2)
        .map(|i| mel_to_hz(ml + (mh - ml) * i as f32 / (N_MELS + 1) as f32))
        .collect();
    let bin = |hz: f32| (hz / SR * N_FFT as f32).floor() as usize;
    let mut fb = vec![vec![0.0f32; nbins]; N_MELS];
    for m in 0..N_MELS {
        let (l, c, r) = (bin(pts[m]), bin(pts[m + 1]), bin(pts[m + 2]));
        for k in l..c.min(nbins) {
            if c > l {
                fb[m][k] = (k - l) as f32 / (c - l) as f32;
            }
        }
        for k in c..r.min(nbins) {
            if r > c {
                fb[m][k] = (r - k) as f32 / (r - c) as f32;
            }
        }
    }
    fb
}

fn povey_window() -> Vec<f32> {
    (0..FRAME_LEN)
        .map(|n| {
            (0.5 - 0.5
                * (2.0 * std::f32::consts::PI * n as f32 / (FRAME_LEN - 1) as f32).cos())
            .powf(0.85)
        })
        .collect()
}

/// fbank kaldi-style (80-dim) con normalización de media por utterance (CMN).
fn fbank(x: &[f32], fb: &[Vec<f32>], win: &[f32], fft: &dyn Fft<f32>) -> Vec<Vec<f32>> {
    let mut frames: Vec<Vec<f32>> = Vec::new();
    let mut i = 0;
    while i + FRAME_LEN <= x.len() {
        let mut fr: Vec<f32> = x[i..i + FRAME_LEN].to_vec();
        let mean = fr.iter().sum::<f32>() / FRAME_LEN as f32;
        for v in &mut fr {
            *v -= mean;
        }
        for n in (1..FRAME_LEN).rev() {
            fr[n] -= PREEMPH * fr[n - 1];
        }
        fr[0] -= PREEMPH * fr[0];
        for n in 0..FRAME_LEN {
            fr[n] *= win[n];
        }
        let mut buf: Vec<Complex<f32>> = fr.iter().map(|&v| Complex::new(v, 0.0)).collect();
        buf.resize(N_FFT, Complex::new(0.0, 0.0));
        fft.process(&mut buf);
        let nb = N_FFT / 2 + 1;
        let pow: Vec<f32> = buf[..nb].iter().map(|c| c.norm_sqr()).collect();
        frames.push(
            fb.iter()
                .map(|f| f.iter().zip(&pow).map(|(a, b)| a * b).sum::<f32>().max(1e-10).ln())
                .collect(),
        );
        i += FRAME_SHIFT;
    }
    if !frames.is_empty() {
        let t = frames.len() as f32;
        for d in 0..N_MELS {
            let m = frames.iter().map(|f| f[d]).sum::<f32>() / t;
            for f in &mut frames {
                f[d] -= m;
            }
        }
    }
    frames
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let d: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    d / (na * nb + 1e-9)
}
