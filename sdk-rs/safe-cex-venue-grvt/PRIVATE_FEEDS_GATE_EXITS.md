# GRVT private feed gate exits

Branch: `feat/rust-grvt-private-feeds`.
Actual starting parent: `4f2700770ae788d7b3f02acac6157085ca8f9606`.
Implementation: `0b311cf5a916e4fc5189fc6ebf6939b9f98eed1c`.
Binding documentation: `d123ccf55d18a8db5c4ee310c72933cd1abba1da`.
Measured gate and control baseline: `1c0b058df09dd246c289accbcbf8f9f17e0c2f20`.
Integration merged down: `ef054ba0b8e9038241576f44c09f652348f2f4bb`.
Merge commit: `ab4bb6c9bd7ad16e8a3d92aa36dbd233ed6ea726`.
No unrelated unreviewed feature branch was included as an ancestor.

## Rows moved

Independent Python 3 `tomllib` reads of git objects enumerate nine GRVT manifests with an independent expectation of 164 symbols. Starting parent and integration parent each have 78 ported / 48 divergent / 38 pending. The implementation descendants and merged tree have 87 / 48 / 29. The merged-tree census exited 0 after asserting exactly nine pending->ported and zero pending->divergent:

- parsePositiveFiniteDecimal
- GrvtPrivateWebSocket.onMessage
- GrvtPrivateWebSocket#handleOrderUpdate
- GrvtPrivateWebSocket#handlePositionUpdate
- GrvtPrivateWebSocket#handleFillUpdate
- GrvtPrivateWebSocket#parseOrder
- GrvtPrivateWebSocket#parsePosition
- GrvtPrivateWebSocket#throttledFetchOrders
- GrvtPrivateWebSocket#fetchThrottledOrders

The executable Rust bodies live in `src/private_feeds.rs`. Opaque values retain raw IDs, reduceOnly, own undefined and feed identity. Existing PrivateCommands owns the fill ledger and pending-position flag; its fill key is generic with String as the compatible default. FillKey binds SameValueZero and reference identity without string conversion or a parallel ledger. Store operations, JS primitives, source helpers, position throttling and timer scheduling bind through PrivateFeedHost. Lifecycle and a concrete socket/store adapter remain outside this slice.

## Measured gates at the baseline SHA

Cargo executable: `/opt/homebrew/opt/rustup/bin/cargo`.
Every cargo invocation used `--manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-grvt --jobs 2`, serially.
Every oracle test set `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript`.

| Gate at 1c0b058df09dd246c289accbcbf8f9f17e0c2f20 | Measured exit/output |
| --- | --- |
| cargo check, `--all-targets` | 0; compile completed |
| cargo test, `--test private_feeds --test private_commands`, Node 24.20.0 | 0; nine new + nine regression tests passed |
| Same targets, Node 26.7.0 | 0; nine new + nine regression tests passed |
| pinned parity:extract:check | 0; Would update 0 manifests; 0 added, 0 orphaned symbols |

Node 26 commands prepend `PATH=/opt/homebrew/opt/node@26/bin:$PATH`.
Extractor command: `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`. Its output text was inspected.
After manifest edits, smol-toml serialization and the pinned `parity:extract` writer completed, exit 0, `Updated 1 manifests; 0 added, 0 orphaned symbols.`

## Discriminating controls at the baseline SHA

Each temporary production mutation starts from the baseline SHA plus the recorded change. Each broken invocation compiled and ran its one exact named target, exited 101 at the named assertion, then byte restoration and the same command exited 0 with one passed. All nine controls used Node 24.20.0, command suffix `--test private_feeds <target> -- --exact`. No control survived.

| Target | Temporary mutation | Named failure | Broken / restored exit |
| --- | --- | --- | --- |
| positive_decimal_native | Double accepted amount | DECIMAL native value | 101 / 0 |
| order_parser_native | Remaining uses size + filled | ORDER native record | 101 / 0 |
| position_parser_native | Keep signed contracts | POSITION native record | 101 / 0 |
| order_handler_native | Record positions stats for orders | PRIVATE native eager prefix | 101 / 0 |
| position_handler_native | Clear pending flag on zero-size update | POSITION native pending flag | 101 / 0 |
| fill_handler_native | Collect template operands before coercing them | PRIVATE native eager prefix | 101 / 0 |
| message_native | Remove authentication-failed guard | PRIVATE native eager prefix | 101 / 0 |
| order_debounce_native | Callback omits fetch_throttled_orders | TIMER native slot native number shape | 101 / 0 |
| order_fetch_native | Loaded.orders becomes false | STORE native live loaded flags | 101 / 0 |

The first callback-removal attempt at d123ccf55d18a8db5c4ee310c72933cd1abba1da compiled and exited 101 through an unnamed numeric-shape unwrap; it was not counted as a named proof. The test now asserts numeric shape before reading bits. All nine controls and baseline gates were rerun at the baseline SHA above. Restoration before merge was proved with scoped `git diff --exit-code`, exit 0.

## Coverage floor and review

The actual TypeScript class and helpers execute in `tests/private-feeds-source.cjs`. Independent fixed case lists stop at 99 cases: decimal 15/overflow, order parser 14/numeric-no-market, position parser 10/numeric-no-market, order handler 7/raw-id-object, position handler 8/subscriber-mutation, fill handler 12/log-object, message 19/track-error, debounce 6/fire-callback, fetch 8/catch-log-error. This is a floor, not completeness.

`tests/private_feeds.rs` asserts decoded native records, Number bits (NaN classification), own fields, original feed identity, ordered raw fill keys, native nonce/emission/store effects, live flags, eager prefixes and operation order. Request-like effect, dedup and timer counts come from independent fixed expectations; order and position field counts are fixed. This slice performs no output wire comparisons or native decode/re-encode comparisons.

Measured witnesses include placeholder retention, nonce effects before later validation, strict decimal rejection, numeric versus string fill IDs, unknown-market dedup consumption, emitter failure followed by retry, 500-key eviction, zero-size position snapshots surviving subscriber replacement, per-position virtual-clock reads, loose 1000 versus strict noise codes, interpolation failure before clock/key enumeration, contextual uppercase diagnostics, live loaded reads, disposal during fetch, and timer callback continuation after its returned handle is ignored.

Read-only sub-agent review_order_helpers approved the corrected production and tests and measured source oracle exit 0 across 99 cases. Review found interpolation coercion order and uppercase-context gaps; both were corrected before the implementation commit. The reviewer ran no cargo, native regression, extractor or mutation gates. The binding docs specify primitive interpolation strings and CreateDataProperty/CopyDataProperties, bypassing inherited setters. Error identity/prototype exhaustiveness and arbitrary hostile method/getter overrides are not proved by this floor.

## Merge survival

The required `git diff --exit-code 1c0b058df09dd246c289accbcbf8f9f17e0c2f20 ab4bb6c9bd7ad16e8a3d92aa36dbd233ed6ea726 -- sdk-rs/safe-cex-venue-grvt/src sdk-rs/safe-cex-venue-grvt/tests` exited 0. A broader comparison of the whole GRVT crate, shared types/num/exchange/crypto/store/transport crates, workspace Cargo.toml/Cargo.lock, and all actual TypeScript oracle inputs also exited 0. The measured cargo and oracle exits therefore stand after merge. The global extractor was rerun at the merge SHA: exit 0, `Would update 0 manifests; 0 added, 0 orphaned symbols.`

The pre-push fetch advanced integration to `46c2ef2da86346d9ef56cbc8b2b128b47f662cca`, merged at `8f399aff046cb295fa7753e755d88090dbb563fb`. The same source/tests/dependencies/oracle-input comparison against the baseline exited 0 again. Independent tomllib census against this integration parent still asserted nine pending->ported, zero pending->divergent and 29 pending. Pinned extractor at this second merge exited 0 and printed `Would update 0 manifests; 0 added, 0 orphaned symbols.`

This sheet is an evidence-only descendant of the measured trees. No source, tests, manifests or dependency changes accompany its addition.

## Explicit nonclaims

No workspace check, full crate or monorepo suite, host clippy, wasm compile/clippy, CI rerun, browser automation, live websocket/REST/account test, complete venue construction or concrete host adapter was run or claimed. No shared type or shared trait was changed by this slice.
