# syntax=docker/dockerfile:1.7
FROM rust:1.94-bookworm AS builder

WORKDIR /app

# 1) Pre-compile dependencies against a dummy crate so a code-only change
#    reuses the ~200-crate `target/` cache layer instead of rebuilding from
#    scratch (`cargo fetch` alone caches downloads but not compilation).
COPY Cargo.toml Cargo.lock ./
COPY crates/phoenix-eternal-types/Cargo.toml crates/phoenix-eternal-types/Cargo.toml
RUN mkdir -p src crates/phoenix-eternal-types/src \
 && echo 'fn main() {}' > src/main.rs \
 && : > src/lib.rs \
 && : > crates/phoenix-eternal-types/src/lib.rs \
 && cargo build --release --locked \
 && rm -rf src crates/phoenix-eternal-types/src \
            target/release/deps/cinder-* \
            target/release/deps/libcinder-* \
            target/release/deps/phoenix_eternal_types-* \
            target/release/deps/libphoenix_eternal_types-* \
            target/release/cinder \
            target/release/.fingerprint/cinder-* \
            target/release/.fingerprint/phoenix-eternal-types-*

# 2) Compile the actual workspace crates against the cached dependency graph.
COPY src/ src/
COPY crates/phoenix-eternal-types/ crates/phoenix-eternal-types/
RUN cargo build --release --locked --offline

# ---------------------------------------------------------------------------
# Distroless `cc` ships glibc + CA certs (needed for HTTPS RPC and WSS).
FROM gcr.io/distroless/cc-debian12:nonroot

COPY --from=builder /app/target/release/cinder /usr/local/bin/cinder

ENTRYPOINT ["cinder"]
