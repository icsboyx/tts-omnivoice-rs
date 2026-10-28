//! Rust interface to the OmniVoice C ABI.
//!
//! A model handle is synchronous and owns GPU resources. Keep it alive across
//! requests and use a dedicated worker thread when calling it from async code.

use std::{
    any::Any,
    ffi::{CStr, CString, NulError, c_char, c_void},
    fmt,
    io::Cursor,
    marker::PhantomData,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::Path,
    ptr::NonNull,
    rc::Rc,
    sync::{Arc, OnceLock, RwLock},
};

/// The complete generated C API, for operations that need raw ABI access.
/// All functions in this module are unsafe.
#[allow(
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    dead_code
)]
pub mod raw {
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}
use raw as ffi;

/// GGML device registry, used to resolve `InitOptions::device`. It is not part of the
/// OmniVoice ABI; these layouts match the GGML commit listed in the README.
mod ggml {
    use std::ffi::{c_char, c_int, c_void};

    #[repr(C)]
    pub struct DevCaps {
        pub async_ops: bool,
        pub host_buffer: bool,
        pub buffer_from_host_ptr: bool,
        pub events: bool,
        pub mmap_support: bool,
    }

    #[repr(C)]
    pub struct DevProps {
        pub name: *const c_char,
        pub description: *const c_char,
        pub memory_free: usize,
        pub memory_total: usize,
        pub device_type: c_int,
        pub device_id: *const c_char,
        pub caps: DevCaps,
    }

    unsafe extern "C" {
        pub fn ggml_backend_dev_count() -> usize;
        pub fn ggml_backend_dev_get(index: usize) -> *mut c_void;
        pub fn ggml_backend_dev_get_props(device: *mut c_void, props: *mut DevProps);
    }
}

/// The only OmniVoice commit this crate supports. The bundled C header comes from it.
pub const SUPPORTED_NATIVE_COMMIT: &str = "1a468d8b9d65b3544ca1004d2d7c05dfc65d1f3e";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Nul(NulError),
    Decode(hound::Error),
    InvalidInput(&'static str),
    InvalidAudio,
    UnsupportedNative {
        version: String,
    },
    Native {
        status: Option<i32>,
        message: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Nul(_) => write!(f, "input contains a NUL byte"),
            Self::Decode(error) => write!(f, "cannot decode reference audio: {error}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
            Self::InvalidAudio => write!(f, "OmniVoice returned an invalid audio buffer"),
            Self::UnsupportedNative { version } => write!(
                f,
                "unsupported OmniVoice library {version:?}; build commit {SUPPORTED_NATIVE_COMMIT}"
            ),
            Self::Native { status, message } => {
                if let Some(status) = status {
                    write!(f, "OmniVoice error {status}: {message}")
                } else {
                    write!(f, "OmniVoice: {message}")
                }
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Nul(error) => Some(error),
            Self::Decode(error) => Some(error),
            _ => None,
        }
    }
}

impl From<NulError> for Error {
    fn from(error: NulError) -> Self {
        Self::Nul(error)
    }
}

impl From<hound::Error> for Error {
    fn from(error: hound::Error) -> Self {
        Self::Decode(error)
    }
}

/// Interleaved floating-point PCM. OmniVoice currently returns 24 kHz mono.
#[derive(Debug, Clone)]
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// A language accepted by the native synthesis API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Language {
    pub id: String,
    pub name: String,
}

/// Returns the native library build identity.
pub fn version() -> String {
    native_string(unsafe { ffi::ov_version() })
}

/// Returns all language identifiers and display names.
pub fn languages() -> Vec<Language> {
    let count = unsafe { ffi::ov_n_languages() }.max(0);
    (0..count)
        .map(|index| Language {
            id: native_string(unsafe { ffi::ov_language_id(index) }),
            name: native_string(unsafe { ffi::ov_language_name(index) }),
        })
        .collect()
}

/// A compute device registered in the native GGML backend.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    pub name: String,
    pub description: String,
    /// Lower-case PCI bus ID such as "0000:01:00.0", when the backend reports one.
    pub pci_bus_id: Option<String>,
}

/// Lists the devices compiled into the native library, such as "CUDA0" and "CPU".
pub fn devices() -> Vec<Device> {
    let count = unsafe { ggml::ggml_backend_dev_count() };
    (0..count)
        .filter_map(|index| {
            let device = unsafe { ggml::ggml_backend_dev_get(index) };
            if device.is_null() {
                return None;
            }
            let mut props = unsafe { std::mem::zeroed::<ggml::DevProps>() };
            unsafe { ggml::ggml_backend_dev_get_props(device, &mut props) };
            let pci_bus_id = native_string(props.device_id);
            Some(Device {
                name: native_string(props.name),
                description: native_string(props.description),
                pci_bus_id: (!pci_bus_id.is_empty()).then(|| pci_bus_id.to_ascii_lowercase()),
            })
        })
        .collect()
}

/// Options used while loading the model and codec.
#[derive(Debug, Clone)]
pub struct InitOptions<'a> {
    pub model_path: &'a str,
    pub codec_path: Option<&'a str>,
    /// A GGML device name ("CUDA0", "CPU") or a DRM node ("/dev/dri/renderD129"), matched
    /// by PCI bus ID. None, or a device that matches nothing, selects the best available one.
    pub device: Option<&'a str>,
    pub use_flash_attention: bool,
    pub clamp_fp16: bool,
}

impl<'a> InitOptions<'a> {
    pub fn new(model_path: &'a str, codec_path: &'a str) -> Self {
        Self {
            model_path,
            codec_path: Some(codec_path),
            device: None,
            use_flash_attention: true,
            clamp_fp16: false,
        }
    }
}

/// All seven MaskGIT generation controls exposed by the C ABI.
#[derive(Debug, Clone, Copy)]
pub struct MaskgitOptions {
    pub steps: i32,
    pub guidance_scale: f32,
    pub t_shift: f32,
    pub layer_penalty_factor: f32,
    pub position_temperature: f32,
    pub class_temperature: f32,
    pub seed: u64,
}

impl Default for MaskgitOptions {
    fn default() -> Self {
        Self {
            steps: 32,
            guidance_scale: 2.0,
            t_shift: 0.1,
            layer_penalty_factor: 5.0,
            position_temperature: 5.0,
            class_temperature: 0.0,
            seed: 42,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DurationOverride {
    Frames(i32),
    Seconds(f32),
}

/// Encoded voice reference: codebooks by frames, with frames varying fastest.
#[derive(Debug, Clone)]
pub struct VoiceReference {
    codes: Vec<i32>,
    frames: i32,
    codebooks: i32,
}

impl VoiceReference {
    pub fn from_codes(codes: Vec<i32>, frames: i32, codebooks: i32) -> Result<Self> {
        let expected = usize::try_from(frames).ok().and_then(|frames| {
            usize::try_from(codebooks)
                .ok()
                .and_then(|books| frames.checked_mul(books))
        });
        if expected.is_none() || expected == Some(0) || expected != Some(codes.len()) {
            return Err(Error::InvalidInput(
                "voice reference code dimensions do not match",
            ));
        }
        Ok(Self {
            codes,
            frames,
            codebooks,
        })
    }

    pub fn codes(&self) -> &[i32] {
        &self.codes
    }

    pub fn frames(&self) -> i32 {
        self.frames
    }

    pub fn codebooks(&self) -> i32 {
        self.codebooks
    }
}

/// A reference voice can be provided as 24 kHz mono PCM or pre-encoded codes.
#[derive(Debug, Clone, Copy)]
pub enum Reference<'a> {
    RawAudio {
        samples_24k_mono: &'a [f32],
        transcript: &'a str,
    },
    Encoded {
        voice: &'a VoiceReference,
        transcript: &'a str,
    },
}

/// Owns a decoded 24 kHz mono WAV and its transcript for use as a reference.
#[derive(Debug, Clone)]
pub struct WavReference {
    samples: Vec<f32>,
    transcript: String,
}

impl WavReference {
    pub fn from_wav_bytes(bytes: impl AsRef<[u8]>, transcript: impl Into<String>) -> Result<Self> {
        let reader = hound::WavReader::new(Cursor::new(bytes.as_ref()))?;
        let spec = reader.spec();
        if spec.channels != 1 || spec.sample_rate != 24_000 {
            return Err(Error::InvalidInput("reference WAV must be mono at 24 kHz"));
        }
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .into_samples::<f32>()
                .collect::<std::result::Result<_, _>>()?,
            hound::SampleFormat::Int => {
                // Scale signed integers to [-1, 1), as 2^(bits - 1) is full scale.
                let scale = 1.0 / (1_u64 << (spec.bits_per_sample - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .map(|sample| sample.map(|sample| sample as f32 * scale))
                    .collect::<std::result::Result<_, _>>()?
            }
        };
        if samples.is_empty() {
            return Err(Error::InvalidInput("reference WAV is empty"));
        }
        Ok(Self {
            samples,
            transcript: transcript.into(),
        })
    }

    pub fn as_reference(&self) -> Reference<'_> {
        Reference::RawAudio {
            samples_24k_mono: &self.samples,
            transcript: &self.transcript,
        }
    }
}

/// Parameters for one buffered or streaming synthesis.
#[derive(Debug, Clone)]
pub struct SynthesisRequest<'a> {
    pub text: &'a str,
    pub language: &'a str,
    pub instruction: Option<&'a str>,
    pub duration: Option<DurationOverride>,
    pub chunk_duration_sec: f32,
    pub chunk_threshold_sec: f32,
    pub denoise: bool,
    pub preprocess_prompt: bool,
    pub maskgit: MaskgitOptions,
    pub reference: Option<Reference<'a>>,
    pub dump_dir: Option<&'a str>,
    /// Applies only to buffered output. Streaming always post-processes.
    pub postprocess: bool,
}

impl<'a> SynthesisRequest<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            language: "",
            instruction: None,
            duration: None,
            chunk_duration_sec: 15.0,
            chunk_threshold_sec: 30.0,
            denoise: true,
            preprocess_prompt: true,
            maskgit: MaskgitOptions::default(),
            reference: None,
            dump_dir: None,
            postprocess: true,
        }
    }
}

/// Owns the native model context and frees it on drop.
///
/// The native API is blocking. This type is intentionally neither Send nor
/// Sync; create and use it on the same dedicated thread.
pub struct OmniVoice {
    context: NonNull<ffi::ov_context>,
    _not_send_or_sync: PhantomData<Rc<()>>,
}

impl OmniVoice {
    pub fn with_paths(model_path: &str, codec_path: &str) -> Result<Self> {
        Self::with_options(InitOptions::new(model_path, codec_path))
    }

    /// Fails with `Error::UnsupportedNative` unless the loaded library was built from
    /// `SUPPORTED_NATIVE_COMMIT`.
    pub fn with_options(options: InitOptions<'_>) -> Result<Self> {
        check_native_version()?;
        let model_path = CString::new(options.model_path)?;
        let codec_path = options.codec_path.map(CString::new).transpose()?;
        let device = options
            .device
            .and_then(|device| {
                let available = devices();
                let resolved = resolve_device(device, &available);
                if resolved.is_none() {
                    let names: Vec<_> = available.iter().map(|d| d.name.as_str()).collect();
                    log_warning(&format!(
                        "device {device:?} matches none of {names:?}; using the best available device"
                    ));
                }
                resolved
            })
            .map(CString::new)
            .transpose()?;
        let mut params = unsafe { std::mem::zeroed::<ffi::ov_init_params>() };
        unsafe { ffi::ov_init_default_params(&mut params) };
        params.model_path = model_path.as_ptr();
        params.codec_path = codec_path.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        params.device = device.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
        params.use_fa = options.use_flash_attention;
        params.clamp_fp16 = options.clamp_fp16;
        let context =
            NonNull::new(unsafe { ffi::ov_init(&params) }).ok_or_else(|| native_error(None))?;
        Ok(Self {
            context,
            _not_send_or_sync: PhantomData,
        })
    }

    /// Converts seconds to codec frames. The model must have a loaded codec.
    pub fn duration_to_frames(&self, seconds: f32) -> Result<i32> {
        if !seconds.is_finite() || seconds <= 0.0 || seconds > 86_400.0 {
            return Err(Error::InvalidInput(
                "duration must be a finite positive number of seconds",
            ));
        }
        let frames = unsafe { ffi::ov_duration_sec_to_tokens(self.context.as_ptr(), seconds) };
        if frames < 1 { Err(native_error(None)) } else { Ok(frames) }
    }

    pub fn num_codebooks(&self) -> i32 {
        unsafe { ffi::ov_num_codebooks(self.context.as_ptr()) }
    }

    /// Encodes reference speech once so it can be reused for later requests.
    /// Samples must be mono floating-point PCM at 24 kHz.
    pub fn extract_voice_reference(&mut self, samples_24k_mono: &[f32]) -> Result<VoiceReference> {
        if samples_24k_mono.is_empty() || samples_24k_mono.len() > i32::MAX as usize {
            return Err(Error::InvalidInput(
                "reference audio must contain 1..i32::MAX samples",
            ));
        }
        let mut native = NativeVoiceRef(unsafe { std::mem::zeroed::<ffi::ov_voice_ref>() });
        let status = unsafe {
            ffi::ov_extract_voice_ref(
                self.context.as_ptr(),
                samples_24k_mono.as_ptr(),
                samples_24k_mono.len() as i32,
                &mut native.0,
            )
        };
        check_status(status)?;
        if native.0.ref_codes.is_null() || native.0.ref_T <= 0 || native.0.num_codebooks <= 0 {
            return Err(Error::InvalidInput("native voice reference is empty"));
        }
        let count = (native.0.ref_T as usize)
            .checked_mul(native.0.num_codebooks as usize)
            .ok_or(Error::InvalidInput("native voice reference is too large"))?;
        let codes = unsafe { std::slice::from_raw_parts(native.0.ref_codes, count) }.to_vec();
        VoiceReference::from_codes(codes, native.0.ref_T, native.0.num_codebooks)
    }

    /// Simple voice-design synthesis with automatic duration.
    pub fn synthesize(
        &mut self,
        text: &str,
        language: &str,
        instruction: Option<&str>,
    ) -> Result<Audio> {
        let mut request = SynthesisRequest::new(text);
        request.language = language;
        request.instruction = instruction;
        self.synthesize_with(&request)
    }

    pub fn synthesize_with(&mut self, request: &SynthesisRequest<'_>) -> Result<Audio> {
        self.synthesize_cancellable(request, None)
    }

    /// Cancellation is polled by the native pipeline between long-form chunks.
    pub fn synthesize_cancellable(
        &mut self,
        request: &SynthesisRequest<'_>,
        cancel: Option<&mut dyn FnMut() -> bool>,
    ) -> Result<Audio> {
        let context = self.context.as_ptr();
        with_native_params(context, request, |params| {
            let mut cancellation = cancel.map(CancelState::new);
            if let Some(state) = cancellation.as_mut() {
                params.cancel = Some(cancel_trampoline);
                params.cancel_user_data = (state as *mut CancelState<'_>).cast();
            }
            let mut audio = NativeAudio(unsafe { std::mem::zeroed::<ffi::ov_audio>() });
            let status = unsafe { ffi::ov_synthesize(context, params, &mut audio.0) };
            propagate_cancel_panic(&mut cancellation);
            check_status(status)?;
            if audio.0.n_samples <= 0
                || audio.0.samples.is_null()
                || audio.0.sample_rate <= 0
                || audio.0.channels <= 0
            {
                return Err(Error::InvalidAudio);
            }
            let samples = unsafe {
                std::slice::from_raw_parts(audio.0.samples, audio.0.n_samples as usize).to_vec()
            };
            Ok(Audio {
                samples,
                sample_rate: audio.0.sample_rate as u32,
                channels: audio.0.channels as u16,
            })
        })
    }

    /// Emits borrowed 24 kHz mono PCM chunks. Return false to cancel.
    pub fn stream(
        &mut self,
        request: &SynthesisRequest<'_>,
        on_chunk: &mut dyn FnMut(&[f32]) -> bool,
    ) -> Result<()> {
        self.stream_cancellable(request, on_chunk, None)
    }

    pub fn stream_cancellable(
        &mut self,
        request: &SynthesisRequest<'_>,
        on_chunk: &mut dyn FnMut(&[f32]) -> bool,
        cancel: Option<&mut dyn FnMut() -> bool>,
    ) -> Result<()> {
        let context = self.context.as_ptr();
        with_native_params(context, request, |params| {
            let mut chunk = ChunkState::new(on_chunk);
            let mut cancellation = cancel.map(CancelState::new);
            params.on_chunk = Some(chunk_trampoline);
            params.on_chunk_user_data = (&mut chunk as *mut ChunkState<'_>).cast();
            if let Some(state) = cancellation.as_mut() {
                params.cancel = Some(cancel_trampoline);
                params.cancel_user_data = (state as *mut CancelState<'_>).cast();
            }
            let status = unsafe { ffi::ov_synthesize(context, params, std::ptr::null_mut()) };
            if let Some(panic) = chunk.panic.take() {
                resume_unwind(panic);
            }
            propagate_cancel_panic(&mut cancellation);
            check_status(status)
        })
    }
}

impl Drop for OmniVoice {
    fn drop(&mut self) {
        unsafe { ffi::ov_free(self.context.as_ptr()) };
    }
}

struct NativeAudio(ffi::ov_audio);
impl Drop for NativeAudio {
    fn drop(&mut self) {
        unsafe { ffi::ov_audio_free(&mut self.0) };
    }
}

struct NativeVoiceRef(ffi::ov_voice_ref);
impl Drop for NativeVoiceRef {
    fn drop(&mut self) {
        unsafe { ffi::ov_voice_ref_free(&mut self.0) };
    }
}

fn with_native_params<T>(
    context: *mut ffi::ov_context,
    request: &SynthesisRequest<'_>,
    run: impl FnOnce(&mut ffi::ov_tts_params) -> Result<T>,
) -> Result<T> {
    if request.text.is_empty() {
        return Err(Error::InvalidInput("text must not be empty"));
    }
    if !request.chunk_duration_sec.is_finite()
        || !request.chunk_threshold_sec.is_finite()
        || request.chunk_threshold_sec < 0.0
    {
        return Err(Error::InvalidInput("chunk settings must be finite"));
    }
    let m = request.maskgit;
    if m.steps < 1
        || !m.guidance_scale.is_finite()
        || !m.t_shift.is_finite()
        || !m.layer_penalty_factor.is_finite()
        || !m.position_temperature.is_finite()
        || !m.class_temperature.is_finite()
    {
        return Err(Error::InvalidInput(
            "MaskGIT settings must be finite and steps positive",
        ));
    }
    let text = CString::new(request.text)?;
    let language = CString::new(request.language)?;
    let instruction = request.instruction.map(CString::new).transpose()?;
    let dump_dir = request.dump_dir.map(CString::new).transpose()?;
    let mut params = unsafe { std::mem::zeroed::<ffi::ov_tts_params>() };
    unsafe { ffi::ov_tts_default_params(&mut params) };
    params.text = text.as_ptr();
    params.lang = language.as_ptr();
    params.instruct = instruction
        .as_ref()
        .map_or(std::ptr::null(), |v| v.as_ptr());
    params.dump_dir = dump_dir.as_ref().map_or(std::ptr::null(), |v| v.as_ptr());
    params.T_override = match request.duration {
        None => 0,
        Some(DurationOverride::Frames(frames)) if frames > 0 => frames,
        Some(DurationOverride::Seconds(seconds))
            if seconds.is_finite() && seconds > 0.0 && seconds <= 86_400.0 =>
        {
            let frames = unsafe { ffi::ov_duration_sec_to_tokens(context, seconds) };
            if frames < 1 {
                return Err(native_error(None));
            }
            frames
        }
        _ => return Err(Error::InvalidInput("duration must be positive and finite")),
    };
    params.chunk_duration_sec = request.chunk_duration_sec;
    params.chunk_threshold_sec = request.chunk_threshold_sec;
    params.denoise = request.denoise;
    params.preprocess_prompt = request.preprocess_prompt;
    params.mg_num_step = m.steps;
    params.mg_guidance_scale = m.guidance_scale;
    params.mg_t_shift = m.t_shift;
    params.mg_layer_penalty_factor = m.layer_penalty_factor;
    params.mg_position_temperature = m.position_temperature;
    params.mg_class_temperature = m.class_temperature;
    params.mg_seed = m.seed;
    params.postproc = request.postprocess;

    let reference_text = match request.reference {
        None => None,
        Some(Reference::RawAudio {
            samples_24k_mono,
            transcript,
        }) => {
            if samples_24k_mono.is_empty() || samples_24k_mono.len() > i32::MAX as usize {
                return Err(Error::InvalidInput(
                    "reference audio must contain 1..i32::MAX samples",
                ));
            }
            params.ref_audio_24k = samples_24k_mono.as_ptr();
            params.ref_n_samples = samples_24k_mono.len() as i32;
            Some(CString::new(transcript)?)
        }
        Some(Reference::Encoded { voice, transcript }) => {
            let model_codebooks = unsafe { ffi::ov_num_codebooks(context) };
            if voice.codebooks != model_codebooks {
                return Err(Error::InvalidInput(
                    "reference codebook count does not match model",
                ));
            }
            params.ref_audio_tokens = voice.codes.as_ptr();
            params.ref_T = voice.frames;
            Some(CString::new(transcript)?)
        }
    };
    if request.reference.is_some() && reference_text.as_ref().is_some_and(|text| text.is_empty()) {
        return Err(Error::InvalidInput(
            "reference transcript must not be empty",
        ));
    }
    params.ref_text = reference_text
        .as_ref()
        .map_or(std::ptr::null(), |v| v.as_ptr());
    run(&mut params)
}

/// Returns the GGML name for a device name (case-insensitive) or a DRM node path.
fn resolve_device(device: &str, available: &[Device]) -> Option<String> {
    let found = if device.contains('/') {
        let bus_id = drm_pci_bus_id(Path::new(device))?;
        available
            .iter()
            .find(|d| d.pci_bus_id.as_deref() == Some(bus_id.as_str()))
    } else {
        available
            .iter()
            .find(|d| d.name.eq_ignore_ascii_case(device))
    };
    found.map(|d| d.name.clone())
}

/// "/dev/dri/renderD129" -> "0000:01:00.0", through /sys/class/drm/<node>/device.
fn drm_pci_bus_id(node: &Path) -> Option<String> {
    let node = node.canonicalize().ok()?;
    let sysfs = Path::new("/sys/class/drm")
        .join(node.file_name()?)
        .join("device")
        .canonicalize()
        .ok()?;
    Some(sysfs.file_name()?.to_str()?.to_ascii_lowercase())
}

/// Sends a warning to the installed log callback, or to stderr like native logs.
fn log_warning(message: &str) {
    let callback = LOG_CALLBACK.get().and_then(|slot| {
        slot.read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    });
    match callback {
        Some(callback) => callback(LogLevel::Warn, message),
        None => eprintln!("[xomnivoice] {message}"),
    }
}

/// The library reports "<short hash> (<date>)". The struct ABI version is not exposed at
/// runtime, so the commit is the only reliable compatibility check.
fn check_native_version() -> Result<()> {
    let version = version();
    let commit = version.split_whitespace().next().unwrap_or_default();
    if commit.len() >= 7 && SUPPORTED_NATIVE_COMMIT.starts_with(commit) {
        Ok(())
    } else {
        Err(Error::UnsupportedNative { version })
    }
}

fn check_status(status: ffi::ov_status) -> Result<()> {
    if status == ffi::ov_status_OV_STATUS_OK { Ok(()) } else { Err(native_error(Some(status))) }
}

fn native_error(status: Option<i32>) -> Error {
    let message = native_string(unsafe { ffi::ov_last_error() });
    Error::Native {
        status,
        message: if message.is_empty() { "unknown native error".to_owned() } else { message },
    }
}

fn native_string(pointer: *const c_char) -> String {
    if pointer.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(pointer) }
        .to_string_lossy()
        .into_owned()
}

struct CancelState<'a> {
    callback: &'a mut dyn FnMut() -> bool,
    panic: Option<Box<dyn Any + Send>>,
}
impl<'a> CancelState<'a> {
    fn new(callback: &'a mut dyn FnMut() -> bool) -> Self {
        Self {
            callback,
            panic: None,
        }
    }
}
unsafe extern "C" fn cancel_trampoline(data: *mut c_void) -> bool {
    if data.is_null() {
        return true;
    }
    let state = unsafe { &mut *(data as *mut CancelState<'_>) };
    if state.panic.is_some() {
        return true;
    }
    match catch_unwind(AssertUnwindSafe(|| (state.callback)())) {
        Ok(cancel) => cancel,
        Err(panic) => {
            state.panic = Some(panic);
            true
        }
    }
}
fn propagate_cancel_panic(state: &mut Option<CancelState<'_>>) {
    if let Some(panic) = state.as_mut().and_then(|state| state.panic.take()) {
        resume_unwind(panic);
    }
}

struct ChunkState<'a> {
    callback: &'a mut dyn FnMut(&[f32]) -> bool,
    panic: Option<Box<dyn Any + Send>>,
}
impl<'a> ChunkState<'a> {
    fn new(callback: &'a mut dyn FnMut(&[f32]) -> bool) -> Self {
        Self {
            callback,
            panic: None,
        }
    }
}
unsafe extern "C" fn chunk_trampoline(samples: *const f32, count: i32, data: *mut c_void) -> bool {
    if data.is_null() || samples.is_null() || count < 0 {
        return false;
    }
    let state = unsafe { &mut *(data as *mut ChunkState<'_>) };
    if state.panic.is_some() {
        return false;
    }
    let audio = unsafe { std::slice::from_raw_parts(samples, count as usize) };
    match catch_unwind(AssertUnwindSafe(|| (state.callback)(audio))) {
        Ok(keep_going) => keep_going,
        Err(panic) => {
            state.panic = Some(panic);
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
    Unknown(u32),
}
impl From<ffi::ov_log_level> for LogLevel {
    fn from(level: ffi::ov_log_level) -> Self {
        match level {
            ffi::ov_log_level_OV_LOG_DEBUG => Self::Debug,
            ffi::ov_log_level_OV_LOG_INFO => Self::Info,
            ffi::ov_log_level_OV_LOG_WARN => Self::Warn,
            ffi::ov_log_level_OV_LOG_ERROR => Self::Error,
            other => Self::Unknown(other),
        }
    }
}

pub type LogCallback = Arc<dyn Fn(LogLevel, &str) + Send + Sync + 'static>;
static LOG_CALLBACK: OnceLock<RwLock<Option<LogCallback>>> = OnceLock::new();

/// Installs a process-wide logging callback. None restores native stderr.
/// A panicking callback is caught and ignored at the C boundary.
pub fn set_log_callback(callback: Option<LogCallback>) {
    let slot = LOG_CALLBACK.get_or_init(|| RwLock::new(None));
    *slot
        .write()
        .unwrap_or_else(|poisoned| poisoned.into_inner()) = callback;
    let installed = slot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some();
    unsafe {
        ffi::ov_log_set(
            if installed { Some(log_trampoline) } else { None },
            std::ptr::null_mut(),
        );
    }
}

unsafe extern "C" fn log_trampoline(
    level: ffi::ov_log_level,
    message: *const c_char,
    _: *mut c_void,
) {
    let Some(slot) = LOG_CALLBACK.get() else { return };
    let callback = slot
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    if let Some(callback) = callback {
        let text = native_string(message);
        let _ = catch_unwind(AssertUnwindSafe(|| callback(level.into(), &text)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(name: &str, pci_bus_id: Option<&str>) -> Device {
        Device {
            name: name.into(),
            description: String::new(),
            pci_bus_id: pci_bus_id.map(Into::into),
        }
    }

    #[test]
    fn resolves_device_names_case_insensitively() {
        let available = [device("CUDA0", Some("0000:01:00.0")), device("CPU", None)];
        assert_eq!(resolve_device("cpu", &available).as_deref(), Some("CPU"));
        assert_eq!(
            resolve_device("CUDA0", &available).as_deref(),
            Some("CUDA0")
        );
        assert_eq!(resolve_device("Vulkan0", &available), None);
        assert_eq!(resolve_device("/dev/dri/missing", &available), None);
    }
}
