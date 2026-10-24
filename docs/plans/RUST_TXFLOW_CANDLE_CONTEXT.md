# TxFlow command candle snapshot custody

## Exact checkpoint and scope

Base: actual `219d4bc6725b03afbab5411096d60fe38f6a3350`. Owner: codex-hcmq3f.
This checkpoint contains a plan and new original-source controls only. No runtime,
Cargo, existing fixture, inventory or credential-selection change is included;
no push or CI dispatch is authorized for this checkpoint.

One unsigned command `candleSnapshot` POST `/info` is the physical scope. The
future conversion removes THREE lexical terminal occurrences in `lib.rs`:
`c5a021da6919a28b758460074e6af502c4c6d47cf4f00443bd987daccb600548`
(count one, response success) and
`c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa`
(count two, response codec failure and transport failure). Tip inventory is
25 occurrences; this slice alone yields 22. Separately reviewed #2040 removes
15 initial local occurrences, so their eventual composition yields 7, without
claiming #2040 is landed. The two slices touch distinct command phases.

`info()` remains a genuine legacy sender for metadata/history and other native
consumers. Its fingerprint is NOT removed by managing only candles. The future
candle branch uses the real unsigned managed API directly; no renamed raw sender,
raw compatibility branch or artificial credential is introduced.

## Native behavior to preserve

`on_command -> fetch_candles` is the sole command entry. Preserve native asset
resolution, unsupported interval and range precedence before physical issuance;
those three initial refusals belong to #2040. Keep the exact numeric asset ID,
interval, floor-to-integer start/end, default 500/cap 1500 range calculation,
POST `https://rpc.txflow-testnet.xyz/info`, PublicCors intent, header order
`Content-Type: application/json`, then `x-api-version: 2026-08-16`, and streamed
`data` array. No agent, signer, account identity or credential is required.

Keep the native CandleRow/Header codecs, seconds-based result timestamp, every
OHLCV field, valid empty array, malformed-row poisoning, envelope/status failure
and zero automatic retries. Subscriptions remain API-feed work with unchanged
150ms pacing. Metadata/instrument hints, ticker caches, account refresh, order
reducers, history, signing, shared engine and host source are outside this slice.

## Existing API fit

After native initial validation and before reserving opaque work, inspect the
real `command_wait`: None uses `reject_unstamped_command(Read)`, Expired uses
`reject_expired_command(Read)`, Remaining admits the real caller root through
`admit_caller_read_root(None, None)`. Create one child with `http_leg_context`:
Read, credential None, timeout 20,000ms, no pacing, RetryPolicy::Never. This is a
NEW physical admission-to-expiry policy including stalled Prepare/Bind, not a
claim about a native request_timeout or a fresh caller wait. The actual receipt
bounds the root exactly; each child is capped by it. Do not reconstruct a wait
or use a default deadline when its genuine stamp is absent.

Pending::Candles retains the exact root and child. Queue with the landed
`request_managed_with_headers` API and `stream_response(request, ArrayAt::Field("data"))`. Every
streamed row and whole response must pass exact `validate_read_http_delivery`
BEFORE codec/envelope/body work. Success and codec failure use contextual
completion of the original root, and retire its exact child. A queue refusal
receives the original Pending back and cleans exactly that owner, without
numeric fallback or a replacement admission. Typed preparation/sign/transport/
physical-timeout failures keep their supplied metadata and need no DATA grant.
There is no retry, fallback, page, Plan, group or new core seam.

A duplicate command cannot mint a second physical attempt, replace the receipt,
retire its original owner or steal its terminal. Late DATA, duplicate response,
stale preparation and retired permits must not affect a later generation. All
terminals retire children/root through existing APIs; no write fact or outcome
is invented for the public Read.

## Original controls and source gates

New isolated `tests/candle_context.rs` must compile against unchanged production.
Expected assertion REDs cover eager raw release, missing unprepared deadline,
late DATA at the new physical bound, exact shorter caller bound, invalid clock,
shared capacity, host preparation custody and typed released transport failure. Native
wire/financial codec, empty/error envelopes, zero retries, local precedence and
engine duplicate-response suppression are working controls. Every RED is a
new-contract adoption boundary, not evidence that a pre-existing managed path
regressed. Setup/compile/fixture failures are excluded from RED receipts. Any
suffix after the first failing assertion is explicitly unexecuted until GREEN.

Before source: obtain bounded core/API and native/caller plan/control review.
Before freeze: preserve every original assertion, run the scoped crate target
with CARGO_BUILD_JOBS=4 and no competing local Cargo, strict Clippy/fmt, normal
bans/inventory/credential ratchets, genuine Prepare/Bind/Claim plus typed refusal
and credit-recovery controls, stale-generation recovery, and actual #2040
composition as required by landing order. Reuse the existing family CI/size gates
without budget/profile changes; only push a head intended to freeze and never
dispatch a duplicate explicit run when a PR-triggered run covers that head.

## Executed original receipt

Plan commit: `337b9c9c7ef`. Original controls: `9e85259220e`.
Scoped unchanged-source execution: **14 tests, 8 assertion REDs, 6 GREENs,
zero ignored**. Strict Clippy for only the new target passes. No source, old
fixture, Cargo, map or key-selection change is included.

The eight first failures are: eager raw HTTP; missing pre-Prepare 20s timer;
raw HTTP on NaN; no shared capacity terminal after 128 parked requests; missing
Timeout at the physical deadline; missing Timeout at the 37ms caller receipt;
zero managed preparations for a preparation-refusal control; and released
Network losing its kind. Capacity recovery, stale late Bind and typed precommit
outcome assertions after those failures remain unexecuted adoption gates.

The six GREENs pin exact URL/method/header order/PublicCors/body and all OHLCV
fields, native fractional flooring plus empty success, strict malformed/status/
envelope errors without retries, initial validation precedence, duplicate-entry
preservation of a real first request, and duplicate-response suppression.
All fixtures use native metadata/ticker startup at explicit event clocks;
managed cases use genuine prepare/bind/claim via `prepared_actions`.
Absence-of-new-HTTP assertions explicitly permit cleanup Cancel actions.

An initial test-only CommandId-width compile error was corrected before this
executable checkpoint and is excluded from RED evidence. The first post-build
collector refinement allowed cleanup Cancel; the final immutable control file
was rerun with the same 8 RED/6 GREEN result.

## PublicCors source gate

Actual `admit_caller_read_root(None,None)` has no egress argument, while
`http_leg_context` only inherits its parent's egress. `route_request` does not
rewrite a queued ManagedHttp Prepare. Therefore the straightforward child API
above does not yet prove preservation of native explicit PublicCors.

Core-owner fit review is requested for an existing-API composition: after the
real retained root, issue a same-command physical Read with
`http_context_with_egress(Scope::Command(actual_command), policy,
Egress::PublicCors)`, its timeout capped by min(20,000, genuine Remaining).
The engine enforces the retained root's upper bound and exact DATA validation
checks real root/current context/same owner. This is a proposal, not source
clearance: no admission, fake identity, raw transport fallback, fresh numeric
completion or recreated caller receipt may replace the original root. If this
shape is not supported, the scope waits for a separate owner-reviewed egress
entry hook. No shared runtime edit is authorized by this plan checkpoint.

## Reviewed implementation and source controls

Uufub4 CLEARed the existing PublicCors geometry and exact two-file source
preview `1ce6a01bfd74d5e3eb5bc2b2f7b7a2236d4055e20d83595331bd9367a4975ed5`:
retain the real caller root, issue the bounded physical same-command Read with
explicit PublicCors immediately in that genuine entry, and finish only the root.
5c57jl CLEARed native/caller/original controls at `acbb8b95334`. No new core seam
is needed. The helper captures min(genuine Remaining,20,000) at issuance;
no later callback or numeric replay may use this construction.

Source commit `a0ed1156966` applies the reviewed preview, plus removal of the
now-orphaned lib.rs CommandId import caught by the initial strict compile.
That compile failure is excluded from runtime results. All 14 unchanged
original controls now PASS, including the previously gated pressure/recovery,
late Bind and typed precommit suffixes.

Five additive controls pin actual Prepare intent plus PublicCors route/header
order through Bind/Claim; fractional caller-deadline release and DATA equality;
unclaimed Bind expiry without renewal; stale preparation/failure/DATA against
actual command-generation reuse; and full 128-owner completion followed by
reacquisition of all 256 root+child slots. Original control bytes remain an exact
prefix; no oracle was relaxed. The combined target passes 19/19, zero ignored.

Scoped adoption verification: all 164 TxFlow native tests pass with zero ignored,
including the 19 dedicated candle controls. Strict all-target crate Clippy and
package formatting pass. The pre-existing public candle fixture now collects
real managed HTTP through `prepared_actions` at the unchanged command clock;
its wire, body and financial assertions are unchanged. The first native run's
raw-HTTP collector failure and the formatting-only import wrap are retained in
the local verification logs; neither is counted as an original RED.
