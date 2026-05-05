# Changelog

All notable changes to Cinder are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.8] - 2026-05-04

### Added
- Wallet path persistence in user configuration so the last-used wallet is
  remembered across sessions.
- Skip-preflight setting in user configuration.
- Skip order confirmation setting in user configuration.
- Auto priority fee refresh; the TUI now keeps the compute unit price in sync
  with live network conditions.

### Changed
- Auto-derived compute unit price now tracks the p90 (instead of p75) priority
  fee for more reliable inclusion under load.
- Signature status is polled every 350ms during confirmation for snappier
  transaction feedback.

## [0.1.7] - 2026-05-04

### Added
- Custom referral code modal and improved referral handling flow.
- First-run referral choice modal with localized titles and messages across
  all supported languages.
- Public RPC fan-out option in user settings.
- Compute unit price and limit settings in user configuration.
- Spline collection derivation for market accounts.
- Friendly status mapping for BPF compute-unit-meter RPC errors.
- Localized error messages for transaction failures across all supported
  languages.

### Changed
- Streamlined spline collection derivation tests.
- Updated dependencies for improved stability and performance.
- README: refreshed demo image, expanded i18n notes, clarified referral
  funding/discount details and donations section, and added a note about
  pre-compiled Windows and Linux binaries.

### Fixed
- Wallet file path modal now normalizes backslashes on Windows.
- Improved formatting of referral messages in Spanish and Russian.

## [0.1.6] - 2026-05-03

### Added
- Russian and Spanish localizations.
- Version information in the status tray.
- Microprice EMA replaces midpoint data in the price chart, with localized
  labels in all supported languages.
- Progress bar and credit display on the splash screen.

## [0.1.5] - 2026-05-02

### Added
- Hidden iceberg flag on spline and book row structures, with iceberg price
  tracking and a marker indicator in the order book UI.

### Changed
- Removed the obsolete `cinder_spline_debug.txt` file from the repo.
- Spline data parsing now performs an active region check.

### Fixed
- rustfmt fix for the l2_book RPC client init.

## [0.1.4] - 2026-05-02

### Changed
- Replaced the bandwidth-heavy CLOB websocket subscription with 500 ms
  polling to reduce data usage.
- RPC commitment level changed from `confirmed` to `processed` for lower
  latency UI updates.
- Reduced CPU usage in the app loop.
- Adjusted spacing in the status tray rendering.

## [0.1.3] - 2026-05-02

### Added
- Isolated margin support, including UI rendering of an `isolated_only` flag
  on `MarketInfo` and dedicated cross-margin error messages.
- CLOB quotes are now derived from splines; the price-range column has been
  replaced and traders on the same price level are coalesced.
- Trader display logic improvements in the order book; trader prefix shortened
  to a single character.

### Changed
- Updated homepage URL.

### Fixed
- Spline-to-quote logic now correctly handles hidden size.
- Isolated-order "insufficient transferable funds" API errors are mapped to
  the existing insufficient-funds status.
- rustfmt build fix.

## [0.1.2] - 2026-05-01

### Added
- Spline bootstrap messaging when switching markets.

### Fixed
- Order book correctness on market switch.
- Liquidity checks refined and crossed-spline rows handled.
- Post-only market errors normalized in transaction status.
- clippy `manual-clamp` lint in the liquidation feed modal.
- rustfmt fixes for liquidation UI and status hints.

## [0.1.1] - 2026-05-01

### Added
- Liquidation feed modal with backfilling status, position-side parsing,
  trader prefix display, "open market" action, and localized strings.
- Compact time formatter for the liquidation feed.
- `stat_handles` on `LoadedSetup` for managing market-stats subscriptions;
  market stats are now cached and shown instantly when switching markets.
- Loading screen / splash renderer.
- Multi-platform GitHub release workflow; Windows artifact published as ZIP.

### Changed
- Increased backfill concurrency limit and backfill transaction fetch limit.
- README cleanup: removed requirements, trading safety, tests/linting and
  contributing sections; refined feature descriptions and build instructions.

### Fixed
- Crossed splines rendering issue.
- Stop-loss orders missing from the orders modal.
- rustfmt fix in splash renderer.
- macOS build step removed from the release workflow; the workflow now
  continues on cache errors.

## [0.1.0] - 2026-04-29

Initial public release of Cinder.

### Added
- First Cinder TUI release with core trading workflows.
- Wallet input resolution that handles filesystem paths and JSON keypair
  files.
- Docker Compose setup with `RPC_URL` documented as optional.
- README and project assets, including the Cinder TUI screenshot.

### Changed
- Removed the dependency-audit job from the CI workflow.
- Streamlined README by removing outdated keyboard shortcuts, environment
  variable instructions, TUI layout, and repository layout sections.
- Removed the `NOTICE` file.

[0.1.8]: https://github.com/skynetcap/cinder-release/compare/v0.1.7...v0.1.8
[0.1.7]: https://github.com/skynetcap/cinder-release/compare/v0.1.6...v0.1.7
[0.1.6]: https://github.com/skynetcap/cinder-release/compare/v0.1.5...v0.1.6
[0.1.5]: https://github.com/skynetcap/cinder-release/compare/v0.1.4...v0.1.5
[0.1.4]: https://github.com/skynetcap/cinder-release/compare/v0.1.3...v0.1.4
[0.1.3]: https://github.com/skynetcap/cinder-release/compare/v0.1.2...v0.1.3
[0.1.2]: https://github.com/skynetcap/cinder-release/compare/v0.1.1...v0.1.2
[0.1.1]: https://github.com/skynetcap/cinder-release/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/skynetcap/cinder-release/releases/tag/v0.1.0
