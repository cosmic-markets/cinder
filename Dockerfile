# syntax=docker/dockerfile:1.7
FROM rust:1.94-bookworm AS builder

WORKDIR /app

# 1) Pre-compile dependencies against a dummy crate so a code-only change
#    reuses the `target/` cache layer instead of rebuilding from scratch
#    (`cargo fetch` alone caches downloads but not compilation).
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src \
    && echo 'fn main() {}' > src/main.rs \
    && : > src/lib.rs \
    && cargo build --release --locked \
    && rm -rf src target/release/deps/cinder-* target/release/deps/libcinder-* target/release/cinder target/release/.fingerprint/cinder-*

# 2) Compile the real crate against the cached dependency graph (includes
#    `cosmic-phoenix-eternal-types` from crates.io).
COPY src/ src/
RUN cargo build --release --locked --offline

# ---------------------------------------------------------------------------
# Distroless `cc` ships glibc + CA certs (needed for HTTPS RPC and WSS).
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /app/target/release/cinder /usr/local/bin/cinder

ENTRYPOINT ["cinder"]
