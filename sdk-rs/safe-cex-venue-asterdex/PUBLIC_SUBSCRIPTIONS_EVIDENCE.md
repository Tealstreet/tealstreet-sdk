# Asterdex public subscription lifetime: eight rows

Branch `feat/rust-asterdex-public-subscriptions-8`, actual starting parent
`1674d852f7687b8d427cd4b77c3aee067688979f` (integration; PR 7645 merged).
No unreviewed lane is an ancestor. Full public class, constructor, connection
and onMessage assembly remain pending; no complete-socket target is fabricated.

## Independent movement

The tomllib census `scripts/public-subscriptions-census.py` independently names
all eight expected moved methods. Iteration 1 stops at eight Asterdex files /
231 rows. Counts are expectations compared to the parent and current manifests:

| Status | Parent | Slice |
| --- | ---: | ---: |
| ported | 121 | 123 |
| divergent | 60 | 66 |
| pending | 50 | 42 |

**2 pending -> ported; 6 pending -> divergent.** All eight distinct targets are
real `public_subscriptions.rs::PublicSubscriptions` bodies. Clear-book-retries
cancels real live/fired timer registrations and preserves listeners/topics;
unsubscribe-kline removes its registrations, cleans statistics, intentionally
catches send rejection, and preserves the source's orphan waiting closure.
Six distinct CHOSEN notes describe each method's owned native timing, typed
callback/cleanup/pending representation or exception channel. The onOpen note
explicitly records native replay/finally in one poll versus the source additional
await/microtask boundary; timing parity across every scheduler is not claimed.
No shared field or existing shared trait changed. No query-value conversion or
projection was added.

The controller stores actual ordered topics, book callback identities/retry
slots, replaceable kline callbacks, pending work, binary64 listener/subscription
ids and readiness. It performs actual send, cleanup, ping, reconnect-event and
base-socket disposal calls through an explicit local I/O contract. Timer/task
work uses production BaseExchange ownership, not a patch-plan-only target.

## Source floor, findings and review

The oracle extracts the eight unedited TS members, state fields and production
subscribe/unsubscribe/depth/kline bodies. It loads the real
`utils/orderbook.ts::calcOrderBookTotal`, observes actual send frames and callback
records, and freezes nested topic arrays at each observation. WebSocket/base
creation and configuration remain injected seams, not a complete-assembly claim.
The book-only detached unsubscribe rejection is observed; kline's actual catch
continues to consume its error. Node positive timeout truncation is modeled,
including 337.5ms becoming337ms.

Iteration 1 stops at **nine scenarios / 37 snapshots / 24 frame attempts /
six callback deliveries**: seven ordinary scenarios and two explicit source /
native disposal differences. Independent names and literal per-case counts
precede native state, timer deadlines, decoded frame method/params/id, callback
book/candle fields, cleanup/errors and call-order assertions. Every native
assertion in a family precedes its wire-byte assertions; no native re-encoding
comparison is used.

The floor covers shared listeners and per-listener book copies, cancelled old
registration generations, failed sends advancing ids, book retries through50,
callback replacement, kline unsubscribe leaving a wait that later re-adds the
topic, kline waiting through retry40 and its506ms delay, initialization queuing
that remains undrained on open, ordered replay across repeated opens, live and
fired book retry cancellation, swallowed versus detached unsubscribe errors,
and common disposal resets with counters retained. **The2000ms kline backoff
cap is not proved.**

Observed source bugs are preserved or explicitly bounded:

- Queued klines remain undrained, and unsubscribe leaves the captured wait alive;
  native code preserves both ordinary source behaviors.
- Source dispose leaves a disconnected kline timeout alive (one live timeout
  before and after another tick); native dispose cancels it (zero).
- Source's unawaited open IIFE can emit and finally mark ready after dispose;
  native dispose cancels its continuation (zero emits, ready false).

Those last two are explicit CHOSEN `ts-bug` boundaries on dispose, witnessed on
both sides, not survivors hidden from a parity comparison. Full BaseWebSocket
close effects, arbitrary callable/prototype identities, malformed callback data,
every host exception and all event-loop interleavings remain unproved.

The first book control compiled and failed at the earlier native attempt floor
(1 versus independently expected2), then restored0. The runner initially expected
the later deadline assertion and falsely labelled the mutant uncaught. That
diagnostic is retained in `PUBLIC_SUBSCRIPTIONS_CONTROL_DIAGNOSTIC.json`; the
corrected named controls below reran unchanged production bytes. This was a
runner-label finding, rebutted by the actual named failure, not a dropped
production survivor.

Read-only sub-agent review found no settled-state body mismatch. Its helper and
microtask-scope gaps were addressed by real helper execution and the explicit
onOpen divergence; the live ETH retry cancellation witness and frozen topic
snapshots were reviewed again. The reviewer ran no gates. Preliminary focused
runs failed on an unused fixture method (101) and a mutable oracle snapshot
(101); both were corrected before the code-SHA gates below.

## Gates

All four code gates measured at **`f5f4b9c47dd69a57a06846849cf1e4f76210b8e2`**.
Exact commands, raw outputs and environment are in the committed JSON evidence.

| Gate | Exit |
| --- | ---: |
| Scoped Asterdex cargo check, all targets, jobs2 | 0 |
| Full Asterdex cargo tests, all targets, jobs2, include ignored, threads2 | 0 |
| Extractor check: zero updates/additions/orphans | 0 |
| Independent tomllib census, notes/targets | 0 |

Three compiled production controls at that same SHA, with exact-byte restoration:

| Control / named native assertion | Broken | Restored |
| --- | ---: | ---: |
| Book retry100->50ms: `book-delay-multi-listener native attempt floor` | 101 | 0 |
| Drop queued kline: `kline-initialization-queue native pending` | 101 | 0 |
| Ready true->false: `reopen-insertion-order native ready` | 101 | 0 |

Each failure reached its named assertion after compilation; original/restored
hashes match. Iteration1 stops at three controls, one per family; zero survivors.

Not run: workspace compilation, other venue suites, host/wasm clippy, wasm
builds, real-network calls, full monorepo suites, CI reruns, complete socket
assembly, or the kline2000ms backoff-cap proof.

## Merge-down preservation

Merged integration `acf29148c3f26a07b72f93d1d964b49e81016ec6` at
`84c537a257859e2017383edca919bd0e28eb3829`. Required source/tests comparison
against gate SHA `f5f4b9c47dd69a57a06846849cf1e4f76210b8e2` exited **0**.
Extended comparison of crate scripts/manifests, live TS source/helper inputs,
Asterdex parity manifests, workspace/toolchain configuration and the eight
reachable local Rust package sources/manifests also exited **0**. Reachable
Cargo.lock graph equality exited **0**; no lockfile package changed. Commands,
scopes and comparison script are embedded in the JSON evidence.

Cargo exits remain original-code-SHA measurements, preserved by these input
comparisons; Cargo was not rerun at the merge SHA. At the merge SHA extractor
check exited **0** with zero updates/additions/orphans, and tomllib census exited
**0**, reconfirming eight moved and 42 pending. Exact commands and raw outputs
are recorded under `post_merge_gates`.
