# NVIDIA GeForce RTX 40 series (Ada Lovelace)

Use this folder for RTX 40 series cards, such as the RTX 4060, 4060 Ti, 4070, 4080, 4090. OmniVoice is compiled for CUDA architecture `89` with CUDA 12.9.

Check your card before building: the compute capability must be 8.9.

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

Tested on an RTX 4060 Ti with driver 615 and Podman: 0.75 s for "Ciao a tutti!" and 1.19 s for a 4.7-second sentence. See [Measured speed](../README.md#measured-speed).
