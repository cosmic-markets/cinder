# Cinder

**Cinder** is a Rust terminal UI for [Phoenix](https://phoenix.trade) perpetuals on Solana: live charts, a merged on-chain **spline** + optional **CLOB** order book, market and wallet flows, and signed transactions from the shell.

![Rust](https://img.shields.io/badge/rust-2021-orange?logo=rust&logoColor=white)
![ratatui](https://img.shields.io/badge/ratatui-TUI-00ADD8?logo=terminal)
![Solana](https://img.shields.io/badge/Solana-RPC%20%2B%20WSS-9945FF?logo=solana)

<p align="center">
  <img
    src="assets/demo.gif"
    alt="Cinder TUI — Phoenix SOL perpetuals: order book, chart, trade panel, and wallet"
  />
</p>

> 🔥 **No Phoenix invite yet?** Sign up through Cinder's referral and start with a **10% fee discount** on every trade — see [Referral Funding](#referral-funding) below, or visit [cosmic.markets/phoenix/trade](https://cosmic.markets/phoenix/trade) to register with the `COSMIC` code.

## Features

- **Markets** — Loads Active / PostOnly markets from the Phoenix HTTP API; background refresh about every 60s.
- **Spline Liquidity** — `accountSubscribe` to the market’s on-chain spline account for ladder-style liquidity.
- **CLOB Liquidity** — Optional merge of FIFO L2 levels from the market orderbook account (toggle in user config).
- **Top positions** — Periodic scan of the protocol-wide Active Trader Buffer for a leaderboard-style modal (`T`).
- **Trading** — Market / limit / stop-style flows with confirmation modals; deposits and withdrawals when a wallet is loaded.
- **i18n** — UI strings in English, Chinese, Spanish, and Russian.

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
        Runtime[TUI runtime]
    end

    Run --> PhoenixHTTP
    Run --> PhoenixWS
    Runtime --> Solana
```

## Environment

| Variable | Required | Description |
|----------|----------|-------------|
| `RPC_URL` or `SOLANA_RPC_URL` | Yes | Solana HTTP RPC |
| `RPC_WS_URL` or `SOLANA_WS_URL` | No | WebSocket endpoint (inferred from HTTP when omitted) |
| `PHX_WALLET_PATH` or `KEYPAIR_PATH` | No | Keypair file (default `~/.config/solana/id.json`) |
| `RUST_LOG` | No | e.g. `info` or `cinder=debug,phoenix_rise=warn` |
| `CINDER_LOG_DIR` | No | Directory for transaction error logs (default `~/.config/phoenix-cinder/logs`) |
| `CINDER_SKIP_REFERRAL` | No | Set to `1` to skip auto-registering new wallets with the COSMIC referral. You'll need to register manually at app.phoenix.trade before trading. |

## Build and run

```bash
# Debug
cargo build
cargo run

cargo build --release
RPC_URL=https://api.mainnet-beta.solana.com ./target/release/cinder
```

Pre-compiled binaries for Windows and Linux are available in the Releases.

## Docker

```bash
docker compose build               # one-time (or after Cargo/source changes)
docker compose run --rm cinder     # interactive TUI run
```

For signing, mount a Solana keypair via the CLI. The binary defaults `PHX_WALLET_PATH` to `~/.config/solana/id.json`, which inside the distroless `nonroot` image resolves to `/home/nonroot/.config/solana/id.json`:

```bash
docker compose run --rm \
  -v "$HOME/.config/solana/id.json:/home/nonroot/.config/solana/id.json:ro" \
  cinder
```

## Referral Funding
Cinder is partially funded through Phoenix's referral program. New, unregistered Phoenix users connecting through Cinder are automatically registered into the private beta with the `COSMIC` referral code and receive a 10% fee discount per Phoenix's current referral program (terms set by Phoenix and subject to change). Cinder earns a share of trading fees from referred wallets.

## Risk Disclaimer
Trading perpetual futures is high-risk and can result in the rapid and total loss of your funds. Cinder is provided **as-is** under the MIT license with no warranties; the authors are not liable for any losses, missed fills, RPC outages, on-chain errors, or other issues arising from use of this software. You are solely responsible for your trades, your keys, and your compliance with the laws of your jurisdiction. Nothing in this project is financial advice.

# Donations
❤️ Donations are greatly appreciated: cosmic.sol

## License
MIT