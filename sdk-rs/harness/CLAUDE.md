# Rust harness: replay evidence and WASM budgets

Bybit create-batch peer acknowledgement matching changes native venue inputs;
it needs fresh optimized family/combined artifacts and executed native consumers.
The bounded server audit projection grants no resend or reconciliation authority.
Its peer scope decoder and identity checks share existing codec/sort machinery;
size-only changes still require fresh canonical artifacts, with profiles and caps unchanged.

Aster's independent request oracle validates every authenticated V3 method with
network-specific EIP-712 exact-query signing and a fresh, unreused agent nonce.
Timestamp/recvWindow and ABI/EIP-191 requests are rejected; V1 HMAC stays independent.

Sam removed the CPU gate matrix, shared certification, Bitget paired ratio gate,
planner and nightly monitor on 2026-11-17, for both master and pull requests.
The root `CLAUDE.md` records that decision; historical CPU thresholds and
certification instructions are not current CI requirements.

- Replay measurement and correctness jobs remain. Public replays retain CPU and
  memory evidence; private tapes remain correctness-only. Retained CPU metrics
  are observations, not the removed CPU gate statuses.
- Required coverage, actually executed combined consumers, strict replay
  validation and declared-known-failure checks remain required. A fast but
  incomplete replay cannot replace the audited correctness workload.
- The retained offline performance helpers still implement their historical
  comparisons, including the 10% retained-memory rule. Removing CPU gate jobs
  does not waive correctness, memory validation or WASM artifact budgets.
- Fingerprint movers still need fresh optimized web/Node CI size evidence;
  the module must fit its current cap (see Generated WASM caps).

## Binance replay isolation (Lever 9)

The former eleven-case Binance suite is partitioned across `binance`,
`binancespot`, `binanceinverse`, `binancepm` and `binanceoptions` in both correctness
and measurement matrices. The union, exact tapes/books and 16 public pairs are
unchanged; each physical measurement row seals and verifies its complete subset.
Every product still builds and compares the `binance` family against that same
family at the authenticated baseline, including baselines predating this split.

The measurement lane's native wire/fixture assertions run in
`Rust / replay native consumers`; its combined build, budget and WASM consumer
assertions run in `Rust / replay combined consumers`, parallel to the five rows. Nothing is
skipped or run concurrently inside a measured process. Truncated corpora, missing
cases and mismatched family seals still fail; budgets and policy are unchanged.
Full harness preflights serialize unit-test files on each runner to avoid
CPU-contention timeouts in scripted child fixtures. The full test glob, per-child
10-second timeout and replay assertions stay unchanged.

## Generated WASM caps (AGT-23, 2026-11-19)

Sam approved generated caps in place of hand-edited rule-17 commits: on #3640
those cap raises cost four extra CI/review rounds and `wasm-budget.mjs` was the
recurring merge conflict.

- **The numbers live in `wasm-caps.json`**, one cap per venue FAMILY plus
  `combined`; aliases resolve through `venues.mjs`. `readWasmCaps` in
  `artifact-census.mjs` refuses raw duplicates, unknown/missing families (against
  `artifact-census.json` `budgetVenues`), non-KiB values and any reserve other
  than 10 KiB. `wasm-budget.mjs` holds only the mechanism.
- **PRs fail only on the byte ceiling** (`wasmBudgetError`: measured > current
  cap). A reserve under 10 KiB is a `::warning::` and step-summary line
  (`wasmReserveWarning`), never a failure, and the bot restores it.
- **Over-cap uploads are PR-only (rule-17 owner, 2026-11-19).** On a PR a
  successfully sealed family stays downloadable after a failed byte ceiling or
  isolation check, so the author can run `update-wasm-caps.mjs` on it. On master
  (push or dispatch) a family uploads only when every step passed: an over-cap
  family stays red with no artifact, so `wasm-caps.yml` can never raise a cap
  master already overran. Failed sealing or cancelled runs publish nothing.
  Retention is measurement evidence only: the family job still fails, required
  coverage still refuses, and replay still requires a successful family job.
- **A PR that outgrows its cap raises it itself**, in a separate commit that
  edits `wasm-caps.json`, adds a dated `wasm-caps-history.md` line, and carries
  a CLEAR naming the growth. The bot is not a path for absorbing an overrun.
- **The reserve is restored on master by a bot.** `.github/workflows/wasm-caps.yml`
  runs after every master push run of `Rust SDK`, downloads that run's
  `wasm-family-*` and `replay-combined-package` artifacts and runs
  `update-wasm-caps.mjs`: every family seal is validated with the MEASURED tree's
  own `validateCiWasm` (its input list defines that run's fingerprint), the Node
  module must match its seal, size = max(web, Node), and
  cap = max(cap, ceil((measured + 10 KiB) / KiB) KiB). Caps are never lowered;
  unmeasured families keep theirs; an unknown family refuses. Raised caps are
  pushed (lease-guarded, recreated from master, keeping raises it already
  carried) to the bot-owned branch `bot/wasm-caps` and written to the job
  summary. **The workflow calls no PR API and has no `pull-requests`
  permission**: Actions PR creation/approval stays OFF in this repo because these
  images run on boxes holding user keys (coordinator, #3686). Sam's local
  `fleet-watch` opens "chore(wasm): restore rule-17 reserve" with his token when
  the branch's caps differ from master and no PR is open; nothing auto-merges.
  **A bot push to the open PR queues its pull_request runs as
  `action_required`**, and those empty suites shadow any dispatched run's
  checks in the PR rollup, so #3690 stayed BLOCKED with green dispatched `ci`
  and `Rust / required coverage` at 16b8c590. The job therefore skips the push
  when the caps equal the bot branch's, and after a push approves that head's
  queued runs (its only use of `actions: write`). Do not go back to dispatching.
- **Ceilings that are decisions, not measurements** (a combined raise ahead of
  measurement, a new family) are hand edits to `wasm-caps.json` with a dated
  line in `wasm-caps-history.md`, which also holds every pre-cutover provenance
  comment and measurement note. `wasm-budget.test.mjs` pins the mechanism and the
  cutover floors; do not reintroduce per-PR measurement census tuples.
- `wasm-caps.json`, its history and the updater are excluded from the beta
  wasm fingerprint (`HARNESS_NOT_BUILT`), so a bot raise forces no beta cut.

Native message-stat IDs borrow the existing topic key and one cached feed prefix
when encoding a frame, rather than retaining another formatted copy per topic.
The feed prefix uses inline decimal bytes; registering a feed adds no heap allocation.
The captured OKXEU X-Perp allocation regression preserves every emitted identity;
UTF-8, empty keys, interleaved strings and reconnect controls cover the writer.
The fixed 16-pair public replay and its 10% memory bound remain unchanged.

Bybit order-history retention is decided at completion and preserves earlier
page-failure causes.

OKX ordinary-history replay serves both recent and archive sources on every
product. The pinned eight-day window contains an archive-only fill, a stale
overlapping filled identity and a recent unfilled cancellation. The test-only
TS adapter reuses the frozen mapper/pager for both sources, keeping recent rows
before deduplication and applying the caller limit after the union. Strict
request matching and existing diagnostic allowlists remain required.
The archive-reference control seeds a complete market through `nativeFixture`
into `FrameStore` and publishes through `applyReferenceFixture`, matching the
neighboring balance reference; it never restores the removed mutable store API.

MEXC's HTTP 425 read labels are diagnostics only; typed refusals, backoff,
signing and write outcomes retain their existing controls.

WOO's exact signature-error auth classification changes diagnostics only; socket
recovery, signing, retry and write behavior retain their lifecycle controls.
Exact source 85a85576995b, Rust SDK run 44220127210, artifact 14390881543
measures equal optimized web/Node/replay modules at 2,683,302 bytes. The
coordinator's Rule 5 floor is 2,694,144 bytes, retaining 10,842 bytes and every
historical floor.

HyroTrader correlated auth-refusal metadata changes a Bybit-family input.
Keep its native admission/uncertainty controls and fresh optimized family evidence;
server negative-join fixtures retain the protected-provider and zero-write guards.

OKX empty-envelope diagnostics retain code-0 cancellation continuation, nonzero
stop behavior and the original native outcomes. Its scoped ordered-write tests
pin complete per-order cancellation reporting and the explicit unknown-diagnostic
fallback; optimized family evidence remains separate from these native controls.

BingX Spot qualification adds independent Spot public/private cases to the `bingx` suite.
The nondefault `bingx-spot-replay` WASM feature exports a fixed-credential fixture constructor;
normal factories still refuse Spot. Shipped family size checks run before the fixture build.
The ordinary BingX family includes the existing book and symbol-cancel input
codecs. Offline production probes run through the shipped/fixture admission
entry point and require no dispatch of unadvertised commands; perpetual capability
flags and the Spot admission/smoke gates remain independent.

BingX history diagnostics inspect failed responses with closed streaming visitors.
Compact optional detail preserves engine error and socket allocation budgets;
fresh web/Node artifacts must fit the existing family cap.

TxFlow agent refresh #3314 at merge 9bb43250830d measures 2,147,684 bytes;
refusal fix e959e2a7dc2e measures optimized web/Node 2,147,758 bytes. Its
2,158,592-byte floor retains the required 10 KiB reserve at that source.
The review fix based on 2c37c0e6af2d measures optimized web/Node 2,148,376 bytes;
its 2,159,616-byte floor retains 11,240 bytes. The private tape supplies the
owner-bound disposable agent approval before writes. Replay identity equates only
absent and explicit default `signatureChainId:2511`; economic action fields and
nondefault chain values stay exact. Signing validation remains independent.
WEEX in the same composition measures 2,028,105 bytes and retains a 2,038,784-byte
floor by the same rule. Merge f19209120d28 measures one byte more in each
family (2,148,377 and 2,028,106 bytes), retaining those floors and 10 KiB reserves.
Pinned profiles and remaining family/global budgets are unchanged.

TxFlow construction-feature guards require dispatch signing, address/nonce setup
and its hosted-wallet codec, with independent missing-feature refusals. The HTTP
HMAC hosted-signing feature cannot substitute for that testnet wallet codec.

Bybit product parity fixtures service the engine's set/clear timers and retain
original waiting HTTP permits until release or cancellation. Their monotonic
virtual clock also validates native signature timestamps; waits never refresh deadlines.

Shared artifact membership is frozen in `sdk-rs/artifact-census.json`. Additions
and retirements update that independent census explicitly; value-only edits do not.
Budget imports validate raw duplicate/name membership before evaluating the map.
The closed source scanner ignores nested template text and expressions, rejecting
unsupported regex/division syntax instead of guessing their lexical boundaries;
construction feature checks reject raw duplicate keys and missing policy rows.

Bybit terminal order-history retention is documented in `sdk-rs/venues/bybit/README.md`.
Its `history.rs` change is a counted beta-WASM input and requires a fresh release/pin;
this routing correction does not change family caps or construction membership.

Session adapter integration consumes native frames through `FrameStore`, including
fresh snapshot baselines on reconnect. The retained Lighter TS replay oracle uses
the generated nullable liquidation price contract: unknown prices, including
zero-quantity placeholders, are null before fixture encoding. It retains the
order-ID randomness and signature verification assertions.

The retained Bybit and inverse TS replay oracles publish unknown/nonpositive
liquidation prices as native null, including flat and option positions. Their
SCXF fixtures must pass the same generated Position contract as the Rust engine;
a zero/invalid liquidation sentinel cannot complete TS private replay startup.

BingX Spot replay tracks production decoder receipts alongside pending account reads.
Bounded drains precede ticks, checkpoints and final capture; event-loop yields
alone cannot establish completion of gzip decoding or its queued text messages.

#3640 retained BingX Spot and OKX fixture callbacks publish SCXF through the
explicit reference producer. Aster installs its taped axios transport before
construction so its closed secondary read client inherits it as well.
GitHub runner assignment metadata may lag started measurement steps: CI44248450357
returned runner_id=0 beyond the former 60-second window. Physical-job discovery
now re-lists for at most five minutes; missing/duplicate identities, retries and
nonpositive runner IDs still refuse, and replay/signing deadlines stay unchanged.

The native workspace test row installs pinned Node and locked Yarn dependencies:
store-frames' risk and ownership tests execute maintained TypeScript/FrameStore
oracles. Missing tsx/TypeScript must fail setup rather than drop those cross-language tests.

Orderly account settings preserve simultaneous Cross/Isolated leverage: a
selected margin mode updates only that mode's positions. Unknown/global settings
retain their existing behavior. Orderly opts into full-order TIF comparison, representing absence as explicit
null across order types while retaining every reported value. Other families
keep their established limit-only comparison scope. The projection never
manufactures a compared TIF. Known-failure and accepted-difference manifests
are unchanged by this correction.

Native-store frame memory reservations include product metadata, ticker observation
columns, message-stat numeric rows and their string tail before temporary interning
tables are allocated. Full books reserve their known level columns, and the string
tail includes final alignment. LOCAL copies each owned flush once to JavaScript; snapshots keep independent
baselines.
The public 16-pair memory gate and all compared fields remain unchanged.

Managed HTTP preparation observations change shipped host JS, so they move the
beta input fingerprint and require a new release/pin. They do not change Rust
sources or waive fresh combined-consumer/size evidence or any retained floor.

Native CI is fail-complete. Every `Rust / native <check>` row runs its matrix
`command` through `run-all-checks.mjs`: one command per line (no `&&` chains),
each line runs even after an earlier failure, every `cargo test` carries
`--no-fail-fast`, and the step ends with a per-crate list of failing tests in the
log and step summary while still failing the job. `test-http-plan-feature-modes.py`
likewise runs every engine mode in its shard before failing. CI splits the modes
round-robin over three `engine-feature-modes (i/3)` jobs (`--shard i/3`; one serial
job took 30-38 min and set the Rust critical path); every shard sorts the same list,
so together they run each mode exactly once. #3640 needed four runs to surface
~124 failures because the first failing target stopped the job;
`affected-ci-workflow.test.mjs` pins this shape.

Bitunix managed sockets retain private ping bytes, the independent ten-second
login-answer bound with Observe, first-pong idle gating, balance pacing and
read/reconciliation controls. Native host configuration validates the declared
4096 capacity; direct book/candle commands bound canonical future topics before
intent/settings mutation, independent of epoch state. Aliases share native topics;
refusal preserves accepted intent and the live feed. Frame-fed engines register
no managed Bitunix feeds. An unopened private dial is not a login-answer timeout.
Historical #2301 measurements and retained branch ceilings are archived in
`wasm-caps-history.md`; use the generated cap mechanism above, without restoring
per-PR measurement census tests or superseded batch/reviewer-count holds.

Binance saturated-history paging preserves the existing request pacing and caller
deadlines. The measured #3671 Binance floor and its historical CI provenance are
retained in `wasm-caps-history.md` as historical evidence. AGT-23 supersedes that
PR-local floor: the active cap follows master, and only the master artifact bot
restores its reserve. Fresh artifacts must fit the byte ceiling; reserve warnings
remain advisory. Native/combined consumers remain independent gates.
