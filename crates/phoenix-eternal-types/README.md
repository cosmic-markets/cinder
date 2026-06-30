# Phoenix Eternal Types

Vendored read-only account layouts and helper types for Phoenix Eternal
perpetuals on Solana.

Published on crates.io as `cosmic-phoenix-eternal-types`. Cinder consumes it
under the historical local ident via Cargo's `package =` rename:

```toml
phoenix-eternal-types = { version = "0.1.0", package = "cosmic-phoenix-eternal-types" }
```

## What It Provides

- Zero-copy account views for Phoenix Eternal on-chain state.
- PDA derivation helpers for protocol accounts.
- Quantity wrappers for lots, ticks, symbols, and position sizes.
- Read-only orderbook and spline collection structures used by the TUI.
- Optional CLI utilities for local inspection and protocol debugging.

## CLI Features

The `cli` feature builds the `phoenix-eternal` inspection binary:

```bash
cargo run -p phoenix-eternal-types --features cli --bin phoenix-eternal -- markets
```

The Yellowstone gRPC subscriber is behind the `geyser` feature and is supported
on Unix targets:

```bash
cargo run -p phoenix-eternal-types --features geyser --bin spline-fill-subscriber -- \
  subscribe --spline <PUBKEY> --grpc-url <URL>
```

## Repository Role

`phoenix-eternal-types` is intentionally narrow: it mirrors protocol account
layouts and exposes helpers Cinder needs to render on-chain state. Transaction
submission in Cinder uses `phoenix-rise`; this crate should stay focused on
deserialization, account addressing, and debugging tools.

## License

Apache-2.0. See `LICENSE` in this directory.
