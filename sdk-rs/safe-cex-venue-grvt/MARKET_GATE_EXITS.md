# GRVT shared market field and runtime read gates

Measured implementation: `94a3ef0df663499692115dd8f41b9da7b1bc67d3`.
Immediate parent: `87cf25ff3bba094c15fcc8c72f7dd1c8f0d02a75`.
The field branch started at integration `54851c81b7`; field commit
`dc94458895` then merged integration `f22d7b2c94b74e1f29f96d28b6d07c3163bb5409`
down through `ba2f8efb5f`. PR 7439 was merged before these measurements.
The evidence commit changes only this file; its tip is not the measured SHA.

Independent Python tomllib comparison of committed GRVT family manifests at
`f22d7b2c94` and the measured SHA found **2 rows pending -> divergent, 0 ported**:
`GrvtExchange.fetchMarkets` and `GrvtExchange.refreshMarkets`.
The census stopped after the family's 164 manifest rows: counts moved from
33 ported / 40 divergent / 91 pending to 33 / 42 / 89.
The existing shared `Market` row stays ported and now names its optional boolean.
Both shared declarations, strict decoder, numeric projection, and Store patch
mapping carry that field; there is no venue-local Market or flag sidecar.

The real CHOSEN differences are recorded in the rows. Fetch refuses invalid
raw base/quote values for the shared string fields, after all source evaluation
and success logging; its JSON-data info omits undefined slots and uses the
shared UTF-8 boundary. Refresh owns captured old values and replacement/event
arrays rather than JS aliases; source listeners can change arrays and subsequent
counts/results. Its explicit alias witness observes source `mutated-a` versus
native retained `old-a`. These are implemented operations with named differences,
not placeholders for the still-pending constructor or HTTP client factories.

## Measured exits

Every cargo command used `--manifest-path sdk-rs/Cargo.toml --jobs 2` and ran
sequentially. All oracle tests set
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate at measured implementation | Exit and observed outcome |
| --- | --- |
| `cargo check -p safe-cex-types --all-targets` | 0, Finished dev profile |
| `cargo check -p safe-cex-venue-grvt --all-targets` | 0, Finished dev profile |
| `cargo test -p safe-cex-types --test market_perpetual` | 0, 1 passed |
| `cargo test -p safe-cex-venue-grvt --test markets`, Node 24.20.0 | 0, 4 passed |
| Same market target, PATH selecting Node 26.7.0 | 0, 4 passed |
| Pinned Yarn JS `parity:extract:check` | 0, output: 0 manifests updated, 0 added, 0 orphaned |

`market_perpetual_native_boolean_and_absence` directly asserts decoded None,
Some(true), Some(false), independent spot absence, and present-null refusal;
all native assertions precede its wire assertions. Fetch asserts returned native
Market fields and Number bits, including -0/overflow signing info, before any
wire comparison (there is no wire roundtrip). Its fixed HTTP source/native floor
stops after 11 cases plus one explicit typed-refusal case. The configured post
hook supplies already transformed response.data; hook spies do not prove Rust
Axios factory/transport implementation. Refresh stops after seven fixed
metadata/membership/reorder/error cases plus one measured alias boundary.
Counts use fixed independent expectations; the existing source witness's raw
input controls were not rerun in this slice.

## Discriminating controls

Each control compiled, failed a native assertion (exit 101), then was restored
byte-for-byte and its exact target passed (exit 0). No controls survived.

| Deliberate change | Target | Named assertion |
| --- | --- | --- |
| Rename strict decoder's isPerpetual input key | `market_perpetual_native_boolean_and_absence` | `MARKET native perpetual boolean` |
| Negate mapped is_perpetual in src/markets.rs | `fetch_markets_native_source_outcomes` | `MARKETS native perpetual branch` |
| Skip Store replacement in src/markets.rs | `refresh_markets_native_store_and_effect_order` | `REFRESH native replaced signing hash` |

An earlier working-tree GRVT check failed exit 101 because the shared numeric
Market projection needed the added field in its exhaustive destructure. Commit
`87cf25ff3b` fixes it; the measured checks above pass. Native sub-agent review
approved the shared field, projection ancestor, and runtime at the measured SHA.
The reviewer also ran its seven-case refresh source oracle and a callback alias
probe, both exit 0; those are reviewer measurements, separate from root gates.
No unreviewed feature-branch work was imported from another open PR.

Not run: workspace compilation, other venue crate checks, full test suites,
wasm checks/tests, host clippy, wasm clippy, or CI reruns. The inherited 26 Rust
manifest-validation diagnostics were left alone as instructed; the Rust
validator was not rerun. This file claims host scoped gates only.
