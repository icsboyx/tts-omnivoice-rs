# AMD Radeon RX 7000 series (RDNA 3)

Use this folder for RX 7000 series cards: RX 7600 (`gfx1102`), RX 7700 XT and RX 7800 XT (`gfx1101`), RX 7900 XT and XTX (`gfx1100`). OmniVoice is compiled with ROCm 7.2.4 (HIP) for these targets.

Check your card's target before building; it must be one of the targets above:

```bash
rocminfo | grep -m1 -o 'gfx[0-9a-f]*'
```

## Requirements

- The `amdgpu` kernel driver, which mainline Linux kernels include. The container brings its own ROCm user-space libraries.
- Access to `/dev/kfd` and `/dev/dri`. If the server cannot open them, add the host's render group ID under `group_add:` in `docker-compose.yml`:

  ```bash
  getent group render
  ```

- The images are large: about 7.4 GB each for build and runtime, because ROCm's math libraries are included.

## Run

From this folder, after downloading the models into `xtts/models/`:

```bash
docker compose up -d --build
curl -fsS http://127.0.0.1:8002/health
```

Stop it with `docker compose down`. With Podman, use `podman compose`.

## Notes

OmniVoice ships no ROCm build script; this build uses GGML's HIP backend directly. This image has not been built or run in testing. Cards that ROCm does not officially support may need `HSA_OVERRIDE_GFX_VERSION` under `environment:`.
