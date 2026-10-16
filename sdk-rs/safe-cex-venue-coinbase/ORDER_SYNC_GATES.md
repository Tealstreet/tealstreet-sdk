# Coinbase signing and order-sync slice

Eight pending rows became ported at tested SHA `7d866bbaba2d49a8ced1f263c7a90aff204397db`: getSigningKey, sign, signWsAuth, tick, fetchOrders, fetchOpenOrdersSnapshot, handleUserEventOrders and resolveOrderMarket. Independent tomllib census against reviewed integration base `89a8dc764a77c2ae0fa2e67a51aa28756f0cd895`: **394 / 17 / 15 -> 402 / 17 / 7** (ported / divergent / pending), stopping after 16 Coinbase manifests / 426 rows. Every moved row resolves to a committed source file and real public function; the unchanged scoped AST checker independently passed.

This is a new branch `feat/rust-coinbase-order-sync-auth` directly from that reviewed integration base. Its two authoring commits contain this slice's unmerged work; no separate unreviewed lane branch was used as an ancestor. The read-only sub-agent reviewed source at `68c7928162c79d8ba5070ece45d2718a252ab6b6`, ran no gates, and found no remaining implementation blocker within the declared scope. Its resolver-note correction was committed at the tested SHA. `git diff --exit-code 68c7928162c79d8ba5070ece45d2718a252ab6b6 7d866bbaba2d49a8ced1f263c7a90aff204397db -- <crate>/src <crate>/tests <crate>/scripts/order-sync-auth.cjs` measured **0**.

## Measured gates

All these exits belong to `7d866bbaba2d49a8ced1f263c7a90aff204397db`; full commands and output are tracked in [ORDER_SYNC_GATE_RESULTS.json](ORDER_SYNC_GATE_RESULTS.json).

- Scoped `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --all-targets --jobs 2`: **0**.
- Scoped `cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-coinbase --test order_sync_auth --test credentials --test orders --test crypto_encoding --jobs 2`: **0**, **18 passed** across those four targets (8 new native/oracle tests). This is not the entire crate suite.
- Direct Node/Yarn `parity:extract:check`: **0**, output **Would update 0 manifests; 0 added, 0 orphaned symbols.** Output text, not the Volta shim status alone, is the extractor gate.
- Unchanged `scripts/scoped-parity.cjs`: **0**, **426 rows / 878 public Rust items**. This is Coinbase inventory, not a repository-wide parity claim.
- Direct Yarn writer was run after each manifest edit; last writer output **Updated 1 manifests; 0 added, 0 orphaned symbols.** No arrays were hand-spelled.

An earlier broader crate-suite attempt at `68c7928162c79d8ba5070ece45d2718a252ab6b6` was interrupted with **Cargo exit -15 (SIGTERM)** to keep runtime gating proportional on the shared machine. Six completed target outputs showed 31 successes before interruption; that observation is **not** a full-suite gate. Its all-targets check exit was 0. The attempt and exact commands/output are tracked in [ORDER_SYNC_GATE_ATTEMPTS.json](ORDER_SYNC_GATE_ATTEMPTS.json). The later scoped run above completed normally.

## Native/source evidence and limits

The full content fingerprint is pinned separately from literal case counts. Every filtered control calls the same source helper which checks the content pin and independent case floors: signing 11, auth 11, snapshots 10, events 10 and ticks 14, **56 cases**, stopping there. Extra focused checks cover cache hits, eager races, PEM exception identity, alias-root selection and one manually invoked second tick iteration. These are floors, not exhaustive JavaScript or mutation enumerations.

Source AST extraction executes the eight actual TypeScript field/function bodies with recorded imported leaves. Native header/payload claims (including NaN and negative zero), algorithm, usages, seed/DER bytes and key identity are asserted before JWT message/output bytes. Native orders and fill scalars, original info/order/market roots, live iteration, id replacement rank, pagination query fields, diagnostic counts/order, original error carriers, own undefined response slots, callback/store roots, disposal and detached loop behavior are read directly before any wire assertion. There is no decode/re-encode equality gate.

These are field-body ports over declared dense order/page/supported-type arrays, typed own-key Market records, inert request/options/error scalar records, UTF8 credentials/URI inputs, stable receiver methods and retained native Frame roots. Caller-supplied capabilities stand for imported crypto/PEM, HTTP, store, alias resolver, formatter logging, snapshot helpers, clocks and timers; the source-module signing cache must be shared by its receivers. Cryptographic primitive correctness, timer implementation, class/transport assembly, sparse arrays, arbitrary getters/prototype values and method replacement are outside this proof. In particular the existing **CORRECTED** resolver divergence for inherited Object.prototype keys and the existing **CHOSEN** pure PEM decoder divergence remain unchanged; the signing host's PEM capability preserves the original exception carrier and DOM metadata instead of reconstructing the old unit error. Class/API assembly rows remain pending.

## Compiled discriminating controls

Baseline SHA `7d866bbaba2d49a8ced1f263c7a90aff204397db`. Each temporary mutation is recorded with original bytes hash and exact replacement; source/tests were restored byte-for-byte. A compiler failure is rejected as a control: each recorded failure is the selected test's named runtime assertion. All ten controls were caught, and all ten restored tests passed. No survivors were observed in this ten-control floor. Full outputs and call sites: [ORDER_SYNC_MUTATION_RESULTS.json](ORDER_SYNC_MUTATION_RESULTS.json).

| Control | Selected test | Named assertion | Mutated exit | Restored exit |
| --- | --- | --- | --- | --- |
| cache-hit | `signing_key_cache_races_and_pem_exception_identity` | `same-secret cached key root` | 101 | 0 |
| jwt-issuer | `signing_native_claims_precede_wire_and_jwt` | `native issuer` | 101 | 0 |
| auth-id-read | `websocket_auth_native_own_slots_and_id_timing` | `native auth id captured at source time` | 101 | 0 |
| snapshot-id-replacement | `snapshot_pagination_native_orders_and_fetch_error_identity` | `.amount: native numeric bits` | 101 | 0 |
| fetch-error-carrier | `snapshot_pagination_native_orders_and_fetch_error_identity` | `fetch wrapper rethrows original failure` | 101 | 0 |
| private-filled | `private_event_native_values_before_store_and_fill` | `.filled: native numeric bits` | 101 | 0 |
| market-product-forwarding | `market_resolver_retains_map_product_and_selected_root` | `resolver forwards requested product` | 101 | 0 |
| detached-loop | `polling_tick_native_roots_disposal_callbacks_and_detached_loop` | `detached loop rejection must not reject tick` | 101 | 0 |
| fixture-content | `corpus_is_content_pinned_and_independently_counted` | `full corpus content fingerprint` | 101 | 0 |
| scheduled-callback | `polling_tick_native_roots_disposal_callbacks_and_detached_loop` | `normal tick without order poll: source trace unconsumed` | 101 | 0 |

## Not run / not established

No completed full crate suite, workspace compilation, full monorepo suite, host clippy, wasm clippy, wasm build/tests, exhaustive corpus/mutation enumeration, real crypto primitive oracle or CI re-run is claimed. No PR was opened or merged by this lane. Merge-down preservation and any subsequent gates are recorded separately in ORDER_SYNC_MERGE_RESULTS.json after the merge.

## Merge-down verification

Merged actual fetched integration `198f871b9b004f6384d910a8c3af5909bd706b48` into this branch at `ffcd2d11c327f3227a6d62d530707e77ab3d0004` with no conflicts. Source/tests diff from tested SHA `7d866bbaba2d49a8ced1f263c7a90aff204397db` measured **0**; scripts/manifests diff and the live source oracle input paths diff each also measured **0**. The compile floor was re-established after shared exchange/transport/workspace metadata changes: all-targets check **0**, extractor **0** with zero proposed manifest changes, scoped mapping **0** (426 rows / 878 items). Runtime tests and mutation controls were **not rerun after merge**; their exits are attached to the earlier tested SHA and byte preservation, not a newly executed merged-commit test claim. Full commands and dependency changes: [ORDER_SYNC_MERGE_RESULTS.json](ORDER_SYNC_MERGE_RESULTS.json).

Independent tomllib census against the actual merged integration parent confirms true gain **8 pending->ported, 0 divergent**; final Coinbase triple **402 / 17 / 7**, stopping after all 16 manifests / 426 rows. Every moved target is a committed public function. [ORDER_SYNC_FINAL_CENSUS.json](ORDER_SYNC_FINAL_CENSUS.json).
