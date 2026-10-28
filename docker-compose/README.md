# Docker Compose

Each folder builds OmniVoice for one device family, builds the example HTTP server against it, and runs the server. Pick the folder for your hardware; its README lists the matching models.

| Folder | Hardware | Backend | Tested |
| --- | --- | --- | --- |
| [`nvidia-gtx-10-pascal/`](nvidia-gtx-10-pascal/README.md) | GeForce GTX 10 series | CUDA, `61` | GTX 1070 Ti |
| [`nvidia-rtx-20-turing/`](nvidia-rtx-20-turing/README.md) | GeForce RTX 20 and GTX 16 series | CUDA, `75` | no |
| [`nvidia-rtx-30-ampere/`](nvidia-rtx-30-ampere/README.md) | GeForce RTX 30 series | CUDA, `86` | no |
| [`nvidia-rtx-40-ada/`](nvidia-rtx-40-ada/README.md) | GeForce RTX 40 series | CUDA, `89` | RTX 4060 Ti |
| [`nvidia-rtx-50-blackwell/`](nvidia-rtx-50-blackwell/README.md) | GeForce RTX 50 series | CUDA, `120a` | no |
| [`amd-rx-6000-rdna2/`](amd-rx-6000-rdna2/README.md) | Radeon RX 6000 series | ROCm/HIP | no |
| [`amd-rx-7000-rdna3/`](amd-rx-7000-rdna3/README.md) | Radeon RX 7000 series | ROCm/HIP | no |
| [`amd-rx-9000-rdna4/`](amd-rx-9000-rdna4/README.md) | Radeon RX 9000 series | ROCm/HIP | no |
| [`amd-radeon-integrated/`](amd-radeon-integrated/README.md) | Radeon graphics in Ryzen processors | Vulkan | Radeon 610M |
| [`intel-integrated/`](intel-integrated/README.md) | Intel UHD, Iris Xe, Arc | Vulkan | UHD Graphics 630 |
| [`cpu/`](cpu/README.md) | any x86-64 processor | CPU | Ryzen 9 7950X, i5-10600K |

The NVIDIA images use CUDA 12.9, which supports every NVIDIA family above; the AMD discrete images use ROCm 7.2.4. The images do not contain the models or voices: they are mounted read-only from `xtts/models` and `xtts/samples`. Each folder's `docker-compose.yml` is complete on its own; the shared `Dockerfile` holds the build steps for every backend.

## Measured speed

Time to synthesize Italian text with voice `32460c45-…`, after one warm-up request that encodes the voice. RTF is synthesis time divided by audio length: below 1 is faster than real time.

| Hardware | Backend | "Ciao a tutti!" (about 1.2 s of audio) | 82-character sentence (about 4.7 s of audio) |
| --- | --- | --- | --- |
| GeForce RTX 4060 Ti | CUDA | 0.75 s (RTF 0.63) | 1.19 s (RTF 0.25) |
| GeForce GTX 1070 Ti | CUDA | 3.8 s (RTF 2.4) | 5.6 s (RTF 1.2) |
| Ryzen 9 7950X, 16 threads | CPU | 19.6 s (RTF 17) | 27.2 s (RTF 5.8) |
| Radeon 610M (integrated) | Vulkan | 55.5 s (RTF 36) | 82.2 s (RTF 17) |
| Core i5-10600K, 6 threads | CPU | 80.7 s (RTF 70) | 114.6 s (RTF 24) |
| UHD Graphics 630 (integrated) | Vulkan | 209 s (RTF 137) | 297 s (RTF 62) |

Each value is a single run, so treat it as an order of magnitude. The CPU rows used the CPU backend inside the Vulkan images (`XTTS_DEVICE=CPU`), which is the same code as the `cpu/` image. On both test hosts the integrated GPU was slower than the processor next to it; compare both before choosing an integrated-GPU folder.

## Requirements

- Docker with the Compose plugin, or Podman with `podman compose`.
- The two GGUF files in `xtts/models/`; see [Model files](../xtts/README.md#model-files).
- Per backend:
  - **NVIDIA:** a driver supporting CUDA 12.9 (version 575 or newer) and the [NVIDIA Container Toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/latest/install-guide.html). Check that containers see the GPU:

    ```bash
    docker run --rm --gpus all docker.io/nvidia/cuda:12.9.1-base-ubuntu24.04 nvidia-smi
    ```

  - **AMD discrete (ROCm):** the `amdgpu` kernel driver and access to `/dev/kfd` and `/dev/dri`.
  - **Integrated GPUs (Vulkan):** the `amdgpu` or `i915`/`xe` kernel driver and access to `/dev/dri`. The container brings its own Mesa Vulkan drivers.

## Run

From the folder for your device, for example:

```bash
cd docker-compose/nvidia-rtx-40-ada
docker compose up -d --build
curl -fsS http://127.0.0.1:8002/health
docker compose logs -f
```

The first build clones and compiles OmniVoice and the Rust server, which takes several minutes. The server then listens on `127.0.0.1:8002`, loopback only, because it has no authentication. Its API is described in [HTTP server](../xtts/README.md#http-server).

Stop it from the same folder with `docker compose down`.

### Podman

Replace `docker compose` with `podman compose`. For the NVIDIA folders, also add [`podman-nvidia.yml`](podman-nvidia.yml), because Podman ignores Compose's GPU request and needs the GPU by its CDI name:

```bash
podman compose -f docker-compose.yml -f ../podman-nvidia.yml up -d --build
```

This needs a CDI specification, created once with `sudo nvidia-ctk cdi generate --output=/etc/cdi/nvidia.yaml`.

## Settings

The image sets `XTTS_MODEL_DIR=/data/models`, `XTTS_SAMPLES_DIR=/data/samples`, and `XTTS_BIND=0.0.0.0:8002` inside the container. To choose a device, for example on a host with several GPUs, add `XTTS_DEVICE` (such as `CUDA1`, `CPU`, or `/dev/dri/renderD128`) under `environment:` in the folder's `docker-compose.yml`; see [Environment variables](../README.md#environment-variables).

To publish the port on another interface, change `127.0.0.1:8002:8002` in the folder's `docker-compose.yml`, and only behind access controls.
