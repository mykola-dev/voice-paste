//! Single-thread-owned native handles. Encoded features cannot outlive the engine.
use std::{
    ffi::{CStr, CString, c_char, c_void},
    marker::PhantomData,
    path::Path,
    ptr::NonNull,
    rc::Rc,
};
use voicepaste_core::settings::Device;

unsafe extern "C" {
    fn vp_open(
        path: *const c_char,
        device: i32,
        result: *mut *mut c_void,
        actual: *mut i32,
        mels: *mut usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn vp_close(engine: *mut c_void);
    fn vp_encode(
        engine: *mut c_void,
        values: *const f32,
        count: usize,
        result: *mut *mut c_void,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn vp_encoded_close(encoded: *mut c_void);
    fn vp_language(
        encoded: *mut c_void,
        language: *mut c_char,
        language_capacity: usize,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
    fn vp_generate(
        encoded: *mut c_void,
        prompt: *const u32,
        prompt_count: usize,
        suppressed: *const i32,
        suppressed_count: usize,
        beam: usize,
        temperature: f32,
        output: *mut u32,
        output_capacity: usize,
        output_count: *mut usize,
        score: *mut f32,
        no_speech: *mut f32,
        error: *mut c_char,
        capacity: usize,
    ) -> i32;
}

pub struct Engine {
    handle: NonNull<c_void>,
    pub device: &'static str,
    pub mels: usize,
    _thread: PhantomData<Rc<()>>,
}
pub struct Encoded<'a> {
    handle: NonNull<c_void>,
    _engine: PhantomData<&'a Engine>,
}
pub struct Generation {
    pub ids: Vec<u32>,
    pub score: f32,
    pub no_speech: f32,
}

fn checked(status: i32, error: &[c_char]) -> Result<(), String> {
    if status == 0 {
        return Ok(());
    }
    // SAFETY: all buffers are zero-initialized, nonempty, and native error
    // formatting is bounded and NUL-terminated. No native pointers are retained.
    Err(unsafe { CStr::from_ptr(error.as_ptr()) }
        .to_string_lossy()
        .into_owned())
}

impl Engine {
    pub fn open(path: &Path, device: Device) -> Result<Self, String> {
        let path = std::path::absolute(path).map_err(|e| e.to_string())?;
        let path = CString::new(path.to_str().ok_or("Model path is not UTF-8")?)
            .map_err(|e| e.to_string())?;
        let mut pointer = std::ptr::null_mut();
        let mut actual = 0;
        let mut mels = 0;
        let mut error = [0; 4096];
        let device = match device {
            Device::Cpu => 0,
            Device::CudaOnly => 1,
            Device::CudaAuto => 2,
            Device::MetalOnly => return Err("Metal requires the macOS whisper.cpp backend".into()),
        };
        // SAFETY: buffers live for the synchronous call. Success transfers one
        // native Engine owner to this !Send/!Sync RAII wrapper.
        checked(
            unsafe {
                vp_open(
                    path.as_ptr(),
                    device,
                    &mut pointer,
                    &mut actual,
                    &mut mels,
                    error.as_mut_ptr(),
                    error.len(),
                )
            },
            &error,
        )?;
        let engine = Self {
            handle: NonNull::new(pointer).ok_or("Native engine returned null")?,
            device: if actual == 0 { "cpu" } else { "cuda" },
            mels,
            _thread: PhantomData,
        };
        if !matches!(mels, 80 | 128) {
            return Err("Unsupported Whisper mel layout".into());
        }
        Ok(engine)
    }
    pub fn encode(&self, features: &[f32]) -> Result<Encoded<'_>, String> {
        let mut pointer = std::ptr::null_mut();
        let mut error = [0; 4096];
        // SAFETY: features remain live and are copied synchronously. Native
        // adapter validates size/finite values and retains no Rust pointers.
        checked(
            unsafe {
                vp_encode(
                    self.handle.as_ptr(),
                    features.as_ptr(),
                    features.len(),
                    &mut pointer,
                    error.as_mut_ptr(),
                    error.len(),
                )
            },
            &error,
        )?;
        Ok(Encoded {
            handle: NonNull::new(pointer).ok_or("Native encoder returned null")?,
            _engine: PhantomData,
        })
    }
}
impl Encoded<'_> {
    pub fn language(&self) -> Result<String, String> {
        let mut output = [0; 32];
        let mut error = [0; 4096];
        // SAFETY: native owned encoded handle is live; both output buffers are bounded.
        checked(
            unsafe {
                vp_language(
                    self.handle.as_ptr(),
                    output.as_mut_ptr(),
                    output.len(),
                    error.as_mut_ptr(),
                    error.len(),
                )
            },
            &error,
        )?;
        // SAFETY: native adapter always NUL-terminates output on success.
        Ok(unsafe { CStr::from_ptr(output.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }
    pub fn generate(
        &self,
        prompt: &[u32],
        suppressed: &[i32],
        beam: u8,
        temperature: f32,
    ) -> Result<Generation, String> {
        let mut ids = vec![0; 448];
        let mut count = 0;
        let mut score = 0.0;
        let mut no_speech = 0.0;
        let mut error = [0; 4096];
        // SAFETY: synchronous call; all input slices and output buffers remain
        // valid, native code checks lengths and catches all C++ exceptions.
        checked(
            unsafe {
                vp_generate(
                    self.handle.as_ptr(),
                    prompt.as_ptr(),
                    prompt.len(),
                    suppressed.as_ptr(),
                    suppressed.len(),
                    beam as usize,
                    temperature,
                    ids.as_mut_ptr(),
                    ids.len(),
                    &mut count,
                    &mut score,
                    &mut no_speech,
                    error.as_mut_ptr(),
                    error.len(),
                )
            },
            &error,
        )?;
        if count > ids.len() {
            return Err("Native token count exceeds capacity".into());
        }
        ids.truncate(count);
        Ok(Generation {
            ids,
            score,
            no_speech,
        })
    }
}
impl Drop for Engine {
    fn drop(&mut self) {
        /* SAFETY: uniquely owned native handle. */
        unsafe {
            vp_close(self.handle.as_ptr());
        }
    }
}
impl Drop for Encoded<'_> {
    fn drop(&mut self) {
        /* SAFETY: unique handle, engine borrow remains live. */
        unsafe {
            vp_encoded_close(self.handle.as_ptr());
        }
    }
}
