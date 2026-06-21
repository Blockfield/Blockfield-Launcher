# ── Build stage ──────────────────────────────────────────────
FROM rust:1.88-slim-bookworm AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY shared/ shared/
COPY server/ server/
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml
# Create dummy source files so cargo can resolve the workspace
RUN mkdir -p src-tauri/src && \
    echo 'fn main() {}' > src-tauri/src/main.rs && \
    echo '' > src-tauri/src/lib.rs

RUN cargo build --release -p blockfield-server && \
    cp target/release/blockfield-server /usr/local/bin/

# ── Runtime stage ────────────────────────────────────────────
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y ca-certificates curl tini && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/blockfield-server /usr/local/bin/blockfield-server

# Volumes for modpack files and extracted content
VOLUME ["/data/files", "/data/extracted"]

ENV FILES_DIR=/data/files
ENV EXTRACTED_DIR=/data/extracted
ENV BASE_URL=https://play.blockfield.gg
ENV MODPACK_VERSION=0.1.43
ENV MINECRAFT_VERSION=1.20.1
ENV BIND_HOST=0.0.0.0
ENV PORT=3000
ENV API_PREFIX=api/launcher/v1
ENV FALLBACK_BRAND=BLOCKFIELD
ENV FALLBACK_BRAND_SUBTITLE="TACTICAL OPS"
ENV FALLBACK_CHROME_TITLE="BLOCKFIELD LAUNCHER"
ENV FALLBACK_OPERATION_NAME="IRON FRONT"
ENV FALLBACK_SERVER_IP=play.blockfield.gg:25565
ENV FALLBACK_LAUNCHER_VERSION=0.4.2
ENV FALLBACK_UPDATE_URL=https://play.blockfield.gg/downloads/blockfield-launcher_0.1.0_x64-setup.exe

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

ENTRYPOINT ["tini", "--"]
CMD ["blockfield-server"]
