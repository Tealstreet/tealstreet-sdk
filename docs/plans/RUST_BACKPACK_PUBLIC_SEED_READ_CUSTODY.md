# Backpack public-seed Read custody

## Scope and baseline

Plan before originals and production. Base: actual #2101 landing
3614cd119b606c1da9b7f2ad8cb3bc52e86ba970. Owner: codex-mxb4mq;
bounded native/API/source reviewer: codex-hcmq3f; landing auditor: claude-8x5tm1.
Overseer approved Markets, daily tickers and MarkPrices; approximately 200–300
production lines plus controls, one legacy request occurrence. No generic engine,
private account/trading/signing/socket production or key-selection change.

The shared get helper in venues/backpack/src/lib.rs has one request_streamed
fingerprint, 9cce7db7c9b199779663966a17b3855d01e71c0b43228a87d1e38cc70b74cd6a.
Its five call sites issue three native GET routes: /api/v1/markets,
/api/v1/tickers and /api/v1/markPrices. All have empty query/body,
X-Broker-Id: 1200, Public intent and ArrayAt::Root streaming. Remove the helper
only after all callers migrate; prune only this fingerprint, count one.

## Native behavior to preserve

Markets filter hidden, non-PERP, invalid-symbol and nontradable rows. Precision,
limits, leverage, row ordering and diagnostics stay exact. Empty catalogues retry;
partial catalogues publish accepted rows, retain listed markets on refresh and
use the native bounded-progress re-read. The first usable catalogue connects
public feeds, initializes native private startup if eligible, then requests both
ticker and mark seeds. A catalogue refresh does not restart those side effects.
Daily rows retain native percentage/volume and malformed-known-symbol placeholder
handling. Empty/unusable daily rows retry. Empty mark arrays succeed; malformed or
unusable nonempty marks retry. The first failed mark read unblocks startup with
last-price fallback, and a later successful retry updates marks in place. Live
WebSocket fields retain their existing overlay protection. No financial mapping,
readiness, independent retry jitter/backoff counter or diagnostic text changes.

## Proposed managed boundary (review gate before source)

Use the existing Scope::Background + Operation::Read, no credential, no pacing,
RetryPolicy::Never Context and request_managed_with_headers(context, None, GET,
url, PUBLIC_READ_HEADERS, None, pending), with ArrayAt::Root managed streaming.
Retain the exact Context in each Markets/Daily/Marks pending value. The proposed
20,000 ms admission-to-expiry bound is NEW policy, not legacy parity; list it in
CALLERS. Preparation/Bind/Claim and DATA use the engine's actual authority.

Native startup::Backoff is an indefinite independent recovery scheduler, not a
physical retry on a caller's finite root. Proposed interpretation: each scheduled
seed attempt is a new bounded background observation, retired before backoff;
no Context is renewed or retried, and no policy grants Retry on a prior Context.
The original native scheduler owns when a new attempt starts. Reviewer must
explicitly clear this interpretation before production. If a retained logical
root is required instead, use a reviewed existing seam; do not invent authority
or modify the engine in this slice.

Validate exact background DATA before item parsing, skipped-row notices, status,
Store publication or startup side effects. Retire the exact Context on every
terminal, including failed delivery, preparation/queue failure, released network
failure and expiry. Failure-only cleanup must not fabricate DATA. Associate an
attempt with its seed kind/current generation so stale failure/success cannot
schedule recovery or publish for a replacement. An admission failure emits no
HTTP and follows the reviewed native recovery path without a numeric fallback.
Invalid clock errors must not schedule invalid timers.

## Overlap and fixtures

Checked all 24 open PR file maps and 51 local Backpack branches at the baseline.
No unmerged production overlap in lib.rs or the new public_seed_context.rs target.
#2093 owns sign.rs; #1961 owns trading; #2033/#1990 also touch shared test support.
Do not edit their production. The fixture audit finds raw startup Action::Http
collectors in existing tests (including public.rs, book_seed_context.rs and
support/mod.rs). Managed adoption necessarily requires genuine prepared_actions
at those startup collection boundaries. Preserve every existing assertion,
financial payload, event clock and private signing collector. Before editing the
shared support file, send its exact minimal startup-only collector hunk to the
reviewer and notify the overlapping owners; no broad fixture rewrite or blind
replacement of all drain_actions calls. Additional affected fixture paths must
be named in the source review and qualified as mechanical transport adoption.

## Original controls and source-stage gates

Commit originals separately on unchanged runtime. New target public_seed_context.rs
uses self-contained fixtures and actual engine Prepare/Bind/Claim. Native working
controls: exact three-route wire/header/method; accepted financial rows and load
ordering; empty marks; malformed/empty catalogue/daily recovery; failed marks
fallback and later repair; partial catalogue and live overlay. Contract REDs:
initial market and derived ticker/marks requests issue managed Prepare, no raw
HTTP; held preparation cannot accept fabricated DATA; delayed preparation and
released response at the exact NEW 20s bound cannot publish; malformed late
payload emits no row diagnostics; released failure retains native recovery.
Use controls that execute on the old runtime and fail assertions, not imports.

Before freezing: preparation and queue/capacity refusals; terminal disposal of
exact contexts; duplicate/late response inertness after a replacement; repeated
recovery/turnover under pressure without retained context growth; exact native
success and failure assertions across existing Backpack tests. Evidence limits
must identify suffixes that cannot execute until source exists. Native-backoff
clock behavior and the new bound are separately labelled.

## Qualification and delivery

After bounded plan/original CLEAR, implement and commit source. At FIRST source
commit measure combined and Backpack WASM using the same pinned toolchain on
bare base and composed source; no budget raise. Bybit/Bitget are required only
if generic engine source is touched (none planned). Record bytes, deltas, hashes,
commands and versions outside git. Scoped Cargo tests/strict Clippy use jobs4,
one own build at a time; local bans and all relevant inventory checks must pass.
No new full-workspace/all-feature build. Shared test fixtures get preservation
review before freeze. One intended frozen push/draft PR and one qualifying PR
Rust run. Green exact-head CI plus hcm source CLEAR -> READY to 8x5tm1 with tree,
size receipts and run ID. Landing requires auditor exact-tree GO and guard.

## Checkpoints

- Plan only; runtime, inventories and fixtures unchanged.

- Originals on unchanged runtime: 17 executed, 5 native GREEN and 12 assertion
  RED, zero ignored. Three each: managed Prepare, held DATA, NEW 20s equality,
  late malformed diagnostics. Marks boundary permits native last-price fallback;
  it forbids adoption of the failed observation. Working controls cover three-route
  wire/financial mapping, empty marks, mark failure/recovery, duplicate success,
  and empty catalogue/daily recovery. Receipt is external, no runtime proof yet.

- hcmq3f CLEARed plan/originals at 6ced, including distinct native Backoff
  observations and exact startup-only fixture patch 613a23e3. Initial source
  patch 96b22d91 was NOT CLEAR: completing credit outlived successor admission,
  and nonfinite released failure clocks could schedule nonfinite retry timers.
- Added three original assertion controls on unchanged runtime: 254 engine-issued
  peer Contexts leave catalogue credit to be retired before both derived
  preparations; probe credits after each terminal must be 0/1/2. Genuine issued
  and released responses/failures at NaN and either Infinity must not publish
  payload, schedule a timer, or reissue. Native failed-mark fallback is permitted.
  Original 17-test source prefix is byte-exact; now 20 executed: 5 GREEN, 15
  assertion RED, zero ignored. Corrected source remains unapplied pending review.
