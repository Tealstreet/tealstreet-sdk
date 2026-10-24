# MEXC unsigned discovery and depth Read ownership

Plan first, codex-ih35js, 2026-10-24. Exact base:
`77f124089477a6aed502d9b02aa7f08752b7bfa5`.
Read with RUST_REQUEST_CONTEXT.md, RUST_MEXC_REQUEST_CONTEXT.md and the caller,
audit and quality ledgers. No production or inventory change in this checkpoint.

## Inventory and ownership

The base checker inventory has **18 fingerprints / 18 occurrences** for MEXC:
account 6, history 1, identity 2, lib 3, secondary_tests 2, trading 4.
This bounded slice retires exactly **2 fingerprints / 2 occurrences** from
`venues/mexc/src/lib.rs`, leaving **16 / 16** for MEXC and one lib witness.

| Existing site | Consumer | Managed destination |
| --- | --- | --- |
| `get`: `cx.request`, fingerprint `a1d7eb3cdf7f4e938351efa132ee68dac79fc9e6e26db557c0ffd67282e9ae14` | Subscription depth snapshot | Unsigned Background Read, one managed HTTP operation |
| `stream`: `cx.request_streamed`, fingerprint `e7ec3b5f8c170e9a5887721f2eedda084ecf9f5a5eba8d1e899957c63019e0bb` | Markets and ticker discovery | Unsigned streamed Background Read per physical operation |

The conservative `matches!(request.call, AmendRead { .. })` witness
`2cb4c59f38cca507a15b20d1ace303afd27defc1b4fe0aba50dddcbcb1f9c37c`
stays unchanged. Account/history/identity/trading and the signed funnel stay
legacy until dispatch recipes and their actual native pacing/custody are ready.
No private or command-owned read is relabelled Background.

Open PR file lists inspected before editing: no MEXC production/test path is held.
The MEXC signer owner codex-5c57jl has only plan/RED work and clears this unsigned
scope. WOO lib/account are held by socket #1930 and WOO/Crypto account/trading by
#1788; Aster remote retry/clock custody remains a shared prerequisite. Phemex is
held on #1974/5b. Crypto's single public sender offers a smaller shrink.

## Actual engine authority

Use actual `http_context(Scope::Background, Policy { Read, credential: None,
timeout_ms: 20_000, pacing: None, retry: Never })`, followed by
`request_managed` with the existing GET URL, empty headers/body and original
EnvelopeField("data") streaming geometry. Store the exact Context in each
Markets/Tickers/Depth Pending. Native retry is a subsequent Background operation;
never renew a Context or add an automatic same-call retry, owner, timer or loop.

Use actual #1856 `validate_background_read_http_delivery` synchronously before
stream-row decoding, `volume_units` edits, accumulation, Store publication or
book snapshot mutation. Do not copy issuer/attempt/expiry checks into venue code.
Native generation, current subscription, envelope and financial validation remain
necessary independently. A terminal success retires the exact Context only after
its final DATA validation. Submission, preparation, transport, parse and timeout
failures retire through `finish_http_context` before the native failure path;
expired current handles may retire, revoked handles cannot touch newer work.
Failure ownership is not DATA authority. No command terminal or auth-health
outcome is created for these credential-free Background operations.

Native response headers still supply 429 pause evidence in their existing scope
before stale DATA is discarded. Keep the discovery envelope check precedence:
an invalid discovery envelope does not newly become rate evidence. Preserve
`depth_paused_until` and the matching account pause without using a stale payload
as rows, progress or snapshot. Preserve failure/backoff once after a rejected
late response, including when the host deadline timer has not fired yet.

## Native contracts and caller effects

Markets: `/api/v1/contract/detail`; ticker: `/api/v1/contract/ticker`.
Catalogue rows keep malformed-symbol tracking, valid-peer publication, contract
size/volume-unit conversion and partial-catalogue rereads. The private startup
starts after accepted markets, independently of ticker completion. Transient
failure keeps startup::Backoff's nominal 1/2/4/... second retry and existing jitter;
it never becomes terminal credential failure. An expiry or failed preparation
has the same native retry shape as an ordinary failed operation.

Depth: `/api/v1/contract/depth/{native_id}?limit=1000`, GET, no credential/header/body.
Keep the public-open gate, per-symbol generation fence, buffering bound 2000,
contract-size scaling, exact u64 version, version+1 replay and gap resync. Preserve
Backoff(250ms, 7, 30s), native nominal book schedule, and Retry-After handling.
Unsubscribe/re-subscribe retains its native generation fence. An old operation
cannot retire or publish the replacement's snapshot or another symbol's peer.

Caller-visible correction: route preparation/final release now precedes public
HTTP, and data arriving at or after the immutable 20-second operation deadline
cannot load markets/tickers or replace a book, even before timer ingress. Failed
or expired operations still retry through native startup/book recovery. Wire,
subscription ordering, financial assertions and live successful results remain.
Record that correction in CALLERS with the implementation, not as a claim here.

## Original controls and review sequence

1. Commit this plan before tests; preserve production and the full inventory.
2. Add a dedicated real-Engine integration target. Original assertion REDs:
   discovery and depth wait for Public route preparation without raw HTTP/Sign;
   parked preparation expires and late readiness has no authority; at-equality
   late market/ticker/depth DATA is inert before host timer delivery; a streamed
   catalogue whose valid row arrived live cannot publish on an expired HttpEnd.
3. Working controls pin exact unsigned URLs/headers/body/stream selection, native
   market/ticker values, buffered-depth replay, old-generation rejection, no
   public credential/auth evidence, native startup backoff and depth 429 pause.
   Failed current operation cleanup permits a fresh retry; late retired replies
   cannot mutate that retry. Add real preparation-refusal controls at adoption.
4. Obtain independent source/adversarial, caller and full-map plan/RED reviews.
   Then implement only the mapped public callbacks and prepared fixture glue.
   Original assertions stay identifiable and unchanged. No signer/core changes.
5. Exact removal-only manifest prune, both request-context checks, focused MEXC
   plus Binance native tests, touched strict clippy/fmt and bans. Independent
   source/caller/full-map reviews again. CI alone measures family bytes/CPU.
   One draft PR; no merge without the overseer's explicit window.

Architecture gate: **No**. This adopts existing Background Read issuance,
managed dispatch and exact delivery validation. It adds no venue executor,
issuer, timer, retry, credential policy or lifetime validator.
