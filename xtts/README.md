# Xtts

`xtts` builds on [`xomnivoice`](../xomnivoice/README.md) to synthesize speech from recorded voices under `xtts/samples/` or from OmniVoice voice-design instructions. It keeps one model loaded, encodes each voice reference once, and applies optional speed and pitch changes. The native library must be installed first; see [Install the native library](../xomnivoice/README.md#install-the-native-library).

## Model files

The default model pair comes from [Serveurperso/OmniVoice-GGUF](https://huggingface.co/Serveurperso/OmniVoice-GGUF). `Xtts::new(...)` expects these exact filenames in the directory named by `XTTS_MODEL_DIR`, or in `xtts/models/` when it is unset:

- `omnivoice-base-Q8_0.gguf` — speech model
- `omnivoice-tokenizer-F32.gguf` — audio codec

The repository does not include them; run these commands from the workspace root. The revision pins the files used and tested here:

```bash
mkdir -p xtts/models
curl -fL --retry 3 -o xtts/models/omnivoice-base-Q8_0.gguf \
  https://huggingface.co/Serveurperso/OmniVoice-GGUF/resolve/017094167b5c9ed565a5076ac9b3b93c5ecf5c73/omnivoice-base-Q8_0.gguf
curl -fL --retry 3 -o xtts/models/omnivoice-tokenizer-F32.gguf \
  https://huggingface.co/Serveurperso/OmniVoice-GGUF/resolve/017094167b5c9ed565a5076ac9b3b93c5ecf5c73/omnivoice-tokenizer-F32.gguf
```

Verify the downloaded files:

```bash
sha256sum -c <<'EOF'
2882d887921798aea13d45236556bdf8012842ab6f8cd2690943eead6289f298  xtts/models/omnivoice-base-Q8_0.gguf
83820c6316da023076af7c1d06de5e38dcd09ae9f42203675bf8b3bd9a58e330  xtts/models/omnivoice-tokenizer-F32.gguf
EOF
```

## Voices

The 1000 catalog voices in `xtts/samples/` were generated with the OmniVoice model; they are not recordings of real people. They are grouped in one subdirectory per language code, ten voices each, such as `xtts/samples/it/`.

`Xtts::new(samples_dir)` recursively discovers every UUID JSON file under `samples_dir`. Each JSON has a matching mono 24 kHz WAV with the same UUID in the same directory. All voices appear in one collection through `voices()` and `voice(id)`, including those in any subdirectory; subdirectories only organize files. Each `Voice` contains its `profile` and WAV `path`. The WAV is decoded and its reference encoded only when that voice is first used.

Custom JSON uses the same fields as catalog voice JSON. Fields unavailable at upload (`technical_id`, `character_name`, `gender`) are `null`; `speed` and `pitch` default to `0`. The `reference_text` must match the words in its WAV.

```rust
use uuid::Uuid;
use xtts::Xtts;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Run from the workspace root, or supply an absolute samples path.
    let mut tts = Xtts::new("xtts/samples")?;
    let id = Uuid::parse_str("32460c45-4bcc-4266-8920-582bbbb4824a")?;
    let audio = tts.synthesize_voice(id, "Ciao a tutti!")?;
    println!("{} samples at {} Hz", audio.samples.len(), audio.sample_rate);
    Ok(())
}
```

Call `reload_voices()` after adding, removing, or changing voice files; an unknown UUID also triggers a rescan. `synthesize_voice_with_options(id, text, speed, pitch)` overrides the profile's speed and pitch:

- `speed` from -10 to 10: +10 is 10 times faster, -10 is 10 times slower;
- `pitch` from -20 to 20 semitones.

Both effects are applied after synthesis, so changing them does not invalidate the encoded reference.

## Voice design

Use OmniVoice instructions to synthesize without a recorded reference:

```rust
let audio = tts.synthesize_custom(
    "Ciao a tutti!",
    "female, young adult, moderate pitch",
    Some("it"),
)?;
```

Pass `None` for the language to let OmniVoice infer it from the text. The instruction is passed to OmniVoice as written.

## Model location and device

`Xtts::new` reads two [environment variables](../README.md#environment-variables) when it loads the model:

- `XTTS_MODEL_DIR`: the directory with the two GGUF files;
- `XTTS_DEVICE`: `CPU`, `CUDA0`, or a DRM node such as `/dev/dri/renderD129`, with the rules in [device selection](../xomnivoice/README.md#device-selection).

For other file names or other `InitOptions`, build an `xomnivoice::OmniVoice` yourself and pass it to `Xtts::with_model(model, samples_dir)`.

## Command-line examples

From the workspace root:

```bash
cargo run --locked -p xtts_examples --bin synthesize -- voice 32460c45-4bcc-4266-8920-582bbbb4824a "Ciao a tutti!"
cargo run --locked -p xtts_examples --bin synthesize -- voice 2e3970c6-e7ef-4a03-ba72-93a73c4ab8ca "Ciao a tutti!"
cargo run --locked -p xtts_examples --bin synthesize -- custom "female, young adult, moderate pitch" "Ciao a tutti!" it
```

Each command saves a 24 kHz mono float WAV as `prova_xtts.wav` in the system temporary directory (`$TMPDIR`, usually `/tmp`) and prints its path. `xtts_examples/raw_synthesize.rs` is a direct `xomnivoice` example without `xtts`.

## HTTP server

```bash
cargo run --locked -p xtts_examples --bin server
```

The server loads the model once, keeps encoded references in memory, and handles requests sequentially. It listens on `XTTS_BIND` (default `127.0.0.1:8002`) and reads voices from `XTTS_SAMPLES_DIR` (default `xtts/samples/`). Stop it with Ctrl+C.

| Method and path | Response |
| --- | --- |
| `GET /health` | `{"status":"ok"}` |
| `GET /voices` | JSON array of voice profiles, sorted by `id` |
| `POST /synthesize` | 32-bit float WAV; JSON `{"error": ...}` with 400, 404, 413, or 500 on failure |

The `/synthesize` body is JSON with `id` (voice UUID), `text`, and optional `speed` and `pitch` as described in [Voices](#voices). The body limit is 32 KiB.

```bash
curl -fsS http://127.0.0.1:8002/voices -o voices.json
curl -fsS http://127.0.0.1:8002/synthesize \
  -H 'Content-Type: application/json' \
  -d '{"id":"32460c45-4bcc-4266-8920-582bbbb4824a","text":"Ciao a tutti!","speed":2,"pitch":-2}' \
  -o speech.wav
```

The server has no authentication; keep it on the loopback address unless you add access controls.

## Use outside this workspace

An executable that links `xtts` must find `libomnivoice.so`, the GGML libraries, and, for a GPU build, the CUDA runtime at startup. A system-wide installation is found by the dynamic linker. With `OMNIVOICE_LIB_DIR`, add an rpath from a build script as described in [4b](../xomnivoice/README.md#4b-or-use-the-build-directory). Set `XTTS_MODEL_DIR`, since the default model path points into the directory where `xtts` was built.

## Verify

From the workspace root:

```bash
cargo test -p xtts --locked
cargo test -p xtts --locked -- --ignored
```

The ignored test loads the models from `xtts/models/` and the voices from `xtts/samples/`; a GPU is recommended.
