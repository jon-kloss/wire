# Wire web playground — single self-contained container.
# Build from the repo root:
#   docker build -f crates/wire-web/Dockerfile -t wire-web .
#   docker run --rm -p 8787:8787 wire-web

# 1. Build the UI bundle.
FROM node:22-slim AS ui
WORKDIR /app/ui
COPY ui/package.json ui/package-lock.json ./
RUN npm ci
COPY ui/ ./
RUN npm run build

# 2. Build the server binary (only wire-web + wire-core compile; the Tauri GUI
#    crate is not built, so no GTK system libraries are needed). pkg-config and
#    libssl-dev are needed because reqwest's default native-tls backend builds
#    openssl-sys.
FROM rust:1-slim AS server
RUN apt-get update \
    && apt-get install -y --no-install-recommends pkg-config libssl-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ ./crates/
RUN cargo build --locked -p wire-web --release

# 3. Minimal runtime image.
FROM debian:bookworm-slim
# ca-certificates for TLS roots; libssl3 because the binary dynamically links
# OpenSSL (reqwest's native-tls backend).
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=server /app/target/release/wire-web /usr/local/bin/wire-web
COPY --from=ui /app/ui/dist ./ui/dist
ENV WIRE_WEB_ADDR=0.0.0.0:8787 \
    WIRE_WEB_UI_DIR=/app/ui/dist
# Run unprivileged: a container escape shouldn't land as root.
RUN useradd --system --uid 10001 wire
USER wire
EXPOSE 8787
CMD ["wire-web"]
