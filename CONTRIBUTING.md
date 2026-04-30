# Contributing

Thanks for helping improve Cinder. This project is a Rust TUI for Phoenix
perpetuals on Solana, so correctness and trading safety matter more than raw
feature velocity.

## Development Setup

Install a recent stable Rust toolchain. The repository also includes
`mise.toml` if you use mise:

```bash
cargo build
cargo test --workspace --locked
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo fmt --all --check
```

Run the TUI with a Solana RPC endpoint:

```bash
RPC_URL=https://api.mainnet-beta.solana.com cargo run
```

Use `.env.example` as a local template. Do not commit `.env`, wallet keypairs,
RPC tokens, or transaction logs.

## Pull Request Expectations

- Keep changes focused and explain user-facing behavior changes.
- Add tests for trading math, transaction preparation, account parsing, and
  state transitions when possible.
- Prefer explicit validation over silent fallback for user-entered trading
  values.
- Update README or changelog entries when behavior, setup, or release process
  changes.

## Safety Notes

Cinder can sign real Solana transactions. Avoid PRs that make transaction
submission faster at the expense of clearer errors, preflight checks, or safer
confirmation messaging unless the tradeoff is documented and reviewed.
