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
        let candidates: Vec<_> = catalog
            .iter()
            .flat_map(|m| &m.files)
            .filter(|file| model.join(&file.path).is_file())
            .collect();
        let artifact = match candidates.as_slice() {
            [artifact] => *artifact,
            [] => return Err("No supported full-size GGML artifact in model directory".into()),
            _ => {
                return Err(
                    "Ambiguous GGML model directory; refusing to substitute a different model"
                        .into(),
                );
            }
        };
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ambiguous_model_directory_is_rejected_before_any_native_load() {
        let root =
            std::env::temp_dir().join(format!("voicepaste-ambiguous-ggml-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        struct Owned(PathBuf);
        impl Drop for Owned {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _owner = Owned(root.clone());
        std::fs::write(
            root.join("ggml-large-v2.bin"),
            b"metadata fixture, not weights",
        )
        .unwrap();
        std::fs::write(
            root.join("ggml-large-v3.bin"),
            b"metadata fixture, not weights",
        )
        .unwrap();
        let settings = voicepaste_core::settings::Settings::initial_for(
            voicepaste_core::settings::SettingsPlatform::Macos,
        )
        .unwrap();
        let error = Pipeline::open(&root, &root, TranscriptionOptions::from(&settings))
            .err()
            .expect("ambiguous models must fail");
        assert!(error.contains("Ambiguous GGML"));
    }
}
