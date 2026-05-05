# Cinder — Agent Reference

Rust terminal trading client for the Phoenix perpetuals exchange on Solana. Renders a ratatui TUI with live price charts, a coalesced spline+CLOB order book, market selector, position/order management, top-positions leaderboard, and a live liquidation feed. Built on top of the `phoenix-rise` SDK and a vendored `phoenix-eternal-types` crate that knows the on-chain account layouts.

This file is the technical map. Use it before reading code so you know which crate / module / channel owns the thing you're about to change.

---

## Quick orientation

| Question | Answer |
| --- | --- |
| Binary entry | `src/main.rs` → `cinder::run()` (`src/lib.rs`, `src/app.rs`) |
| Library crate name | `cinder` (single binary `cinder`, `publish = false`) |
| Workspace members | `.` and `crates/phoenix-eternal-types` |
| Rust edition / MSRV | `2021`, `rust-version = "1.94"` |
| Async runtime | `tokio` (multi-thread, `macros`, `time`, `sync`, `signal`) |
| TUI stack | `ratatui` 0.30 + `crossterm` 0.29 (event-stream feature) |
| TLS / WSS | `rustls` 0.23 with the `ring` provider — required by `solana-pubsub-client` ≥ 2.3 (no aws-lc-rs) |
| Solana wire stack | Pinned `=2.3.13` for `solana-rpc-client`, `solana-pubsub-client`, `solana-rpc-client-types`, `solana-account-decoder-client-types`, `solana-transaction-status-client-types` so HTTP RPC and WSS resolve to one `solana-commitment-config` graph |
| Phoenix SDK | `phoenix-rise = 0.1.2` (HTTP client, WS client, account types, IX builders, metadata) |
| Vendored on-chain types | `phoenix-eternal-types` (Trader / Spline / GTI / inner-instruction event decoder) |
| Persisted user config | `~/.config/phoenix-cinder/config.json` — `rpc_url`, `language` (`en`/`cn`), `show_clob` |
| Config defaults RPC fallback | `https://api.mainnet-beta.solana.com` (with a warn log) |
| Wallet path resolution | `phoenix.json` (cwd) → `PHX_WALLET_PATH` / `KEYPAIR_PATH` → `~/.config/solana/id.json` (first that exists) |

---

## Directory map

```
.
├── Cargo.toml                  # workspace root + cinder bin manifest
├── Cargo.lock
├── Dockerfile                  # multi-stage musl build (statically linked)
├── docker-compose.yml          # service `cinder`
├── rustfmt.toml                # max_width=100, edition=2021, field-init/try shorthand
├── crates/
│   └── phoenix-eternal-types/  # vendored zero-copy account / event types
│       └── Cargo.toml          # no_std-friendly; optional cli/geyser/serde features
└── src/
    ├── main.rs                 # dotenvy + tracing init, calls cinder::run()
    ├── lib.rs                  # re-exports `app::run`; `spline` alias for older callers
    ├── app.rs                  # Phoenix HTTP markets + WSS stats subscribe + 60s poll loop
    └── tui/
        ├── mod.rs              # crate-internal module root, public surface for `app.rs`
        ├── config.rs           # UserConfig, SplineConfig, RPC/WS env, keypair load helpers
        ├── constants.rs        # SOL_SYMBOL, MAX_PRICE_HISTORY, TOP_N, ORDER_SIZE_PRESETS, palette
        ├── format.rs           # fmt_price/size/balance, compact notation, pubkey_trader_prefix
        ├── i18n/               # Strings struct + EN/CN tables (no allocation per render)
        │   ├── mod.rs          # `strings()` reads UserConfig.language at call time
        │   ├── en.rs
        │   └── zh.rs
        ├── math.rs             # ticks_to_price, base_lots_to_units, lot conversions, pct_change_24h
        ├── splash.rs           # opt-in startup splash task (paints over alt-screen during load)
        ├── terminal.rs         # crossterm/ratatui setup, panic-hook teardown, restore helpers
        ├── data/               # on-chain decoders + caches (read path)
        │   ├── mod.rs
        │   ├── spline_book.rs           # SplineCollection → ParsedSplineData; L2 book parser
        │   ├── trader_index.rs          # GTI sokoban tree + `node_addr → authority` cache
        │   └── position_leaderboard.rs  # ActiveTraderBuffer scan for largest positions
        ├── trading/            # Trading domain types (no async, no I/O)
        │   ├── mod.rs
        │   ├── balance.rs         # fetch_phoenix_balance_and_position
        │   ├── input_mode.rs      # InputMode enum (Normal / EditingSize / Confirming(...) / ...)
        │   ├── order_info.rs      # OrderInfo (per-row trader-orders snapshot)
        │   ├── order_kind.rs      # OrderKind: Market | Limit { price } | StopMarket { trigger }
        │   ├── pending_action.rs  # PendingAction: PlaceOrder / Close / Cancel / Deposit / Withdraw
        │   ├── position_info.rs   # PositionInfo, FullPositionInfo (per-symbol)
        │   ├── side.rs            # TradingSide::{Long,Short} + toggle()
        │   └── top_position_entry.rs
        ├── runtime/            # event loop, input routing, async tasks (write path)
        │   ├── mod.rs                  # crate-internal sub-module root + tuning consts
        │   ├── channels.rs             # Channels/Receivers structs, KeyAction, TxCtxMsg
        │   ├── connection.rs           # initial WS config, RPC reconnect, market-switch helpers
        │   ├── event_loop.rs           # `spawn_spline_poller` — the central tokio::select! hub
        │   ├── keyboard.rs             # InputMode-dispatch from KeyEvent
        │   ├── redraw.rs               # full-frame redraw helpers (chart bounds + ratatui draw)
        │   ├── submit.rs               # PendingAction → tx submit dispatch
        │   ├── update_handlers.rs      # per-channel state-mutation handlers
        │   ├── wallet.rs               # connect/disconnect bookkeeping
        │   ├── input/                  # one file per InputMode
        │   │   ├── mod.rs
        │   │   ├── amounts.rs          # numeric editing (size, price, deposit, withdraw)
        │   │   ├── clipboard.rs        # arboard wrapper (copy txid / pubkey)
        │   │   ├── forms.rs            # config / RPC URL / wallet-path edit modes
        │   │   ├── market.rs           # market-selector navigation
        │   │   ├── normal.rs           # main hotkeys (q/m/c/o/p/Tab/t/e/s/+ -/w/Enter/x/d/D/L/T/F)
        │   │   ├── settings.rs         # config-modal field cycling + save
        │   │   └── views.rs            # positions/orders/top/liquidations/ledger key handlers
        │   └── tasks/                  # one spawnable task per stream / workload
        │       ├── mod.rs
        │       ├── balances.rs            # spawn_balance_fetch (HTTP balance + position pull)
        │       ├── connect_flow.rs        # connect_wallet (keypair → tx_context bootstrap)
        │       ├── l2_book.rs             # spawn_phoenix_l2_book_rpc (Solana accountSubscribe on market)
        │       ├── liquidations.rs       # spawn_liquidation_feed_task (logsSubscribe → events)
        │       ├── orders.rs              # trader-orders WS subscription
        │       ├── position_leaderboard.rs# spawn_top_positions_refresh (5s tick)
        │       ├── tx_context.rs          # one-shot TxContext::new + warm-up blockhash poller
        │       └── wallet_stream.rs       # USDC ATA + native SOL balance WSS subscriptions
        ├── ui/                  # ratatui widgets (no async, no I/O)
        │   ├── mod.rs           # render_frame entrypoint; layout constants
        │   ├── chart.rs         # price chart (canvas) with trade markers + order chart markers
        │   ├── status.rs        # status tray + funds panel
        │   ├── orderbook/       # MergedBook → table cells
        │   │   ├── mod.rs
        │   │   └── table.rs
        │   ├── trade_panel/
        │   │   ├── mod.rs
        │   │   ├── actions.rs   # bottom action row (`[m] markets`, etc.)
        │   │   ├── layout.rs
        │   │   ├── order_entry.rs
        │   │   └── position.rs
        │   └── modals/          # one file per modal
        │       ├── mod.rs
        │       ├── chrome.rs              # rounded-border, title + footer hint helper
        │       ├── config.rs              # RPC URL / language / show_clob
        │       ├── ledger.rs              # last 50 actions
        │       ├── liquidation_feed.rs    # `[F]` modal
        │       ├── market_selector.rs     # `[m]` modal
        │       ├── orders.rs              # `[o]` modal
        │       ├── position_leaderboard.rs# `[T]` modal (capital T)
        │       ├── positions.rs           # `[p]` modal
        │       ├── quit.rs                # `[q]` confirm
        │       └── wallet_path.rs         # `[w]` load-wallet modal
        ├── state/               # mutable runtime state containers
        │   ├── mod.rs           # re-exports + `make_status_timestamp`
        │   ├── tui.rs           # TuiState (the top-level state owned by event_loop)
        │   ├── tui_tests.rs     # path = "tui_tests.rs" test module for tui.rs
        │   ├── book.rs          # MergedBook, BookRow, RowSource, ClobLevel, L2BookStreamMsg
        │   ├── liquidation_feed_view.rs
        │   ├── markers.rs       # TradeMarker, OrderChartMarker, LedgerEntry
        │   ├── market.rs        # MarketInfo, MarketSelector, MarketListUpdate, MarketStatUpdate
        │   ├── orders_view.rs
        │   ├── position_leaderboard_view.rs
        │   ├── positions_view.rs
        │   ├── trade_panel.rs   # TradingState (in-flight inputs, wallet, status, ledger)
        │   └── updates.rs       # TxStatusMsg, BalanceUpdate, LiquidationEntry payload types
        └── tx/                  # transaction building + submission (write path)
            ├── mod.rs
            ├── compute_budget.rs   # CU price/limit instruction builder
            ├── confirmation.rs     # signatureSubscribe over the shared TxContext.sig_pubsub
            ├── context.rs          # TxContext (RPCs, metadata, blockhash pool, sig pubsub)
            ├── error.rs            # log-scraping for Phoenix-program errors → user strings
            ├── funds.rs            # USDC deposit / withdraw
            ├── limit_order.rs
            ├── market_order.rs
            ├── stop_market_order.rs
            ├── cancel.rs           # cancel-orders / cancel-stop-loss batch builder
            └── positions.rs        # close_all_positions (cross-market batch)
```

---

## Build / test commands

```bash
cargo build                                                  # debug
cargo build --release                                        # size-tuned release (LTO=thin, opt=s, strip)
cargo build --profile release-size                           # smallest-binary profile (LTO=fat, opt=z)

cargo test                                                   # all tests
cargo test --workspace --locked                              # CI-equivalent
cargo test --lib format                                      # tests under tui::format
cargo test balance_formatting                                # single test by name
cargo test -- --nocapture --test-threads=1                   # see println; serial

cargo clippy --workspace --locked --all-targets -- -D warnings   # CI clippy gate
cargo fmt --all --check                                      # CI fmt gate

docker compose build && docker compose run --rm cinder       # containerised
```

The `[profile.release]` settings (`codegen-units=1`, `lto=thin`, `opt-level="s"`, `panic=abort`, `strip=true`) are size-focused. Don't change them as part of unrelated work.

---

## Environment variables

| Variable | Required | Notes |
| --- | --- | --- |
| `RPC_URL` / `SOLANA_RPC_URL` | Runtime | Solana JSON-RPC HTTP endpoint. User config `rpc_url` overrides both. |
| `RPC_WS_URL` / `SOLANA_WS_URL` | Optional | Derived from the HTTP URL when unset (`http_rpc_url_to_ws`); localhost `:8899` → `:8900`. |
| `PHX_WALLET_PATH` / `KEYPAIR_PATH` | Optional | Used as wallet candidate after `phoenix.json` and before `~/.config/solana/id.json`. |
| `RUST_LOG` | Optional | `tracing-subscriber` filter, e.g. `phoenix_sdk=warn,info`. |
| `USERPROFILE` / `HOME` | Optional | Used to expand `~` for config and wallet paths (Windows uses `USERPROFILE`). |

`tui::config::current_user_config()` is cached behind a `OnceLock<RwLock<UserConfig>>`; first access reads `~/.config/phoenix-cinder/config.json`, later writes through `save_user_config` go to disk **and** update the cache. Existing RPC clients keep their URL — only newly-built ones see the change.

---

## High-level architecture

```
                              ┌──────────────────────────────────────────────┐
                              │                  app::run                    │
                              │ ───────────────────────────────────────────  │
                              │  1. setup_terminal + spawn_splash            │
                              │  2. PhoenixHttpClient::new_from_env()        │
                              │     PhoenixClient::new_from_env() (WSS)      │
                              │  3. http.get_markets()  → tradable filter    │
                              │  4. subscribe_market_stats(per symbol)       │
                              │       → forwarder tasks → stat_rx (mpsc 128) │
                              │  5. build MarketInfo + SplineConfig per sym  │
                              │  6. spawn_spline_poller (TUI runtime)        │
                              │  7. 60s poll loop discovers new markets,     │
                              │     sends MarketListUpdate via market_tx     │
                              │  8. tokio::select on ctrl_c | tui_task       │
                              │  9. cleanup_terminal                         │
                              └──────────────────────────────────────────────┘
                                                  │
                                                  ▼
                ┌───────────────────────────────────────────────────────────────────┐
                │                  tui::runtime::event_loop                         │
                │ ───────────────────────────────────────────────────────────────── │
                │  Owned by one tokio task (`spawn_spline_poller`):                 │
                │    • TuiState  + SplineConfig (current symbol)                    │
                │    • watch::channel<SplineConfig> for L2 task                     │
                │    • mpsc<L2BookStreamMsg> from L2 task                           │
                │    • (Channels, Receivers) for tx status / balances /             │
                │      wallet WSS / tx_ctx / orders / top positions / liquidations  │
                │                                                                   │
                │  Outer loop reconnects spline pubsub on WSS_RETRY_*.              │
                │  Inner `'sub` loop runs a tokio::select! biased on                │
                │  clock → keyboard → spline-stream → all other channels.           │
                │                                                                   │
                │  Background tasks owned via JoinHandles inside the loop:          │
                │    • spawn_phoenix_l2_book_rpc        (only when show_clob)       │
                │    • spawn_gti_loader                 (always)                    │
                │    • spawn_liquidation_feed_task      (always; survives reconnect)│
                │    • spawn_balance_fetch              (1.1 s tick, in-flight gate)│
                │    • spawn_top_positions_refresh      (5 s tick, in-flight gate)  │
                │    • spawn_wallet_*_subscribe         (USDC ATA, native SOL)      │
                │    • spawn_trader_orders              (Phoenix WS trader stream)  │
                │    • spawn_blockhash_refresh          (warm pool inside TxContext)│
                │    • spawn_tx_context                 (one-shot bootstrap)        │
                └───────────────────────────────────────────────────────────────────┘
                                                  │
                                                  ▼
                              ┌──────────────────────────────────────────────┐
                              │              tui::ui::render_frame           │
                              │ ───────────────────────────────────────────  │
                              │  Pure: TuiState + SplineConfig → Frame paint │
                              │  Layout: orderbook | chart  /  trade panel | │
                              │          funds  /  status tray  /  modals    │
                              └──────────────────────────────────────────────┘
```

### Concurrency model

- **One owning task** runs `spawn_spline_poller` and holds the entire `TuiState`. Mutable state is never shared via `Arc<Mutex<…>>`; it lives on the event loop and is mutated synchronously inside `tokio::select!` arms.
- **Background tasks** never touch `TuiState` directly. They send typed payloads through `tokio::sync::mpsc::Unbounded*` channels (or one bounded channel for stats). The event loop drains those receivers and applies updates via `update_handlers::*`.
- **Static work** (decoders, IX builders, formatters, math) is sync and free of `tokio` types.
- **`watch::channel<SplineConfig>`** drives the L2 book task without an extra mpsc — the task `borrow().clone()`s on every reconnect so a market switch propagates by replacing the watched value.
- **Backpressure**: the only bounded channel is `MarketStatUpdate` (cap 128). On full it drops via `try_send`. Every other channel is unbounded; producers are externally rate-limited (`L2_POLL_INTERVAL = 500 ms`, balances 1.1 s, top positions 5 s).
- **Reconnect backoff**: `WSS_RETRY_INIT = 2 s` doubling to `WSS_RETRY_CAP = 30 s`. The spline pubsub uses a flat 5 s sleep instead.

### Channel topology (`tui::runtime::channels`)

| Channel | Direction | Payload | Producer | Consumer |
| --- | --- | --- | --- | --- |
| `tx_status` | unbounded | `TxStatusMsg` | every `tx::*` submit flow | `update_handlers::handle_tx_status_update` |
| `balance_tx` | unbounded | `BalanceUpdate` | `tasks::balances` | `handle_balance_update` |
| `wallet_usdc_tx` | unbounded | `f64` | `tasks::wallet_stream` (USDC ATA WSS) | `handle_wallet_usdc_update` |
| `wallet_sol_tx` | unbounded | `f64` | `tasks::wallet_stream` (native SOL WSS) | `handle_wallet_sol_update` |
| `tx_ctx_tx` | unbounded | `(authority, symbol, Arc<TxContext>)` | `tasks::tx_context` (one-shot per connect) | `handle_tx_context_update` |
| `orders_tx` | unbounded | `Vec<OrderInfo>` | `tasks::orders` (Phoenix WS trader sub) | `handle_orders_update` |
| `top_positions_tx` | unbounded | `Vec<TopPositionEntry>` | `tasks::position_leaderboard` (5 s) | `handle_position_leaderboard_update` |
| `liquidation_tx` | unbounded | `LiquidationEntry` | `tasks::liquidations` (always-on) | `handle_liquidation_update` |
| `market_tx` (in `app.rs`) | bounded 16 | `MarketListUpdate` | 60 s market poll | `handle_market_list_update` |
| `stat_tx` (in `app.rs`) | bounded 128 | `MarketStatUpdate` | per-symbol Phoenix WS stats | `handle_stat_update` |
| `l2_book_tx` (in event loop) | unbounded | `L2BookStreamMsg` | `tasks::l2_book` | `connection::handle_l2_book_msg` |

`KeyAction` (the return type of every key handler) tells the event loop what to do next: `Nothing` / `Redraw` / `BreakInner` (e.g. market switch — re-subscribe spline) / `BreakOuter` (quit) / `ReconnectRpc` (rebuild every WSS) / `ToggleClob` (start or abort the L2 task).

### Redraw policy

- **Force redraw**: keypress, clock tick (every 1 s aligned to UTC second).
- **Coalesced redraw**: stream + stats updates respect `FEED_REDRAW_MIN_INTERVAL = 150 ms` (`last_feed_paint`). State always updates; only the actual `terminal.draw` is throttled.
- Bump the constant up if CPU is high; down for snappier visuals.

---

## On-chain data model

The crate sees Phoenix Eternal in three flavours, each with a distinct decoder:

1. **Spline collection account** (per market) — the per-market spline-bid/ask account. Decoded by `phoenix_rise::types::accounts::SplineCollection::try_from_account_bytes` in [src/tui/data/spline_book.rs](src/tui/data/spline_book.rs). The decoder iterates `active_splines()` and emits `(trader_pda, price_start, price_end, density, filled, total_size)` rows. We wrap the call in `catch_unwind` to isolate panicky bytemuck mismatches.
2. **Phoenix CLOB market account** (the `Orderbook`) — decoded in `data::spline_book::parse_l2_book_from_market_account`. Yields `L2Level { trader_id: u32, price: f64, qty: f64 }`. `trader_id` is a sokoban pointer into the `GlobalTraderIndex`, **not** a wallet pubkey.
3. **GlobalTraderIndex (GTI) + Trader accounts** — `tui::data::trader_index` builds two maps in one refresh pass:
   - `node_addr (u32) → wallet authority pubkey` — for CLOB rows.
   - `trader_pda → wallet authority pubkey` — for spline rows (their `trader` is the PDA, not a node pointer).
   The cache fetches the GTI arena 0 raw bytes (`UiAccountEncoding::Base64`), reads `Superblock.num_arenas` at offset `GTI_HEADER_SIZE + 4 = 52`, then walks the sokoban tree (`GlobalTraderIndexTree`). It batches `getMultipleAccounts` (`RPC_BATCH_SIZE = 100`) for each trader PDA and reads the `authority` field from `DynamicTraderHeader` at offset `56..88`. Misses trigger a refresh via the `gti_refresh` `Notify`, throttled to once per `REFRESH_MIN_INTERVAL = 10 s`. Resolved rows are dropped (not flashed as placeholders) until the cache catches up.
4. **ActiveTraderBuffer** — `data::position_leaderboard::fetch_top_positions` scans the per-market ATB to build the `[T]` modal's leaderboard. Indexed by `SplineConfig::asset_id`.
5. **Liquidation events** — `tasks::liquidations` `logsSubscribe`s on Phoenix's sole liquidator (currently `BP7sV1VFnbPMPyJX1tZNbXHbZkyLNFEaBWJhyMvkbxKz`), then for each tx `getTransaction`s and walks inner instructions through `phoenix_eternal_types::events::parse_events_from_inner_instructions_with_context`. `MarketEvent::Liquidation`s are converted to display units via the cached `SplineConfig` table. The task survives reconnects so the buffer is warm on first modal open. `SIGNATURE_DEDUP_CAP = 256`, `MAX_CONCURRENT_GET_TX = 8`, `GET_TX_TIMEOUT = 8 s`.

### Display math

`tui::math` is the canonical place for tick / lot conversions:

- `ticks_to_price(ticks, tick_size, base_lot_decimals)` = `ticks * tick_size * 10^bld / 10^QUOTE_LOT_DECIMALS` where `QUOTE_LOT_DECIMALS = 6`.
- `base_lots_to_units(lots, bld)` = `lots / 10^bld` (negative `bld` ⇒ each lot is many units).
- `ui_size_to_num_base_lots(size, bld)` returns `Result<u64, LotConversionError>` with explicit handling of NaN / non-positive / over-cap (`MAX_UI_ORDER_SIZE_UNITS = 1e9`) / below one lot / `> u64::MAX` after scaling.
- `phoenix_decimal_to_num_base_lots(value, value_decimals, bld)` converts an HTTP-API `Decimal` exactly using checked integer arithmetic. Used during close-position to round-trip the on-chain raw lots when available (`PositionInfo::position_size_raw`).
- `pct_change_24h(mark, prev)` returns 0 when `prev == 0` (avoids div-by-zero for new markets).
- `compute_price_decimals(tick_size, bld)` is shared by `app.rs` and `config.rs`. Pathological inputs (`tick_size=1, bld=18`) are clamped to 18 decimals max; `tick_size=0` falls back to 2.

`SplineConfig` carries `tick_size`, `base_lot_decimals`, `spline_collection`, `market_pubkey`, `symbol`, `asset_id`, `price_decimals`, `size_decimals`. The spline pubkey from the HTTP API is verified against `program_ids::get_spline_collection_address_default(&market_pk)` and the derived address wins on mismatch (with a warn log).

---

## Trading domain

```
TradingState (state/trade_panel.rs)
├── side: TradingSide                            ── Tab toggles
├── size_index, custom_size                      ── + / - / Up / Down or [s]+digits
├── order_kind: OrderKind                        ── [t] cycles Market → Limit → StopMarket
├── input_mode: InputMode                        ── selects which input/* handler runs
├── input_buffer / deposit_buffer / withdraw_buffer / wallet_path_buffer
├── keypair: Option<Arc<Keypair>>                ── set on connect, cleared on disconnect
├── tx_context: Option<Arc<TxContext>>           ── set when one-shot tx_ctx_tx fires
├── usdc_balance / phoenix_balance / sol_balance
├── position: Option<PositionInfo>               ── active-symbol position only (PositionsView holds the rest)
├── status_timestamp / status_title / status_detail   ── status tray rendering
├── ledger: VecDeque<LedgerEntry>                ── newest-first, cap 50
└── config: UserConfig                           ── editable copy; saved via save_user_config
```

`TuiState` (in `state/tui.rs`) owns the rest: `price_history` (`MAX_PRICE_HISTORY = 150`), `merged_book`, `market_selector`, `positions_view`, `orders_view`, `top_positions_view`, `liquidation_feed_view`, plus chart-cache fields (`chart_data_cache`, `price_bounds_cache`, `chart_min`, `chart_max`).

- `push_price` keeps a running `chart_min`/`chart_max` so per-tick chart bound updates are O(1); a full rescan happens only when the popped sample was an extremum.
- `rebuild_merged_book` re-sorts both sides and computes `spread`; for `SOL` it floors at `MIN_SOL_SPREAD_USD = 0.01` so a near-zero spread does not flash.
- `begin_market_switch` / `complete_market_switch` keep stale chart/book visible until the first new-market WSS payload arrives, then flush. Chart markers are scrolled by `push_price` (subtract 1 from x each pop); trade markers are pruned when `x < 0`, **order chart markers are not** (the order is still live; the chart widget clips x out of range).

### Order kinds and confirmation

`OrderKind` is `Market | Limit { price } | StopMarket { trigger }` (one file per concept under `tui/trading/`). Submission paths:

| Action | Builder | File |
| --- | --- | --- |
| Market | `submit_market_order` | `tx/market_order.rs` |
| Limit | `submit_limit_order` | `tx/limit_order.rs` |
| Stop-market | `submit_stop_market_order` | `tx/stop_market_order.rs` |
| Cancel orders (batch incl. stops) | `submit_cancel_orders` | `tx/cancel.rs` |
| Close one / all positions | `submit_close_all_positions` | `tx/positions.rs` |
| Deposit / withdraw USDC | `submit_funds_transfer` | `tx/funds.rs` |

Stop-market direction mapping (matches Phoenix on-chain semantics):

```
TradingSide::Long  → Direction::LessThan     (long stops below)
TradingSide::Short → Direction::GreaterThan  (short stops above)
```

Cancellation must mirror the same mapping or the IX won't match the resting order; see `submit::stop_direction_for`.

### Pending action flow

```
Normal mode  ── KeyCode::Enter ──▶  InputMode::Confirming(PendingAction::PlaceOrder { ... })
                                    └─ status tray shows the "Confirm … (Y/N)" prompt

Confirming   ── 'y' / 'Y' ───────▶  submit::execute_confirmed_action
                                    └─ Dispatches to tx::submit_* with TxContext, keypair, and
                                       fresh `TxStatusMsg` updates streamed back through tx_status
             ── 'n' / 'N' / Esc ──▶  Reverts to InputMode::Normal with submit::cancel_message
```

Status messages are localized through `tui::i18n::strings()`. New status strings must be added to **both** `en.rs` and `zh.rs`.

### TxContext (per wallet × symbol)

[src/tui/tx/context.rs](src/tui/tx/context.rs) holds the per-session state needed to send transactions:

- Primary `RpcClient` at the configured URL with `CommitmentConfig::processed()`.
- `secondary_send_rpc`: an extra `Arc<RpcClient>` pointed at `https://api.mainnet-beta.solana.com` purely for `send_transaction` fan-out, **only** when the primary isn't already that URL (so we don't double-send to the same host). Confirmation always listens on the primary.
- `phoenix_rise::PhoenixMetadata` (cached `getExchange` result) and `MarketAddrs` (orderbook, spline, perp_asset_map, GTI vec, ATB vec) for the active market.
- `authority_v2` and `trader_pda_v2` (derived via `TraderKey::derive_pda`); `trader_registered: AtomicBool` is set true once an account exists at the PDA.
- `blockhash_pool: Mutex<VecDeque<[u8; 32]>>` — capped at 30 entries, refreshed by a background task. `pop_blockhash` consumes from the back (newest = most validity); when empty, falls back to a 5 s-bounded HTTP fetch (`BLOCKHASH_FETCH_TIMEOUT`). Each blockhash is consumed exactly once.
- `sig_pubsub: Mutex<Option<Arc<PubsubClient>>>` — a single shared pubsub client used by every order's `signatureSubscribe`, so we don't open one WSS per tx.

`TxContext::new` is awaited inside `tasks::tx_context::spawn_tx_context`; the resulting `Arc<TxContext>` is shipped via `tx_ctx_tx` so the event loop can attach it to `TradingState.tx_context` only if the `(authority, symbol)` still matches (late completions for replaced wallets / old markets are dropped — see `update_handlers::handle_tx_context_update`).

---

## Rendering pipeline

Layout (`tui::ui::render_frame`):

```
┌──────────────────────────────────────────────────────────────────────┐
│ Top vertical block (orderbook_height rows)                           │
│  ┌─────────────────────── 65% ──────────────────┬──── 35% ─────────┐ │
│  │ Order book (bids+asks, header+separator+...) │ Price chart       │ │
│  └──────────────────────────────────────────────┴───────────────────┘ │
├──────────────────────────────────────────────────────────────────────┤
│ Trading panel (height 6)        │ Funds panel (height 6)            │
├──────────────────────────────────────────────────────────────────────┤
│ Status tray (height 4, 2 body lines)                                 │
├──────────────────────────────────────────────────────────────────────┤
│ Min(0) — unused; modals overlay the whole frame area                 │
└──────────────────────────────────────────────────────────────────────┘
```

Modals (mutually exclusive via `InputMode`): MarketSelector, Positions, TopPositions, Liquidations, Orders, Ledger, Config / EditingRpcUrl, EditingWalletPath, ConfirmQuit. The "switching to …" modal can overlay any input mode while `state.switching_to` is `Some`.

`user_trader_prefix` is the user wallet's first 4 base58 chars. The book table uses this (not price) to decide which CLOB rows get the `>` arrow — multiple traders can share a tick.

`MODAL_BORDER` and `MODAL_HIGHLIGHT_BG` are deliberate single sources of truth for modal chrome; reuse them in new modals.

---

## Hotkey reference (Normal mode)

| Key | Action | File |
| --- | --- | --- |
| `q` | Open Quit confirm | `runtime/input/normal.rs` |
| `Ctrl+C` | Hard exit (`KeyAction::BreakOuter`) | `runtime/input/normal.rs` |
| `m` | Open market selector | `runtime/input/market.rs` |
| `c` | Open config modal | `runtime/input/settings.rs` |
| `o` | Open orders modal | `runtime/input/views.rs` |
| `p` | Open positions modal | `runtime/input/views.rs` |
| `T` | Open top-positions modal (capital — lowercase `t` is taken) | `runtime/input/views.rs` |
| `F` | Open liquidation feed modal | `runtime/input/views.rs` |
| `L` | Open ledger modal | `runtime/input/views.rs` |
| `Tab` | Toggle Long / Short | `runtime/input/normal.rs` |
| `t` | Cycle Market → Limit → StopMarket (seeds price from mark on entry) | `runtime/input/normal.rs` |
| `e` | Edit limit/stop price (seeds Limit from mark if currently Market) | `runtime/input/amounts.rs` |
| `s` | Edit size | `runtime/input/amounts.rs` |
| `+` / `=` / `↑` | Step up size preset (clears `custom_size`) | `runtime/input/normal.rs` |
| `-` / `↓` | Step down size preset | `runtime/input/normal.rs` |
| `w` | Load wallet (or disconnect when loaded) | `runtime/input/normal.rs` + `runtime/wallet.rs` |
| `Enter` | Confirm an order | `runtime/input/normal.rs` |
| `x` | Close active-market position | `runtime/input/normal.rs` |
| `d` / `D` | Deposit / Withdraw USDC | `runtime/input/normal.rs` |

`ORDER_SIZE_PRESETS` in `tui::constants` holds 29 entries from `0.0001` to `100_000.0`; `DEFAULT_SIZE_INDEX = 12` (= `0.1`).

---

## Style and conventions

The crate mirrors the [phx](https://github.com/skynetcap/phx) SDK. Stick to these:

- **rustfmt**: stable settings only (`max_width = 100`, `edition = "2021"`, `use_field_init_shorthand = true`, `use_try_shorthand = true`). Local `cargo fmt --all --check` must match CI.
- **Module headers**: every file opens with a `//!` doc header describing its role. Keep these one or two short lines.
- **One concept per file**: enums, structs, free functions live next to their tests in focused modules under `tui/trading/`, `tui/state/`, `tui/tx/`, etc. Don't pile types into a shared `types.rs`.
- **Cargo manifest**: table-aligned `key = value` formatting in `[package]` and similar blocks.
- **Tests**: `#[cfg(test)] mod tests { … }` lives at the bottom of the same file (or `#[path = "x_tests.rs"]` for state/tui-style splits). Use `tokio::test` only when the function is genuinely async.
- **`tracing`**: prefer `warn!` for recoverable errors. Logs go through `tracing-subscriber` with `RUST_LOG`; never `println!` in runtime code.
- **i18n**: any new user-visible string must be a field on `Strings` and have entries in both `en.rs` and `zh.rs`. Format placeholders ("Switching to {}…") are concatenated at the call site, not via `format!` strings inside the table.
- **No global mutable state** other than the existing `OnceLock`s in `config.rs`. Pass things through `&mut TuiState` or `Channels`.
- **No new `Arc<Mutex<…>>` over UI data** — channels go through `tokio::sync::mpsc`, and the event loop is the single owner.

---

## Common pitfalls

- **Stat channel is bounded** (`MarketStatUpdate`, cap 128). Producers `try_send` and drop on full. Don't switch to `send().await` — Phoenix recv loops indefinitely and a slow consumer would back up the WS stream.
- **Spline pubkey in HTTP API can mismatch** the derived `program_ids::get_spline_collection_address_default(market_pk)`. Cinder always uses the derived address (with a warn log) — don't change this without coordinating with Phoenix.
- **CLOB rows reference traders by `u32` node pointer**, not by pubkey. Resolving them needs the GTI cache. Spline rows reference traders by **PDA** (different pointer space). Both are mapped to wallet authority through `GtiCache`, but via different fields (`authorities` vs `pda_to_authority`).
- **Stop-market direction mapping is asymmetric** (long ↔ LessThan, short ↔ GreaterThan). Mismatched direction at cancel time silently fails to find the resting order.
- **Wallet path detection** distinguishes paths from base58 keypair strings via `looks_like_filesystem_path`. Slashes, backslashes, leading `./` / `../`, leading `/`, or a Windows drive prefix (`C:\…` / `C:/…`) all force the path branch. Don't accept arbitrary strings as base58 if any path-like character appears.
- **Blockhashes are consumed once** — never put one back into the pool. The pool grows from the back (`push_blockhash`) and pops from the back (newest first) so each tx gets ~150 blocks of validity. An empty pool falls back to a 5 s HTTP fetch.
- **`accountSubscribe` on the spline / market account uses `CommitmentConfig::processed()`** for snappier UI; transaction confirmation uses `processed` for polling and progresses through the standard commitment escalation. Don't mix these up when adding new subscriptions.
- **Reconnect tears down**: a full RPC reconnect (user saved a new RPC URL) aborts the wallet WSS, blockhash refresh, tx-context, liquidation feed, and L2 task before rebuilding. See `connection::handle_full_rpc_reconnect`. Adding a new long-lived task means wiring it into both the spawn site and that teardown helper.
- **Liquidation feed is process-lifetime**. Toggling the modal does not stop the task — that's intentional so the buffer is warm on first open. Don't add a "stop on close" path.
- **Transmute-heavy interop**: `tui/mod.rs` allows `clippy::missing_transmute_annotations`, `clippy::too_many_arguments`, and `clippy::type_complexity`. These are deliberate concessions to the Solana SDK shape — don't try to "fix" them with annotations or refactors unrelated to your change.

---

## Editing this file

Keep this document tight and pointer-rich — it's read by future agents before they touch anything. Update it when you:

- Add or remove a top-level module / file under `src/tui/`.
- Change the channel topology (new producer / consumer, new payload).
- Change a pinned dependency version (especially the `=2.3.13` solana stack).
- Touch a tuning constant (`FEED_REDRAW_MIN_INTERVAL`, `L2_POLL_INTERVAL`, `WSS_RETRY_*`, `MAX_PRICE_HISTORY`, `TOP_N`, `LEDGER_CAPACITY`, `SIGNATURE_DEDUP_CAP`, `MAX_CONCURRENT_GET_TX`, `GET_TX_TIMEOUT`, `BLOCKHASH_FETCH_TIMEOUT`, `REFRESH_MIN_INTERVAL`, `RPC_BATCH_SIZE`).
- Add a new InputMode, KeyAction variant, or modal.

Out of scope for this file: per-task implementation detail (read the source), one-off bug fixes (commit message), and ephemeral release notes (CHANGELOG / git log).
