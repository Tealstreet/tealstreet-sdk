# GRVT snapshot gate exits

Implementation SHA: `cfb6583b192dec80575e6d068e4e8cfe60f18557`. Parent built on: `b6334d224847aa18aacb3f1da51b2d7bbdcb619d`.
No unreviewed branch is an ancestor; PR 7403 was not cherry-picked or merged.
Normal gates below ran on this exact implementation tree. Mutation gates used that SHA plus the stated temporary function-body change. Every mutation was restored before normal gates.

Moved **9 rows pending→ported, 1→divergent**, measured independently with Python 3.14 `tomllib` over `parity/manifest/exchanges/grvt` and symbol-by-symbol comparison with `git show` of the parent.
Family census before: `{'pending': 103, 'ported': 24, 'divergent': 37}`; after: `{'pending': 93, 'ported': 33, 'divergent': 38}`. This is a manifest census, not completeness proof.

| Source symbol | Status |
| --- | --- |
| `GrvtExchange.dispose` | ported |
| `GrvtExchange.recordNonce` | ported |
| `GrvtExchange.getPositionMode` | ported |
| `GrvtExchange#calculatePositionUpnlFromLastPrice` | ported |
| `GrvtExchange#calculateUpnlFromPositions` | ported |
| `GrvtExchange.syncBalanceUpnlFromPositions` | ported |
| `GrvtExchange.getAccount` | ported |
| `GrvtExchange.fetchTickers` | ported |
| `GrvtExchange.getFakePositions` | divergent |
| `GrvtExchange.fetchOrders` | ported |

The single divergence is CHOSEN closed typed output: inherited fetched leverage yields source `undefined` buyLeverage, while native Position requires a Number. The whole output batch is explicitly refused. No local Market.isPerpetual carrier was added.

## Gate exits

Scoped test command: `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-venue-grvt --test snapshots --jobs 2 -- --nocapture`.
All cargo invocations were sequential. Node captures were arm64, v24.20.0 and v26.7.0. NaN only is canonicalized in source capture.

| Gate | Observed exit | Result |
| --- | ---: | --- |
| baseline-node24 | 0 | 12 tests passed |
| restored-node24 | 0 | 12 tests passed |
| restored-node26 | 0 | 12 tests passed |
| cargo-check | 0 | GRVT crate and its dependencies, all targets |
| `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --all-targets --jobs 2` | 0 | scoped compilation; no workspace compilation claim |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract` | 0 | writer reconciled one manifest; 0 added/orphaned symbols |
| `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check` | 0 | output: Would update 0 manifests; 0 added, 0 orphaned symbols |
| changed Rust files: `rustfmt --edition 2024 --check` | 0 | snapshots.rs source and test |
| `git diff --check` | 0 | no whitespace errors |
| Rust `parity-check` via scoped `cargo run` | 1 | inherited baseline failures; see below |

Rust validator command: `cargo run --manifest-path sdk-rs/Cargo.toml --locked -p safe-cex-parity --bin parity-check --jobs 2 --`.
Parent comparison: both exits were 1, with 26 identical diagnostics and no GRVT diagnostics. The parent probe temporarily restored the parent module root, dependency manifest and parity manifest; the new snapshots module was absent from that module graph. All committed files were restored byte-for-byte afterward.

## One compiling mutation per newly mapped row

Each row below compiled, then its named native assertion failed with exit **101**. There were **0 survivors**. The restored Node24/26 suites both passed afterward.

| Method | Temporary mutation | Named assertion | Exit |
| --- | --- | --- | ---: |
| `record_nonce` | `while creation_times.len() > 200 → while creation_times.len() > 201` | NONCE independent retention count | 101 |
| `get_position_mode` | `PositionMode::OneWay → PositionMode::Hedged` | MODE native variant | 101 |
| `calculate_position_upnl_from_last_price` | `let derived = direction * → let derived = -direction *` | POSITION PNL native bits | 101 |
| `calculate_upnl_from_positions` | `let mut total = 0.0; → let mut total = 1.0;` | TOTAL PNL native bits | 101 |
| `sync_balance_upnl_from_positions` | `patch.set_number("unrealizedPnl", derived); → patch.set_number("realizedPnl", derived);` | SYNC native unrealizedPnl | 101 |
| `get_fake_positions` | `.unwrap_or(PositionSide::Long) → .unwrap_or(PositionSide::Short)` | FAKE native side | 101 |
| `fetch_tickers` | `id: market.id.clone(), → id: market.symbol.clone(),` | TICKER native id | 101 |
| `dispose` | `disposed.set(true); → disposed.set(false);` | DISPOSE native disposed state | 101 |
| `fetch_orders` | `.filter(\|values\| values.len() == 1) → .filter(\|values\| values.len() == 2)` | ORDERS native forwarded filter | 101 |
| `get_account` | `let response = post("/lite/v1/account_summary", request); → let response = post("/lite/v1/account_summary", request);     let captured = account_id();; account_id().unwrap_or_default() -> captured.unwrap_or_default()` | ACCOUNT native user id | 101 |

## Proof floor and limits

The actual AST-extracted source methods supply 209 nonce calls, one mode result, 19 PnL/sync cases, four fake-position cases, six ticker cases, three disposal cases, eight order-filter/result/error cases, and 19 account cases. Enumeration stopped after those fixed input lists; it is not exhaustive. Counts are literal independent expectations, and decoded native fields/Number bits precede effect checks. No wire roundtrip comparison is used.
The extra native sync probes verify original subscriber name/message/context, partial position mutation before listener failure, and a balance field changed by a reentrant subscriber. Duplicate-ID sync inputs remain outside this corpus floor. Fake positions instead test duplicate symbols with distinct IDs so the shared Store does not deduplicate the fixture before the helper sees it.
Sub-agent approved exact implementation SHA cfb6583b192dec80575e6d068e4e8cfe60f18557. No human review was requested.
Configured client, socket and open-order hooks remain their own pending runtime rows. fetchMarkets and refreshMarkets remain pending: shared Market.isPerpetual is absent and the user reserved it to the types owner; a clarification is pending. No local sidecar or prior unreviewed PR work was imported.
Not run: workspace compilation, full GRVT venue-oracle suite, host Clippy, wasm check/Clippy, CI, live exchange requests.

Implementation inputs verified against git show before evidence recording:

- `sdk-rs/safe-cex-venue-grvt/src/snapshots.rs` SHA-256 `c67e184df520cc827fb1952e02b9f673749d334374dfb252722bb18346194124`
- `sdk-rs/safe-cex-venue-grvt/tests/snapshots.rs` SHA-256 `990dbc301da2d465aa3bb6bb7997b88f75f3a52aa14f9ec6296bd1311ebf690b`
- `sdk-rs/safe-cex-venue-grvt/tests/snapshots.cjs` SHA-256 `883ccbd94ac38c7ac06749e3d8a53ef6119ea7ce5fca5329e490b3923fdbb0b5`
- `sdk-rs/parity/manifest/exchanges/grvt/grvt.exchange.ts.toml` SHA-256 `5f13d859b65b42175dca6e2ad1cff684a2cd1444b7187f2303d3e843954b0f82`

Merge-down verification: integration parent `54851c81b7` was merged into the
snapshot branch; measured implementation SHA
`95bdc748211201ceeb03a0514001663e5510be20`.
The only runtime compatibility fix initializes the new optional Ticker timestamp
as absent. The native ticker assertion now checks that absence explicitly.
Scoped GRVT all-targets cargo check (jobs 2): exit 0. Host Node 24 snapshot test
file (12 tests, jobs 2, explicit TypeScript path): exit 0, 12 passed.
Extractor check via the pinned Yarn JS entry point: exit 0, output says
0 manifests updated, 0 added, 0 orphaned symbols. Initial merge check failed
exit 101 on the missing timestamp initializer; the corrected check above passed.
Native sub-agent review of `b57b8759ac454f3d69b9faa52735c28cdc870b70` found no bugs;
its missing native timestamp coverage finding was addressed in the measured SHA.
No new rows moved by the merge. Node 26, wasm checks, clippy, workspace builds,
and the earlier mutation controls were not rerun for this merge.
