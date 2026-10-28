//! faster-whisper-compatible Hann/STFT/Slaney log-mel. See THIRD_PARTY_NOTICES.md.
use rustfft::{FftPlanner, num_complex::Complex};
use std::{f64::consts::TAU, path::Path};
pub const RATE: usize = 16000;
const FFT: usize = 400;
const HOP: usize = 160;
const BINS: usize = 201;
pub const WINDOW: usize = 3000;

pub fn read_wav(path: &Path) -> Result<Vec<f32>, String> {
    let mut wav = hound::WavReader::open(path).map_err(|e| e.to_string())?;
    let spec = wav.spec();
    if spec.channels != 1
        || spec.sample_rate != RATE as u32
        || spec.bits_per_sample != 16
        || spec.sample_format != hound::SampleFormat::Int
    {
        return Err("STT requires PCM16 mono 16 kHz WAV".into());
    }
    let mut samples = Vec::new();
    samples
        .try_reserve_exact(wav.duration() as usize)
        .map_err(|e| format!("Not enough memory for audio: {e}"))?;
    for sample in wav.samples::<i16>() {
        samples.push(sample.map_err(|e| e.to_string())? as f32 / 32768.0);
    }
    Ok(samples)
}

fn reflect(index: isize, length: usize) -> usize {
    let period = 2 * (length - 1) as isize;
    let index = index.rem_euclid(period) as usize;
    if index < length {
        index
    } else {
        period as usize - index
    }
}

pub struct Features {
    data: Vec<f32>,
    frames: usize,
    mels: usize,
}
impl Features {
    pub fn content_frames(&self) -> usize {
        self.frames - 1
    }
    pub fn window(&self, seek: usize, size: usize) -> Vec<f32> {
        let count = size
            .min(WINDOW)
            .min(self.content_frames().saturating_sub(seek));
        let mut result = vec![0.0; self.mels * WINDOW];
        for band in 0..self.mels {
            result[band * WINDOW..band * WINDOW + count].copy_from_slice(
                &self.data[band * self.frames + seek..band * self.frames + seek + count],
            );
        }
        result
    }
}

pub fn features(samples: &[f32], mels: usize) -> Result<Features, String> {
    if samples.len() < 2 || !samples.iter().all(|v| v.is_finite()) || !matches!(mels, 80 | 128) {
        return Err("Invalid waveform or Whisper mel layout".into());
    }
    let frequencies: Vec<_> = (0..mels + 2)
        .map(|i| {
            let mel = 45.245_640_471_924_965 * i as f64 / (mels + 1) as f64;
            if mel < 15.0 {
                mel * (200.0 / 3.0)
            } else {
                1000.0 * ((6.4_f64.ln() / 27.0) * (mel - 15.0)).exp()
            }
        })
        .collect();
    let mut filters = vec![0.0f32; mels * BINS];
    for band in 0..mels {
        let (left, center, right) = (
            frequencies[band],
            frequencies[band + 1],
            frequencies[band + 2],
        );
        for bin in 0..BINS {
            let hz = bin as f64 * RATE as f64 / FFT as f64;
            filters[band * BINS + bin] = (((hz - left) / (center - left))
                .min((right - hz) / (right - center))
                .max(0.0)
                * 2.0
                / (right - left)) as f32;
        }
    }
    let waveform_len = samples
        .len()
        .checked_add(HOP)
        .ok_or("Audio size overflow")?;
    let mut waveform = Vec::new();
    waveform
        .try_reserve_exact(waveform_len)
        .map_err(|e| format!("Not enough memory for audio padding: {e}"))?;
    waveform.extend_from_slice(samples);
    waveform.resize(waveform_len, 0.0);
    let frames = waveform.len() / HOP;
    let window: Vec<_> = (0..FFT)
        .map(|i| (0.5 - 0.5 * (TAU * i as f64 / FFT as f64).cos()) as f32)
        .collect();
    let fft = FftPlanner::<f32>::new().plan_fft_forward(FFT);
    let mut scratch = vec![Complex::default(); fft.get_inplace_scratch_len()];
    let mut frame = vec![Complex::default(); FFT];
    let mut power = [0.0; BINS];
    let feature_count = mels.checked_mul(frames).ok_or("Feature size overflow")?;
    let mut data = Vec::new();
    data.try_reserve_exact(feature_count)
        .map_err(|e| format!("Not enough memory for features: {e}"))?;
    data.resize(feature_count, 0.0);
    let mut maximum = f32::NEG_INFINITY;
    for t in 0..frames {
        for (i, value) in frame.iter_mut().enumerate() {
            *value = Complex::new(
                waveform[reflect((t * HOP + i) as isize - 200, waveform.len())] * window[i],
                0.0,
            );
        }
        fft.process_with_scratch(&mut frame, &mut scratch);
        for (p, v) in power.iter_mut().zip(&frame) {
            *p = v.norm_sqr();
        }
        for band in 0..mels {
            let energy: f32 = filters[band * BINS..(band + 1) * BINS]
                .iter()
                .zip(&power)
                .map(|(w, p)| w * p)
                .sum();
            let value = energy.max(1e-10).log10();
            data[band * frames + t] = value;
            maximum = maximum.max(value);
        }
    }
    // Global normalization precedes window seeking, including the extra frame.
    for value in &mut data {
        *value = (value.max(maximum - 8.0) + 4.0) / 4.0;
    }
    Ok(Features { data, frames, mels })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_windows_pad_after_normalization_and_support_both_layouts() {
        for mels in [80, 128] {
            let f = features(&vec![0.0; 1600], mels).unwrap();
            assert_eq!(f.content_frames(), 10);
            let w = f.window(0, 10);
            assert_eq!(w.len(), mels * WINDOW);
            for band in 0..mels {
                assert!(
                    w[band * WINDOW..band * WINDOW + 10]
                        .iter()
                        .all(|v| *v == -1.5)
                );
                assert_eq!(w[band * WINDOW + 10], 0.0);
            }
        }
    }
}
