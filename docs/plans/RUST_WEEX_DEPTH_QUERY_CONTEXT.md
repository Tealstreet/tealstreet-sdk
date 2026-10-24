# WEEX public order-book query custody

Status: original assertion evidence reviewed; production conversion under source review.
Base: actual `77f124089477a6aed502d9b02aa7f08752b7bfa5`.

## Scope and exact inventory

Convert only command-owned unsigned `FetchOrderBook` / `Task::BookQuery`.
The current semantic slice has four numeric completion occurrences:

| File | Fingerprint | Count | Current role |
| --- | --- | --- | --- |
| venues/weex/src/depth_query.rs | 7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b | 1 | success `cx.done` |
| venues/weex/src/depth_query.rs | c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | 2 | HTTP/native and codec `cx.fail` |
| venues/weex/src/lib.rs | 0d6bbb8e01da4f9bc9a177c0e8ca9d0d015c922780050cfeda6b33807cac8e78 | 1 | BookQuery transport `cx.fail` |

The two shared `get` request sites remain for unrelated discovery/subscription
callers. Bypassing them for this command does not remove their fingerprints.
The scanner must determine the exact removal-only manifest at source cutover;
there is no STAGING/bootstrap or whole-family completion claim.

No shared API or in-flight engine PR is required. Caller-wait stamping (#1945),
original Read root expiry (#1976), managed HTTP and the original-root completion
bridge (#1848) are on the base. This is neither Background nor Plan work, and
uses no SelectedRead because the native request is public and unsigned.

## Native contract to retain

- Unseeded/keyless queries work without `start`, market lookup, subscription or
  account data. URL is `/capi/v3/market/depth?symbol=<encoded>&limit=<value>`;
  absent limit is 200 and explicit 0/u32::MAX remain unmodified. Headers are
  empty, body is absent, and no signer/provider is invoked.
- Preserve alias precedence (`bids ?? b`, `asks ?? a`), native row order, price
  and amount validation, cumulative base totals and exact string/numeric version
  handling. No contract-value scaling, cache/book publication or level IDs.
- Non-2xx/native error classification, venue code/message and retry-after fields
  retain their native shape. Existing explicit native HTTP/codec NotSent fields
  are not silently normalized by this slice. Genuine Read entry/Context admission
  uses its canonical outcome None; managed pre-release preparation/provider
  refusals retain their exact typed NotSent fields. Released Network failures,
  engine physical Timeout/integrity failure and root expiry use Read outcome None. The native
  transport wrapper classified even host timeout as Network/NotSent; the new
  typed Network/Timeout distinction is an intentional CALLERS change. A query
  parked behind a pause previously waited without a venue whole-query bound; it
  now ends at the caller receipt deadline, also an explicit CALLERS obligation.
- Preserve the native shared 429 pause only after exact live root/leg/attempt
  admission for BookQuery. Managed ingress/root expiry discards expired or revoked
  deliveries before the venue callback; their 429 cannot extend the shared pause.
  This is an intentional change to document in CALLERS and prove with late-429
  versus unrelated-live-query control; a slow 429 that native formerly observed
  after caller abandonment is no longer learned. Unrelated discovery/account stale-seed or
  cycle rate-signal policy remains unchanged. A new query can park until a live
  response's pause ends; the failed owner never retries. No new pacing or retry.

## Intended direct cutover

1. In the genuine Fresh command entry, handle Expired only with the existing
   `reject_expired_command(Read)` and absent provenance with
   `reject_unstamped_command(Read)`. Otherwise admit one original
   `admit_caller_read_root(None, None)` at the exact receipt deadline.
2. Retain that root and its original-root completion in the BookQuery request.
   If paused, park the original owner in the existing deferred GET queue with
   no physical child. At eligible dequeue validate the original owner, then
   mint one Read/Never 20,000 ms child capped by the root and queue the unsigned
   request through managed HTTP. Never renew the root while waiting.
3. Validate the exact original root/physical Context/Attempt before envelope,
   codec, native output or success. Finish through the original completion
   authority, never numeric `done`/`fail`. Publish the command result before
   retiring the physical child; failure/admission paths retire first, then fail
   the exact live root. Only proven revoked owners drop without a new terminal.
4. The no-Cx `read_root_retired` hook removes parked work by exact Owner including
   generation. It cannot clear an already scheduled venue timer; a later fire
   finds no matching queue entry and is inert. Existing history cleanup composes
   with this hook but is not a dependency. Stale DATA and reused IDs are inert.
5. Shared get/discovery/subscription paths and all signing, account and trading
   code remain outside this slice. Remove helpers only if this conversion makes
   them provably orphaned. No engine change, new registry or timer policy.

## Original-source assertion controls

Use the real Engine and caller receipt, plus a dual-mode protocol drain that
counts legacy HTTP and real Prepare/Bind/Claim. Native URL/headers/body positives
must run before custody assertions. Do not fabricate missing methods or calls.

- Managed release: expected 0 legacy, 1 Prepare, 1 Claim after native wire checks.
- Configured receipt caps the physical timeout (RED). A raised wait retains the
  native effective 20 s physical bound (GREEN control): legacy timeout_ms None
  already defaults to 20,000 ms, so explicit representation alone is not a bug.
- In-flight DATA at the original receipt deadline is strict Timeout exactly once
  without success or book publication. The live-before-deadline control succeeds.
- A shared rate pause admits no child while parked; receipt expiry before dequeue
  strictly fails once. The stale pause timer cannot later release the request.
- A late/revoked 429 cannot pause an unrelated live query, while a live 429
  still carries native classification and pause; a within-bound deferred
  query still works. Public response never needs credentials/signing or startup.

Additional source/readiness gates include exact delivery and issuer refusal,
physical timeout, held Bind/final-claim expiry, local mint/queue refusal cleanup,
same-id reuse with unrelated live peers, no deactivation/account auth, and no
book/cache publication. Preserve existing native depth-query test expectations
except fixture transport adaptation and the explicitly documented typed transport
outcome change. Compiling original REDs are evidence only, not adoption readiness.

## Review and landing gates

Plan and original assertions precede production. Request bounded native and
Context API fit from the WEEX owners. The source commit must list caller-visible
bound/error changes in CALLERS and run scoped native/managed controls, strict
Clippy/fmt, bans and the exact inventory ratchet. Fresh CI includes WEEX CPU/size.
The legacy TypeScript/AWS runtime and hosted signing setup are untouched.

## Original evidence receipt

The plan was committed first at `e1f0451410d`; the exact live-429 wording was
refined at `a170d99231e` before freezing this assertion checkpoint. Bounded native
and Context reviewers identified the same late-429 limitation; the plan now
records that caller delta. Native review also corrected the physical-timeout
representation test: None already means 20 s, so this is a GREEN control.

Executed on unchanged actual-base production:

- `cargo test -p safe-cex-venue-weex --test depth_query_context`: **5 independent
  assertion REDs, 3 GREEN, 0 ignored**, no compile/setup failure. Native unsigned
  URL/headers/body positives precede the release-custody mismatch `(1,0,0)` vs
  `(0,1,1)`. Receipt cap is None vs Some(1234); equality emits success instead of
  a typed root Timeout; parked expiry emits zero terminals; an expired 429
  parks the unrelated owner instead of releasing its request.
- GREEN: effective native 20 s physical limit with a raised receipt, a live
  fractional-boundary success, and a successful query after a real native pause.
- Existing `--test depth_query`: **8/8 GREEN**. Strict all-feature Clippy on the
  new target and workspace fmt check pass.

The post-cutover stale pause-timer suffix and strict late-429 terminal assertion
follow each test's original first RED; they are readiness gates, not claimed
originally executed. The driver deliberately expects successful Claim and does
not yet exercise typed held-claim refusal. Production, manifest, checker and
STAGING are byte-identical to the base; only plan/index/new test differ.


## Production checkpoint

The original eight assertion bodies are unchanged and now all pass. The new
request stores only its original ReadCompletion and optional physical Context;
there is no numeric terminal list, account/subscription coupling or new registry.
Delivery admission precedes both the native 429 pause and unchanged decoder.
Success finishes the root before physical retirement; failure/preparation and
queue/mint refusal retire the child first and finish through the root. The
no-Cx hook removes only the exact owner's parked query; old timers are inert.

The native decoder/output structs and helpers are byte-identical to the base.
Existing depth-query fixtures now drive real managed preparation, select the
native pause timer by its deadline, and use monotonic response timestamps.
Their financial/alias/version expectations remain. The public subscription
isolation fixture likewise uses managed drain and a live clock; it still proves
all Store/account/ticker/depth state unchanged and normal stream progress.
Native released Network expectation is updated only for the declared outcome
change. The scanner corrects the initial table's lib fingerprint to its actual
`0d6bbb8e...` identity; scope remains four occurrences, three fingerprints.

Local receipts: WEEX all-target run **263 passed, 0 failed/ignored**; strict
all-target/all-feature Clippy, fmt and bans plus signing/request/socket ratchets
pass. A subsequent custody-only capacity control gives **14/14 GREEN**, with
strict all-feature target Clippy passing. It uses 256 genuine paused Read roots,
captures the 257th admission Capacity terminal, then compares the child-mint
refusal to that full value, verifies a live peer releases, and completes fresh
same-ID work. Physical Timeout, held final claim at expiry, typed preparation
NotSent without auth evidence, real non-UTF8 integrity failure, old-generation
DATA/429 with a same-ID successor and unrelated live peer also pass. The original
post-RED stale-timer/late-429 suffix assertions are now executed GREEN.

The first source compile had an empty-header Vec/slice mismatch and was fixed;
the first full venue run found the old raw-HTTP/response-clock fixture and was
fixed without weakening its publication assertions. These are not original RED
claims. Native synchronous post-child queue refusal remains source/shared-helper
evidence, not a separately injected native transport control. No CI/size/CPU,
production source-review or landing approval is claimed by this checkpoint.
Engine, signing recipes, checker policy, STAGING and legacy TS/AWS runtime are
unchanged. Fresh source/native reviews are the next gate.
