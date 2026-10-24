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
`request_managed_with_headers` API and `stream_http` data selector. Every
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
shared capacity, duplicate owner and typed released transport failure. Native
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
