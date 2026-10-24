# Crypto.com unsigned public discovery ownership

Plan first, codex-ih35js, 2026-10-24. Actual base
`db6fa45bbb7cbbfdfeb0aa40d3dd93caac9c9e0b`.

## Exact scope and count

Markets and Tickers use one shared unsigned GET sender, lib.rs::get:
`121dc8add2c9a7d1d8fd691023a64c9ebb736360f0c97363cfc66956b0ec1405`,
one occurrence. Retire that sender using actual managed Background Read APIs.
Delete only the Crypto.com lib.rs manifest entry. Actual-tip venue inventory
is 20 fingerprints / 21 occurrences; this slice leaves 19 / 20. #2045 separately
removes eight trading sites, so after both land the venue becomes 11 / 12.
This is the largest collision-free, signer-independent direct sender remaining.

No open PR touches the Crypto.com lib.rs discovery regions or dedicated tests;
#2045 owns trading.rs only. All Aster source is either held or needs separately
proven retry/clock custody, and MEXC remaining sends require private signing.
No engine, crypto, host, signer, credential-selection, Cargo or combo-5b source
change. Options validation, read-key references and all private paths stay exact.

## Native wire and retry behavior

Preserve unsigned GET {rest}/public/get-instruments and /public/get-tickers,
empty headers/body, default/testnet hosts, streamed result.data path and all
row parsers. Each physical discovery read becomes Background + Read,
credential None, 20 s, Never. The bound starts at issue, before route readiness.
Native discovery retries remain separate independent operations issued only
by the existing startup Backoff<bool>: nominal 1000, 2000, 4000 ... ms with its
existing jitter and cap/reset. Never introduce another retry loop or timer.
No credential health evidence, signer or command terminal belongs to discovery.

Store publication, malformed-market/index maps and all row diagnostics retain
the original validity/ordering. Preserve usable-peer partial catalogue commit,
listed-market retention, malformed exposure coverage, partial reread bound,
independent ticker recovery and existing startup side effects. Private snapshot
starts after successful initial catalogue while the ticker is pending; public
socket/channel initialization still waits for successful tickers. Failed or
empty ticker lists retain prior quotes. Do not change status/native envelope
policy or invent a venue error-code ladder.

## Authority and bounded adoption

Each Markets/Tickers pending continuation carries its original Background Read
Context. Issue with http_context(Scope::Background, policy), then submit through
request_managed and stream_response on result.data. Issuance/submission refusal
re-enters the same native discovery failure/backoff path without numeric failure.

Gate on_item and successful/error response DATA handling through the actual
validate_background_read_http_delivery before parsing/staging/index/Store edits.
Expiry at equality cannot publish rows, diagnostics derived from failed DATA,
or startup side effects, in either timer/response order. Stale deliveries are
inert. After an accepted catalogue publishes Store, retire its Context before
issuing ticker/private follow-ups so full capacity can admit the next stage.
Successful ticker publication retires before socket/channel side effects.

Use finish_http_context for cleanup, including failure cleanup, not as DATA
admission. An actual engine-admitted preparation/transport/timeout failure must
retain its native retry exactly once even if sibling issuance has already pruned
expired registry metadata; failed cleanup is not a reason to drop that callback.
Failed payloads never become DATA. Do not copy issuer/attempt validation or add
venue context registries. Unexpected custody/API gaps go to the engine owner.

## Original REDs and working controls

Commit a dedicated real-Engine target after this plan, before production. Tests
use existing public fixture literals but no changed shared fixture. They can
drive real preparation/bind/final claim with test_support::prepared_actions;
original legacy behavior stays executable and assertions, rather than missing
methods or compile errors, provide the RED evidence.

RED controls: catalogue and ticker wait for Public preparation; parked route
expires at 20 s and late readiness cannot release; current DATA at equality
cannot publish in either timer/response order; full genuine command-owner
capacity refuses initial discovery, recovers after freeing one peer and never
replaces the other owners; accepted catalogue retires its slot before ticker
handoff at capacity; expired ticker retains cached quotes and cannot open a
socket; failure remains retriable after sibling issuance prunes expiry metadata.

Working native controls: exact public URLs/method/headers/body and demo host;
valid catalogue/ticker financial values; 1000/2000/4000 jittered backoff;
independent ticker failure and cached-quote retention; no command/auth/signing
notices; partial catalogue peers commit; private startup precedes ticker success;
old physical callbacks after retry are inert. Real capacity peers use the
already-managed public book command, never fabricated Contexts/receipts.
Preserve original assertions in a separate RED commit and record executed
RED/GREEN counts independently from planned adoption coverage.

## Gates and caller effects

Independent source/adversarial, caller/native-policy and full-map plan/control
review before production. The overseer authorizes production after those gates.
Then exact one-site prune and append CALLERS/AUDITS/QUALITY: public wire, retry
cadence, rows and startup sequence stay native; route preparation is now
bounded before I/O and expired rows cannot commit. Existing fixtures only gain
actual managed preparation glue, with financial assertions unchanged.

Sequential scoped Crypto.com tests and strict Clippy use CARGO_BUILD_JOBS=4;
formatting, bans, request ratchet/test pair and read-only credential-count
preservation checks pass before push. No local broad workspace/all-feature,
WASM, performance or load sweep. CI owns broad replay/size/CPU/memory. Push a
small draft with original RED and independent review receipts; no merge without
an explicit overseer GO.

Architecture gate: **No**. Venue codecs consume the actual shared Background
Read and startup Backoff choke points. No local selection, lifetime, retry,
issuer, finalizer or scheduling algorithm is added.
