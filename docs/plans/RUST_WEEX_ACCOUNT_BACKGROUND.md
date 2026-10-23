# WEEX account-cycle Background reads

Plan first, 2026-10-24. Base `508b24a080b135d0e9254bc711c8f3cbc589fea4`
from `origin/feat/rust-rewrite`. Owner: codex-cxxyqr; native signer and row
review: claude-4yjlw0. Draft only; no landing authorization.

## Scope and existing behavior

Convert only `WeexFutures::send_account`: balance, positions, normal orders,
algo orders, symbol configuration and account configuration. BingX is excluded:
its account snapshots already use Background Context; its remaining `request`
helper serves command-owned metadata in #1884/#1747.

WEEX currently sends these reads through `signed_request`, which creates legacy
HTTP then stages the existing dispatch signing recipe. Preserve all native URLs,
query sorting, streaming selectors, row codecs, revision/cycle fences, complete
snapshot publication, malformed-row retention, auth evidence and rate handling.
GET selects the configured Secondary identity or falls back to Primary, including
passphrase fallback, through the existing `credentials.select(Method::Get)`.
The selected slot and that identity's credential reference define the policy.

Each physical page receives its own Background Read Context at eligible dequeue:
20,000 ms, no pacing, RetryPolicy::Never. There is no whole-cycle root, retry,
re-sign, command terminal or Plan. Preserve the native cycle deadline and
continuation-page rearming, the 2-second poll, settings backoff, 429 pause, native
page limits and per-kind initial page numbers. No Context is minted while paused.

## Direct cutover

Add optional physical-page Context custody to the account Request: None while
constructed or parked, Some only after Background admission. This is queue state,
not a legacy fallback. Build the existing dispatch::Http recipe with the same
sorted path/query and selected identity, call request_signed_managed, apply the
native stream selector and tag_account_read on that managed id. Keep the legacy
helper for its other callers; no shared engine API changes.

Before any account streamed-row decode, require the exact Background delivery,
the current seed generation and the existing cycle fence. At response ingress,
check those gates before rate/auth/native reducers, then release the Context
with finish_http_context before the native reducer. Failure and sign-error
paths release it before their existing native failure reducer, even when stale;
stale seed/cycle deliveries have no native effects. Continuations finish the
previous physical Context before admitting a fresh page. Admission/preparation
refusal follows account_failed promptly without HTTP, retry, auth inference or
deactivation. Provider refusal remains a pre-send failure. Transport failure and
20-second Timeout retain last-good state and permit the next poll.

## Original controls before production

Commit tests on unchanged runtime. Distinguish assertion REDs from compatibility
controls. Real native startup plus provider signing must show all six account
reads use managed Prepare/Bind/Claim (legacy Action::Http before Prepare is RED),
with no signed account bytes released before Claim. Check both configured
Secondary and Primary fallback, native paths/query/streaming and successful
snapshot publication as positives. The provider must delegate to the fixture's
normal signing recipe when not refusing.

Add controls for refusal and physical Timeout recovery at the next poll; no
retry or auth/deactivation notice. Preserve settings-backoff behavior. Cover
normal/algo continuation pages and 429 delayed dequeue, cycle/seed stale delivery,
no publication after expiry, and an unrelated live read staying usable. Existing
account snapshot, partial/malformed retention, stream/ACK fences and auth tests
remain unchanged except mechanical managed-action draining where required.

## Inventory, callers and validation

Regenerate actual WEEX inventory from source in incoming order; require a strict
removal-only prune of send_account's signed_request site. Never widen/rekey/add
STAGING_BOOTSTRAPS. Its lib.rs staging entry remains until all remaining legacy
callers migrate; this slice retires one blocker, not the entire staging helper.

The source commit's CALLERS entry records shared preparation/claim authority,
20-second per-page/no-retry behavior, prompt pre-send failure and next-poll
recovery, with native snapshots and wire shapes preserved. Run focused new tests,
account/auth/dispatch native targets, strict WEEX all-target Clippy, fmt and
request-context inventory/checker controls. CI supplies full suites and WEEX
WASM/replay size/CPU gates. Independent native/source and preservation review
precedes readiness; open a draft PR targeting feat/rust-rewrite and do not merge.
