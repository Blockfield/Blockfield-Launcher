# ── Build stage ──────────────────────────────────────────────
FROM rust:1.88-slim-bookworm@sha256:38bc5a86d998772d4aec2348656ed21438d20fcdce2795b56ca434cf21430d89 AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# ── Cargo dependency cache — copy only manifests, build deps first ──
COPY Cargo.toml Cargo.lock ./
COPY shared/Cargo.toml shared/
COPY server/Cargo.toml server/
COPY src-tauri/Cargo.toml src-tauri/

RUN mkdir -p shared/src server/src src-tauri/src && \
    echo 'pub fn dummy() {}' > shared/src/lib.rs && \
    echo 'fn main() {}' > server/src/main.rs && \
    echo 'fn main() {}' > src-tauri/src/main.rs && \
    echo '' > src-tauri/src/lib.rs

# Build only dependencies (this layer is cached until Cargo.toml/lock changes)
RUN cargo build --release -p blockfield-server

# ── Now copy real source and build the binary ──
COPY shared/src/ shared/src/
COPY server/src/ server/src/

# ponytail: touch after COPY — Docker layer cache can leave mtime of
# source files older than cargo artifacts from the cached dummy build,
# so cargo skips recompilation. Explicitly freshen timestamps.
RUN touch shared/src/lib.rs server/src/main.rs && \
    cargo build --release -p blockfield-server && \
    cp target/release/blockfield-server /usr/local/bin/

# ── Runtime stage ────────────────────────────────────────────
FROM debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818

RUN apt-get update && apt-get install -y ca-certificates curl tini && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/local/bin/blockfield-server /usr/local/bin/blockfield-server

# Keep active, staged, and previous releases on one filesystem. Mounting the
# active directory itself prevents the atomic directory swap used on publish.
VOLUME ["/data"]

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
ENV GITHUB_REPO=netherg-io/blockfield-launcher
ENV GITHUB_UPDATE_TAG=develop

EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=30s --retries=3 \
    CMD curl -f http://localhost:3000/health || exit 1

ENTRYPOINT ["tini", "--"]
CMD ["blockfield-server"]
