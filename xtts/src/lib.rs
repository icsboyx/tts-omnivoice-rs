//! Higher-level OmniVoice synthesis using reference voices or voice-design instructions.

use std::{
    collections::HashMap,
    fmt, fs, io,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use xomnivoice::{
    InitOptions, OmniVoice, Reference, SynthesisRequest, VoiceReference, WavReference,
};

pub use xomnivoice::Audio;

const DEFAULT_MODEL_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/models");
const MODEL_FILE: &str = "omnivoice-base-Q8_0.gguf";
const CODEC_FILE: &str = "omnivoice-tokenizer-F32.gguf";

/// `XTTS_MODEL_DIR` at runtime, otherwise `xtts/models` from the build location.
fn model_dir() -> PathBuf {
    std::env::var_os("XTTS_MODEL_DIR")
        .map_or_else(|| PathBuf::from(DEFAULT_MODEL_DIR), PathBuf::from)
}

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    OmniVoice(xomnivoice::Error),
    AudioEffects(timestretch::StretchError),
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Metadata {
        path: PathBuf,
        source: serde_json::Error,
    },
    InvalidMetadata {
        path: PathBuf,
        message: &'static str,
    },
    UnknownVoice(Uuid),
    InvalidInput(&'static str),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::OmniVoice(error) => write!(f, "{error}"),
            Self::AudioEffects(error) => write!(f, "audio effects: {error}"),
            Self::Io { path, source } => write!(f, "{}: {source}", path.display()),
            Self::Metadata { path, source } => {
                write!(f, "invalid voice metadata {}: {source}", path.display())
            }
            Self::InvalidMetadata { path, message } => {
                write!(f, "invalid voice metadata {}: {message}", path.display())
            }
            Self::UnknownVoice(id) => write!(f, "unknown voice: {id}"),
            Self::InvalidInput(message) => write!(f, "invalid input: {message}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::OmniVoice(error) => Some(error),
            Self::AudioEffects(error) => Some(error),
            Self::Io { source, .. } => Some(source),
            Self::Metadata { source, .. } => Some(source),
            _ => None,
        }
    }
}

impl From<xomnivoice::Error> for Error {
    fn from(error: xomnivoice::Error) -> Self {
        Self::OmniVoice(error)
    }
}

impl From<timestretch::StretchError> for Error {
    fn from(error: timestretch::StretchError) -> Self {
        Self::AudioEffects(error)
    }
}

/// Metadata from a voice JSON sidecar.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceProfile {
    pub id: Uuid,
    pub technical_id: Option<String>,
    pub character_name: Option<String>,
    pub language: String,
    pub language_code: String,
    pub gender: Option<String>,
    pub reference_text: String,
    pub speed: f32,
    pub pitch: i32,
}

/// A discovered voice and the WAV recording used as its reference.
#[derive(Debug, Clone)]
pub struct Voice {
    pub path: PathBuf,
    pub profile: VoiceProfile,
}

/// Keeps one OmniVoice model loaded and caches encoded reference voices on first use.
pub struct Xtts {
    model: OmniVoice,
    samples_dir: PathBuf,
    voices: HashMap<Uuid, Voice>,
    voice_cache: HashMap<Uuid, VoiceReference>,
}

impl Xtts {
    /// Loads the model and recursively discovers all UUID JSON sidecars under `samples_dir`.
    /// The GGUF files are read from `XTTS_MODEL_DIR`, or `xtts/models` when it is unset.
    /// `XTTS_DEVICE` selects the device, as described for `InitOptions::device`.
    pub fn new(samples_dir: impl Into<PathBuf>) -> Result<Self> {
        let samples_dir = samples_dir.into();
        let voices = scan_voices(&samples_dir)?;
        let model_dir = model_dir();
        let (Some(model_path), Some(codec_path)) = (
            model_dir.join(MODEL_FILE).to_str().map(str::to_owned),
            model_dir.join(CODEC_FILE).to_str().map(str::to_owned),
        ) else {
            return Err(Error::InvalidInput("model directory must be valid UTF-8"));
        };
        let device = std::env::var("XTTS_DEVICE")
            .ok()
            .filter(|device| !device.is_empty());
        let mut options = InitOptions::new(&model_path, &codec_path);
        options.device = device.as_deref();
        let model = OmniVoice::with_options(options)?;
        Ok(Self {
            model,
            samples_dir,
            voices,
            voice_cache: HashMap::new(),
        })
    }

    /// Uses an already-loaded model, including one configured with custom model paths or device.
    pub fn with_model(model: OmniVoice, samples_dir: impl Into<PathBuf>) -> Result<Self> {
        let samples_dir = samples_dir.into();
        let voices = scan_voices(&samples_dir)?;
        Ok(Self {
            model,
            samples_dir,
            voices,
            voice_cache: HashMap::new(),
        })
    }

    /// Rescans the entire samples tree and invalidates encoded references.
    pub fn reload_voices(&mut self) -> Result<()> {
        self.voices = scan_voices(&self.samples_dir)?;
        self.voice_cache.clear();
        Ok(())
    }

    /// All discovered voices, regardless of their subdirectory.
    pub fn voices(&self) -> &HashMap<Uuid, Voice> {
        &self.voices
    }

    pub fn voice(&self, id: Uuid) -> Option<&Voice> {
        self.voices.get(&id)
    }

    /// Synthesizes with any discovered reference selected by UUID.
    pub fn synthesize_voice(&mut self, id: Uuid, text: &str) -> Result<Audio> {
        self.synthesize_voice_with_options(id, text, None, None)
    }

    /// Synthesizes with optional speed (-10..=10) and pitch (-20..=20 semitones).
    /// Omitted values use the defaults in the voice profile.
    pub fn synthesize_voice_with_options(
        &mut self,
        id: Uuid,
        text: &str,
        speed: Option<f32>,
        pitch: Option<i32>,
    ) -> Result<Audio> {
        if text.trim().is_empty() {
            return Err(Error::InvalidInput("text must not be empty"));
        }
        if self.voice(id).is_none() {
            self.reload_voices()?;
        }
        let voice = self.voices.get(&id).ok_or(Error::UnknownVoice(id))?;
        let speed = speed.unwrap_or(voice.profile.speed);
        let pitch = pitch.unwrap_or(voice.profile.pitch);
        validate_controls(speed, pitch)?;
        if !self.voice_cache.contains_key(&id) {
            let path = voice.path.clone();
            let bytes = fs::read(&path).map_err(|source| Error::Io { path, source })?;
            let decoded = WavReference::from_wav_bytes(bytes, &voice.profile.reference_text)?;
            let Reference::RawAudio {
                samples_24k_mono, ..
            } = decoded.as_reference()
            else {
                unreachable!("WavReference always contains raw audio")
            };
            let codes = self.model.extract_voice_reference(samples_24k_mono)?;
            self.voice_cache.insert(id, codes);
        }

        let mut request = SynthesisRequest::new(text);
        request.language = &voice.profile.language_code;
        request.reference = Some(Reference::Encoded {
            voice: self.voice_cache.get(&id).expect("reference was cached"),
            transcript: &voice.profile.reference_text,
        });
        let audio = self.model.synthesize_with(&request)?;
        apply_controls(audio, speed, pitch)
    }

    /// Synthesizes a designed voice from OmniVoice instructions, without a reference recording.
    pub fn synthesize_custom(
        &mut self,
        text: &str,
        instruction: &str,
        language: Option<&str>,
    ) -> Result<Audio> {
        if text.trim().is_empty() {
            return Err(Error::InvalidInput("text must not be empty"));
        }
        if instruction.trim().is_empty() {
            return Err(Error::InvalidInput("instruction must not be empty"));
        }
        let mut request = SynthesisRequest::new(text);
        request.language = language.unwrap_or("");
        request.instruction = Some(instruction);
        self.model.synthesize_with(&request).map_err(Into::into)
    }
}

fn validate_controls(speed: f32, pitch: i32) -> Result<()> {
    if !speed.is_finite() || !(-10.0..=10.0).contains(&speed) {
        return Err(Error::InvalidInput("speed must be between -10 and 10"));
    }
    if !(-20..=20).contains(&pitch) {
        return Err(Error::InvalidInput(
            "pitch must be between -20 and 20 semitones",
        ));
    }
    Ok(())
}

fn apply_controls(mut audio: Audio, speed: f32, pitch: i32) -> Result<Audio> {
    if speed != 0.0 || pitch != 0 {
        if audio.channels != 1 {
            return Err(Error::InvalidInput("audio effects require mono output"));
        }
        let time_ratio = 10_f64.powf(-f64::from(speed) / 10.0);
        let pitch_factor = 2_f64.powf(f64::from(pitch) / 12.0);
        // One stretch pass sets both duration and pitch. Resampling then
        // restores the requested duration without a second stretch pass.
        let params = timestretch::StretchParams::new(time_ratio * pitch_factor)
            .with_sample_rate(audio.sample_rate)
            .with_channels(u32::from(audio.channels));
        let stretched = timestretch::stretch(&audio.samples, &params)?;
        if pitch == 0 {
            audio.samples = stretched;
        } else {
            let output_len = (audio.samples.len() as f64 * time_ratio).round() as usize;
            audio.samples =
                timestretch::core::resample::resample_sinc_default(&stretched, output_len);
        }
    }
    Ok(audio)
}

fn scan_voices(samples_dir: &Path) -> Result<HashMap<Uuid, Voice>> {
    let mut voices = HashMap::new();
    let mut directories = vec![samples_dir.to_owned()];
    while let Some(directory) = directories.pop() {
        let entries = fs::read_dir(&directory).map_err(|source| Error::Io {
            path: directory.clone(),
            source,
        })?;
        for entry in entries {
            let entry = entry.map_err(|source| Error::Io {
                path: directory.clone(),
                source,
            })?;
            let path = entry.path();
            let file_type = entry.file_type().map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            if file_type.is_dir() {
                directories.push(path);
                continue;
            }
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            if !file_type.is_file() {
                return Err(Error::InvalidMetadata {
                    path,
                    message: "voice JSON must be a regular file",
                });
            }
            let bytes = fs::read(&path).map_err(|source| Error::Io {
                path: path.clone(),
                source,
            })?;
            let sidecar: VoiceProfile =
                serde_json::from_slice(&bytes).map_err(|source| Error::Metadata {
                    path: path.clone(),
                    source,
                })?;
            let id = sidecar.id;
            let expected_stem = id.to_string();
            if path.file_stem().and_then(|stem| stem.to_str()) != Some(expected_stem.as_str()) {
                return Err(Error::InvalidMetadata {
                    path,
                    message: "filename and voice ID differ",
                });
            }
            if sidecar.language_code.trim().is_empty() || sidecar.reference_text.trim().is_empty() {
                return Err(Error::InvalidMetadata {
                    path,
                    message: "language code and reference text must not be empty",
                });
            }
            let wav = path.with_extension("wav");
            if !wav.is_file() || wav.is_symlink() {
                return Err(Error::InvalidMetadata {
                    path,
                    message: "matching UUID WAV file is missing or linked",
                });
            }
            if voices
                .insert(
                    id,
                    Voice {
                        path: wav,
                        profile: sidecar,
                    },
                )
                .is_some()
            {
                return Err(Error::InvalidMetadata {
                    path,
                    message: "duplicate voice UUID",
                });
            }
        }
    }
    Ok(voices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone_power(samples: &[f32], frequency: f64) -> f64 {
        let (sine, cosine) = samples.iter().enumerate().fold(
            (0.0_f64, 0.0_f64),
            |(sine, cosine), (index, sample)| {
                let phase = std::f64::consts::TAU * frequency * index as f64 / 24_000.0;
                (
                    sine + f64::from(*sample) * phase.sin(),
                    cosine + f64::from(*sample) * phase.cos(),
                )
            },
        );
        sine * sine + cosine * cosine
    }

    #[test]
    fn speed_changes_duration_and_pitch_preserves_it() {
        let samples: Vec<f32> = (0..24_000)
            .map(|index| (std::f32::consts::TAU * 440.0 * index as f32 / 24_000.0).sin() * 0.2)
            .collect();
        let audio = Audio {
            samples,
            sample_rate: 24_000,
            channels: 1,
        };

        let faster = apply_controls(audio.clone(), 10.0, 0).unwrap();
        assert_eq!(faster.samples.len(), 2_400);
        let shifted = apply_controls(audio.clone(), 0.0, 12).unwrap();
        assert_eq!(shifted.samples.len(), 24_000);
        assert!(tone_power(&shifted.samples, 880.0) > tone_power(&shifted.samples, 440.0));

        let combined = apply_controls(audio, -1.0, -5).unwrap();
        assert_eq!(combined.samples.len(), 30_214);
        assert!(tone_power(&combined.samples, 329.6) > tone_power(&combined.samples, 440.0));
    }

    #[test]
    fn rejects_invalid_voice_controls() {
        assert!(validate_controls(f32::NAN, 0).is_err());
        assert!(validate_controls(10.1, 0).is_err());
        assert!(validate_controls(0.0, 21).is_err());
    }

    #[test]
    fn discovers_voices_in_any_subdirectory() {
        let root = std::env::temp_dir().join(format!("xtts-voices-{}", Uuid::new_v4()));
        let nested = root.join("custom").join("more");
        fs::create_dir_all(&nested).unwrap();

        let ids = [Uuid::new_v4(), Uuid::new_v4()];
        for (directory, id) in [(&root, ids[0]), (&nested, ids[1])] {
            let profile = VoiceProfile {
                id,
                technical_id: None,
                character_name: None,
                language: "Italian".into(),
                language_code: "it".into(),
                gender: None,
                reference_text: "Ciao".into(),
                speed: 0.0,
                pitch: 0,
            };
            fs::write(
                directory.join(format!("{id}.json")),
                serde_json::to_vec(&profile).unwrap(),
            )
            .unwrap();
            fs::write(directory.join(format!("{id}.wav")), b"test").unwrap();
        }

        let voices = scan_voices(&root).unwrap();
        assert_eq!(voices.len(), 2);
        assert_eq!(voices[&ids[0]].path, root.join(format!("{}.wav", ids[0])));
        assert_eq!(voices[&ids[1]].path, nested.join(format!("{}.wav", ids[1])));

        fs::remove_dir_all(root).unwrap();
    }
}
