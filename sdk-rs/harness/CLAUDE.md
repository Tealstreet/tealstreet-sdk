# Rust harness: replay evidence and WASM budgets

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
  (`wasmReserveWarning`), never a failure, and PRs do not edit caps.
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
  the branch's caps differ from master and no PR is open, so CI runs on it
  normally; nothing auto-merges.
- **Ceilings that are decisions, not measurements** (a combined raise ahead of
  measurement, a new family) are hand edits to `wasm-caps.json` with a dated
  line in `wasm-caps-history.md`, which also holds every pre-cutover provenance
  comment and measurement note. `wasm-budget.test.mjs` pins the mechanism and the
  cutover floors; do not reintroduce per-PR measurement census tuples.
- `wasm-caps.json`, its history and the updater are excluded from the beta
  wasm fingerprint (`HARNESS_NOT_BUILT`), so a bot raise forces no beta cut.

Bybit order-history retention is decided at completion and preserves earlier
page-failure causes.

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
likewise runs every engine mode before failing. #3640 needed four runs to surface
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
