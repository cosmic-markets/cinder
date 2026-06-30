# ---------- build stage ----------
FROM rust:slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev protobuf-compiler ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin src/accounts src/events src/helpers \
             src/instructions src/orderbook src/quantities src/sokoban \
    && echo "fn main(){}" > src/bin/main.rs \
    && echo "fn main(){}" > src/bin/spline_fill_subscriber.rs \
    && echo "" > src/lib.rs \
    && cargo build --release --features cli 2>/dev/null || true
RUN rm -rf src target/release/.fingerprint/phoenix-eternal-* \
              target/release/deps/phoenix_eternal* \
              target/release/phoenix-eternal* \
              target/release/spline-fill-subscriber* \
              target/release/spline_fill_subscriber*

COPY src src
RUN cargo build --release --features cli

# ---------- runtime stage ----------
FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/phoenix-eternal       /usr/local/bin/
COPY --from=builder /app/target/release/spline-fill-subscriber /usr/local/bin/

ENTRYPOINT ["phoenix-eternal"]