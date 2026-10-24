# CPU evidence transport retrieval retry

Owner codex-mxb4mq; reviewer codex-hcmq3f. Priority over Backpack history.
Base: actual landing `79506be705aad98cda97eeee5435c6aa169b62e7`.

## Failure and contract

After successful CPU timing, the evidence end-check can fail fetching GitHub
metadata (`fetch failed`, cause EPIPE); observed #2064 Rust40127077663,
job120975392857. Existing retrieval retries only one immutable binary socket
closure. It does not cover metadata used by the final fresh inventory read.

Add a fixed three-attempt retrieval budget with backoff1000ms,2000ms for each
reader GET, including body reads and validated artifact/log redirects. Retry
only known network error codes (including EPIPE/socket/reset/timeouts/DNS) and
HTTP500–599. HTTP4xx (especially403/429), untrusted redirects, empty/malformed
metadata, size limits, JSON, API identity, digest, upload/attempt and final
provenance mismatches still fail immediately. Preserve the prior single empty
immutable-artifact retry; it shares the same total retrieval budget rather
than nesting retries. Preserve existing cache/snapshot semantics for complete responses, never
failed partial bytes. Retry the original API URL with identical authorization;
CDN redirects must still strip the token and revalidate the destination.

No repeated timing/measurement, workflow rerun, metadata-convergence refresh,
changed verdict, baseline or capsule identity. All attempt1, producer, digest,
ZIP and end-check comparisons remain exact and outside transport recovery.
No workflow/time-budget increase is needed for three seconds scheduled delay.

## Original controls and review

Commit plan, then originals: EPIPE during end-check converges and passes;
HTTP502 metadata converges; permanent EPIPE/5xx hits a fixed cap; exact real
producer/artifact mismatch after recovery remains fatal without another fetch;
HTTP403/429 and malformed metadata never recover. Pin repeated URL/options,
backoff, unchanged snapshot/cache behavior and token stripping. Update existing
transport-contract tests only where the approved retrieval policy changes.
Run scoped CPU provenance/collection/budget controls. One hcmq3f review, draft
PR and one qualifying CI head; auditor owns landing.

## Scoped verification and source review

Originals9eddc57cfa0 execute12 intended transport failures and9 working controls.
Hcmq3f independently hash-verified originals and exact source patchc2a50c72;
plan/original/source direction CLEAR. This is not independent test execution.
Applied source matches that patch exactly. Existing tests change only the
approved transport contract (three attempts, metadata/active-log recovery and
5xx recovery) and inject sleep instead of waiting; all evidence assertions stay.

Mixed empty/network/5xx controls pin the shared cap, including failed partial
reads, a second empty response, and successful final bytes cached once. Unknown
body errors and a provenance-labelled Error stay fatal. CDN5xx exhaustion
replays the API authority three times without CDN authentication. Existing
response-size boundary, duplicate/attempt/digest/upload/ZIP and request-budget
controls remain part of the scoped run. Snapshot/cache behavior is preserved,
not a new validation guarantee. Delays total at most3s per retrieval chain;
existing transport timeouts and producer-metadata convergence are unchanged.

Frozen qualification: CPU provenance/CI/artifact/producer/budget and transport
controls108/108 PASS; collection/cache/remeasure controls35/35 PASS, 0 skipped.
Production reader blob1257bdc55123 matches hcmq3f's exact reviewed patch.
A local bans run on the stale base reported only incoming Aster inventory growth;
integrate actual #2027 before the single CI push and rerun the local check.
