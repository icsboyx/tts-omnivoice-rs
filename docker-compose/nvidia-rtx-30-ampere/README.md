# NVIDIA GeForce RTX 30 series (Ampere)

Use this folder for RTX 30 series cards, such as the RTX 3050, 3060, 3070, 3080, 3090. OmniVoice is compiled for CUDA architecture `86` with CUDA 12.9.

Check your card before building: the compute capability must be 8.6.

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
