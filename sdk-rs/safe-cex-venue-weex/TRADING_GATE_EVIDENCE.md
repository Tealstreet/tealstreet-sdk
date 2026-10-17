# WEEX trading actions and order/position models

Authoring parent: `6d8cb632677f074c1452e9ea047a73f0c7a732bb` (integration).
Implementation and baseline gate SHA: `c42a45c84da5210a3c873075d38e9034a35a6606`.
Branch: `feat/rust-venue-weex-order-models`. No separate unreviewed feature ancestor.

Independent `python3.12`/`tomllib` census of committed manifests against the authoring parent: **9 pending -> ported, 0 pending -> divergent**. WEEX has 156 rows: 104/31/21 ported/divergent/pending before, 113/31/12 after. The nine rows are `normalizeOrder`, `normalizeAlgoOrder`, `normalizePosition`, `WeexExchange.fetchPositions`, `WeexExchange.placeOrder`, `WeexExchange.placeOrders`, `WeexExchange.updateOrder`, `WeexExchange#placeRegularOrderBatch`, and `WeexExchange#formatOrderPayload`.

Every newly moved row has a nonempty Rust target. Independent declaration lookup in committed source verified every target, including `WeexTradingRuntime`; zero missing targets. This is declaration existence plus scoped status/invention validation, not proof of arbitrary host implementations.

## Measured baseline gates

All four commands below completed with exit **0** at the implementation SHA above. Cargo commands ran serially.

```sh
/opt/homebrew/opt/rustup/bin/cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2
SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript /opt/homebrew/opt/rustup/bin/cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test trading
python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py
node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check
```

The trading target reported `1 passed; 0 failed`. Its independent literal floor is **76 fixtures**. Last enumerated fixture: `positions_failure_unfiltered` (iteration 76). All decoded native value assertions run before prefix or full trace assertions; no native encode/roundtrip comparison is used. Native outputs include models, errors, requests, updates and eager initial receipts. Other independently specified floors are three algo records, four position IDs and two chunk requests.

Canonical scoped parity reported `invention and status gates passed (156 rows, 181 public Rust items)`. Global exemptions: 920; WEEX exemptions: **0**. The extractor's output was read: `Would update 0 manifests; 0 added, 0 orphaned symbols.` It enumerated 7341 symbols. The extractor writer had completed with exit 0 before the implementation commit: two manifests updated, zero added/orphaned symbols. Manifest `rust_files` arrays were writer-produced.

## Compiled discrimination controls

`python3.12 sdk-rs/safe-cex-venue-weex/tests/trading-mutations.py` completed with exit **0**, bound to `c42a45c84da5210a3c873075d38e9034a35a6606`. Each control changed production code, compiled and executed the same scoped trading target, then restored the original source bytes and reran it. Every broken run exited **101** at the named native equality assertion below; every restored run exited **0**, reporting `1 passed; 0 failed`. No control survived. Enumeration stopped after control **9**, `position-existing-id-filter`; no wider mutation census is claimed.

| Control / production call site | Named native assertion | Broken exit | Restored exit |
| --- | --- | --- | --- |
| amount/filled swap / `normalize_order` | `trading::normalizeOrder_native order_distinct` | 101 | 0 |
| TP/SL virtual ID swap / `normalize_algo_order` | `trading::normalizeAlgoOrder_native algo_virtual_pair` | 101 | 0 |
| unrealized/realized PnL swap / `normalize_position` | `trading::normalizePosition_native position_distinct` | 101 | 0 |
| quantity/price swap / `format_order_payload` | `trading::formatOrderPayload_native payload_limit` | 101 | 0 |
| regular/algo endpoint swap / `place_order` | `trading::placeOrder_native single_regular` | 101 | 0 |
| batch cap five -> four / `place_orders` | `trading::placeOrders_native orders_chunking` | 101 | 0 |
| order/client ID swap / `place_regular_order_batch` | `trading::placeRegularOrderBatch_native batch_reordered` | 101 | 0 |
| trigger/execution price swap / `update_order` | `trading::updateOrder_native update_stop` | 101 | 0 |
| invert existing-ID filter / `fetch_positions` | `trading::positions_native_four_ids` | 101 | 0 |

After all controls, `git diff --exit-code c42a45c84da5210a3c873075d38e9034a35a6606 -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests` exited **0**. Control logs and SHA-bound JSON stay ignored under `sdk-rs/target/weex-trading-gates/`; this manually authored evidence sheet is tracked.

## Merge-down and integration-relative audit

Fetched integration parent: `f2b6e51c1a29d354ad93b5602fbc44227d9714c5`.
Merge commit: `d6f682e2dba6a5e279d1eed96de27a67016a2397`; merge exited **0**, with no conflicts.

Independent committed-manifest census against that actual integration parent again measured **9 pending -> ported, 0 -> divergent**, with 113 ported / 31 divergent / **12 pending** after the merge. Independent declaration lookup again found zero missing targets among the nine moved rows. This gain is relative to the merged integration parent, not the stale authoring base.

`git diff --exit-code c42a45c84da5210a3c873075d38e9034a35a6606 HEAD -- sdk-rs/safe-cex-venue-weex/src sdk-rs/safe-cex-venue-weex/tests sdk-rs/parity/manifest/exchanges/weex` exited **0** at the merge SHA. Workspace Cargo definitions, the WEEX crate definition, Rust types/num/store/parity crates, extractor sources, WEEX TypeScript directory, shared TypeScript types/utils/logging, and yarn lock were unchanged. Cargo lock changed only the `safe-cex-venue-bitget` package entry. A separate parsed-lock dependency traversal found **zero changed entries** in the conservative WEEX closure (114 dependency names). The package script change was only `venue-oracle:kucoin`; both extractor commands were unchanged.

The global exemption list changed, so the scoped canonical parity command and extractor check were rerun, serially, at `d6f682e2dba6a5e279d1eed96de27a67016a2397`. Both completed with exit **0**. Canonical parity again passed 156 rows / 181 public Rust items, with 1019 global exemption blocks and **zero WEEX exemptions**. Extractor output was read: `Symbols: 7341 across 512 source files`; `Would update 0 manifests; 0 added, 0 orphaned symbols.`

Cargo check, the trading target and the nine controls were **not rerun after merge**. Their measured exits remain bound to the implementation SHA; the byte-preservation and dependency-input audits above establish their applicability to the merged slice. The evidence update following the merge changes this document only.

## Scope and limits

Production code implements native guards, field selection and model construction, payload branches, request routing, batch validation/correlation, amendment sequencing, and position composition over explicit host capabilities. Async entries execute their synchronous prefix eagerly; JS await boundaries suspend even when observations are ready. The module uses shared host handles and owned request values; it introduces no store-wide borrow guard.

The oracle extracts actual TypeScript functions and methods, with their actual imported helpers where applicable. Transport, store, base methods and UUID behavior are bounded test dependencies. The native fixture host covers ordinary objects/collections, undefined/null, UTF-16 strings, binary64 bits and array holes. It is not a general JavaScript engine. Arbitrary getters, proxies, collection species, overridden Set/Map methods, broader timestamp/coercion inputs, observer-drop transport lifetime and a complete scheduler have not been enumerated. Full exchange class assembly remains pending; imported dependencies retain their existing manifest statuses. No shared Rust model or trait was widened, and no income pager was written.

Read-only sub-agent review checked the nine mappings, source evaluation order, host boundaries and control anchors; it ran no cargo or mutation commands. Source changes from that review are included in the implementation SHA.

Preparatory failures before the frozen SHA: oracle fixture floor initially found 72 of the planned 76 cases (four real cases were added); two scoped test attempts exited 101 on compiler errors, which were corrected. These are not successful gates at the frozen SHA.

Not run: full WEEX test suite, workspace compilation, full monorepo suite/build, host clippy, wasm clippy, live venue calls, CI reruns or a PR creation/merge.

## Production batch order-ID admission (2026-10-18)

The earlier generic-host source corpus used the correct positive-decimal predicate,
but the assembled production host accepted any nonempty string. The public
ValidatedBatch -> Exchange -> installed placeOrders/placeRegularOrderBatch
regression compiled and failed against master28b7c69d65: successful orderId
"0" returned ["0"] where the maintained source rejects it.

The production callback now checks the exact /^[1-9]\d*$/ ASCII rule. Fourteen
actual-source cases cover11 malformed strings and3 valid IDs, including a valid
ID beyond the safe-integer range. They compare returned IDs or Error name/message,
exactly one visible refusal, and unchanged Store TIF on refusal versus the source
PostOnly patch on acceptance. Terminal HTTP is scripted; production handlers,
formatters and Store are used. The oracle supplement leaves the original76-case
corpus unchanged. The older default batch fixture now uses valid1008 instead of
invalid placed-8; single-order fallback behavior remains unchanged.

Final normal-master integration c64f62e04b: binding53 +original source corpus1
(76cases) pass; strict native/Wasm all-target Clippy, formatting/diff checks,
all-feature cancellation7/override8/binding25, parity7459/16619 and extraction0
pass. Logs /tmp/weex-batch-id-{master-red,fixed,native,inventories,native-clippy,
wasm-clippy,parity,extract}.log. No shared body API, model carrier or pin changed.
