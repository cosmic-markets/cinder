# Cinder - Phoenix Perpetuals TUI

Rust terminal trading client for the Phoenix perpetuals exchange on Solana. Renders a ratatui TUI with live price charts, order book, market selection, and position/order management. Not the Elixir/Phoenix web stack and not a JavaScript SPA framework; directory naming is historical.

## Directory Structure

```
phx-ember/
├── Cargo.toml              # Crate manifest
├── rustfmt.toml            # rustfmt config
├── lefthook.yml            # Optional pre-commit checks
├── mise.toml               # Tool versions and tasks
├── docker-compose.yml      # Container build
├── crates/phoenix-eternal-types/ # Vendored read-only Phoenix account layouts
└── src/
    ├── main.rs             # Binary entry — init .env, tracing, calls cinder::run()
    ├── lib.rs              # Crate root
    ├── app.rs              # Startup orchestration (HTTP markets + WS stats + TUI poller)
    └── spline/
        ├── mod.rs          # Module root
        ├── config.rs       # UserConfig, SplineConfig, env/keypair helpers
        ├── constants.rs    # Colors, MAX_PRICE_HISTORY, TOP_N, ORDER_SIZE_PRESETS
        ├── format.rs       # fmt_price, fmt_size, fmt_balance, compact notation
        ├── gti.rs          # Global Trader Index cache
        ├── i18n.rs         # User-facing strings (English + Chinese)
        ├── math.rs         # Tick/lot conversions, percent change
        ├── parse.rs        # On-chain spline account decode
        ├── terminal.rs     # Crossterm/ratatui setup and teardown
        ├── top_positions.rs # On-chain top-N position scan
        ├── trading/        # Trading domain types (Side, OrderKind, PositionInfo, ...)
        ├── poller/         # TUI event loop and async tasks
        ├── render/         # Frame rendering (chart, orderbook, trading panel, modals)
        ├── state/          # TUI state containers
        └── tx/             # Solana transaction builders and submission
```

## Build Commands

```bash
cargo build                # debug
cargo build --release      # release

cargo test                 # all tests
cargo test --lib format    # tests in a module
cargo test balance_formatting  # single test by name
cargo test -- --nocapture --test-threads=1

cargo clippy --workspace --locked --all-targets -- -D warnings
cargo fmt --all --check

docker compose build
docker compose run --rm cinder
```

## Environment Variables

| Variable                           | Required        | Notes                                                          |
| ---------------------------------- | --------------- | -------------------------------------------------------------- |
| `RPC_URL` / `SOLANA_RPC_URL`       | Runtime         | Solana JSON-RPC HTTP endpoint                                  |
| `RPC_WS_URL` / `SOLANA_WS_URL`     | Runtime (opt.)  | Derived from `RPC_URL` if unset                                |
| `PHX_WALLET_PATH` / `KEYPAIR_PATH` | Runtime (opt.)  | Defaults to `~/.config/solana/id.json`                         |
| `RUST_LOG`                         | Optional        | e.g. `phoenix_sdk=warn,info`                                   |

## Architecture

```
app::run()
  ├── Phoenix HTTP API  →  fetch tradable markets at startup + every 60s
  ├── Phoenix WebSocket →  per-symbol stats (price, volume, 24h change)
  └── Spline TUI poller →  Solana accountSubscribe (WSS) for on-chain state
```

**Startup** (`src/app.rs`): loads Active/PostOnly markets from Phoenix HTTP, subscribes per-symbol to WebSocket stats, builds `SplineConfig` entries, launches the ratatui TUI. A background task refreshes markets every 60s.

**TUI event loop** (`src/spline/poller/`): crossterm events → key handlers in `input.rs`; order submission in `submit.rs`; background wallet/market refresh tasks in `tasks.rs`.

**Rendering** (`src/spline/render/`): each frame paints chart, order book, trading panel, status strip, and optional modal overlays (market selector, positions view, confirmation prompts).

**Transactions** (`src/spline/tx/`): builds and signs Solana transactions for market/limit/stop orders, close position, USDC deposit/withdrawal. `compute_budget.rs` estimates fees; `confirmation.rs` drives the confirm-before-send modal.

**State** (`src/spline/state/`): `TuiState`, `TradingState`, `MarketSelector`, `PositionsView`, `TradeMarker` — all owned by the poller and passed to render.

## Style

This crate mirrors the [phx](https://github.com/skynetcap/phx) SDK conventions:

- `rustfmt.toml` uses stable rustfmt settings so local `cargo fmt --all --check` matches CI.
- Every module file opens with a `//!` doc header describing its role.
- One concept per file: enums, structs, and free functions live next to their tests in focused modules under `spline/trading/`, `spline/state/`, `spline/tx/`, etc.
- Cargo manifest uses table-aligned key=value formatting in `[package]` and similar blocks.
