#!/usr/bin/env docker
# syntax=docker/dockerfile:1

# -------- Rust build (webserver + wasm pkg) --------
FROM python:3.10-slim AS rust-builder

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    pkg-config \
    libpcre2-dev \
    libssl-dev \
    git \
    curl \
    build-essential \
    cmake \
    zlib1g-dev \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
ENV PATH="/root/.cargo/bin:${PATH}"

RUN rustup target add wasm32-unknown-unknown
RUN cargo install  wasm-bindgen-cli --version 0.2.105

# Cache-friendly: fetch deps first
COPY webserver/Cargo.toml webserver/Cargo.lock ./webserver/
WORKDIR /app/webserver
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs
RUN cargo fetch --locked

# Now copy full sources and build
WORKDIR /app
COPY webserver/ ./webserver/
COPY pskit/toolkit/ ./pskit/toolkit/

WORKDIR /app/webserver
RUN cargo build --release

RUN install -D -m 0755 target/release/pskit-webserver /out/pskit-webserver

WORKDIR /app/pskit/toolkit/crates/pskit-wasm
RUN cargo build --lib --target wasm32-unknown-unknown --release
RUN wasm-bindgen /app/pskit/toolkit/target/wasm32-unknown-unknown/release/pskit_wasm.wasm \
    --out-dir /out/pskit-wasm-pkg \
    --target bundler

WORKDIR /app
RUN git clone https://github.com/PDB-REDO/dssp.git

WORKDIR /app/dssp

RUN cmake -S . -B build
RUN cmake --build build
RUN cmake --install build

WORKDIR /app
RUN curl -LO https://github.com/steineggerlab/foldseek/releases/download/10-941cd33/foldseek-linux-avx2.tar.gz
RUN tar -xvzf foldseek-linux-avx2.tar.gz
RUN mv ./foldseek/bin/foldseek /usr/local/bin/foldseek

# -------- Frontend build --------
FROM node:22-slim AS frontend-builder

WORKDIR /app

COPY webpage/package.json webpage/package-lock.json ./webpage/

COPY pskit/pskit-wasm-pkg/package.json /app/pskit/pskit-wasm-pkg/package.json
COPY --from=rust-builder /out/pskit-wasm-pkg/ /app/pskit/pskit-wasm-pkg/

WORKDIR /app/webpage
RUN npm ci

WORKDIR /app
COPY webpage/ ./webpage/

WORKDIR /app/webpage
RUN npm run build


# -------- Rosetta source (copy binaries + database) --------
FROM rosettacommons/rosetta@sha256:f5ea86a2909144ee5d95481160e96dabdb4a003941667942b3fc7889117e66d6 AS rosetta-src


# -------- Runtime (python + rosetta binaries) --------
FROM python:3.10-slim

WORKDIR /app

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    libgomp1 \
    libstdc++6 \
    build-essential \
    && rm -rf /var/lib/apt/lists/*

# Rosetta database path (commonly used by Rosetta tools)
ENV ROSETTA3_DB=/usr/local/database

# Copy Rosetta from the official image into this runtime image
COPY --from=rosetta-src /usr/local/bin/ /usr/local/bin/
COPY --from=rosetta-src /usr/local/database/ /usr/local/database/

# Python deps must be installed into the env we run
COPY pskit/ai/requirements.txt /app/requirements.txt
RUN python -m pip install --no-cache-dir -r /app/requirements.txt

WORKDIR /app
COPY pskit/ai/ /app/pskit/ai/

COPY --from=rust-builder /out/pskit-webserver /usr/local/bin/pskit-webserver
COPY --from=rust-builder /usr/local/bin/mkdssp /usr/local/bin/mkdssp
COPY --from=rust-builder /usr/local/share/libcifpp/ /usr/local/share/libcifpp/
COPY --from=rust-builder /usr/local/share/man/ /usr/local/share/man/
COPY --from=rust-builder /usr/local/bin/foldseek /usr/local/bin/foldseek
COPY --from=frontend-builder /app/webpage/dist/ /app/webpage/dist/

EXPOSE 10706
CMD ["pskit-webserver", "/app/", "0.0.0.0:10706", "2"]