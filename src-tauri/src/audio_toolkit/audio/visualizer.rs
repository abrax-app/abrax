use rustfft::{num_complex::Complex32, Fft, FftPlanner};
use std::sync::Arc;

const DB_MIN: f32 = -55.0;
const DB_MAX: f32 = -8.0;
const GAIN: f32 = 1.3;
const CURVE_POWER: f32 = 0.7;

/// How many of the lowest bands feed the `bass` aggregate (with 32 log bands
/// over 70–8000 Hz this covers roughly 70–250 Hz — the vocal fundamental).
const BASS_BANDS: usize = 4;

/// Floor for the slow-decaying RMS peak tracker. Keeps silence normalized to
/// ~0 instead of letting the tracker collapse and amplify noise.
const RMS_PEAK_FLOOR: f32 = 0.04;

/// One spectrum analysis frame for the audio-reactive overlay.
///
/// `bands` are normalized 0..1 energies ordered low → high frequency; the
/// aggregates map onto the sphere's visual language: `rms` drives the core,
/// `bass` the heartbeat, `dominant` the membrane push.
#[derive(Clone, Debug, serde::Serialize)]
pub struct SpectrumFrame {
    pub bands: Vec<f32>,
    pub rms: f32,
    pub bass: f32,
    pub dominant: u16,
}

pub struct AudioVisualiser {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    bucket_ranges: Vec<(usize, usize)>,
    fft_input: Vec<Complex32>,
    noise_floor: Vec<f32>,
    buffer: Vec<f32>,
    window_size: usize,
    buckets: usize,
    rms_peak: f32,
}

impl AudioVisualiser {
    pub fn new(
        sample_rate: u32,
        window_size: usize,
        buckets: usize,
        freq_min: f32,
        freq_max: f32,
    ) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(window_size);

        // Pre-compute Hann window
        let window: Vec<f32> = (0..window_size)
            .map(|i| {
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / window_size as f32).cos())
            })
            .collect();

        // Pre-compute bucket frequency ranges. True logarithmic spacing
        // (each band spans a constant frequency *ratio*), so pitch moves
        // linearly across the bands — what the sphere expects for its
        // núcleo→borde mapping. Clamped to Nyquist for low-rate devices.
        let nyquist = sample_rate as f32 / 2.0;
        let freq_min = freq_min.min(nyquist * 0.5).max(1.0);
        let freq_max = freq_max.min(nyquist * 0.95).max(freq_min * 2.0);
        let ratio = freq_max / freq_min;

        let mut bucket_ranges = Vec::with_capacity(buckets);

        for b in 0..buckets {
            let start_hz = freq_min * ratio.powf(b as f32 / buckets as f32);
            let end_hz = freq_min * ratio.powf((b + 1) as f32 / buckets as f32);

            let start_bin = ((start_hz * window_size as f32) / sample_rate as f32) as usize;
            let mut end_bin = ((end_hz * window_size as f32) / sample_rate as f32) as usize;

            // Ensure each bucket has at least one bin
            if end_bin <= start_bin {
                end_bin = start_bin + 1;
            }

            // Clamp to valid range
            let start_bin = start_bin.min(window_size / 2);
            let end_bin = end_bin.min(window_size / 2);

            bucket_ranges.push((start_bin, end_bin));
        }

        Self {
            fft,
            window,
            bucket_ranges,
            fft_input: vec![Complex32::new(0.0, 0.0); window_size],
            noise_floor: vec![-40.0; buckets], // Initialize to reasonable noise floor
            buffer: Vec::with_capacity(window_size * 2),
            window_size,
            buckets,
            rms_peak: RMS_PEAK_FLOOR,
        }
    }

    pub fn feed(&mut self, samples: &[f32]) -> Option<SpectrumFrame> {
        // Add new samples to buffer
        self.buffer.extend_from_slice(samples);

        // Only process if we have enough samples
        if self.buffer.len() < self.window_size {
            return None;
        }

        // Take the required window of samples
        let window_samples = &self.buffer[..self.window_size];

        // Remove DC component
        let mean = window_samples.iter().sum::<f32>() / self.window_size as f32;

        // Time-domain RMS (loudness), normalized against a slow-decaying peak
        // tracker so quiet and hot microphones both land in 0..1 — the same
        // AGC idea the per-band noise floor applies in the frequency domain.
        let energy = window_samples
            .iter()
            .map(|&s| {
                let d = s - mean;
                d * d
            })
            .sum::<f32>();
        let rms_raw = (energy / self.window_size as f32).sqrt();
        self.rms_peak = (self.rms_peak * 0.995).max(rms_raw).max(RMS_PEAK_FLOOR);
        let rms = (rms_raw / self.rms_peak).clamp(0.0, 1.0);

        // Apply window function and prepare FFT input
        for (i, &sample) in window_samples.iter().enumerate() {
            let windowed_sample = (sample - mean) * self.window[i];
            self.fft_input[i] = Complex32::new(windowed_sample, 0.0);
        }

        // Perform FFT
        self.fft.process(&mut self.fft_input);

        // Compute power spectrum and bucket levels
        let mut buckets = vec![0.0; self.buckets];

        for (bucket_idx, &(start_bin, end_bin)) in self.bucket_ranges.iter().enumerate() {
            if start_bin >= end_bin || end_bin > self.fft_input.len() / 2 {
                continue;
            }

            // Calculate average power in this frequency range
            let mut power_sum = 0.0;
            for bin_idx in start_bin..end_bin {
                let magnitude = self.fft_input[bin_idx].norm();
                power_sum += magnitude * magnitude;
            }

            let avg_power = power_sum / (end_bin - start_bin) as f32;

            // Convert to dB with proper scaling
            let db = if avg_power > 1e-12 {
                20.0 * (avg_power.sqrt() / self.window_size as f32).log10()
            } else {
                -80.0 // Very low floor for zero power
            };

            // Only update noise floor when signal is quiet (below current floor + 10dB)
            if db < self.noise_floor[bucket_idx] + 10.0 {
                const NOISE_ALPHA: f32 = 0.001; // Very slow adaptation
                self.noise_floor[bucket_idx] =
                    NOISE_ALPHA * db + (1.0 - NOISE_ALPHA) * self.noise_floor[bucket_idx];
            }

            // Map configurable dB range to 0-1 with gain and curve shaping
            let normalized = ((db - DB_MIN) / (DB_MAX - DB_MIN)).clamp(0.0, 1.0);
            buckets[bucket_idx] = (normalized * GAIN).powf(CURVE_POWER).clamp(0.0, 1.0);
        }

        // Apply light smoothing to reduce jitter
        for i in 1..buckets.len() - 1 {
            buckets[i] = buckets[i] * 0.7 + buckets[i - 1] * 0.15 + buckets[i + 1] * 0.15;
        }

        // Aggregates for the sphere: bass = mean of the lowest bands,
        // dominant = loudest band index (the membrane push origin).
        let bass_n = BASS_BANDS.min(buckets.len());
        let bass = if bass_n > 0 {
            buckets[..bass_n].iter().sum::<f32>() / bass_n as f32
        } else {
            0.0
        };
        let dominant = buckets
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i as u16)
            .unwrap_or(0);

        // Clear processed samples from buffer
        self.buffer.clear();

        Some(SpectrumFrame {
            bands: buckets,
            rms,
            bass,
            dominant,
        })
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
        // Reset noise floor and RMS peak tracker to initial values
        self.noise_floor.fill(-40.0);
        self.rms_peak = RMS_PEAK_FLOOR;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pure tone should light up a narrow group of bands, and the frame's
    /// aggregates must point at that group.
    #[test]
    fn tone_produces_dominant_band_and_frame_aggregates() {
        let sample_rate = 16_000;
        let window = 512;
        let mut viz = AudioVisualiser::new(sample_rate, window, 32, 70.0, 8000.0);

        // 440 Hz sine at healthy amplitude
        let samples: Vec<f32> = (0..window)
            .map(|i| {
                (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin() * 0.5
            })
            .collect();

        let frame = viz.feed(&samples).expect("full window must produce frame");
        assert_eq!(frame.bands.len(), 32);
        assert!(frame.rms > 0.5, "loud tone should have high rms");
        assert!(
            (frame.bands[frame.dominant as usize]
                - frame.bands.iter().cloned().fold(0.0, f32::max))
            .abs()
                < 1e-6,
            "dominant must index the loudest band"
        );
        // 440 Hz with log spacing 70→8000 lands in the lower third
        assert!(
            (frame.dominant as usize) < 16,
            "440 Hz should sit in the lower half of the spectrum, got band {}",
            frame.dominant
        );
    }

    /// Band edges must grow monotonically (true log spacing) and every band
    /// must own at least one FFT bin.
    #[test]
    fn log_band_ranges_are_monotonic_and_nonempty() {
        let viz = AudioVisualiser::new(48_000, 2048, 32, 70.0, 8000.0);
        let mut prev_end = 0usize;
        for (i, &(start, end)) in viz.bucket_ranges.iter().enumerate() {
            assert!(end > start, "band {i} is empty ({start}..{end})");
            assert!(start >= prev_end.saturating_sub(1), "band {i} regressed");
            prev_end = end;
        }
        // Last band must not exceed Nyquist bin
        assert!(viz.bucket_ranges.last().unwrap().1 <= 2048 / 2);
    }

    /// Silence must normalize to ~0 rms — the peak tracker floor prevents
    /// silence from being amplified into a false signal.
    #[test]
    fn silence_stays_quiet() {
        let mut viz = AudioVisualiser::new(16_000, 512, 32, 70.0, 8000.0);
        let silence = vec![0.0f32; 512];
        let frame = viz.feed(&silence).unwrap();
        assert!(
            frame.rms < 0.05,
            "silence rms should be ~0, got {}",
            frame.rms
        );
        assert!(frame.bass < 0.2, "silence bass should be low");
    }
}
