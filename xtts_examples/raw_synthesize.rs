use std::{error::Error, fs, path::Path};

use uuid::Uuid;
use xomnivoice::{OmniVoice, SynthesisRequest, WavReference};

// Example code that deserializes and serializes the model.
// extern crate serde;
// #[macro_use]
// extern crate serde_derive;
// extern crate serde_json;
//
// use generated_module::UploadedVoices;
//
// fn main() {
//     let json = r#"{"answer": 42}"#;
//     let model: UploadedVoices = serde_json::from_str(&json).unwrap();
// }

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub type UploadedVoices = HashMap<Uuid, UploadedVoicesValue>;

#[derive(Serialize, Deserialize)]
pub struct UploadedVoicesValue {
    language_code: String,
    reference_text: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let models_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../xtts/models");
    let model_path = models_dir.join("omnivoice-base-Q8_0.gguf");
    let codec_path = models_dir.join("omnivoice-tokenizer-F32.gguf");
    let mut voice =
        OmniVoice::with_paths(model_path.to_str().unwrap(), codec_path.to_str().unwrap())?;
    // let audio = voice.synthesize(
    //     "Ciao a tutti!",
    //     "Italian",
    //     Some("female, young adult, moderate pitch"),
    // )?;

    let _voices = UploadedVoices::default();

    let sample_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtts_examples is inside local_tts")
        .join("xtts/samples/it/32460c45-4bcc-4266-8920-582bbbb4824a.wav");
    let reference_audio = fs::read(sample_path)?;
    let reference_audio_transcript = "Auspicabilmente, la maggior parte delle persone comprendera che la scelta migliore anche per loro nel lungo periodo e collaborare con gli altri.";
    let reference = WavReference::from_wav_bytes(reference_audio, reference_audio_transcript)?;
    let mut synthesis_request =
        SynthesisRequest::new("Un testo un po piu lungo 1,2,3,4,5,6,7,8,9,0");
    synthesis_request.reference = Some(reference.as_reference());
    let audio = voice.synthesize_with(&synthesis_request)?;

    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let output = std::env::temp_dir().join("prova_rust.wav");
    let mut wav = hound::WavWriter::create(&output, spec)?;
    for sample in audio.samples {
        wav.write_sample(sample)?;
    }
    wav.finalize()?;
    println!("Saved {}", output.display());
    Ok(())
}
