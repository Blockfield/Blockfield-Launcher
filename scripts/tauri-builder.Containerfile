FROM docker.io/library/rust:1-bookworm
RUN apt-get update && apt-get install -y --no-install-recommends \
    libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev \
    libayatana-appindicator3-dev librsvg2-dev pkg-config && rm -rf /var/lib/apt/lists/*
RUN rustup component add clippy rustfmt
