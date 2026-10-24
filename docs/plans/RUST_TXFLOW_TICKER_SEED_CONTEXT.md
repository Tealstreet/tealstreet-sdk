# TxFlow unsigned ticker seed custody

Plan first, 2026-10-24. Owner codex-hcmq3f. Local branch
`feat/rust-txflow-ticker-seed-context` starts from actual
`a10974b2f22a2be01b0cc68ff78f91bd0e253539`. This checkpoint changes only this
plan, the plan index and a new original-source test target. No production,
manifest, credential selection, Cargo or shared engine edits are authorized by
this checkpoint alone.

## Inventory and source boundary

The canonical inventory has one `cx.request` in `ticker_request`, lib.rs:220:
`4acf3c28534e3da6c67ffe837e424df6be41024ef99ee653ab4d68994a510f42`, count one.
Adoption removes precisely that sender: actual family 25 to 24. If the separate
local-custody #2040 and command-candle #2071 land first, compose them genuinely
and report the resulting 7 to 6 instead; neither is claimed consumed here.

The future source boundary is Pending::Ticker, ticker_request and its exact
response/typed failure ownership. Metadata's shared streamed info sender stays
raw in this slice. Account, history, trading, signed requests, sockets, command
candle handlers and all key-selection sites remain unchanged. CPU #2031 is
actual: preserve Instrument hints, ticker row indexing, context/ticker reducers
and live overlay application exactly. Only the ticker seed request gains custody.

## Existing API and intentional bound

Each native physical attempt uses a genuine keyless Background Read Context,
Never retry, no pacing and explicit PublicCors egress. Pending and a private
per-asset current-attempt marker retain that exact Context. Existing managed
Prepare/Bind/Claim APIs send the unchanged POST /info body
`{"type":"activeAssetCtx","coin":"<numeric asset>"}` with JSON Content-Type
then x-api-version. No fabricated credential, command, Plan or selection is used.

The 20-second admission-to-expiry bound, including stalled Prepare/Bind, is NEW
policy for this previously unbounded raw sender, not native timeout parity.
There is no logical 20-second deadline for the whole sequential seed walk.
Exact current Context and Background DATA validation must run before HTTP status,
envelope/identity decoding, seed/live state, diagnostics or ticker publication.
At equality DATA has no authority. Failure-only returned Pending cleanup does
not require DATA; preserve typed error fields and retire only its exact owner.
No command or write outcome is manufactured.

Finish the old Context before native success/failure progression or a new
attempt. Current success still marks its asset seeded, clears its keyed Backoff
and arms the original 25ms next-ticker timer. Current transport/preparation/expiry
failure still schedules its keyed Backoff AND the 25ms next-ticker timer, so an
independent queued asset progresses while the failed one waits. Native final
loaded publication, partial-seed/private startup eligibility and socket ordering
are unchanged. Stale callbacks may neither clear a replacement marker nor touch
its native cadence/backoff/reducer. Admission or queue pressure must use bounded
native failure progression without recursive admissions or leaked credits.
Nonfinite admission must not issue HTTP or arm nonfinite recovery timers.

## Original controls and gates

Commit original controls before executing against unchanged source. Separate
first assertions pin Prepare before physical HTTP, a pre-Prepare expiry timer,
and no ticker publication from DATA at the new exact deadline. Working controls
pin exact unsigned wire/PublicCors/version header, valid financial projection
and native 25ms progression, identity mismatch rejection, transport-failure
Backoff and subsequent genuine recovery. Use actual Engine events and released
managed actions; no private state or stubbed provider is needed. Compile or
fixture failures are excluded from assertion RED counts.

Before source freeze add real pressure/credit recovery, typed precommit and
released failures, stale same-asset replacement, late host Prepare and exact
expiry failure progression controls. Preserve every original assertion. Obtain
bounded native/caller review from the CPU owner and core API/custody review
before production. Source follows the standing rule after CLEAR, with a draft
freeze candidate and one PR-triggered Rust run in parallel with exact committed
source review. No WIP push or explicit duplicate dispatch. Scoped crate/tests
only, CARGO_BUILD_JOBS=4 sequentially; CI owns workspace/feature/size/performance.

Original fixture compile correction: the first build matched HttpAction::Prepare
as a tuple variant. It is a struct variant; the pattern is now `{ .. }`. No
assertion or runtime changed. That compile failure is excluded from RED counts.
