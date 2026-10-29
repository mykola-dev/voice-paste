//! Privacy-preserving transcription measurements; no transcript is retained.
use crate::settings::{Device, Language, Settings};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Measurement {
    pub inference_us: u64,
    pub audio_samples: u64,
    pub vad_us: Option<u64>,
    pub preparation_us: Option<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Filter {
    pub model: String,
    pub revision: String,
    pub beam_size: u8,
    pub vad: bool,
    pub language: Language,
    pub device: Device,
}

impl Filter {
    pub fn from_settings(settings: &Settings) -> Result<Self, String> {
        Ok(Self {
            model: settings.model.clone(),
            revision: crate::models::find(&settings.model)?.revision,
            beam_size: settings.beam_size,
            vad: settings.enable_vad,
            language: settings.language_mode,
            device: settings.device,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub filter: Filter,
    pub device: String,
    pub measurement: Measurement,
    pub words: usize,
    pub characters: usize,
}

impl Sample {
    pub fn new(filter: Filter, device: &str, measurement: Measurement, text: &str) -> Option<Self> {
        if text.trim().is_empty()
            || !matches!(device, "cpu" | "cuda")
            || measurement.audio_samples == 0
        {
            return None;
        }
        Some(Self {
            filter,
            device: device.into(),
            measurement,
            // A word is a whitespace-delimited item containing a letter or digit.
            words: text
                .split_whitespace()
                .filter(|word| word.chars().any(char::is_alphanumeric))
                .count(),
            characters: text.chars().count(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ModelLoad {
    pub filter: Filter,
    pub device: String,
    pub loading_us: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub device: String,
    pub count: u64,
    pub average_ms: f64,
    pub average_words: f64,
    pub average_audio_seconds: f64,
    /// Total inference time / total input audio duration (not an average of ratios).
    pub ms_per_audio_second: f64,
    pub model_load_count: u64,
    pub average_model_loading_ms: Option<f64>,
    pub vad_count: u64,
    pub average_vad_ms: Option<f64>,
    pub preparation_count: u64,
    pub average_preparation_ms: Option<f64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_unicode_not_utf8_bytes_and_keeps_no_text() {
        let filter = Filter::from_settings(&Settings::initial().unwrap()).unwrap();
        let measurement = Measurement {
            inference_us: 1234,
            audio_samples: 16000,
            ..Measurement::default()
        };
        let sample = Sample::new(filter.clone(), "cuda", measurement, "Привіт, бро! — 42").unwrap();
        assert_eq!(sample.words, 3);
        assert_eq!(sample.characters, 17);
        assert!(Sample::new(filter.clone(), "cpu", measurement, " \n ").is_none());
        assert!(Sample::new(filter, "auto", measurement, "hello").is_none());
    }
}
