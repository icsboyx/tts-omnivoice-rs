# Builds OmniVoice and the example HTTP server. Build context: the workspace root.
# Each folder's docker-compose.yml picks the backend, GPU targets, and base images.
ARG BUILD_IMAGE=docker.io/nvidia/cuda:12.9.1-devel-ubuntu24.04
ARG RUNTIME_IMAGE=docker.io/nvidia/cuda:12.9.1-runtime-ubuntu24.04

FROM ${BUILD_IMAGE} AS build
# cuda | hip | vulkan | cpu
ARG BACKEND=cuda
# CUDA architectures (e.g. 89) or AMD gfx targets (e.g. gfx1100;gfx1101); unused for vulkan and cpu.
ARG GPU_TARGETS=89
ARG OMNIVOICE_COMMIT=1a468d8b9d65b3544ca1004d2d7c05dfc65d1f3e
ARG DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake clang libclang-dev git curl ca-certificates \
    && if [ "${BACKEND}" = "vulkan" ]; then \
           apt-get install -y --no-install-recommends libvulkan-dev glslc spirv-headers; \
       fi \
    && rm -rf /var/lib/apt/lists/*
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
ENV PATH=/root/.cargo/bin:${PATH}

# Native library, as in xomnivoice/README.md "Install the native library".
WORKDIR /build
RUN git clone https://github.com/ServeurpersoCom/omnivoice.cpp.git \
    && cd omnivoice.cpp \
    && git checkout ${OMNIVOICE_COMMIT} \
    && git submodule update --init --recursive
COPY xomnivoice/patches/short-unprompted-duration.patch /build/
RUN cd omnivoice.cpp \
    && git apply /build/short-unprompted-duration.patch \
    && case "${BACKEND}" in \
           cuda) set -- -DGGML_CUDA=ON -DGGML_CUDA_NCCL=OFF "-DCMAKE_CUDA_ARCHITECTURES=${GPU_TARGETS}" ;; \
           hip) export HIPCXX="$(hipconfig -l)/clang" HIP_PATH="$(hipconfig -R)"; \
                set -- -DGGML_HIP=ON "-DGPU_TARGETS=${GPU_TARGETS}" ;; \
           vulkan) set -- -DGGML_VULKAN=ON ;; \
           cpu) set -- ;; \
           *) echo "unknown BACKEND ${BACKEND}" >&2; exit 1 ;; \
       esac \
    && cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DOMNIVOICE_SHARED=ON "$@" \
           -DCMAKE_BUILD_RPATH_USE_ORIGIN=ON '-DCMAKE_BUILD_RPATH=$ORIGIN' \
    && cmake --build build --target omnivoice --parallel "$(nproc)" \
    && install -d /usr/local/lib/omnivoice \
    && cp -P build/libomnivoice.so build/libggml*.so* /usr/local/lib/omnivoice/ \
    && echo /usr/local/lib/omnivoice > /etc/ld.so.conf.d/omnivoice.conf \
    && ldconfig

# Rust server, linked against the system-wide library above.
WORKDIR /build/workspace
COPY Cargo.toml Cargo.lock ./
COPY xomnivoice xomnivoice
COPY xtts xtts
COPY xtts_examples xtts_examples
RUN cargo build --release --locked -p xtts_examples --bin server

FROM ${RUNTIME_IMAGE}
ARG BACKEND=cuda
ARG DEBIAN_FRONTEND=noninteractive
RUN apt-get update \
    && apt-get install -y --no-install-recommends libgomp1 libstdc++6 \
    && if [ "${BACKEND}" = "vulkan" ]; then \
           apt-get install -y --no-install-recommends libvulkan1 mesa-vulkan-drivers; \
       fi \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /usr/local/lib/omnivoice /usr/local/lib/omnivoice
COPY --from=build /etc/ld.so.conf.d/omnivoice.conf /etc/ld.so.conf.d/omnivoice.conf
RUN ldconfig
COPY --from=build /build/workspace/target/release/server /usr/local/bin/tts-server

# Models and voices are mounted by the compose files.
ENV XTTS_MODEL_DIR=/data/models \
    XTTS_SAMPLES_DIR=/data/samples \
    XTTS_BIND=0.0.0.0:8002
EXPOSE 8002
CMD ["tts-server"]
