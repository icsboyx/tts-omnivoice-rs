# XOmniVoice

Rust wrapper for the local [OmniVoice C ABI](include/omnivoice.h). It loads the model once, then runs synchronous speech synthesis on the host GPU or CPU. No HTTP server is involved.

## Supported native version

This crate includes the C ABI header but not the OmniVoice C++ implementation. It supports exactly one native build:

| Component | Supported version |
| --- | --- |
| OmniVoice | [ServeurpersoCom/omnivoice.cpp](https://github.com/ServeurpersoCom/omnivoice.cpp) commit `1a468d8b9d65b3544ca1004d2d7c05dfc65d1f3e`, struct ABI `OV_ABI_VERSION 4` |
| GGML | [ggml-org/ggml](https://github.com/ggml-org/ggml) commit `cc5df49a6b878db2b68b9a7b45976e279594627d`, the submodule of that OmniVoice commit |
| Platform | Linux x86-64 |

Device selection also calls the GGML device registry directly, so `libggml.so` and `libggml-base.so` from the same build must be beside `libomnivoice.so`. Other OmniVoice commits are not supported, even when they keep the same `OV_ABI_VERSION`: the header and the library must come from the same revision. `OmniVoice::with_options` compares `version()` with `SUPPORTED_NATIVE_COMMIT` and returns `Error::UnsupportedNative` for any other build. The library does not report its `OV_ABI_VERSION` at runtime, so the commit is the check.

The optional [duration patch](patches/short-unprompted-duration.patch) does not change the ABI and is supported. It sets a 45-frame minimum for short speech without a reference, because local tests produced mostly noise for some short unprompted utterances. Reference-conditioned duration estimation is unchanged.

## Install the native library

Cargo does not compile C++ or install CUDA. Build OmniVoice once, then either install it system-wide or point `OMNIVOICE_LIB_DIR` at the build directory.

### 1. Install build tools

| Distribution | Command |
| --- | --- |
| Ubuntu / Debian | `sudo apt update && sudo apt install build-essential cmake clang libclang-dev git` |
| Arch | `sudo pacman -Syu --needed base-devel cmake clang git` |
| Fedora | `sudo dnf install gcc gcc-c++ make cmake clang-devel git` |

Install Rust with your usual toolchain manager. `libclang` is needed for the Rust bindings; CMake and a C/C++ compiler are needed only to build OmniVoice.

For GPU builds, install a compatible NVIDIA driver and CUDA toolkit, including `nvcc`. Check the card and toolkit with:

```bash
nvidia-smi --query-gpu=name,compute_cap --format=csv,noheader
nvcc --version
```

Choose the CUDA architecture from the compute capability:

| NVIDIA family | `CMAKE_CUDA_ARCHITECTURES` | Typical GeForce series |
| --- | --- | --- |
| Pascal | `61` | GTX 10 series, including GTX 1070 Ti |
| Turing | `75` | RTX 20 series |
| Ampere | `86` | RTX 30 series |
| Ada | `89` | RTX 40 series |
| Blackwell | `120a` | RTX 50 series |

Pascal cards need CUDA 12.x; CUDA 13 removed Pascal build support. On Ubuntu/Debian or Fedora, set up NVIDIA's repository for your distribution release, then install the versioned `cuda-toolkit-12-9` package. On Arch, the rolling [`cuda` package](https://archlinux.org/packages/extra/x86_64/cuda/) may be CUDA 13; for Pascal install a CUDA 12.x toolkit from NVIDIA and use its versioned `nvcc`. See the [NVIDIA CUDA Linux installation guide](https://docs.nvidia.com/cuda/archive/12.9.1/cuda-installation-guide-linux/index.html), [NVIDIA's GPU tables](https://developer.nvidia.com/cuda/gpus) and the [legacy GPU table](https://developer.nvidia.com/cuda/gpus/legacy). Match the host GCC version to the toolkit's supported compiler range. Only the RTX 4060 Ti (`89`) has been tested by the maintainers.

### 2. Obtain the supported source

In a directory outside this Rust workspace:

```bash
git clone https://github.com/ServeurpersoCom/omnivoice.cpp.git
cd omnivoice.cpp
git checkout 1a468d8b9d65b3544ca1004d2d7c05dfc65d1f3e
git submodule update --init --recursive
```

Optionally apply the duration patch:

```bash
git apply /absolute/path/to/local_tts/xomnivoice/patches/short-unprompted-duration.patch
```

### 3. Compile `libomnivoice.so`

Run one of these inside the `omnivoice.cpp` checkout. `$ORIGIN` lets `libomnivoice.so` find the GGML libraries in its own directory.

CPU only:

```bash
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DOMNIVOICE_SHARED=ON -DGGML_CUDA=OFF -DCMAKE_BUILD_RPATH_USE_ORIGIN=ON '-DCMAKE_BUILD_RPATH=$ORIGIN'
cmake --build build --target omnivoice --parallel 4
```

NVIDIA GPU: set `CUDA` to the toolkit directory and `ARCH` to the value from the table above. The example is the tested RTX 4060 Ti; a GTX 1070 Ti needs `ARCH=61` and a CUDA 12.x toolkit.

```bash
CUDA=/usr/local/cuda-12.9   # NVIDIA packages on Ubuntu/Debian/Fedora; Arch's cuda package uses /opt/cuda
ARCH=89
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DOMNIVOICE_SHARED=ON -DGGML_CUDA=ON -DGGML_CUDA_NCCL=OFF -DCMAKE_CUDA_ARCHITECTURES=$ARCH -DCUDAToolkit_ROOT=$CUDA -DCMAKE_CUDA_COMPILER=$CUDA/bin/nvcc -DCMAKE_BUILD_RPATH_USE_ORIGIN=ON '-DCMAKE_BUILD_RPATH=$ORIGIN'
cmake --build build --target omnivoice --parallel 4
```

If CUDA rejects the system C++ compiler, add `-DCMAKE_CUDA_HOST_COMPILER=/absolute/path/to/compatible/g++` to the configure command. `GGML_CUDA_NCCL=OFF` avoids requiring NCCL.

Check that every runtime dependency resolves; fix any `not found` line before continuing:

```bash
ldd build/libomnivoice.so
```

### 4a. Install system-wide

OmniVoice has no CMake install target, so copy the libraries into their own directory and register it with the dynamic linker:

```bash
sudo install -d /usr/local/lib/omnivoice
sudo cp -P build/libomnivoice.so build/libggml*.so* /usr/local/lib/omnivoice/
echo /usr/local/lib/omnivoice | sudo tee /etc/ld.so.conf.d/omnivoice.conf
sudo ldconfig
ldconfig -p | grep libomnivoice.so
```

When `OMNIVOICE_LIB_DIR` is unset, the build script finds `libomnivoice.so` in the `ldconfig` cache and links it without an rpath; the executable then finds it at runtime the same way. Registering the directory also exposes these GGML libraries to other programs on the system; if another installed GGML uses the same `libggml.so.0` name, use `OMNIVOICE_LIB_DIR` instead.

To uninstall:

```bash
sudo rm -r /usr/local/lib/omnivoice /etc/ld.so.conf.d/omnivoice.conf
sudo ldconfig
```

### 4b. Or use the build directory

Inside the `omnivoice.cpp` checkout:

```bash
export OMNIVOICE_LIB_DIR="$PWD/build"
```

`OMNIVOICE_LIB_DIR` always takes precedence over a system-wide installation. Cargo links that directory and embeds it as the rpath of this crate's tests; dependent build scripts receive it as `DEP_OMNIVOICE_LIB_DIR` (see [the workspace example](../xtts_examples/build.rs)). Rebuild after moving the directory, since executables embed its absolute path. Export the variable again in every new shell.

In both cases, the CUDA runtime and `libcuda.so.1` come from the installed toolkit and driver. Rebuild the native library on another host or after changing the CUDA major version.

## Use the API

The application supplies GGUF paths; the workspace defaults are in `../xtts/models/` (see the [model download instructions](../xtts/README.md#model-files)).

```rust
use xomnivoice::{OmniVoice, SynthesisRequest};

fn main() -> xomnivoice::Result<()> {
    // Run from the workspace root, or supply paths appropriate for your app.
    let mut model = OmniVoice::with_paths(
        "xtts/models/omnivoice-base-Q8_0.gguf",
        "xtts/models/omnivoice-tokenizer-F32.gguf",
    )?;
    let mut request = SynthesisRequest::new("Ciao a tutti!");
    request.language = "Italian";
    request.instruction = Some("female, young adult, moderate pitch");

    let audio = model.synthesize_with(&request)?;
    // audio.samples is owned f32 PCM; sample_rate and channels describe it.

    model.stream(&request, &mut |chunk: &[f32]| {
        // Copy or play the chunk before returning; it is borrowed from native code.
        let _ = chunk;
        true // Return false to cancel.
    })?;
    Ok(())
}
```

`OmniVoice::synthesize(text, language, instruction)` is the shorter buffered form. `SynthesisRequest` exposes automatic or explicit duration, chunk settings, denoising, prompt preprocessing, MaskGIT generation settings, and postprocessing. `languages()` lists accepted language IDs and names; `version()` reports the native build.

For a reference voice, pass `Reference::RawAudio` with 24 kHz mono float samples and its transcript, or call `extract_voice_reference` once and reuse the returned `VoiceReference` through `Reference::Encoded`. If you have WAV file bytes, `WavReference::from_wav_bytes(bytes, transcript)` decodes a mono 24 kHz WAV and owns the samples; use `wav_reference.as_reference()` while preparing the synthesis request. WAV files with another sample rate or channel count need conversion first.

`synthesize_cancellable` and `stream_cancellable` accept a callback polled by the native pipeline. `set_log_callback` installs a process-wide native log handler. The generated, unsafe C interface is also available under `xomnivoice::raw`.

The native API blocks while generating audio. `OmniVoice` is deliberately not `Send` or `Sync`: create and use a model on one dedicated worker thread when integrating with an async Rust application. Keep that model alive across requests to avoid loading the weights repeatedly.

### Device selection

`InitOptions::new(model_path, codec_path)` leaves `device` unset, so the native library selects the best available compiled backend. `options.device` accepts:

- a GGML device name, case-insensitive: `"CUDA0"`, `"CPU"`, `"Vulkan0"`; `devices()` lists those compiled into the library;
- a DRM node such as `"/dev/dri/renderD129"` or `"/dev/dri/by-path/pci-0000:01:00.0-render"`, matched with a GGML device by PCI bus ID through `/sys/class/drm`.

If the value matches no device, for example an integrated GPU's node with a CUDA-only build, `OmniVoice::with_options` logs a warning (through `set_log_callback`, or stderr) and uses the best available device. To force CPU at runtime in a CUDA build, set `options.device = Some("CPU")`. The device is chosen once, when the model loads.

## Verify

From the workspace root, with the library installed system-wide or `OMNIVOICE_LIB_DIR` exported:

```bash
cargo check -p xomnivoice --all-targets --locked
cargo test -p xomnivoice --locked
cargo test -p xomnivoice --locked -- --ignored
```

The ignored integration test needs the GGUF files in `../xtts/models/`. A GPU is recommended because CPU inference is slow. It checks buffered and streaming synthesis, references, cancellation, and callback behavior.
