//! whisper.cpp/Metal adapter isolated in the owned worker process.
use std::{
    ffi::CStr,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use voicepaste_core::settings::{Device, Language, TranscriptionOptions};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperVadContext,
    WhisperVadContextParams,
};
static GPU_FAILED: AtomicBool = AtomicBool::new(false);

unsafe extern "C" fn native_log(
    _: whisper_rs_sys::ggml_log_level,
    text: *const std::ffi::c_char,
    _: *mut std::ffi::c_void,
) {
    if text.is_null() {
        return;
    }
    // SAFETY: pinned native library supplies a live NUL-terminated log message
    // for this synchronous callback. No pointer escapes the callback.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    // Pinned whisper.cpp falls back only at whisper_backend_init_gpu. Turn its
    // explicit fallback diagnostics into failure before Ready or any text result.
    if text.contains("whisper_backend_init_gpu: no GPU found")
        || text.contains("failed to initialize")
    {
        GPU_FAILED.store(true, Ordering::Release);
    }
    eprint!("{text}");
}

pub struct Pipeline {
    context: WhisperContext,
    options: TranscriptionOptions,
    vad: PathBuf,
    pub model_loading_us: u64,
}
pub struct ResultText {
    pub text: String,
    pub measurement: voicepaste_core::statistics::Measurement,
}
impl Pipeline {
    pub fn open(model: &Path, _: &Path, options: TranscriptionOptions) -> Result<Self, String> {
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
        let path = model.join(&artifact.path);
        let started = Instant::now();
        // SAFETY: worker-local callback has process lifetime; only atomics and
        // stderr are accessed. Native verbosity never writes into IPC stdout.
        unsafe {
            whisper_rs_sys::whisper_log_set(Some(native_log), std::ptr::null_mut());
        }
        GPU_FAILED.store(false, Ordering::Release);
        let mut parameters = WhisperContextParameters::default();
        parameters.use_gpu(options.device == Device::MetalOnly);
        parameters.flash_attn(true);
        let context = WhisperContext::new_with_params(
            path.to_str().ok_or("Model path is not UTF-8")?,
            parameters,
        )
        .map_err(|e| e.to_string())?;
        let probe = context.create_state().map_err(|e| e.to_string())?;
        drop(probe);
        if options.device == Device::MetalOnly && GPU_FAILED.load(Ordering::Acquire) {
            return Err(
                "Metal initialization failed; CPU fallback is forbidden by Metal-only settings"
                    .into(),
            );
        }
        if context.model_ftype() != 1 {
            return Err(
                "Expected full FP16 GGML weights; quantized/model substitution is forbidden".into(),
            );
        }
        Ok(Self {
            context,
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
        self.context.model_n_mels() as usize
    }
    pub fn transcribe(&mut self, path: &Path) -> Result<ResultText, String> {
        let preparation = Instant::now();
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
        let preparation_us = preparation.elapsed().as_micros() as u64;
        let vad_us = if self.options.enable_vad {
            let started = Instant::now();
            let mut vad = WhisperVadContext::new(
                self.vad.to_str().ok_or("VAD path is not UTF-8")?,
                WhisperVadContextParams::default(),
            )
            .map_err(|e| e.to_string())?;
            vad.detect_speech(&audio).map_err(|e| e.to_string())?;
            let probabilities = vad.probabilities();
            if probabilities
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
        GPU_FAILED.store(false, Ordering::Release);
        let mut state = self.context.create_state().map_err(|e| e.to_string())?;
        let mut params = FullParams::new(SamplingStrategy::BeamSearch {
            beam_size: i32::from(self.options.beam_size),
            patience: 1.0,
        });
        params.set_print_progress(false);
        params.set_print_realtime(false);
        params.set_print_special(false);
        params.set_print_timestamps(false);
        params.set_no_context(true);
        params.set_translate(false);
        params.set_temperature(0.);
        params.set_temperature_inc(0.);
        let language = match self.options.language_mode {
            Language::Ua => Some("uk"),
            Language::En => Some("en"),
            Language::Auto => None,
            Language::Bilingual => {
                state.pcm_to_mel(&audio, 4).map_err(|e| e.to_string())?;
                let (_, probabilities) = state.lang_detect(0, 4).map_err(|e| e.to_string())?;
                let uk = whisper_rs::get_lang_id("uk").ok_or("Missing Ukrainian token")? as usize;
                let en = whisper_rs::get_lang_id("en").ok_or("Missing English token")? as usize;
                let uk = *probabilities
                    .get(uk)
                    .ok_or("Missing Ukrainian probability")?;
                let en = *probabilities.get(en).ok_or("Missing English probability")?;
                Some(if uk >= en { "uk" } else { "en" })
            }
        };
        params.set_language(language);
        let prompt = self
            .options
            .custom_initial_prompt
            .trim_end_matches(['\r', '\n']);
        params.set_initial_prompt(prompt);
        let inference = Instant::now();
        state.full(params, &audio).map_err(|e| e.to_string())?;
        if self.options.device == Device::MetalOnly && GPU_FAILED.load(Ordering::Acquire) {
            return Err("Metal backend failed; CPU result was discarded".into());
        }
        let mut text = String::new();
        for segment in state.as_iter() {
            text.push_str(segment.to_str().map_err(|e| e.to_string())?);
        }
        if self.options.language_mode == Language::Ua {
            text = text
                .chars()
                .map(|c| match c {
                    'ы' => 'и',
                    'Ы' => 'И',
                    'э' => 'е',
                    'Э' => 'Е',
                    'ё' => 'е',
                    'Ё' => 'Е',
                    'ъ' | 'Ъ' => '\'',
                    _ => c,
                })
                .collect();
        }
        Ok(ResultText {
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
