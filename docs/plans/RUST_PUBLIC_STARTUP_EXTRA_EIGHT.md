# Additional public startup families

This records the eight-family implementation and focused regression coverage for
`feat/rust-public-startup-resilience`. The shared policy and overall gate status
remain in `RUST_PUBLIC_STARTUP.md`; this is not a claim that all venue or PR gates
have passed. Tests use scripted transports and no live credentials.

| Family / aliases | Public discovery and quote behavior | Regression coverage |
| --- | --- | --- |
| ApeX Omni (`apexomni`) | Catalogue retries preserve usable peers; public read failures warn. Public reference prices must be finite and positive, and computed PnL must be finite. | `startup_resilience`, `public_feed`; native private valuation has separate `startup_valuation` coverage. |
| Bitrue (`bitrue`) | Catalogue read retries warn; catalogue starts private account independently of ticker seeds. | `startup_resilience`; funded private valuation is covered separately in `startup_valuation`. |
| Coinbase (`coinbase`, `coinbasespot`) | Failed paginated discovery never commits a partial failed page walk. All rejected catalogue rows retain the old catalogue and retry. A valid catalogue with unusable quotes remains usable for private startup and retries quotes without replacing prior prices. | `public`, `startup_resilience`; pacing and later-page failures are exercised. |
| GRVT (`grvt`) | All unusable catalogue rows preserve both existing catalogue and native instrument/signing map and retry. Valid peers survive an isolated malformed row. | `startup_resilience` covers initially empty and previously loaded stores, retry and recovery. |
| Kraken (`kraken`, `krakenspot`) | Catalogue/ticker failures retain old state and retry; blank-to-zero parsing is limited to documented REST ticker fields. Strict book/candle and spot WebSocket fields remain strict. Live ticker field masks preserve early quotes and omitted live peers across REST seeding. Spot native currency balances are revalued when the first real quote seed arrives. | `public`, `spot_public`, both startup resilience targets and account tests; blank book sizes cannot delete levels, explicit zero remains a deletion. |
| Paradex (`paradex`) | Entirely unusable catalogue/ticker snapshots retain prior metadata and quotes and retry. BBO/trade/summary fields received live survive a delayed REST seed without suppressing unrelated REST statistics. Depth sizes use the strict decoder. | `public`, `startup_resilience`; all-invalid catalogue preservation and atomic invalid-depth rejection. |
| Polymarket (`polymarket`) | Entirely unusable Gamma discovery preserves catalogue/outcome metadata and retries. Every requested midpoint ID must have a valid price; valid peers publish while missing or invalid peers retry their batch. Foreign-batch IDs do not fulfill or modify another batch. Later batches proceed independently. Live supplied quote fields survive seeds and retry windows. | `public`, `startup_resilience`; omitted ID, invalid ID, 101-outcome batch seam, recovery and early live/late REST races. |
| TxFlow (`txflow`) | Failed reads retry without accumulating failed rows. An ambiguous native catalogue remains rejected with a warning and backoff because guessing an asset identity could misroute signed orders or history. Private sockets start after metadata. Sparse live context fields survive delayed per-market REST seeds; omitted fields still receive valid REST statistics. | `public`, `startup_resilience`, `account`, `trading`; fixtures retain early private actions rather than assuming a ticker prerequisite. |

The public policy must not introduce false empty private exposure after a
catalogue row is skipped. Kraken futures rejects authoritative order/position
snapshots containing unmappable `PF_` exposure, preserves its previous state,
logs and schedules reconciliation; unsupported dated/inverse products remain
outside this adapter's existing scope. Explicit zero positions remain harmless.
Paradex rejects authoritative snapshots containing unmappable perpetual
exposure, retains previous state and warns. Incremental unmappable perpetual
updates warn and schedule repair; intentionally excluded options/spot remain
excluded, and explicit zero/closed positions remain harmless. Both families
have regressions proving preservation and later valid empty removal. This is a
bounded guard against a new public-discovery hazard, not the planned generic
private-row coverage/peer-merging implementation.

Native verification uses the complete tests and all-target Clippy for the eight
crates, with default features (including Coinbase/Kraken spot siblings):

```sh
cargo test --manifest-path sdk-rs/Cargo.toml --locked \
  -p safe-cex-venue-apexomni -p safe-cex-venue-bitrue \
  -p safe-cex-venue-coinbase -p safe-cex-venue-grvt \
  -p safe-cex-venue-kraken -p safe-cex-venue-paradex \
  -p safe-cex-venue-polymarket -p safe-cex-venue-txflow --no-fail-fast
cargo clippy --manifest-path sdk-rs/Cargo.toml --locked \
  -p safe-cex-venue-apexomni -p safe-cex-venue-bitrue \
  -p safe-cex-venue-coinbase -p safe-cex-venue-grvt \
  -p safe-cex-venue-kraken -p safe-cex-venue-paradex \
  -p safe-cex-venue-polymarket -p safe-cex-venue-txflow \
  --all-targets -- -D warnings
```
