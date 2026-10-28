use std::io::Cursor;

use xomnivoice::{Reference, WavReference};

fn wav_bytes(sample_rate: u32, channels: u16) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut bytes = Cursor::new(Vec::new());
    let mut writer = hound::WavWriter::new(&mut bytes, spec).unwrap();
    writer.write_sample(16_384_i16).unwrap();
    writer.write_sample(-16_384_i16).unwrap();
    writer.finalize().unwrap();
    bytes.into_inner()
}

#[test]
fn decodes_owned_mono_reference() {
    let wav = wav_bytes(24_000, 1);
    let reference = WavReference::from_wav_bytes(&wav, "Ciao").unwrap();
    drop(wav);
    let Reference::RawAudio {
        samples_24k_mono,
        transcript,
    } = reference.as_reference()
    else {
        panic!("expected raw audio");
    };
    assert_eq!(transcript, "Ciao");
    assert_eq!(samples_24k_mono.len(), 2);
    assert!((samples_24k_mono[0] - 0.5).abs() < 0.001);
    assert!((samples_24k_mono[1] + 0.5).abs() < 0.001);
}

#[test]
fn rejects_wrong_audio_layout() {
    for (rate, channels) in [(48_000, 1), (24_000, 2)] {
        let wav = wav_bytes(rate, channels);
        assert!(WavReference::from_wav_bytes(wav, "Ciao").is_err());
    }
}
