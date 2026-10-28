# Intel integrated graphics (Vulkan)

Use this folder for Intel graphics: UHD Graphics (from 9th-generation Core processors), Iris Xe, and Arc. OmniVoice is compiled with GGML's Vulkan backend and runs on the Mesa ANV driver inside the container. The Intel oneAPI toolkit is not needed.

## Is it faster than the CPU?

Not on the test host, a Core i5-10600K with UHD Graphics 630, where the processor was about 2.6 times faster:

| Device | "Ciao a tutti!" (about 1.2 s of audio) | Longer sentence (about 4.7 s of audio) |
| --- | --- | --- |
| UHD Graphics 630, Vulkan | 209 s | 297 s |
| Core i5-10600K, 6 threads | 81 s | 115 s |

Newer Iris Xe and Arc graphics are much faster than the UHD 630 but were not measured. This image also contains the CPU backend: set `XTTS_DEVICE: CPU` under `environment:` in `docker-compose.yml` to compare without rebuilding. All measurements are in the [Docker Compose README](../README.md#measured-speed).

## Requirements

- The `i915` or `xe` kernel driver, which mainline Linux kernels include.
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

The log should show the Intel device and `LM backend: Vulkan0`. Stop it with `docker compose down`. With Podman, use `podman compose`.

## Notes

If the host also has a discrete GPU with a Vulkan driver inside the container, select the Intel one with `XTTS_DEVICE`, for example `/dev/dri/renderD128`; see [device selection](../../xomnivoice/README.md#device-selection). The NVIDIA driver is not installed in this image, so an NVIDIA card next to the Intel graphics is not visible.
