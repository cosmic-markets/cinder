```text
⠀⠀⠀⠀⠀⠀⢱⣆⠀⠀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⠈⣿⣷⡀⠀⠀⠀⠀
⠀⠀⠀⠀⠀⠀⢸⣿⣿⣷⣧⠀⠀⠀
⠀⠀⠀⠀⡀⢠⣿⡟⣿⣿⣿⡇⠀⠀
⠀⠀⠀⠀⣳⣼⣿⡏⢸⣿⣿⣿⢀⠀
⠀⠀⠀⣰⣿⣿⡿⠁⢸⣿⣿⡟⣼⡆
⢰⢀⣾⣿⣿⠟⠀⠀⣾⢿⣿⣿⣿⣿
⢸⣿⣿⣿⡏⠀⠀⠀⠃⠸⣿⣿⣿⡿
⢳⣿⣿⣿⠀⠀⠀⠀⠀⠀⢹⣿⡿⡁
⠀⠹⣿⣿⡄⠀⠀⠀⠀⠀⢠⣿⡞⠁
⠀⠀⠈⠛⢿⣄⠀⠀⠀⣠⠞⠋⠀⠀
⠀⠀⠀⠀⠀⠀⠉⠀⠀⠀⠀⠀⠀⠀
```

# Cinder

**Cinder** is a Rust terminal UI for [Phoenix](https://phoenix.trade) perpetuals on Solana: live charts, a merged on-chain **spline** + optional **CLOB** order book, market and wallet flows, and signed transactions from the shell.

![Rust](https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white)
![ratatui](https://img.shields.io/badge/ratatui-TUI-00ADD8?logo=terminal)
![Solana](https://img.shields.io/badge/Solana-RPC%20%2B%20WSS-9945FF?logo=solana)

## Features

- **Markets** — Loads Active / PostOnly markets from the Phoenix HTTP API; background refresh about every 60s.
- **Stats** — Per-symbol WebSocket updates (e.g. mark, volume, 24h change).
- **Spline** — `accountSubscribe` to the market’s on-chain spline account for ladder-style liquidity.
- **CLOB** — Optional merge of FIFO L2 levels from the market orderbook account (toggle in user config).
- **GTI** — In-memory Global Trader Index cache so book rows can show wallet authorities instead of opaque pointers.
- **Top positions** — Periodic scan of the protocol-wide Active Trader Buffer for a leaderboard-style modal (`T`).
- **Trading** — Market / limit / stop-style flows with confirmation modals; deposits and withdrawals when a wallet is loaded.
- **i18n** — UI strings in English and Chinese.

Quit with **`q`** (confirm) or **Ctrl+C**.

## Architecture

```mermaid
flowchart TB
    subgraph external [External]
        PhoenixHTTP[Phoenix HTTP API]
        PhoenixWS[Phoenix WebSocket]
        Solana[Solana HTTP RPC and WSS]
    end

    subgraph cinder [Cinder]
        Run[app::run]
        Poller[Spline TUI poller]
    end

    Run --> PhoenixHTTP
    Run --> PhoenixWS
    Poller --> Solana
```

At startup, `app::run` wires HTTP market discovery, WS stats, and the ratatui loop. The poller drives chart/book state, modal input, async balance and context tasks, and Solana subscriptions.

## TUI layout

```text
┌──────────────────── Chart ────────────────────┬── Order book ──┐
│  price history (last 150 ticks)               │  top asks      │
│  trade markers                                │  spread        │
│                                               │  top bids      │
├───────────────────────────────────────────────┴────────────────┤
│  Trading panel  │ side | size | wallet | position | balances   │
├────────────────────────────────────────────────────────────────┤
│  Actions bar    │ hotkeys                                      │
├────────────────────────────────────────────────────────────────┤
│  Status strip   │ time / last status / transaction detail      │
└────────────────────────────────────────────────────────────────┘
```

**Modals and overlays:** market picker (`m`), positions (`p`), open orders (`o`), top positions (`T`), config (`c`), activity ledger (`L`), and **y** / **n** confirmations.

**Order mode:** `t` cycles Market → Limit → Stop (trigger). `e` edits limit or stop price; `s` edits size.

## Keyboard (normal mode)

| Key | Action |
|-----|--------|
| `q` | Quit (asks for confirmation) |
| **Ctrl+C** | Exit immediately |
| `m` | Market selector |
| `p` | Positions |
| `o` | Orders |
| `T` | Top positions (protocol-wide, by notional) |
| `c` | Config / RPC / options |
| `L` | Ledger (recent tx signatures) |
| `Tab` | Toggle long / short |
| `Enter` | Confirm place order (after wallet loaded) |
| `t` | Cycle order kind (market / limit / stop) |
| `e` | Edit limit or stop price |
| `s` | Edit size |
| `+` `=` **↑** | Larger size preset |
| `-` **↓** | Smaller size preset |
| `x` | Close position (confirm) |
| `d` | Deposit USDC (confirm) |
| `D` | Withdraw USDC (confirm) |
| `w` | Connect wallet path or disconnect |

In lists, use **↑** / **↓** and **Enter** where shown. **Esc** backs out of modals.

**Size presets:** `0.01` through `500.0` (default step `0.1`). See `ORDER_SIZE_PRESETS` in `src/spline/constants.rs`.

## Requirements

- **Rust** toolchain (2021 edition; see `rust-version` in `Cargo.toml`).
- `cargo fmt` uses the checked-in `rustfmt.toml`.
- A **Solana JSON-RPC HTTP** endpoint (`RPC_URL`). WebSocket URL is optional and can be derived from HTTP.
- Optional **keypair** JSON for live trading and wallet-scoped views (env vars below).

## Environment

| Variable | Required | Description |
|----------|----------|-------------|
| `RPC_URL` or `SOLANA_RPC_URL` | Yes | Solana HTTP RPC |
| `RPC_WS_URL` or `SOLANA_WS_URL` | No | WebSocket endpoint (inferred from HTTP when omitted) |
| `PHX_WALLET_PATH` or `KEYPAIR_PATH` | No | Keypair file (default `~/.config/solana/id.json`) |
| `RUST_LOG` | No | e.g. `info` or `cinder=debug,phoenix_rise=warn` |
| `CINDER_LOG_DIR` | No | Directory for transaction error logs (default `~/.config/phoenix-cinder/logs`) |

`dotenvy` loads a **`.env`** in the working directory when present.
Use `.env.example` as a template and never commit wallet keypairs or private RPC credentials.

## Trading safety

Cinder can sign real Solana transactions. The release defaults favor safety:

- Transaction sends run RPC preflight before broadcast.
- Order and close sizes are checked before converting to on-chain base lots.
- Deposit and withdrawal amounts must be finite, positive, and within the release safety limit.
- A confirmation timeout means the transaction status is unknown, not failed. Check the displayed signature before retrying.
- Raw transaction errors are written to `cinder-error.log` under the Cinder log directory rather than the current working directory.

## Repository layout

| Path | Role |
|------|------|
| `Cargo.toml` | Cinder manifest; `phoenix-eternal-types` is a **path** dependency |
| `crates/phoenix-eternal-types/` | Vendored read-only on-chain layouts (GTI, ATB, PDAs, etc.); sourced from [skynetcap/phx-types](https://github.com/skynetcap/phx-types) |
| `src/main.rs` | Binary entry: `.env`, tracing, `cinder::run()` |
| `src/app.rs` | HTTP markets, WS stats, spawns TUI |
| `src/spline/` | TUI: config, parse, poller, render, state, tx, `gti`, `top_positions`, `i18n`, … |

## Stack (high level)

| Crate / area | Role |
|--------------|------|
| **phoenix-rise** | Phoenix HTTP / WS client, transaction helpers, account deserialization used in `parse.rs` |
| **phoenix-eternal-types** (local) | PDA helpers, Global Trader Index and Active Trader Buffer tree views, discriminants |
| **ratatui** / **crossterm** | Terminal UI and input |
| **solana-*** | RPC, pub/sub, signing, transactions (see `Cargo.toml` for versions) |
| **tokio**, **tracing**, **chrono** | Async runtime, logs, clock |

## Build and run

```bash
# Debug
cargo build
cargo run

# Release (profile tuned for size in Cargo.toml)
cargo build --release
RPC_URL=https://api.mainnet-beta.solana.com ./target/release/cinder
```

**Tests and lint:**

```bash
cargo fmt --all --check
cargo clippy --workspace --locked --all-targets -- -D warnings
cargo test --workspace --locked
```

## Docker

```bash
docker compose build               # one-time (or after Cargo/source changes)

docker compose run --rm cinder     # interactive TUI run
```

**Do not use `docker compose up` for this service.** Compose's `up` only streams log bytes; it does not allocate an interactive PTY connecting your shell to the container, so the binary starts inside the container but your terminal stays blank. `compose run` (or `docker run -it`) does allocate one and is the correct command for a TUI.

Set **`RPC_URL`** in your shell env (or a local **`.env`**). Optional overrides: **`RPC_WS_URL`** (derived from **`RPC_URL`** when unset — note that the Compose service uses the bare-key form `RPC_WS_URL` so an unset value is *not* injected as an empty string, which would make the binary hang on a malformed WSS URL), and **`RUST_LOG`**.

For signing, mount a Solana keypair via the CLI. The binary defaults `PHX_WALLET_PATH` to `~/.config/solana/id.json`, which inside the distroless `nonroot` image resolves to `/home/nonroot/.config/solana/id.json`:

```bash
docker compose run --rm \
  -v "$HOME/.config/solana/id.json:/home/nonroot/.config/solana/id.json:ro" \
  cinder
```

Or set a custom path:

```bash
docker compose run --rm \
  -v "/path/to/key.json:/wallet/id.json:ro" \
  -e PHX_WALLET_PATH=/wallet/id.json \
  cinder
```

The Compose file enables a pseudo-TTY (`stdin_open` / `tty`) so Ratatui can allocate the screen; omitting that makes the binary exit with `No such device or address (os error 6)`. **`CINDER_TRACING_STDERR=1`** sends tracing to stderr so RPC/network errors appear inline (the local default keeps tracing silent so it does not draw over the UI).

First **`docker compose build`** can sit on **`Building`** without output while Rust compiles — that is normal (cold build is ~70s on a fast machine, much longer on slower ones). Use **`docker compose build --progress=plain`** to watch each stage.

On Windows, use **[Windows Terminal](https://aka.ms/terminal)** and a moderately large pane; **`cmd.exe`** in the legacy conhost can render poorly.

## Contributing and security

See `CONTRIBUTING.md` for local development and PR expectations. Report
vulnerabilities through `SECURITY.md`.

## License

Cinder is MIT licensed; see `LICENSE`. The vendored
`crates/phoenix-eternal-types` workspace member is Apache-2.0 licensed; see
`crates/phoenix-eternal-types/LICENSE` and `NOTICE`.
