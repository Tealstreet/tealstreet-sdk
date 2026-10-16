# Backpack order-book state and private controls

Code SHA: `69a52cc4c76e676bb773884014521cd85662b735`.
Actual parent: `cb78ccd661ad20629105c9588cc3970f9778b6a5`, integration merge of PR 7520. This is a fresh branch from that head; no separate unreviewed lane branch was added as an ancestor.

Python 3.14 `tomllib` census over all six Backpack manifests: parent 32 ported / 53 divergent / 62 pending; code SHA 42 / 53 / 52, 147 total. **10 pending → ported, 0 → divergent.**

Public methods: createOrderBookState, isInitialOrderBookBridge, isContinuousOrderBookUpdate, applyOrderBookUpdate, resetAndRefetchOrderBooks, dispose. Private methods: ping, unsubscribe, resubscribe, throttledFetchOrders. Every row names its concrete Rust function in `src/orderbook_state.rs` or `src/private_control.rs`.

The bodies preserve fresh state fields and identity handler Sets, native binary64 sequence comparisons, live depth-array lengths and delayed ask lookup, reset object/handler/fetcher identities, snapshotted throttle cancellation, partial state at failures, two socket property reads, eager authentication before future polling, and timer handle/callback identity. Compatible host operations supply ordered Object.keys/values snapshots, live property reads/writes and callback invocation. Close access on an absent second socket remains an explicit provider error. Full transport wiring, arbitrary host prototypes, property descriptors outside those supplied operations, and JS global promise/microtask hooks are not claimed.

## Gates measured at the code SHA

- `cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --all-targets --jobs 2`: exit **0**; dev profile finished without compile errors.
- `SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-backpack --test orderbook_state --jobs 2`: exit **0**, 10 passed / 0 failed after every control was restored.
- `node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check`: exit **0**, output says 7341 symbols across 512 files; would update 0 manifests, 0 added / 0 orphaned symbols. The writer was run after manifest edits and exited 0 with the same zero-update/zero-drift output before the code commit.
- `git diff --exit-code 69a52cc4c7 -- sdk-rs/safe-cex-venue-backpack/src sdk-rs/safe-cex-venue-backpack/tests`: exit **0** after all mutations, and again after the final focused test run.

All Cargo processes were sequential, jobs 2. The oracle executes the actual TS public/private socket classes. Native assertions inspect state fields, f64 results, array levels, Set/object/callback/handle identities, call counts/order, and original errors. Native assertions precede deferred wire comparisons for the private message methods; no native assertion depends on a decode/re-encode roundtrip.

Independent fixture counts/content pin 985 source records: 2 new states, 891 sequence inputs checked by both predicates, 16 depth applications, 12 resets, 12 disposals, 8 resubscriptions, 18 timer cases, 12 private pings, 14 private unsubscriptions. Input pins include each state/first/last ID, UTF-16 price/amount strings, mutation mode, failure stage, handle kind and guard. Freshness observations additionally check that mutating one new state's updates/handler Set leaves the other unchanged.

These are finite floors. Enumeration stopped at created state 1; sequence 890; apply 15; reset 11; disposal 11; private resubscribe 7; timer 17; ping 11; unsubscribe 13. Sequence inputs include undefined IDs, reversed/spanning/exact ranges, signed zero, fractions, NaN, infinities, and the 2^53 rounding boundary. Lifecycle cases cover live deletion/replacement, callback failure, second-socket substitution, second-read failure, and missing second-socket access.

## Discrimination controls at the code SHA

Every mutation compiled, then the selected test exited **101** with `test result: FAILED` and the named native assertion below. Each production file was restored byte-identically, and that same selected test exited **0**, 1 passed / 0 failed. No survivors or compile-only controls.

| Test | Deliberate regression | Named native assertion |
| --- | --- | --- |
| fresh_orderbook_state | initial snapshot flag true | STATE 0 native snapshot flag |
| initial_sequence_bridge | strict instead of inclusive lower bound | BRIDGE native value 30 |
| continuous_sequence_update | next ID advances by 2 | CONTINUOUS native value 30 |
| apply_live_orderbook_levels | amount parsed from price column | APPLY native amount 0/0 |
| reset_preserves_state_and_fetcher_identity | discard retained handler Set | RESET native retained handler Set |
| dispose_partial_state_and_snapshot | close first socket after only one read | DISPOSE native operation order 0 |
| private_ping_guards_and_time | timestamp increased by 1 | PRIVATE-PING 0 native timestamp before send |
| private_unsubscribe_live_streams | send SUBSCRIBE instead | PRIVATE-UNSUB 2 native method |
| private_resubscribe_eager_and_pending | reverse empty-Set guard | PRIVATE-RESUB native eager calls 0 |
| private_order_timer_truthiness_and_identity | timeout 1000 instead of 10000 ms | TIMER native delay 0 |

Restored source SHA-256:
- orderbook_state.rs: `e0c3c09c35120e03281c6eeaad0126a9fc862a84b64aa533db27e8da09e552be`
- private_control.rs: `7c0d5e71f228a70842a658fa2f76f407677e705457d8ef2eef36e96f1c1a5384`

Read-only sub-agent review identified the precommit single-socket-read defect. It was corrected before the code SHA; the actual-source/native disposal corpus was expanded to independently pin alternate-socket, second-read rejection and missing-value rejection. Re-review found no remaining defects and supported all ten ported classifications. The single-read regression was also caught by the root's compiling control above. Review execution is distinct from the root's gate measurements.

Not run: workspace compilation, unrelated venue or earlier Backpack oracle targets, host clippy, wasm compilation/clippy, full monorepo suites, CI, or live exchange sockets.

## Integration validation

Integration `4fa402f6fb4890a9ebd0d2ccf585f720c6f3a10a` was merged down without conflicts at `7b87d5bdb85823031007b200c523ad1ada765a45`. At that merge SHA:

- Full Backpack source/tests comparison against code SHA `69a52cc4c76e676bb773884014521cd85662b735`: exit **0**, byte-unchanged. The actual Backpack TS oracle source and shared TS types also had no changes.
- Cargo.lock changed upstream, so the scoped all-targets Cargo check was rerun: exit **0**, dev profile finished without compile errors.
- Focused `orderbook_state` oracle target rerun with the stated TypeScript environment: exit **0**, 10 passed / 0 failed.
- Extractor check rerun: exit **0**, 7341 symbols / 512 source files, would update 0 manifests, 0 added / 0 orphaned symbols.
- Independent tomllib census: 42 ported / 53 divergent / 52 pending; exactly 10 pending → ported, 0 → divergent against the actual parent.

These exits are measured at the merge SHA, not a later evidence-only commit. The original controls remain attached to the code SHA and byte-identical production/test inputs.

## Delivery preservation

Immediately before push, integration was fetched again and `976c7bf08eb0d23329d7dfd9042035e3aabe2358` was merged down at `470e757e807c8a085f85c3653c48177b2dd1540c` without conflicts. Full Backpack source/tests comparison against measured merge SHA `7b87d5bdb85823031007b200c523ad1ada765a45` exited **0**, byte-unchanged; Backpack TS oracle inputs also had no changes. The measured gates/controls therefore retain their stated source/test scope. Final independent tomllib census remains 42 / 53 / 52, movement exactly 10 pending → ported and 0 → divergent. Gates were not rerun at this final merge or the following evidence-only commit; their measured SHAs above delimit the claims.
