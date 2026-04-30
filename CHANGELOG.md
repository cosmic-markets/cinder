# Changelog

All notable changes to Cinder will be documented in this file.

## 0.1.0 - Unreleased

### Added

- First public release candidate for the Cinder Phoenix perpetuals TUI.
- Live market discovery, Phoenix stats streaming, spline/CLOB book rendering,
  wallet-scoped balances, positions, orders, and top-position views.
- Market, limit, stop-market, close-position, deposit, and withdraw flows with
  confirmation prompts.
- GitHub Actions release checks for formatting, clippy, tests, package smoke,
  Docker build, and dependency audit.

### Changed

- Transaction sends now use preflight by default.
- Confirmation timeout copy now tells users the transaction may still be
  pending and should be checked by signature before retrying.
- Transaction error logs are written under the Cinder config/log directory
  instead of the current working directory.

### Security

- User-entered order, close, deposit, and withdrawal values are validated before
  transaction construction.
