# NVIDIA GeForce RTX 20 / GTX 16 series (Turing)

Use this folder for RTX 20 and GTX 16 series cards, such as the GTX 1650, 1660, RTX 2060, 2070, 2080, 2080 Ti. OmniVoice is compiled for CUDA architecture `75` with CUDA 12.9.

Check your card before building: the compute capability must be 7.5.

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

Not tested on real hardware.
