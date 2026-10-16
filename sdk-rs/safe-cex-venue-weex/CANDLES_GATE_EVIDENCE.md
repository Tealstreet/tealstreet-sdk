# WEEX candle recovery gate evidence

Measured code SHA: `6d58a3a46f6618d7476dfbae73764a9c106cef0d`.
Direct code parent: `c329dca4f17bcfb102279095647b4d55ecb6f0f8`.
Exact recovered remote head: `3a8d14a75d7ee1c2f2e8827eb41860864434f045`.
Integration merged down: `355683b84a7c62f3f5c9e91910c7e15438c57076`
(after the earlier requested `36f5fca044b13fd4b674049b98941aa8632be8bf`).

The local branch named candles was actually at e93d28f0c6 with nine unpublished
KuCoin commits beyond the remote. Its history was preserved untouched. Recovery
uses a separate local branch based on the exact pushed 3a8d14a75d and fast-forwards
origin/feat/rust-venue-weex-candles. Those extra local KuCoin ancestors are excluded.
The requested dead-lane commits themselves remain the recovered unreviewed ancestry;
this sheet establishes fresh gates rather than claiming their old measurements.

Five conflicts were resolved. Both CLAUDE conflict sections were retained in full.
Integration's balance implementation, candle case/refusal assertions and arity
checks were retained alongside recovered cardinality/source inventory guards.
Mutation script labels were reconciled with the earlier recovered assertions.
Production WEEX source is byte-identical to integration; the recovered delta is
additional test coverage, census metadata and two pending income notes.

Counts measured with Python 3.12 tomllib over the six WEEX manifests:

| Revision | Ported | Divergent | Pending |
| --- | ---: | ---: | ---: |
| Recovered 3a8d14a75d | 26 | 19 | 111 |
| Integration 355683b84a | 26 | 24 | 106 |
| Measured merged code | 26 | 24 | 106 |

**45 recovered off-pending rows = 26 ported + 19 divergent. Net row-status moves
against integration: 0 pending -> ported, 0 pending -> divergent.** All 45 are
already mapped in the newer integration. This authorized recovery preserves their
additional verification and resolves PR 7419; it is not claimed as 45 new ports.

Target audit: all 45 recovered rows and all 50 currently mapped rows have nonempty
rs targets and existing Rust files. The canonical scoped status/invention binary
also resolved their Rust symbol targets: zero missing targets, zero WEEX exemptions,
156 rows and 55 public Rust items. Empty targets were not filled with placeholders.

Oracle environment for every cargo test/control:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.
All cargo processes ran sequentially with this lane's check/test jobs set to 2.

| Gate executed at measured code SHA | Exit | Observed result |
| --- | ---: | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0 | Scoped crate/dependencies checked |
| cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 | 0 | 14 tests passed across 12 integration-test targets; unit/doc targets contain 0 tests |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/recovery-family-mutations.py | 0 | Eight family controls plus two census controls failed as required, every restoration passed |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0 | Canonical WEEX invention/status/target gate passed |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0 | Output read: 7341 symbols; would update 0 manifests, 0 added, 0 orphaned |
| Independent tomllib target/file and SHA256 audit | 0 | 45 recovered / 50 mapped rows; zero empty/missing-file targets; eight original oracles unchanged |

Each control compiled and returned cargo exit 101 at the named assertion below;
a compile failure or unrelated assertion was not accepted. Each mutated file was
restored byte-for-byte and its affected target then exited 0 before the next control.

| Recovered family | Rows | Named native assertion observed |
| --- | ---: | --- |
| Constants | 26 | constant::ORDER_SIDE::native |
| Response codecs | 3 | native::kline_signed_zero::direct_raw |
| Enum normalizers | 7 | utility::normalizePositionMode::"json_35"::route_0 |
| Numeric helpers | 3 | numeric_utility::toNumber::"raw_14"::route_0 |
| Timestamp helpers | 2 | timestamp::normalizeTimestampToMs::nanoseconds::UTC::route_0 |
| Order-book helpers | 2 | book::normalizeOrderBookSide::distinct::route_0::0::price |
| Fake positions | 1 | fake_position::0::entryPrice |
| Candle normalizer | 1 | candle::tuple::route_0::open |

Additional merge controls: empty candle cases failed
`cardinality::candles::cases::source_count`; duplicate refusal raw input failed
`cardinality::candles::refusals::unique_identities`. Both compiled with exit 101,
and restored candles passed with exit 0. No control survived these ten controls.
This is one production control per row family, not an individual mutation proof
for each of the 45 symbols or every field.

Tests assert native fields, bits, enum variants, lengths and collection shapes
before wire assertions. Fixed inventories and explicit outcome floors are not
regenerated during test execution. The finite inventory stops at the eight
recovered corpus families in cardinality-census.json: 26 constant exports,
273 response routes, 6384 enum outcomes, 2132 numeric outcomes, 1176 timestamp
outcomes, 160 order-book level comparisons, 168 position comparisons and 92 candle
outcomes. These counts do not claim exhaustive API or every-field coverage.

Read-only sub-agent review found no concrete merged-test defect. Independent
SHA256 checks (main agent and reviewer) confirmed all eight original oracle files
match their recorded hashes and integration bytes. The stored hashes are metadata;
tests do not enforce oracle file hashes at runtime. The reviewer ran no cargo,
test suites or mutation controls.

Not run: full workspace compilation, monorepo suites/builds, host clippy, wasm
clippy, live venue/network tests, CI reruns, all legacy mutation matrices, or the
shared consuming-array mutation script. No income pager was implemented here.

This tracked sheet is an evidence-only descendant. The gate exits above cover
the named code SHA, not a falsely claimed build of the evidence commit itself.
