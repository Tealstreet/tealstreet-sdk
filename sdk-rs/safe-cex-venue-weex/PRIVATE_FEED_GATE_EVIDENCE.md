# WEEX private websocket feed and refresh lifecycle

Authoring parent: `c1b0ce61f2247f5604b26ed5a00b142be75d90ef` on
`feat/safe-cex-rust`. Fresh branch: `feat/rust-venue-weex-private-feed`.
Initial status contained only three local Aimux briefs, no source or mutation
leftovers. No unrelated unreviewed feature branch was used as an ancestor.

## Rows and deliberate differences

Independent tomllib comparison against the authoring parent moved **9 rows off
pending: 6 pending -> ported, 3 pending -> divergent**. Census is
80 ported / 29 divergent / 47 pending, 156 total rows.

Ported: connectAndSubscribe, onMessage, dispose, handleOrders, handlePositions,
handleFills. Their local algorithms preserve connection guards/header-required
provider construction/error classification/listeners, message priority/PONG ID
mutation, repeated result/d reads, stats/dispatch, market receiver ordering,
normalization/truthiness, updates/status handling, live fills/find iteration,
conversion order, native fill fields, and timer cleanup before base disposal.

Divergent rows each carry their own CHOSEN note:

- throttledFetchOrders reports failed fetchOrders/store.update through the error
  sink instead of losing the order refresh in the source empty catch.
- throttledFetchPositions reports failed fetchPositions/full-position replacement
  instead of silently retaining stale positions.
- throttledFetchBalance reports failed fetchBalance/store.update instead of
  silently retaining stale balance.

Successful timer behavior preserves clear/re-register at 250ms, eager fetch,
post-await disposal guard and the distinct orders/positions/balance store calls.
The anonymous callback is split into run_refresh_timer, without an exclusive
host borrow across await. Error reporting adds an observable native outcome;
these rows are not claimed behaviorally identical to the source catch.

Function algorithms use opaque values and explicit primitives for imported
normalizers, parser/coercion, array-map/find/iteration, transport, store and
timer registration. Class assembly and the entire JavaScript host engine are
not claimed ported. Native fixture parser/date conversion/normalizer bindings
are bounded test primitives; parser/normalizer implementation parity is not
established by this suite. Template String semantics have a separate primitive,
but Symbol values/overridden array methods/species were not enumerated.
Header-provider freshness is witnessed by live host nonce; arbitrary replacement
of host methods and transport implementation are outside this proof floor.
No income pager was added.

## Native and mutation measurements

Implementation SHA: `1bfa2f9c5cba0e3c1791e6a123687752a891b50d`.
`python3.12 sdk-rs/safe-cex-venue-weex/tests/private-feed-mutations.py` exited 0.
Each selected production mutation ran:
`SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test private_feed`.
Each broken run exited 101 with assertion left==right failure text and the named
native assertion. Every restored run exited 0; one test passed.

| Family / selected mutation | Named native assertion | Broken / restored exit |
| --- | --- | --- |
| Connection: headersRequired false | native_socket_options::connect_basic | 101 / 0 |
| Dispatch/PONG: increment subscription ID by two | native_subscription_id::message_ping | 101 / 0 |
| Order updates: classify open instead of closed | native_statuses::orders_closed | 101 / 0 |
| Position updates: drop the add operation | native_positions::positions_basic | 101 / 0 |
| Fills: multiply value/size instead of divide | native_fill_price::message_large_fill | 101 / 0 |
| Three refresh callbacks: discard the shared error report | native_refresh_failure_visible::orders_fetch_failure | 101 / 0 |
| Disposal: omit base dispose | native_disposed::dispose_missing_timers | 101 / 0 |

Enumeration stopped after these seven family controls: seven caught, zero
survivors among them. Other mutations were not enumerated. Compiler errors do
not count as proof. Source bytes are restored in a finally block after each
control. This is not nine independent row-specific mutation proofs.

The native target stops at 63 ordered fixtures, with IDs independently fixed
in Rust. Independent literal fill counts additionally check both runtimes; the
live-append and closed-order fixtures assert two fills and two status updates.
Native socket options, listeners, headers, warnings, updates/statuses, fill IDs /
UTF16 side/symbol values and individual binary64 fields, raw PONG ID before wire
encoding, timers, disposal flags and chosen refresh-error causes are asserted
before any common effect-trace comparison. No decode/re-encode proof is used.

The source oracle extracts actual nine source fields, actual toNumber/timestamp
utilities, actual bigint parser, actual shared enums/venue constants. Host
normalizer/store/transport/timer inputs are independently configured fixtures,
not whole function results supplied back to Rust. Cases include sparse map vs
for-of holes, live fill append, iterator close on emitted-fill failure, Unicode
side, numeric overflow, big IDs, live header provider, missing timer handles,
callback disposal-after-fetch, successful refresh and visible fetch/apply errors.
Reporting events intentionally differ for the three CHOSEN rows. Their exact
kind/cause is asserted natively before those added events are removed only for
comparison of the unchanged source effect trace; they are not hidden survivors.
This is a finite floor, not completeness over arbitrary host values or engines.

## Manifest-note correction and scoped gates

`adaba1d2e70cc049a09a305cae1ee3ef9bd422ac` changes only one manifest note:
normalization/truthiness filtering was described without an unimplemented claim
that all normalization precedes every pure truthiness check.
Git diff --exit-code from the implementation SHA to this correction exited 0
for source/tests. Native/control measurements remain bound to the implementation
SHA. These gates were measured at the correction SHA:

| Command | Exit / observed result |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0; scoped compilation, zero errors |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; 156 rows, 138 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols; Would update 0 manifests, 0 added, 0 orphaned |

The extractor writer ran after manifest edits. Its check output was read;
these claims do not depend solely on the Volta shim exit status. All cargo
commands, including the canonical parity gate's cargo command, ran sequentially.

Sub-agent weex_review inspected source/native behavior, fixtures, nine mappings
and seven control edits. Array/conversion/store-receiver scope gaps were
addressed with distinct native protocols; its note-order mismatch was corrected
in the manifest. Follow-up found no concrete remaining ordinary-feed defect.
It ran no cargo/tests/mutations and made no edits or commits.

Not run: workspace compilation, full venue/monorepo suites, host clippy,
wasm clippy, live venue requests or CI reruns. Ignored local gate logs/results:
`sdk-rs/target/weex-private-feed-gates/`.

## Delivery merge

Fetched this worktree's exact integration FETCH_HEAD
`b5ac6229e2502e48106b0a549da0524583b04627` and merged it without conflicts into
`234a6679b3e5f6a180cc70e371e1726c1b7a8a7d`.
An independent tomllib audit against that actual integration parent exited 0,
requiring the same nine transitions, six pending -> ported and three pending
-> divergent, 156 rows and no missing Rust target files. Census is 80 / 29 / 47.

Git diff --exit-code from the implementation SHA to the merge exited 0 for
WEEX src/tests. A separate comparison against the manifest-note correction
checked WEEX source/tests/Cargo.toml/manifests, workspace Cargo manifest/lock,
shared types/num/store, canonical parity engine/extractor/exemptions,
package.json, actual WEEX TypeScript/shared enums/logging enum, actual bigint
parser source and Cargo configuration. Only package.json, Cargo.lock and global
exemptions changed among those inputs; other global manifests also changed.
Original seven control measurements remain bound to their implementation SHA;
source/tests are byte-unchanged. They were not rerun at the merge.

Because compiler/global gate inputs changed, these gates were remeasured at
`234a6679b3e5f6a180cc70e371e1726c1b7a8a7d`:

| Command | Measured exit / result at merge SHA |
| --- | --- |
| cargo check --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --all-targets --jobs 2 | 0; scoped compilation, zero errors |
| SAFE_CEX_TYPESCRIPT_PATH=/Users/sam/cs/tealstreet-next/node_modules/typescript cargo test --manifest-path sdk-rs/Cargo.toml -p safe-cex-venue-weex --jobs 2 --test private_feed | 0; one native target, 63-fixture floor |
| python3.12 sdk-rs/safe-cex-venue-weex/tests/parity-check.py | 0; 156 rows, 138 public items, zero WEEX exemptions |
| node /Users/sam/.volta/tools/image/yarn/1.22.21/bin/yarn.js parity:extract:check | 0; 7341 symbols, Would update 0 manifests, 0 added, 0 orphaned |

This evidence update changes only this narrative.

## Wave-1 silent refresh correction

This supersedes the historical extra-reporting choice above for all three refresh
callbacks. The maintained source's empty catches cover synchronous fetch throws,
awaited rejection and Store-update throws. The native callback now discards those
failures without a reporting capability; reaction suspension, disposal guard and
successful Store operations remain. The 63-fixture actual-source corpus and all
three ready-reaction tests pass unchanged in their source input inventory.

Test-only commit1007787b76 compiles against master and fails through the real
constructed private account frame: the rejected HTTP balance refresh leaves the
balance intact but reports `Request failed with status code 400`. Removing the
reporter makes the same test pass. The complete binding target passes38 tests.
The updated `refresh-update` production mutation compiles and fails
`native_refresh_updates::orders_refresh` with101, then exact restoration passes
all4 private-feed tests with0. This replaces the native-only reporting control;
the other six historical control bodies are unchanged and were not rerun here.

Measured fixed worktree private_feed.rs SHA256 (before implementation commit):
`baf20eae4611b66bf87e2a5ab9b4329620446553df4b3865d999942e818b6d33`.
