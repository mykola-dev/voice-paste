//! Silero v6 CPU inference and faster-whisper's default speech/silence policy.
use ort::{session::Session, value::Tensor};
use std::{ops::Range, path::Path};

pub struct Vad {
    session: Session,
}
impl Vad {
    pub fn open(model: &Path, library: &Path) -> Result<Self, String> {
        ort::init_from(library.to_string_lossy())
            .commit()
            .map_err(|e| e.to_string())?;
        let session = Session::builder()
            .map_err(|e| e.to_string())?
            .with_intra_threads(1)
            .map_err(|e| e.to_string())?
            .with_inter_threads(1)
            .map_err(|e| e.to_string())?
            .commit_from_file(model)
            .map_err(|e| e.to_string())?;
        Ok(Self { session })
    }
    pub fn filter(&mut self, audio: &[f32]) -> Result<Vec<f32>, String> {
        // Reference always appends at least one zero-padding sample and uses
        // previous-window 64-sample context. Hidden state resets per request.
        let windows = audio.len() / 512 + 1;
        let mut h = vec![0.0f32; 128];
        let mut c = h.clone();
        let mut probabilities = Vec::with_capacity(windows);
        for first in (0..windows).step_by(10000) {
            let count = (windows - first).min(10000);
            let mut input = vec![0.0f32; count * 576];
            for i in 0..count {
                let start = (first + i) * 512;
                if start >= 512 {
                    let previous = start - 64;
                    let end = (previous + 64).min(audio.len());
                    if previous < end {
                        input[i * 576..i * 576 + end - previous]
                            .copy_from_slice(&audio[previous..end]);
                    }
                }
                let end = (start + 512).min(audio.len());
                if start < end {
                    input[i * 576 + 64..i * 576 + 64 + end - start]
                        .copy_from_slice(&audio[start..end]);
                }
            }
            let input = Tensor::from_array(([count, 576], input)).map_err(|e| e.to_string())?;
            let h_input =
                Tensor::from_array(([1, 1, 128], h.clone())).map_err(|e| e.to_string())?;
            let c_input =
                Tensor::from_array(([1, 1, 128], c.clone())).map_err(|e| e.to_string())?;
            let outputs = self
                .session
                .run(ort::inputs!["input" => input, "h" => h_input, "c" => c_input])
                .map_err(|e| e.to_string())?;
            probabilities.extend_from_slice(
                outputs[0]
                    .try_extract_tensor::<f32>()
                    .map_err(|e| e.to_string())?
                    .1,
            );
            h = outputs[1]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?
                .1
                .to_vec();
            c = outputs[2]
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?
                .1
                .to_vec();
            if h.len() != 128 || c.len() != 128 {
                return Err("Invalid Silero recurrent-state layout".into());
            }
        }
        if probabilities.len() != windows
            || !probabilities
                .iter()
                .all(|p| p.is_finite() && (0.0..=1.0).contains(p))
        {
            return Err("Invalid Silero probability output".into());
        }
        let ranges = speech_ranges(&probabilities, audio.len());
        Ok(ranges
            .into_iter()
            .flat_map(|range| audio[range].iter().copied())
            .collect())
    }
}

fn speech_ranges(probabilities: &[f32], samples: usize) -> Vec<Range<usize>> {
    // Defaults: threshold .5, negative .35, no minimum speech duration,
    // 2000 ms silence and 400 ms padding. Infinite maximum speech duration.
    let mut start = None;
    let mut end = None;
    let mut ranges = Vec::new();
    for (i, &probability) in probabilities.iter().enumerate() {
        let position = i * 512;
        if probability >= 0.5 {
            end = None;
            if start.is_none() {
                start = Some(position);
                continue;
            }
        }
        if probability < 0.35
            && let Some(begin) = start
        {
            let silence = *end.get_or_insert(position);
            if position - silence >= 32000 {
                if silence > begin {
                    ranges.push(begin..silence.min(samples));
                }
                start = None;
                end = None;
            }
        }
    }
    if let Some(begin) = start
        && samples > begin
    {
        ranges.push(begin..samples);
    }
    if let Some(first) = ranges.first_mut() {
        first.start = first.start.saturating_sub(6400);
    }
    for i in 0..ranges.len() {
        if i + 1 < ranges.len() {
            let silence = ranges[i + 1].start.saturating_sub(ranges[i].end);
            if silence < 12800 {
                ranges[i].end += silence / 2;
                ranges[i + 1].start -= silence / 2;
            } else {
                ranges[i].end = (ranges[i].end + 6400).min(samples);
                ranges[i + 1].start = ranges[i + 1].start.saturating_sub(6400);
            }
        } else {
            ranges[i].end = (ranges[i].end + 6400).min(samples);
        }
    }
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_hysteresis_and_padding() {
        assert!(speech_ranges(&[0.0; 100], 51200).is_empty());
        assert_eq!(speech_ranges(&[0.8; 100], 51200), vec![0..51200]);
        let mut probabilities = vec![0.0; 160];
        probabilities[20..30].fill(0.9);
        assert_eq!(speech_ranges(&probabilities, 81920), vec![3840..21760]);
    }
}
