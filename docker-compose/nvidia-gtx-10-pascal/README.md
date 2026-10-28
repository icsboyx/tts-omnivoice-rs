# NVIDIA GeForce GTX 10 series (Pascal)

Use this folder for GTX 10 series cards, such as the GTX 1050, 1060, 1070, 1070 Ti, 1080, 1080 Ti. OmniVoice is compiled for CUDA architecture `61` with CUDA 12.9.

Check your card before building: the compute capability must be 6.1.

```bash
nvidia-smi --query-gpu=name,compute_cap --format=csv,noheader
```

## Run

From this folder, after downloading the models into `xtts/models/` and installing the [requirements](../README.md#requirements):

```bash
docker compose up -d --build
curl -fsS http://127.0.0.1:8002/health
```

Stop it with `docker compose down`. With Podman, add the GPU override:

```bash
podman compose -f docker-compose.yml -f ../podman-nvidia.yml up -d --build
```

## Notes

Pascal is the oldest supported family. CUDA 13 dropped it, so this image uses CUDA 12.9. Tested on a GTX 1070 Ti with driver 580 and Docker 29: 3.8 s for "Ciao a tutti!" and 5.6 s for a 4.7-second sentence. See [Measured speed](../README.md#measured-speed).
