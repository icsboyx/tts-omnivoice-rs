# AMD Radeon integrated graphics (Vulkan)

Use this folder for the Radeon graphics built into Ryzen processors, such as the Radeon 610M, 680M, 760M, 780M, or 890M. OmniVoice is compiled with GGML's Vulkan backend and runs on the Mesa RADV driver inside the container. ROCm is not needed.

## Is it faster than the CPU?

Not on the test host, a Ryzen 9 7950X with a Radeon 610M, where the processor was about three times faster:

| Device | "Ciao a tutti!" (about 1.2 s of audio) | Longer sentence (about 4.7 s of audio) |
| --- | --- | --- |
| Radeon 610M, Vulkan | 55.5 s | 82.2 s |
| Ryzen 9 7950X, 16 threads | 19.6 s | 27.2 s |

The 610M has only 2 compute units; larger integrated GPUs such as the 780M have 12 and may compare better. This image also contains the CPU backend: set `XTTS_DEVICE: CPU` under `environment:` in `docker-compose.yml` to compare without rebuilding. All measurements are in the [Docker Compose README](../README.md#measured-speed).

## Requirements

- The `amdgpu` kernel driver, which mainline Linux kernels include.
- Read and write access to `/dev/dri/renderD*` for the container:

  ```bash
  ls -l /dev/dri/
  ```

## Run

From this folder, after downloading the models into `xtts/models/`:

```bash
docker compose up -d --build
docker compose logs | grep -E 'ggml_vulkan: 0|LM backend'
curl -fsS http://127.0.0.1:8002/health
```

The log should show the Radeon device and `LM backend: Vulkan0`. Stop it with `docker compose down`. With Podman, use `podman compose`.

## Known issue

On the Radeon 610M, results varied between runs with the same input, and a very short text (`"Prova."`) sometimes produced only silence: the server then answers HTTP 500 with `generation produced no audio`. It happened in 5 of 9 attempts on a freshly started server; longer texts always succeeded. The CUDA and CPU backends did not show this.

## Notes

If the host also has a discrete GPU with a Vulkan driver inside the container, select the integrated one with `XTTS_DEVICE`, for example `/dev/dri/renderD128`; see [device selection](../../xomnivoice/README.md#device-selection).
