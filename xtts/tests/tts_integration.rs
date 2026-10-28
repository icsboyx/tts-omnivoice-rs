use std::path::Path;

use uuid::Uuid;
use xtts::Xtts;

#[test]
#[ignore = "requires NVIDIA GPU, GGUF models, and the local voice samples"]
fn stored_voices_and_custom_instruction() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut tts = Xtts::new(root.join("samples")).unwrap();
    assert_eq!(tts.voices().len(), 1_000);

    let catalog_id = Uuid::parse_str("32460c45-4bcc-4266-8920-582bbbb4824a").unwrap();
    assert_eq!(tts.voice(catalog_id).unwrap().profile.language_code, "it");
    let catalog_audio = tts.synthesize_voice(catalog_id, "Ciao a tutti!").unwrap();
    assert_eq!(catalog_audio.sample_rate, 24_000);
    assert!(!catalog_audio.samples.is_empty());

    let custom = tts
        .synthesize_custom(
            "Ciao a tutti!",
            "female, young adult, moderate pitch",
            Some("it"),
        )
        .unwrap();
    assert_eq!(custom.sample_rate, 24_000);
    assert!(!custom.samples.is_empty());
}
