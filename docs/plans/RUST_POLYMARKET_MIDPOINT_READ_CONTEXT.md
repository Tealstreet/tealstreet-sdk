# Polymarket unsigned midpoint seed custody

Plan first, 2026-10-24. Fresh branch
`feat/rust-polymarket-midpoint-read-context` starts at actual
`557de7ae7fbf6d2a00fdd3cbf1d0a962bd7fc570`. This checkpoint is plan plus
original controls only; production and inventories wait for review.

## One exact request occurrence

Convert only lib.rs::midpoints' unsigned POST to
https://clob.polymarket.com/midpoints: one lexical request occurrence, reducing
Polymarket from14 to13. Gamma keyset discovery remains separate. The existing
public command Read helper and all private account/trading/signing code stay
unchanged. No shared engine/host/Cargo API, G1/G2, new Plan contract, key/nonce
selection, construction rules or credential-selection inventory is needed.
The pending combo-5b source map has no Polymarket paths.

Each midpoint batch is an independent existing Background Read, at most100
current market tokens. Native response processing advances the next batch;
failed offsets retry through the existing jittered startup Backoff. There is no
command receipt, numeric terminal, mutation journal, aggregate Plan or retry
credit. Every actual native retry admits a fresh bounded single HTTP operation.

## Native wire and publication contract

Keep exact POST method, URL, Content-Type application/json and serialized
[{"token_id":"..."}] body, declaration order, hundred-token partition and final
short batch. Signing and account keys remain absent even though HTTP uses POST.
Private startup remains independent of midpoint completion. Preserve successful
subset coverage, ignored out-of-batch IDs, all-bookless empty success, rejected
row handling, quote fields and live-ticker overlay/finish behavior. Native
public retry backoff, warnings, offset progression and publication order remain.

Declare an explicit new20-second admission-to-expiry bound per batch, starting
before host Prepare. This is a caller-visible timeout policy change, not native
equivalence; record it in CALLERS at adoption. No renewal on Prepare, Bind,
transport release or response. A host that never prepares must still time out.
Expired DATA must not decode its envelope/body, publish ticker state or clear a
newer attempt's inflight marker. The typed timeout failure may perform exact
cleanup (including Cancel), advance the next native batch and independently
back off the failed offset; that is failure-owned progress, never expired-DATA
success. No unbounded native
poll/traversal is packed into an immortal Context.

## Existing API and exact cleanup

Use existing http_context(Scope::Background, unsigned Read Policy), timeout20000,
no pacing, RetryPolicy::Never, then the existing managed request with headers.
Carry the actual Context with offset in Pending. Fix each inflight offset to its
exact Context so stale failure or DATA cannot consume a newer retry's ownership.
The release passes through genuine Prepare/Bind/Claim; no raw fallback.

Gate exact Background Read delivery before success(response), JSON, native
coverage/quote bookkeeping or publication. Retire the exact old HTTP Context
before a native next-batch/retry admission. Typed preparation/transport/timeout
failure performs exact failure-only cleanup and the existing native warning and
backoff; it does not require DATA authority or infer auth evidence. Admission or
queue refusal must release that offset's ownership and retain native retry
scheduling, without clearing a different active attempt. Preserve typed failure
metadata at the shared boundary; no fabricated credentials or outcomes.

## Original controls and review gates

Commit this plan before a separate new integration target on unchanged source.
Assertion REDs must exercise real Engine startup, actual Gamma response and
native midpoint request: eager raw release before preparation, response at the
new deadline still publishing, and unprepared work lacking its admission timer.
A missing future API or compile error is not evidence. Working controls pin the
exact unsigned body/headers, subset and bookless success, native retry/recovery,
multiple hundred-token batches and ignored cross-batch rows. No test changes
native reducers or exposes a private runtime hook.

Run only the new scoped test target with CARGO_BUILD_JOBS=4 and one local Cargo
at a time. Record executed failures and GREEN controls honestly; assertions
after an original early failure remain mandatory adoption gates, not separately
reproduced evidence. Seek bounded core/caller review before production. Subsequent
adoption requires unchanged controls GREEN, exact one-site inventory removal,
foreign-map and credential-selection preservation, scoped native/strict checks,
fresh CI and family size/performance. No push, production, READY or merge is
part of this plan/RED checkpoint.

## Original fixture correction

The first compiled run found the three intended custody failures plus an
incorrect working-control assumption that tickers remain unloaded between
batches. Native flush_midpoints publishes accepted partial batches through
set_tickers, which marks them loaded. The working control now asserts that
existing incremental publication explicitly; no production behavior changes.
That fixture mistake is excluded from original custody RED evidence.

## Original executed checkpoint

Plan `8170f608e03` precedes original controls `ca7e4de4183` and the native
incremental-publication correction `da143d47204`. Unchanged production compiles
and executes eight controls: three assertion REDs, five GREEN, zero ignored.
The failures are actual eager raw midpoint HTTP before Prepare, valid late DATA
still publishing at the proposed20-second bound, and absence of the admission
expiry timer before host preparation. Later assertions in each failed control
are mandatory adoption checks, not separately executed original evidence.

Exact unsigned wire, subset/bookless publication, native transport retry and
recovery, hundred-token partition/cross-batch filtering and malformed-response
backoff all pass. The corrected receipt is
`/tmp/codex-hcmq3f-polymarket-midpoint-original-corrected.log`.
Only this plan, README index and new test differ from the exact557de base;
production, every inventory, selection/nonce sites and5b source remain unchanged.

## Core review correction before source

Core review at722a found the expiry test's no-actions suffix contradicted native
102-token failure progress: failed offset0 schedules its retry and immediately
starts offset100. The original early no-publication failure remains genuine,
but the later blanket prohibition is withdrawn. The corrected suffix permits
shared cancellation and requires the exact next batch plus the original
failed-offset backoff/retry. A separate native102-token transport-failure
working control pins that sequence. Production and all maps remain unchanged.
