# CPU only

Use this folder on machines without a supported NVIDIA GPU. OmniVoice is compiled without CUDA, so no NVIDIA driver or Container Toolkit is needed.

Synthesis is much slower than on a GPU. "Ciao a tutti!" took 19.6 s on a Ryzen 9 7950X and 80.7 s on a Core i5-10600K; see [Measured speed](../README.md#measured-speed).

## Run

From this folder, after downloading the models into `xtts/models/`:

```bash
docker compose up -d --build
curl -fsS http://127.0.0.1:8002/health
```

Stop it with `docker compose down`. With Podman, use `podman compose`.
