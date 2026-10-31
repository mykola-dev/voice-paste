//! Narrow RAII boundary to pinned whisper.cpp 1.8.3. Context/state never escape
//! the worker. Precomputed-mel full decoding permits truthful phase timings.
use std::{
    ffi::{CStr, CString},
    path::Path,
    ptr::NonNull,
    sync::atomic::{AtomicBool, Ordering},
};
use voicepaste_core::settings::{Device, Language, TranscriptionOptions};
use whisper_rs_sys as sys;
static GPU_FAILED: AtomicBool = AtomicBool::new(false);
unsafe extern "C" fn log(
    _: sys::ggml_log_level,
    text: *const std::ffi::c_char,
    _: *mut std::ffi::c_void,
) {
    if text.is_null() {
        return;
    }
    // SAFETY: native callback supplies valid text for the duration of the call.
    let text = unsafe { CStr::from_ptr(text) }.to_string_lossy();
    if text.contains("whisper_backend_init_gpu: no GPU found")
        || text.contains("failed to initialize")
    {
        GPU_FAILED.store(true, Ordering::Release);
    }
    eprint!("{text}");
}
pub struct Engine {
    pointer: NonNull<sys::whisper_context>,
    metal: bool,
}
impl Engine {
    pub fn open(path: &Path, device: Device) -> Result<Self, String> {
        let path = CString::new(path.to_str().ok_or("Model path is not UTF-8")?)
            .map_err(|e| e.to_string())?;
        let metal = device == Device::MetalOnly;
        GPU_FAILED.store(false, Ordering::Release);
        // SAFETY: all params are initialized by the native default constructor;
        // path survives the synchronous load, success transfers one context.
        let pointer = unsafe {
            sys::whisper_log_set(Some(log), std::ptr::null_mut());
            let mut params = sys::whisper_context_default_params();
            params.use_gpu = metal;
            params.flash_attn = true;
            sys::whisper_init_from_file_with_params_no_state(path.as_ptr(), params)
        };
        let engine = Self {
            pointer: NonNull::new(pointer).ok_or("Cannot load full-size GGML model")?,
            metal,
        };
        // SAFETY: live owned context; this is a metadata-only native query.
        if unsafe { sys::whisper_model_ftype(engine.pointer.as_ptr()) } != 1 {
            return Err(
                "Expected full FP16 GGML weights; quantized/model substitution is forbidden".into(),
            );
        }
        let probe = engine.session()?;
        drop(probe);
        engine.ensure_gpu()?;
        Ok(engine)
    }
    pub fn mels(&self) -> usize {
        // SAFETY: metadata query on a live owned native context.
        unsafe { sys::whisper_model_n_mels(self.pointer.as_ptr()) as usize }
    }
    pub fn session(&self) -> Result<Session<'_>, String> {
        GPU_FAILED.store(false, Ordering::Release);
        // SAFETY: context lives at least as long as the borrowed Session owner.
        let pointer = unsafe { sys::whisper_init_state(self.pointer.as_ptr()) };
        let session = Session {
            engine: self,
            pointer: NonNull::new(pointer).ok_or("Cannot allocate native transcription state")?,
        };
        self.ensure_gpu()?;
        Ok(session)
    }
    fn ensure_gpu(&self) -> Result<(), String> {
        if self.metal && GPU_FAILED.load(Ordering::Acquire) {
            Err(
                "Metal initialization failed; CPU fallback is forbidden by Metal-only settings"
                    .into(),
            )
        } else {
            Ok(())
        }
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        // SAFETY: exactly one owner, all borrowed sessions have already ended.
        unsafe {
            sys::whisper_free(self.pointer.as_ptr());
        }
    }
}
pub struct Session<'a> {
    engine: &'a Engine,
    pointer: NonNull<sys::whisper_state>,
}
impl Session<'_> {
    pub fn prepare(&mut self, audio: &[f32]) -> Result<(), String> {
        let count = i32::try_from(audio.len()).map_err(|_| "Audio exceeds native sample limit")?;
        // SAFETY: audio and both native owners live through this synchronous call.
        let result = unsafe {
            sys::whisper_pcm_to_mel_with_state(
                self.engine.pointer.as_ptr(),
                self.pointer.as_ptr(),
                audio.as_ptr(),
                count,
                4,
            )
        };
        if result != 0 {
            return Err(format!("Native log-mel preparation failed ({result})"));
        }
        self.engine.ensure_gpu()
    }
    pub fn transcribe(&mut self, options: &TranscriptionOptions) -> Result<String, String> {
        let prompt = CString::new(crate::text::initial_prompt(options))
            .map_err(|_| "Initial prompt contains NUL")?;
        let language = match options.language_mode {
            Language::Ua => Some("uk".to_owned()),
            Language::En => Some("en".to_owned()),
            Language::Auto => None,
            Language::Bilingual => {
                // SAFETY: prepare() populated this live state with log-mel data;
                // null probabilities is expressly supported by the native API.
                let detected = unsafe {
                    sys::whisper_lang_auto_detect_with_state(
                        self.engine.pointer.as_ptr(),
                        self.pointer.as_ptr(),
                        0,
                        4,
                        std::ptr::null_mut(),
                    )
                };
                if detected < 0 {
                    return Err("Native language detection failed".into());
                }
                // SAFETY: a successful language id maps to a process-static string.
                let pointer = unsafe { sys::whisper_lang_str(detected) };
                if pointer.is_null() {
                    return Err("Invalid detected language".into());
                }
                let language = unsafe { CStr::from_ptr(pointer) }
                    .to_str()
                    .map_err(|e| e.to_string())?;
                Some(if language == "ru" { "uk" } else { language }.to_owned())
            }
        };
        let language = language
            .map(CString::new)
            .transpose()
            .map_err(|e| e.to_string())?;
        // SAFETY: native defaults initialize every field. Strings live through
        // the call. n_samples=0 intentionally decodes prepare()'s existing mel;
        // pinned whisper.cpp 1.8.3 supports this path, with token timestamps/VAD
        // disabled here (Silero filtering already ran before prepare()).
        let result = unsafe {
            let mut params = sys::whisper_full_default_params(
                sys::whisper_sampling_strategy_WHISPER_SAMPLING_BEAM_SEARCH,
            );
            params.beam_search.beam_size = i32::from(options.beam_size);
            params.beam_search.patience = 1.;
            params.n_threads = 4;
            params.n_max_text_ctx = 223;
            params.no_context = false;
            params.translate = false;
            params.token_timestamps = false;
            params.vad = false;
            params.print_progress = false;
            params.print_realtime = false;
            params.print_special = false;
            params.print_timestamps = false;
            params.suppress_blank = true;
            params.suppress_nst = true;
            params.temperature = 0.;
            params.temperature_inc = 0.2;
            params.initial_prompt = prompt.as_ptr();
            params.language = language
                .as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr());
            sys::whisper_full_with_state(
                self.engine.pointer.as_ptr(),
                self.pointer.as_ptr(),
                params,
                std::ptr::null(),
                0,
            )
        };
        if result != 0 {
            return Err(format!("Native transcription failed ({result})"));
        }
        self.engine.ensure_gpu()?;
        // SAFETY: live successful state owns segment strings until Session Drop.
        let count = unsafe { sys::whisper_full_n_segments_from_state(self.pointer.as_ptr()) };
        let mut text = String::new();
        for index in 0..count {
            let pointer = unsafe {
                sys::whisper_full_get_segment_text_from_state(self.pointer.as_ptr(), index)
            };
            if pointer.is_null() {
                return Err("Native transcript segment is null".into());
            }
            text.push_str(
                unsafe { CStr::from_ptr(pointer) }
                    .to_str()
                    .map_err(|e| e.to_string())?,
            );
        }
        Ok(text)
    }
}
impl Drop for Session<'_> {
    fn drop(&mut self) {
        // SAFETY: exactly one state owner; its borrowed context is still alive.
        unsafe {
            sys::whisper_free_state(self.pointer.as_ptr());
        }
    }
}
