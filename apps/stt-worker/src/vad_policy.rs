//! Shared Silero speech/silence and padding policy, independent of ML backend.
use std::ops::Range;
pub fn speech_ranges(probabilities: &[f32], samples: usize) -> Vec<Range<usize>> {
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
