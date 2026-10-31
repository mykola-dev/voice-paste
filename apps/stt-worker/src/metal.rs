//! whisper.cpp/Metal pipeline; IO, VAD, preparation and decoding are timed apart.
#[path = "metal/engine.rs"]
mod engine;
use std::{
    path::{Path, PathBuf},
    time::Instant,
};
use voicepaste_core::settings::{Device, Language, TranscriptionOptions};
use whisper_rs::{WhisperVadContext, WhisperVadContextParams};
pub struct Pipeline {
    engine: engine::Engine,
    options: TranscriptionOptions,
    vad: PathBuf,
    pub model_loading_us: u64,
}
pub struct Transcribed {
    pub text: String,
    pub measurement: voicepaste_core::statistics::Measurement,
}
impl Pipeline {
    pub fn open(model: &Path, _: &Path, options: TranscriptionOptions) -> Result<Self, String> {
        if !(1..=10).contains(&options.beam_size) {
            return Err("Beam size must be 1..10".into());
        }
        if !matches!(options.device, Device::MetalOnly | Device::Cpu) {
            return Err("macOS worker supports Metal-only or CPU, not CUDA".into());
        }
        if options.custom_initial_prompt.contains('\0') {
            return Err("Initial prompt contains NUL".into());
        }
        let catalog = voicepaste_core::models::catalog_for(
            voicepaste_core::settings::SettingsPlatform::Macos,
        )?;
        let artifact = catalog
            .iter()
            .flat_map(|m| &m.files)
            .find(|file| model.join(&file.path).is_file())
            .ok_or("No supported full-size GGML artifact in model directory")?;
        let started = Instant::now();
        let engine = engine::Engine::open(&model.join(&artifact.path), options.device)?;
        Ok(Self {
            engine,
            options,
            vad: model.join("vad.bin"),
            model_loading_us: started.elapsed().as_micros().min(u64::MAX as u128) as u64,
        })
    }
    pub fn device(&self) -> &'static str {
        if self.options.device == Device::MetalOnly {
            "metal"
        } else {
            "cpu"
        }
    }
    pub fn mels(&self) -> usize {
        self.engine.mels()
    }
    pub fn transcribe(&mut self, path: &Path) -> Result<Transcribed, String> {
        let mut wav = hound::WavReader::open(path).map_err(|e| e.to_string())?;
        let spec = wav.spec();
        if spec.channels != 1
            || spec.sample_rate != 16000
            || spec.bits_per_sample != 16
            || spec.sample_format != hound::SampleFormat::Int
        {
            return Err("Worker requires mono 16-kHz PCM16 WAV".into());
        }
        let mut audio = wav
            .samples::<i16>()
            .map(|s| s.map(|s| s as f32 / 32768.))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| e.to_string())?;
        let samples = audio.len() as u64;
        let vad_us = if self.options.enable_vad {
            let started = Instant::now();
            let mut vad = WhisperVadContext::new(
                self.vad.to_str().ok_or("VAD path is not UTF-8")?,
                WhisperVadContextParams::default(),
            )
            .map_err(|e| e.to_string())?;
            vad.detect_speech(&audio).map_err(|e| e.to_string())?;
            let probabilities = vad.probabilities();
            if probabilities.len() > audio.len() / 512 + 2
                || probabilities
                    .iter()
                    .any(|p| !p.is_finite() || !(0.0..=1.0).contains(p))
            {
                return Err("Invalid VAD probabilities".into());
            }
            let ranges = crate::vad_policy::speech_ranges(probabilities, audio.len());
            audio = ranges
                .into_iter()
                .flat_map(|range| audio[range].iter().copied())
                .collect();
            Some(started.elapsed().as_micros() as u64)
        } else {
            None
        };
        if audio.is_empty() {
            return Err("No speech detected".into());
        }
        let mut session = self.engine.session()?;
        let preparation = Instant::now();
        session.prepare(&audio)?;
        let preparation_us = preparation.elapsed().as_micros() as u64;
        let inference = Instant::now();
        let mut text = session.transcribe(&self.options)?;
        if matches!(
            self.options.language_mode,
            Language::Ua | Language::Bilingual
        ) {
            text = crate::text::ukrainian_cleanup(&text);
        }
        Ok(Transcribed {
            text: text.trim().into(),
            measurement: voicepaste_core::statistics::Measurement {
                inference_us: inference.elapsed().as_micros() as u64,
                audio_samples: samples,
                vad_us,
                preparation_us: Some(preparation_us),
            },
        })
    }
}
