# WEEX public order-book query custody

Status: plan before original compiling assertions; no production conversion.
Base: actual `77f124089477a6aed502d9b02aa7f08752b7bfa5`.

## Scope and exact inventory

Convert only command-owned unsigned `FetchOrderBook` / `Task::BookQuery`.
The current semantic slice has four numeric completion occurrences:

| File | Fingerprint | Count | Current role |
| --- | --- | --- | --- |
| venues/weex/src/depth_query.rs | 7277348e8c2d96d515028483928e715a3e9c014072f494e18fd2e527dc2f360b | 1 | success `cx.done` |
| venues/weex/src/depth_query.rs | c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | 2 | HTTP/native and codec `cx.fail` |
| venues/weex/src/lib.rs | c873db6fc77d04d3eab3b771caeb9d71cef58a88fd09870a32b33bf5246e69fa | 1 selected occurrence | BookQuery transport `cx.fail` |

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
  are not silently normalized by this slice. Shared admission, physical Network
  and Timeout, and root expiry use canonical Read errors with outcome None.
  The native transport wrapper previously fabricated Network/NotSent; consuming
  the real typed failure is an intentional caller change to document in CALLERS.
- Preserve the native shared 429 pause only after exact live root/leg/attempt
  admission for BookQuery. Managed ingress/root expiry discards expired or revoked
  deliveries before the venue callback; their 429 cannot extend the shared pause.
  This is an intentional change to document in CALLERS and prove with late-429
  versus unrelated-live-query control. Unrelated discovery/account stale-seed or
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
4. The no-Cx `read_root_retired` hook removes parked work/timers by exact Owner
   including generation. Existing history cleanup composes with this hook but
   is not a dependency. Stale timers, DATA and same-id old generations are inert.
5. Shared get/discovery/subscription paths and all signing, account and trading
   code remain outside this slice. Remove helpers only if this conversion makes
   them provably orphaned. No engine change, new registry or timer policy.

## Original-source assertion controls

Use the real Engine and caller receipt, plus a dual-mode protocol drain that
counts legacy HTTP and real Prepare/Bind/Claim. Native URL/headers/body positives
must run before custody assertions. Do not fabricate missing methods or calls.

- Managed release: expected 0 legacy, 1 Prepare, 1 Claim after native wire checks.
- Configured receipt caps the physical timeout; a raised wait retains the native
  20 s physical bound instead of becoming a longer per-call timeout.
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
