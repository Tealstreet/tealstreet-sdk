# BingX non-UTF-8 divergence audit

Seven exact assigned rows are listed in `tests/divergence-rulings.json`.
The probes construct `binding::BingxExchange` with the real HTTP client, Store,
BaseWebSocket and timer adapters. Only the primitive HTTP/socket/runtime ports
are scripted by the existing binding harness.

The public OHLCV error and historical-fill parameter log occur before the
returned future is polled. Tick starts its selected order read before disposal;
a subsequent disposal still prevents post-await Store writes. These three named
probes compile and fail against the original production bodies. They use the
shared typed eager-start boundary, preserving native results and owned continuations.
No venue-specific task resolver is introduced.

The actual private message bridge already starts rebuild synchronously. Its
probe calls the real core message binding, then observes stale socket closure,
cleared key/socket/listeners and rebuilding state before pumping the runtime.
The replacement completes once. The helper's lazy extraction is therefore an
ACCEPT for this production entry, rather than a missing eager prefix.

Live open schedules refresh before Ping; virtual time proves no refresh before
1,800,000 ms, then one PUT at that deadline. Recovery removes the prior refresh,
waits exactly 30,000 ms, and makes one replacement key request/socket. Explicit
disposal clears timer/key while preserving connecting/rebuilding flags, closes
the socket and prevents later HTTP. Replacement open must install its next refresh.

The coordinator's lifetime ruling in `divergence-cluster-rulings.md` accepts
native final-handle drop as deterministic full disposal rather than TS timer
retention. The original constructed exchange left its socket open on drop.
Two compiled named drop probes fail on that production behavior; the Drop
implementation runs the same teardown as explicit disposal. Both refresh and
recovery stay live while the exchange is held, and both cancel on final drop.
Drop is silent; host timers are cancelled even if a Store-reset observer rejects disposal.

The discarded initial tick assertion expected wire HTTP before call return.
Axios itself schedules transport asynchronously, so that assertion was too early.
The retained test measures the call/dispose race and final wire/Store effects.
No output from that discarded assertion is treated as qualifying evidence.

Other pre-existing row boundaries (typed inputs, source error objects and UTF-8)
remain recorded in the original manifest notes. No identity or alias guard changes.

The recovery timer also has a compiled call-turn RED: the original host queued
rebuild's prefix behind the next runtime job. The existing typed Host::spawn
boundary now uses the same shared start_eager driver before scheduling its result
reporter; it does not spawn an existing JS Promise again. The tick timer callback
also invokes the already-eager tick before returning its waiter. The private
message bridge retains its original Promise ownership.

The shared driver requires fixture runtimes to execute their spawned jobs.
`tests/read_streams.rs` now pumps those jobs while waiting for tick; all six
existing tests pass with every source/corpus, Store, opaque error and alias
identity assertion intact. The two native child modules reuse one harness module,
so strict Clippy does not load the same file twice.

Validation after integrating shared eager-start PR #1050: 131 tests across 19
native targets pass, including all constructed bindings and history witnesses.
The full suite then stops at the existing `tests/types.rs:106` expectation that
shared decoding rejects a lone-surrogate `BINGX_BALANCE.userId`. That expectation
is stale after decoder PR #1041. The decoder owner has the focused expectation
migration; no typed-input, identity or corpus guard is relaxed by this PR. Full
crate GREEN is not claimed. All nine production controls compiled RED and passed
after exact restoration; original private listen-key source bytes are unchanged.

Final scoped gates pass: strict native all-target Clippy, separate wasm library
Clippy, pinned formatting, all-features cancellation inventory (6 passed, one
existing discovery-export ignore), and the Node cancellation source suite.
The dependency PR #1050 is merged; this branch integrates master normally.
