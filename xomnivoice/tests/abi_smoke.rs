use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use xomnivoice::{
    InitOptions, LogCallback, OmniVoice, Reference, SynthesisRequest, VoiceReference, languages,
    set_log_callback, version,
};

// Runs explicitly because it loads both GGUF files and needs a CUDA device.
#[test]
#[ignore = "requires NVIDIA GPU and GGUF models in xtts/models"]
fn native_abi_end_to_end() {
    let logs = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&logs);
    let callback: LogCallback = Arc::new(move |_, _| {
        counter.fetch_add(1, Ordering::Relaxed);
    });
    set_log_callback(Some(callback));
    struct RestoreLogging;
    impl Drop for RestoreLogging {
        fn drop(&mut self) {
            set_log_callback(None);
        }
    }
    let _logging = RestoreLogging;

    assert!(!version().is_empty());
    assert!(languages().iter().any(|language| language.id == "it"));
    let models_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../xtts/models");
    let model_path = models_dir.join("omnivoice-base-Q8_0.gguf");
    let codec_path = models_dir.join("omnivoice-tokenizer-F32.gguf");
    let mut options = InitOptions::new(model_path.to_str().unwrap(), codec_path.to_str().unwrap());
    options.device = Some("CUDA0");
    let mut model = OmniVoice::with_options(options).unwrap();
    assert!(logs.load(Ordering::Relaxed) > 0);
    assert!(model.num_codebooks() > 0);
    assert_eq!(model.duration_to_frames(1.8).unwrap(), 45);

    let mut request = SynthesisRequest::new("Ciao a tutti!");
    request.language = "Italian";
    request.instruction = Some("female, young adult, moderate pitch");
    let audio = model.synthesize_with(&request).unwrap();
    assert_eq!(audio.sample_rate, 24_000);
    assert_eq!(audio.channels, 1);
    assert!(!audio.samples.is_empty());

    let mut chunks = 0;
    model
        .stream(&request, &mut |samples| {
            chunks += 1;
            !samples.is_empty()
        })
        .unwrap();
    assert!(chunks > 0);

    let voice = model.extract_voice_reference(&audio.samples).unwrap();
    assert_eq!(voice.codebooks(), model.num_codebooks());
    let voice =
        VoiceReference::from_codes(voice.codes().to_vec(), voice.frames(), voice.codebooks())
            .unwrap();
    let mut cloned = SynthesisRequest::new("Buongiorno a tutti!");
    cloned.language = "Italian";
    cloned.reference = Some(Reference::Encoded {
        voice: &voice,
        transcript: "Ciao a tutti!",
    });
    assert!(!model.synthesize_with(&cloned).unwrap().samples.is_empty());

    cloned.reference = Some(Reference::RawAudio {
        samples_24k_mono: &audio.samples,
        transcript: "Ciao a tutti!",
    });
    assert!(!model.synthesize_with(&cloned).unwrap().samples.is_empty());

    let cancelled = model.stream(&request, &mut |_| false).unwrap_err();
    assert!(cancelled.to_string().contains("-5"));
    let mut polls = 0;
    let cancelled = model
        .synthesize_cancellable(
            &request,
            Some(&mut || {
                polls += 1;
                true
            }),
        )
        .unwrap_err();
    assert!(polls > 0);
    assert!(cancelled.to_string().contains("-5"));
    let caught = catch_unwind(AssertUnwindSafe(|| {
        let _ = model.stream(&request, &mut |_| panic!("callback panic"));
    }));
    assert!(caught.is_err());
}
