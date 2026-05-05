# Security Policy

Cinder is a terminal UI that signs and submits live Solana transactions
against Phoenix perpetual markets. Bugs in this codebase can move user funds,
so we treat security reports as a top priority.

## Supported Versions

Security fixes land on `main` and ship in the next patch release. There is no
long-term support branch; please reproduce on the latest tagged release (or
the current `main`) before reporting.


| Version        | Status            |
| -------------- | ----------------- |
| Latest release | Fixes accepted    |
| `main`         | Fixes accepted    |
| Older tagged   | Upgrade to latest |


## Reporting a Vulnerability

**Do not open a public GitHub issue, PR, or discussion** for anything that
could expose wallet material, forge or re-route transactions, or otherwise
put user funds at risk.

Use one of these private channels instead:

1. **GitHub Security Advisories** — preferred. Open a draft advisory at
  [https://github.com/cosmic-markets/cinder/security/advisories/new](https://github.com/cosmic-markets/cinder/security/advisories/new).
2. **Email** — [michael@cosmic.markets](mailto:michael@cosmic.markets). Use the subject line
  `cinder security:` followed by a short summary. Encrypted mail is welcome;
   request a key in the first message if you want one.

Please include:

- Affected version, commit hash, or branch.
- A minimal reproduction (steps, inputs, RPC endpoint type if relevant).
- Observed vs. expected behavior.
- Whether a wallet, keypair, mnemonic, transaction signature, RPC URL, or
other credential could leak — and the realistic blast radius.
- Suggested mitigation, if you have one.
- Whether you intend to publish or present the finding, and on what timeline.

If you have already accidentally exposed key material while reproducing,
say so — we would rather know.

## Scope

In scope:

- Transaction construction, signing, and submission paths.
- Keypair, mnemonic, and signer handling (loading, in-memory lifetime,
clipboard interaction via `arboard`).
- RPC and WebSocket client behavior, including TLS configuration
(`rustls`, pubsub, commitment handling).
- Phoenix order entry, cancel, and liquidation flows.
- Dependency vulnerabilities that are actually reachable from Cinder.
- Local file handling (config, dotenv, logs) that could leak secrets.

Out of scope:

- Bugs in upstream Solana / Phoenix programs themselves — report those to
the relevant project.
- RPC endpoint outages, rate limits, or third-party node misbehavior.
- Theoretical issues without a working PoC against current `main`.
- Findings that require an already-compromised host (root access, malicious
shell history scraper, etc.) unless Cinder makes the compromise materially
worse.
- Social engineering of maintainers or users.

## Handling Expectations

Once you report:

- **Within 3 business days** — acknowledgement that the report was received
and is being triaged.
- **Within 10 business days** — initial assessment: severity, whether we can
reproduce, and a rough remediation plan.
- **Coordinated disclosure** — we aim to ship a fix within 90 days of the
initial report, sooner for high-severity issues that are exploitable in
the wild. We will agree on a public disclosure date with you before
publishing.

Reporters who follow this policy are credited in the release notes and the
published advisory unless they ask to remain anonymous. We do not currently
run a paid bounty program.

## Working With Us

We welcome security research on Cinder. A few practical notes so we can
work together smoothly:

- Test against your own wallets and accounts.
- Let us know before going public so we can ship a fix alongside disclosure.
- If something is ambiguous, just ask — [michael@cosmic.markets](mailto:michael@cosmic.markets) is the
fastest way to reach a maintainer.

Good-faith research that follows this policy is always welcome here.