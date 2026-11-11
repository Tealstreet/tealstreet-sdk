# MEXC Futures

## Hosted and delegated signing

Local custody and RemoteSigner share provider-owned HMAC-SHA256 signing for
all private REST requests and private WebSocket login. REST signs the public
API key followed by integer protocol milliseconds and the sorted encoded GET
query or exact serialized JSON body. Login signs the public key and timestamp.
Only that canonical payload crosses the signing callback; secret material stays
in the executing device/provider. No signing nonce is required. Native dispatch
retains its existing receive-window clamp and clock-recovery policy.

PayloadSigner is supported for canonical REST HMACs, with REST-only account
polling and no private socket, authentication timers or reconnects. RemoteSigner
takes precedence and never falls back to PayloadSigner or a local secret.
Provider refusals before dispatch remain `notSent`; ambiguous dispatched writes
remain `Unknown`. The persisted Primary account namespace is mapped before
credential trimming in both local and delegated setup.

This Futures API uses API-key HMAC, with no SDK wallet/agent cosign protocol.
Delegated custody may sign for its provisioned account's API key. Secondary read
keys retain their native custody path; the SDK's global policy currently
bypasses saved read keys. Direct secondary RemoteSigner setup refuses with
`unsupported` / `notSent`: the raw HMAC device callback cannot select that slot.
When read keys are enabled, remove them or use browser-local custody.

`tests/fixtures/hosted-signing.json` contains independent fixed HMAC vectors.
SDK tests compare local-reference, hosted capture and actual WASM delegate
bytes, including Unicode, batch arrays and serialized body whitespace. Native
tests cover dispatch, private-feed policy, clock recovery and trading outcomes.
These are deterministic fixtures; no live account connectivity is claimed.

Authentication reference: https://www.mexc.com/api-docs/futures/integration-guide

## Socket liveness

Private sockets use `safe_cex_engine::socket::Supervisor` with a ten-second
connection deadline and a separate ten-second authentication deadline. Native
heartbeats run every 8 seconds; 20 seconds without traffic closes the socket.
Pongs do not extend authentication. Rejected login closes and retires the socket
before scheduling capped exponential backoff with jitter. Stable authenticated
traffic resets backoff; merely opening a socket does not.

The existing public socket policy remains separate. Deterministic lifecycle
regressions are in `tests/socket_liveness.rs`; private replay tapes provide
scripted evidence, not live account connectivity evidence.

Private read refusals are decoded from success:false before list validation.
Startup, background account reads, identity, leverage and history retain the
native code and a bounded native message. Codes 401/602 are invalid-key or
signature refusals, 402 is expiry, and 406/511/701–704 are permission refusals;
background diagnostics use auth.rejected. A malformed envelope, an HTTP
status alone, or error-looking text cannot prove a key refusal.

MEXC's contract error table identifies 513 as the request-time-window error
and 602 as signature verification failure:
https://mexcdevelop.github.io/apidocs/contract_v1_en/#error-code-example
https://www.mexc.com/api-docs/futures/integration-guide

Only a current signed GET receiving 513 without data may issue one unsigned
GET /api/v1/contract/ping, then replay that same GET with the same selected
credential and original ownership bound. The server sample must be a positive,
safe integer from a successful envelope. Failed, malformed or late samples do
not calibrate or replay. A second 513 ends that attempt. Writes and 602 never
enter clock recovery. Signing already takes fresh protocol-clock milliseconds
at dispatch; calibration changes outbound timestamps, not engine deadlines.

## Protective edits

Known-class price edits use MEXC's native plan/TP-SL modify endpoints without a
cancel fallback. Native modifies also require a fresh own-source reference and
an uncrossed trigger at final send. HTTP 429 and typed rate-limit errors (including
HTTP 200/code 510) are Unknown, never proof that a modify was not applied.
Price-only edits omit amount (or use 0 for full-position stops); an explicit
positive amount requests fixed-size protection. Quantity edits inspect the
original, guard the destructive send
with a genuine fresh trigger reference, and require a terminal cancellation read.
Only a standalone stop with exact native provenance permits one put-back after a
definitive refusal; unknown outcomes never restore or resend. See
[`mexc-protective-edits.md`](../../../../docs/plans/callers/mexc-protective-edits.md)
for the endpoint classes, legacy signer-window limit and native controls.

## Batch refusal fallback

An explicit batch-access refusal can switch ordinary orders to individual
submits only under a non-ambiguous HTTP status. A dispatched batch answered
408, 429 or any status at least 500 is Unknown regardless of a code 511 or
access-denied body. It never resubmits those orders or disables batching for
later commands. Definitive refusals retain the existing one-time paced fallback.

## Broker attribution

Every signed private HTTP request carries `source: TEAL`, matching CCXT's scope.
The header is inserted after signing and is not part of the HMAC input. Public
market requests and WebSocket authentication retain their existing wire format.
The native dispatch adapter and the SDK's hosted HTTP capture formatter both
apply attribution, including single and batch creates, attached TP/SL, plan
orders and position TP/SL. The declared native-header policy protects `source`
from callback overrides.
Native protection for this broker header compiles only with the `mexc` feature;
unrelated families retain their prior compiled construction metadata.

This crate implements perpetual futures. MEXC's broker protocol uses the same
header and broker ID for spot and futures; this change adds no spot adapter.
LOCAL and resident DELEGATED providers use the same LocalSigner recipe, while
COSIGNED completes that recipe through the existing external signing path.
MEXC DELEGATED admission remains disabled by the existing Bybit-only flag.

Sources:

- [MEXC broker FAQ, Technical Integration](https://www.mexc.com/api-docs/broker/broker-faq#step-3-technical-integration)
- [MEXC futures signature rules](https://www.mexc.com/api-docs/futures/integration-guide#request-format)
- [MEXC spot signature rules](https://www.mexc.com/api-docs/spot-v3/introduction#signature)
- [CCXT pinned MEXC spot signing, lines 6434–6439](https://github.com/ccxt/ccxt/blob/e27b747210ba4f7505de3c0b0e29dc820a96ada4/ts/src/mexc.ts#L6434-L6439)
- [CCXT pinned MEXC futures signing, lines 6472–6480](https://github.com/ccxt/ccxt/blob/e27b747210ba4f7505de3c0b0e29dc820a96ada4/ts/src/mexc.ts#L6472-L6480)
