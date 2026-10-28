use std::{
    env,
    error::Error,
    path::{Path, PathBuf},
};

use uuid::Uuid;
use xtts::Xtts;

fn main() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    let mode = args.next().ok_or(
        "usage: synthesize voice <UUID> <text> | synthesize custom <instruction> <text> [language]",
    )?;
    let samples_dir = env::var_os("XTTS_SAMPLES_DIR").map_or_else(
        || {
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("xtts is inside local_tts")
                .join("xtts/samples")
        },
        PathBuf::from,
    );
    let mut tts = Xtts::new(samples_dir)?;
    let audio = match mode.as_str() {
        "voice" => {
            let id = Uuid::parse_str(&args.next().ok_or("missing UUID")?)?;
            let text = args.next().ok_or("missing text")?;
            tts.synthesize_voice(id, &text)?
        }
        "custom" => {
            let instruction = args.next().ok_or("missing instruction")?;
            let text = args.next().ok_or("missing text")?;
            let language = args.next();
            tts.synthesize_custom(&text, &instruction, language.as_deref())?
        }
        _ => return Err("mode must be voice or custom".into()),
    };

    let spec = hound::WavSpec {
        channels: audio.channels,
        sample_rate: audio.sample_rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let output = env::temp_dir().join("prova_xtts.wav");
    let mut wav = hound::WavWriter::create(&output, spec)?;
    for sample in audio.samples {
        wav.write_sample(sample)?;
    }
    wav.finalize()?;
    println!("Saved {}", output.display());
    Ok(())
}
