# Spot runtime gate exits

Measured gate input: `0140b2b6dd1a6c4fbb2b2538c4639cfbc504ec17`.
Fresh branch parent: `faee40c0372da23d6558ad3da511c44a22b05331`.
Integration merged down before these gates: `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
The fresh parent was an integration commit, not another feature branch.

## Rows and boundary

Python `tomllib` measured **16 pending → ported, 0 → divergent**.
The assigned nine manifests changed from **145 / 0 / 16** to **161 / 0 / 0**
(ported / divergent / pending): six `hyperliquidspot` exchange/API/rate-limiter/
socket/type files, plus cross-cache, income and market-startup-helpers.
Spot constants/errors add six already-ported rows outside that nine-file count.
The entire Hyperliquid directory at the gate SHA is **398 / 106 / 27**;
remaining perp rows belong to the other lane.
GRVT remains **113 / 51 / 0**; MEXC remains **145 / 40 / 0**.

Canonical references, without duplicating their declarations:
[§14](../parity/MANIFEST-SPEC.md#shared-utf8-string-boundary),
[§15](../parity/MANIFEST-SPEC.md#closed-typed-output-boundary),
[§16](../parity/MANIFEST-SPEC.md#owned-snapshot-vs-live-alias),
[§17](../parity/MANIFEST-SPEC.md#numeric-ingress-defect),
[§18](../parity/MANIFEST-SPEC.md#divergence-taxonomy),
[§19](../parity/MANIFEST-SPEC.md#forced-surrogate-conflict).

## Measured exits

| Named gate | Exit | Observed result |
|---|---:|---|
| Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid --all-targets --jobs 2` | 0 | Finished; no compilation errors |
| Scoped `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-hyperliquid --test spot_runtime --jobs 2`, Node 24.20.0 | 0 | 11 passed, 0 failed |
| Same test target, Node 26.7.0, after restoring production files | 0 | 11 passed, 0 failed |
| `parity:extract` writer, before merge | 0 | Updated 0 manifests; 0 added/orphaned symbols |
| `parity:extract:check`, at gate SHA | 0 | Would update 0 manifests; 0 added/orphaned symbols |
| Direct rustfmt check of five new modules and two Rust fixture/test files | 0 | No formatting changes |
| Source oracle in a temporary source mirror with no ancestor node_modules | 0 | Eight source files through index 7; 30 source cases through index 29 |
| Missing selected compiler negative control | 1 | MODULE_NOT_FOUND; oracle did not skip |
| Live-source mutation child | 101 | ASSEMBLY native keys metadata |
| Source-control replaced by a no-op | 101 | ORACLE live-source mutation must fail native assembly |

The package manager was invoked through its pinned JS entrypoint. Extractor
outcomes above include the output text, not an inference from shim exit status.
The selected TypeScript compiler was supplied through SAFE_CEX_TYPESCRIPT_PATH.
There are no new executed npm dependencies beyond that compiler.

## Discrimination at the gate SHA

Sixteen isolated temporary production mutations were compiled together, then
selected independently. Every run exited **101 at its named native assertion**.
The five production files were restored byte-identically to the gate SHA before
the passing Node 26 run. No mutation selector or broken body is committed.

| Row/family | Deliberately wrong implementation | Named failing assertion |
|---|---|---|
| Exchange class | Constructor length 2 | ASSEMBLY native keys metadata |
| Exchange constructor | isSpot false | ASSEMBLY native keys metadata |
| Limiter class | Changed class name | RATE native ctor state |
| Limiter constructor | Initial tokens 1199 | RATE native ctor state |
| Limiter refill | Refilled to 1199 | RATE native boundary |
| Limiter wait | Added weight to tokens | RATE native immediate |
| Spot API | Returned a fresh object instead of the factory XHR | API native private |
| Public class | Constructor length 2 | ASSEMBLY native keys metadata |
| Public constructor | Initialized wrongTopics instead of topics | ASSEMBLY native keys metadata |
| Candle listener | Divided timestamp by 1001 | CANDLE connected native callback |
| Book listener | Parsed amount as price | BOOK native callback |
| Market snapshot | Used quote as base | MARKETS native fields |
| Placement | reduce_only true | PLACE native sign/post inputs market |
| Cancellation | Copied order ID into asset slot | CANCEL native signing numeric inputs |
| Update | Ignored inherited price update | UPDATE native inherited changes and call order |
| User event | Returned funding coin as attached user | TYPE native attached UTF16 user funding |

No surviving mutation was observed in this enumeration, which stops at mutation
16. These are bounded witnesses, not an exhaustive mutation census.

## Proof scope and reviews

Native assertions read market/ticker/request fields, emitted errors, function
metadata, initial state, collection ordinals and original object handles.
Candle/book native packets and callback members are checked before wire strings.
The four event branches check decoded native fields, signed-zero bits and UTF-16
user units. Four original-alias compiler witnesses accept attached users and
reject missing users. The event type has no whole-event JSON codec; array user
properties remain explicit native data.

The exchange owns all 25 source arrows through Rust dispatchers. Its constructor
keeps the source import of the perp API factory; the independent spot API is a
separate body. Imported signing/numeric helpers, Base construction and native
engine/property/Promise/timer operations remain explicit host dependencies.
The scoped source oracle supplies controlled versions of those dependencies.

Two read-only sub-agent reviews covered client/trading and subscriptions/type/
assembly. Findings corrected before these gates: optional-chain continuation,
timer executor return, literal append bypassing push, native decrement/negation,
and the XHR identity assertion. The reviewers reported no remaining blocker in
the reviewed production bodies. This is not a whole-venue review.

The first integration merge changed other crate/dependency inputs, so positive
gates and all sixteen controls were rerun at the gate SHA. Rust workflow named
steps were 105 before and 105 after, across two Rust workflow files.

Not run: workspace compilation, full venue or monorepo suites, host clippy,
wasm compilation/clippy/tests, real cryptographic signing, live exchange calls,
browser tests, load tests, dependency installation or a CI rerun.

## Delivery preservation

A fresh fetch and merge-down after the gates reported integration already up to
date at `ac0f5fccf24207b26c279e67da2a88f2c72a7aae`.
Compared the gate SHA with documentation successor
`5c63332425` using `git diff --exit-code` over the whole crate src/tests and
Hyperliquid manifests: **0**, byte-identical.
The lock dependency traversal reached 481 packages, including eight local
packages, and emptied its worklist after pop 1390 (last index 1389).
Those package entries and the enumerated local src/tests/Cargo/build-script,
workspace/lock/config and Hyperliquid TypeScript inputs also compared **0**.
This paragraph is a documentation-only successor; it changes none of those inputs.
